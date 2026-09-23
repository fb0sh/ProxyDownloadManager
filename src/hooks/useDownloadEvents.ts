import { useEffect } from "react";
import type { QueryClient } from "@tanstack/react-query";
import { WebviewWindow } from "@tauri-apps/api/webviewWindow";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { t } from "../i18n";
import { subscribeDownloadEvents } from "../downloadEvents";
import { useWindowManager } from "./useWindowManager";

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
      if (window.Notification.permission === "default") {
        await window.Notification.requestPermission();
      }
      if (window.Notification.permission === "granted") {
        new window.Notification(title, { body: body ?? `Download #${id}` });
      }
    } catch { /* web Notification also unavailable */ }
  }
}

/** Main-window consumer of the download-events seam: the cache-write policy
 * lives in the subscription; this hook only adds the domain reactions. */
export function useDownloadEvents({ queryClient }: DownloadEventsOptions) {
  const { openNewDownload, openDetails } = useWindowManager();

  useEffect(() => {
    return subscribeDownloadEvents(queryClient, {
      onBrowserDownloadUrl: (payload) => openNewDownload(payload),
      onCreated: async () => {
        try {
          const mainWin = await WebviewWindow.getByLabel("main");
          if (mainWin) { await mainWin.show(); await mainWin.setFocus(); }
        } catch { /* main window may not exist */ }
      },
      onStarted: (id) => {
        sendDownloadNotification(id, t("notification.started"));
      },
      onCompleted: async ({ id, file_name }) => {
        await sendDownloadNotification(id, t("notification.completed"), file_name);
        openDetails(id);
      },
      onError: ({ id, message }) => {
        sendDownloadNotification(id, t("downloadError.failed"), message.slice(0, 100));
      },
    });
  }, [queryClient, openNewDownload, openDetails]);

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
