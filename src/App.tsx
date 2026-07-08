import { useState, useCallback, useEffect } from "react";
import Layout from "./components/Layout";
import DeleteDialog from "./components/dialogs/DeleteDialog";
import SettingsDialog from "./components/dialogs/SettingsDialog";
import AboutDialog from "./components/dialogs/AboutDialog";
import LogDialog from "./components/dialogs/LogDialog";
import ExtensionDialog from "./components/dialogs/ExtensionDialog";
import { useClipboardDetection } from "./hooks/useClipboard";
import { usePauseDownload, useResumeDownload, useDownloads, useSettings, useRedownloadDownload } from "./query/downloadQueries";
import { useQueryClient } from "@tanstack/react-query";
import type { PluginListener } from "@tauri-apps/api/core";
import { useSettingsStore } from "./stores/settingsStore";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { WebviewWindow } from "@tauri-apps/api/webviewWindow";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { setLanguage, t } from "./i18n";
import type { DownloadItem } from "./types";

type Dialog =
  | { type: "delete"; ids: number[] }
  | { type: "settings" }
  | { type: "about" }
  | { type: "extension" }
  | { type: "log" }
  | null;

function App() {
  console.group('[ProxyDM FE] App');
  console.log('mount version=0.4.0');
  console.groupEnd();

  const [dialog, setDialog] = useState<Dialog>(null);
  const [selectedIds, setSelectedIds] = useState<Set<number>>(new Set());
  const [filter, setFilter] = useState<"all" | "completed" | "incomplete">("all");

  const pauseDownload = usePauseDownload();
  const resumeDownload = useResumeDownload();
  const redownloadDownload = useRedownloadDownload();
  const { data: downloads = [] } = useDownloads();
  const { settings: loadedSettings } = useSettings();
  const setSettings = useSettingsStore((s) => s.setSettings);

  // Sync loaded settings to zustand store for cross-component access
  useEffect(() => {
    if (loadedSettings) {
      console.log('[ProxyDM FE] settings loaded:', loadedSettings.language, loadedSettings.download_dir);
      setSettings(loadedSettings);
      setLanguage(loadedSettings.language || "en");
    }
  }, [loadedSettings, setSettings]);

  const openNewDownloadWindow = useCallback(async (url?: string) => {
    console.log('[ProxyDM FE] openNewDownloadWindow url=', url);
    // Don't re-open if already exists
    const existing = await WebviewWindow.getByLabel("new-download");
    if (existing) { console.log('[ProxyDM FE] new-download window already exists, focusing'); existing.setFocus(); return; }

    const base = window.location.origin + window.location.pathname.replace(/\/+$/, "");
    const params = new URLSearchParams();
    params.set("view", "new-download");
    if (url) params.set("url", url);

    const win = new WebviewWindow("new-download", {
      url: `${base}?${params.toString()}`,
      width: 600,
      height: 490,
      title: t("newDownload.title"),
    });
    win.once("tauri://created", async () => {
      // Send the URL to the new window via event (more reliable than query param)
      if (url) {
        try { await win.emit("new-download-url", url); } catch {}
      }
      await win.show().catch(() => {});
      await win.unminimize().catch(() => {});
      await win.center().catch(() => {});
      await win.setAlwaysOnTop(true).catch(() => {});
      await win.setFocus().catch(() => {});
      const { UserAttentionType } = await import("@tauri-apps/api/window");
      await win.requestUserAttention(UserAttentionType.Critical).catch(() => {});
    });
    win.once("tauri://error", (e) => {
      console.error("Failed to open new download window:", e);
    });
  }, []);

  const onUrlDetected = useCallback((url: string) => {
    openNewDownloadWindow(url);
  }, [openNewDownloadWindow]);

  useClipboardDetection(onUrlDetected);

  // Listen for browser extension download URLs → open New Download window
  useEffect(() => {
    console.log('[ProxyDM FE] registering browser-download-url listener');
    const unlisten = listen<string>("browser-download-url", (event) => {
      console.log('[ProxyDM FE] received browser-download-url:', event.payload);
      onUrlDetected(event.payload);
    });
    return () => { console.log('[ProxyDM FE] unregistering browser-download-url'); unlisten.then(f => f()); };
  }, [onUrlDetected]);

  // Listen for download-created event → refresh list + show main window
  const queryClient = useQueryClient();
  useEffect(() => {
    console.log('[ProxyDM FE] registering download-created listener');
    const unlisten = listen("download-created", async () => {
      console.log('[ProxyDM FE] download-created received, invalidating query');
      queryClient.invalidateQueries({ queryKey: ["downloads"] });
      try {
        const mainWin = await WebviewWindow.getByLabel("main");
        if (mainWin) { await mainWin.show(); await mainWin.setFocus(); }
      } catch {}
    });
    return () => { unlisten.then(f => f()); };
  }, [queryClient]);

  // Listen for real-time progress updates (avoid waiting for DB flush)
  useEffect(() => {
    const unlisten = listen<{id: number; downloaded: number}>("download-progress", (event) => {
      const { id, downloaded } = event.payload;
      queryClient.setQueryData<DownloadItem[]>(["downloads"], (old) => {
        if (!old) return old;
        return old.map((d) => d.id === id ? { ...d, downloaded } : d);
      });
    });
    return () => { unlisten.then(f => f()); };
  }, [queryClient]);

  const openDownloadDetailsWindow = useCallback(async (id: number) => {
    const existing = await WebviewWindow.getByLabel("download-details");
    if (existing) { existing.setFocus(); return; }

    const base = window.location.origin + window.location.pathname.replace(/\/+$/, "");
    const win = new WebviewWindow("download-details", {
      url: `${base}?view=download-details&id=${id}`,
      width: 480,
      height: 460,
      title: t("properties.title"),
    });
    win.once("tauri://created", async () => {
      await win.setAlwaysOnTop(true).catch(() => {});
      await win.setFocus().catch(() => {});
    });
    win.once("tauri://error", (e) => console.error("Failed to open details:", e));
  }, []);

  // Listen for notification clicks
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
        console.log("[ProxyDM FE] Notification clicked:", ntype, id, extra);
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
    return () => { cancelled = true; if (unreg) { unreg.unregister(); } };
  }, [openDownloadDetailsWindow]);

  async function sendDownloadNotification(id: number, title: string, body?: string, _ntype?: string) {
    // Try Tauri plugin notification first
    try {
      const { isPermissionGranted, requestPermission, sendNotification } =
        await import("@tauri-apps/plugin-notification");
      const ok = await isPermissionGranted();
      if (!ok) {
        const perm = await requestPermission();
        if (perm !== "granted") {
          console.warn("[ProxyDM FE] notification permission denied");
          return;
        }
      }
      // Look up filename for body — don't block notification if it fails
      let fileName: string | undefined = body;
      if (!fileName) {
        try {
          const items = await invoke("list_downloads") as Array<{ file_name: string; id: number }>;
          fileName = items.find((d) => d.id === id)?.file_name;
        } catch (e) {
          console.warn("[ProxyDM FE] list_downloads for notification failed:", e);
        }
      }
      sendNotification({
        title,
        body: fileName ?? `Download #${id}`,
      });
      console.log("[ProxyDM FE] notification sent:", title, fileName);
    } catch (e) {
      // Fallback: Web Notification API directly
      try {
        if (window.Notification.permission === "granted") {
          new window.Notification(title, { body: body ?? `Download #${id}` });
        }
      } catch (e2) {
        console.warn("[ProxyDM FE] notification failed:", e, e2);
      }
    }
  }

  // Listen for download started → system notification
  useEffect(() => {
    console.log('[ProxyDM FE] registering download-started listener');
    const unlisten = listen<number>("download-started", (event) => {
      console.log('[ProxyDM FE] download-started id=', event.payload);
      sendDownloadNotification(event.payload, "Download Started", undefined, "started");
    });
    return () => { unlisten.then(f => f()); };
  }, []);

  // Listen for download completed → system notification + details window
  useEffect(() => {
    console.log('[ProxyDM FE] registering download-completed listener');
    const unlisten = listen<number>("download-completed", async (event) => {
      console.log('[ProxyDM FE] download-completed id=', event.payload);
      await sendDownloadNotification(event.payload, "Download Complete", undefined, "completed");
      openDownloadDetailsWindow(event.payload);
    });
    return () => { unlisten.then(f => f()); };
  }, [openDownloadDetailsWindow]);

  // Listen for download errors → system notification (not blocking alert)
  useEffect(() => {
    console.log('[ProxyDM FE] registering download-error listener');
    const unlisten = listen<{id: number; url: string; message: string}>("download-error", (event) => {
      const { id, message } = event.payload;
      console.warn('[ProxyDM FE] download-error id=', id, 'msg=', message);
      sendDownloadNotification(id, t("downloadError.failed"), message.slice(0, 100), "error");
    });
    return () => { unlisten.then(f => f()); };
  }, []);

  // Refresh download list when the main window gains focus
  // (Tauri WebView doesn't fire browser focus events, so TanStack Query's
  //  refetchOnWindowFocus won't work — we listen for the native Tauri event)
  useEffect(() => {
    let unreg: (() => void) | null = null;
    let cancelled = false;
    (async () => {
      const unlisten = await getCurrentWindow().onFocusChanged(({ payload: focused }) => {
        if (cancelled) return;
        if (focused) {
          console.log('[ProxyDM FE] window focused, invalidating downloads query');
          queryClient.invalidateQueries({ queryKey: ["downloads"] });
        }
      });
      if (!cancelled) unreg = unlisten;
    })();
    return () => { cancelled = true; if (unreg) unreg(); };
  }, [queryClient]);

  // Update existing Properties dialog to open as separate window
  const handleProperties = useCallback((id: number) => {
    openDownloadDetailsWindow(id);
  }, [openDownloadDetailsWindow]);

  const handleQuit = async () => {
    try {
      await invoke("exit_app");
    } catch {
      window.close();
    }
  };

  const handlePauseSelected = () => {
    for (const id of selectedIds) {
      pauseDownload.mutate(id);
    }
  };

  const handleResumeSelected = async () => {
    for (const id of selectedIds) {
      try {
        await resumeDownload.mutateAsync(id);
      } catch (e) { console.error("Resume failed:", e); }
    }
  };

  const handleDeleteSelected = () => {
    setDialog({ type: "delete", ids: Array.from(selectedIds) });
  };

  const handleStop = (id: number) => {
    pauseDownload.mutate(id);
  };

  const handleDelete = (ids: number[]) => {
    setDialog({ type: "delete", ids });
  };

  const handleRedownload = async (item: DownloadItem) => {
    try {
      await redownloadDownload.mutateAsync(item.id);
    } catch (err) {
      console.error("Redownload failed:", err);
    }
  };

  const clearSelection = () => setSelectedIds(new Set());

  // Find item data for toolbar redownload
  const selectedForRedownload = selectedIds.size === 1
    ? downloads.find((d) => selectedIds.has(d.id) && (d.status === "completed" || d.status === "failed"))
    : undefined;

  return (
    <>
      <Layout
        onNewDownload={() => openNewDownloadWindow()}
        onExtension={() => setDialog({ type: "extension" })}
        onLog={() => setDialog({ type: "log" })}
        onSettings={() => setDialog({ type: "settings" })}
        onAbout={() => setDialog({ type: "about" })}
        onQuit={handleQuit}
        onResumeSelected={handleResumeSelected}
        onPauseSelected={handlePauseSelected}
        onDeleteSelected={handleDeleteSelected}
        onStop={handleStop}
        onDelete={handleDelete}
        onProperties={handleProperties}
        onRedownloadItem={selectedForRedownload}
        onRedownload={handleRedownload}
        selectedIds={selectedIds}
        onSelectChange={setSelectedIds}
        filter={filter}
        onFilterChange={setFilter}
      />

      {dialog?.type === "delete" && (
        <DeleteDialog
          ids={dialog.ids}
          onClose={() => { setDialog(null); clearSelection(); }}
        />
      )}
      {dialog?.type === "settings" && (
        <SettingsDialog onClose={() => setDialog(null)} />
      )}
      {dialog?.type === "about" && (
        <AboutDialog
          onClose={() => setDialog(null)}
          onDownloadUpdate={(url) => openNewDownloadWindow(url)}
        />
      )}
      {dialog?.type === "extension" && (
        <ExtensionDialog onClose={() => setDialog(null)} />
      )}
      {dialog?.type === "log" && (
        <LogDialog onClose={() => setDialog(null)} />
      )}
    </>
  );
}

export default App;
