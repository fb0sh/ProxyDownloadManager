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

interface DownloadEventsOptions {
  queryClient: QueryClient;
}

async function sendDownloadNotification(id: number, title: string, body?: string, _ntype?: string) {
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
    try {
      if (window.Notification.permission === "granted") {
        new window.Notification(title, { body: body ?? `Download #${id}` });
      }
    } catch {}
  }
}

/** Events that simply invalidate the downloads query. */
const INVALIDATE_EVENTS = [
  EVENTS.DOWNLOAD_PAUSED,
  EVENTS.DOWNLOAD_RESUMED,
  EVENTS.DOWNLOAD_CANCELLED,
];

export function useDownloadEvents({ queryClient }: DownloadEventsOptions) {
  const { openNewDownload, openDetails } = useWindowManager();

  // browser-download-url
  useEffect(() => {
    const unlisten = listen<string>(EVENTS.BROWSER_DOWNLOAD_URL, (event) => {
      openNewDownload(event.payload);
    });
    return () => { unlisten.then((f) => f()); };
  }, [openNewDownload]);

  // All invalidation events in one subscription
  useEffect(() => {
    const unlisteners = INVALIDATE_EVENTS.map((eventName) =>
      listen(eventName, () => {
        queryClient.invalidateQueries({ queryKey: ["downloads"] });
      })
    );
    // download-created also invalidates but has extra side effects
    const createdUnlisten = listen(EVENTS.DOWNLOAD_CREATED, async () => {
      queryClient.invalidateQueries({ queryKey: ["downloads"] });
      try {
        const mainWin = await WebviewWindow.getByLabel("main");
        if (mainWin) { await mainWin.show(); await mainWin.setFocus(); }
      } catch {}
    });
    return () => {
      unlisteners.forEach((u) => u.then((f) => f()));
      createdUnlisten.then((f) => f());
    };
  }, [queryClient]);

  // download-progress (optimistic update)
  useEffect(() => {
    const unlisten = listen<{ id: number; downloaded: number }>(EVENTS.DOWNLOAD_PROGRESS, (event) => {
      const { id, downloaded } = event.payload;
      queryClient.setQueryData<DownloadItem[]>(["downloads"], (old) => {
        if (!old) return old;
        return old.map((d) => (d.id === id ? { ...d, downloaded } : d));
      });
    });
    return () => { unlisten.then((f) => f()); };
  }, [queryClient]);

  // download-started
  useEffect(() => {
    const unlisten = listen<number>(EVENTS.DOWNLOAD_STARTED, (event) => {
      sendDownloadNotification(event.payload, "Download Started", undefined, "started");
    });
    return () => { unlisten.then((f) => f()); };
  }, []);

  // download-completed
  useEffect(() => {
    const unlisten = listen<{ id: number; file_name: string }>(EVENTS.DOWNLOAD_COMPLETED, async (event) => {
      const { id, file_name } = event.payload;
      await sendDownloadNotification(id, "Download Complete", file_name, "completed");
      openDetails(id);
    });
    return () => { unlisten.then((f) => f()); };
  }, [openDetails]);

  // download-error
  useEffect(() => {
    const unlisten = listen<{ id: number; url: string; message: string }>(EVENTS.DOWNLOAD_ERROR, (event) => {
      const { id, message } = event.payload;
      sendDownloadNotification(id, t("downloadError.failed"), message.slice(0, 100), "error");
    });
    return () => { unlisten.then((f) => f()); };
  }, []);

  // Notification clicks
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
          } catch {}
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
