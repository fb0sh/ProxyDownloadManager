import { useEffect } from "react";
import type { QueryClient } from "@tanstack/react-query";
import type { PluginListener } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { WebviewWindow } from "@tauri-apps/api/webviewWindow";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { t } from "../i18n";
import type { DownloadItem } from "../types";
import { EVENTS } from "../constants/events";
import { useWindowManager } from "./useWindowManager";

/** Patch a download-list cache with updated progress for a single download. */
export function patchDownloadProgress(
  cache: DownloadItem[] | undefined,
  id: number,
  downloaded: number,
): DownloadItem[] | undefined {
  if (!cache) return cache;
  return cache.map((d) => (d.id === id ? { ...d, downloaded } : d));
}

interface DownloadEventsOptions {
  queryClient: QueryClient;
}

async function sendDownloadNotification(id: number, title: string, body?: string) {
  try {
    const { isPermissionGranted, requestPermission, sendNotification } =
      await import("@tauri-apps/plugin-notification");
    const ok = await isPermissionGranted();
    if (!ok) {
      const perm = await requestPermission();
      if (perm !== "granted") return;
    }
    sendNotification({ title, body: body ?? `Download #${id}` });
  } catch {
    // Tauri notification API unavailable — fall back to web Notification
    try {
      if (window.Notification.permission === "granted") {
        new window.Notification(title, { body: body ?? `Download #${id}` });
      }
    } catch { /* web Notification also unavailable */ }
  }
}

export function useDownloadEvents({ queryClient }: DownloadEventsOptions) {
  const { openNewDownload, openDetails } = useWindowManager();

  // Single subscription for all Tauri events — one setup/teardown cycle
  useEffect(() => {
    const unlisteners: Promise<() => void>[] = [];

    // Browser download URL from extension
    unlisteners.push(
      listen<string>(EVENTS.BROWSER_DOWNLOAD_URL, (event) => {
        openNewDownload(event.payload);
      })
    );

    // Structural changes: full refetch
    for (const eventName of [EVENTS.DOWNLOAD_PAUSED, EVENTS.DOWNLOAD_RESUMED, EVENTS.DOWNLOAD_CANCELLED]) {
      unlisteners.push(
        listen(eventName, () => {
          queryClient.invalidateQueries({ queryKey: ["downloads"] });
        })
      );
    }

    // Download created: refetch + focus main window
    unlisteners.push(
      listen(EVENTS.DOWNLOAD_CREATED, async () => {
        queryClient.invalidateQueries({ queryKey: ["downloads"] });
        try {
          const mainWin = await WebviewWindow.getByLabel("main");
          if (mainWin) { await mainWin.show(); await mainWin.setFocus(); }
        } catch { /* main window may not exist */ }
      })
    );

    // Progress: optimistic cache update (avoids full refetch for high-frequency events)
    unlisteners.push(
      listen<{ id: number; downloaded: number }>(EVENTS.DOWNLOAD_PROGRESS, (event) => {
        const { id, downloaded } = event.payload;
        queryClient.setQueryData<DownloadItem[]>(["downloads"], (old) =>
          patchDownloadProgress(old, id, downloaded)
        );
      })
    );

    // Started notification
    unlisteners.push(
      listen<number>(EVENTS.DOWNLOAD_STARTED, (event) => {
        sendDownloadNotification(event.payload, "Download Started");
      })
    );

    // Completed: notification + open details
    unlisteners.push(
      listen<{ id: number; file_name: string }>(EVENTS.DOWNLOAD_COMPLETED, async (event) => {
        const { id, file_name } = event.payload;
        await sendDownloadNotification(id, "Download Complete", file_name);
        openDetails(id);
      })
    );

    // Error notification
    unlisteners.push(
      listen<{ id: number; url: string; message: string }>(EVENTS.DOWNLOAD_ERROR, (event) => {
        const { id, message } = event.payload;
        sendDownloadNotification(id, t("downloadError.failed"), message.slice(0, 100));
      })
    );

    return () => {
      unlisteners.forEach((u) => u.then((f) => f()));
    };
  }, [queryClient, openNewDownload, openDetails]);

  // Notification click handler
  useEffect(() => {
    let unreg: PluginListener | null = null;
    let cancelled = false;
    (async () => {
      const mod = await import("@tauri-apps/plugin-notification");
      unreg = await mod.onAction(async (notification) => {
        if (cancelled) return;
        const extra = (notification as any)?.extra;
        const id = extra?.downloadId;
        const ntype = extra?.type;
        if (ntype === "started") {
          try {
            const mainWin = await WebviewWindow.getByLabel("main");
            if (mainWin) { await mainWin.show(); await mainWin.setFocus(); }
          } catch { /* main window may not exist */ }
        } else if (id) {
          openDetails(Number(id));
        }
      });
    })();
    return () => { cancelled = true; if (unreg) unreg.unregister(); };
  }, [openDetails]);

  // Window focus refresh
  useEffect(() => {
    let unreg: (() => void) | null = null;
    let cancelled = false;
    (async () => {
      const unlisten = await getCurrentWindow().onFocusChanged(({ payload: focused }) => {
        if (cancelled) return;
        if (focused) {
          queryClient.invalidateQueries({ queryKey: ["downloads"] });
        }
      });
      if (!cancelled) unreg = unlisten;
    })();
    return () => { cancelled = true; if (unreg) unreg(); };
  }, [queryClient]);
}
