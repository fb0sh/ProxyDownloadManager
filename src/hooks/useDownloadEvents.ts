import { useEffect, useCallback } from "react";
import type { QueryClient } from "@tanstack/react-query";
import type { PluginListener } from "@tauri-apps/api/core";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { WebviewWindow } from "@tauri-apps/api/webviewWindow";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { t } from "../i18n";
import type { DownloadItem } from "../types";

interface DownloadEventsOptions {
  queryClient: QueryClient;
  openNewDownloadWindow: (url?: string) => void;
  openDownloadDetailsWindow: (id: number) => void;
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
    let fileName: string | undefined = body;
    if (!fileName) {
      try {
        const items = await invoke("list_downloads") as Array<{ file_name: string; id: number }>;
        fileName = items.find((d) => d.id === id)?.file_name;
      } catch {}
    }
    sendNotification({ title, body: fileName ?? `Download #${id}` });
  } catch {
    try {
      if (window.Notification.permission === "granted") {
        new window.Notification(title, { body: body ?? `Download #${id}` });
      }
    } catch {}
  }
}

export function useDownloadEvents({
  queryClient,
  openNewDownloadWindow,
  openDownloadDetailsWindow,
}: DownloadEventsOptions) {
  const onUrlDetected = useCallback((url: string) => {
    openNewDownloadWindow(url);
  }, [openNewDownloadWindow]);

  // browser-download-url
  useEffect(() => {
    const unlisten = listen<string>("browser-download-url", (event) => {
      onUrlDetected(event.payload);
    });
    return () => { unlisten.then((f) => f()); };
  }, [onUrlDetected]);

  // download-created
  useEffect(() => {
    const unlisten = listen("download-created", async () => {
      queryClient.invalidateQueries({ queryKey: ["downloads"] });
      try {
        const mainWin = await WebviewWindow.getByLabel("main");
        if (mainWin) { await mainWin.show(); await mainWin.setFocus(); }
      } catch {}
    });
    return () => { unlisten.then((f) => f()); };
  }, [queryClient]);

  // download-paused
  useEffect(() => {
    const unlisten = listen<number>("download-paused", () => {
      queryClient.invalidateQueries({ queryKey: ["downloads"] });
    });
    return () => { unlisten.then((f) => f()); };
  }, [queryClient]);

  // download-resumed
  useEffect(() => {
    const unlisten = listen<number>("download-resumed", () => {
      queryClient.invalidateQueries({ queryKey: ["downloads"] });
    });
    return () => { unlisten.then((f) => f()); };
  }, [queryClient]);

  // download-cancelled
  useEffect(() => {
    const unlisten = listen<number>("download-cancelled", () => {
      queryClient.invalidateQueries({ queryKey: ["downloads"] });
    });
    return () => { unlisten.then((f) => f()); };
  }, [queryClient]);

  // download-progress (optimistic update)
  useEffect(() => {
    const unlisten = listen<{ id: number; downloaded: number }>("download-progress", (event) => {
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
    const unlisten = listen<number>("download-started", (event) => {
      sendDownloadNotification(event.payload, "Download Started", undefined, "started");
    });
    return () => { unlisten.then((f) => f()); };
  }, []);

  // download-completed
  useEffect(() => {
    const unlisten = listen<number>("download-completed", async (event) => {
      await sendDownloadNotification(event.payload, "Download Complete", undefined, "completed");
      openDownloadDetailsWindow(event.payload);
    });
    return () => { unlisten.then((f) => f()); };
  }, [openDownloadDetailsWindow]);

  // download-error
  useEffect(() => {
    const unlisten = listen<{ id: number; url: string; message: string }>("download-error", (event) => {
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
          openDownloadDetailsWindow(Number(id));
        }
      });
    })();
    return () => { cancelled = true; if (unreg) unreg.unregister(); };
  }, [openDownloadDetailsWindow]);

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
