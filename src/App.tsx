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
import { useSettingsStore } from "./stores/settingsStore";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { WebviewWindow } from "@tauri-apps/api/webviewWindow";
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
      setSettings(loadedSettings);
      setLanguage(loadedSettings.language || "en");
    }
  }, [loadedSettings, setSettings]);

  // Listen for download errors from backend
  useEffect(() => {
    const unlisten = listen<{id: number; url: string; message: string}>("download-error", (event) => {
      const { url, message } = event.payload;
      alert(`${t("downloadError.failed")}: ${url}\n\n${message}`);
    });
    return () => { unlisten.then(f => f()); };
  }, []);

  const openNewDownloadWindow = useCallback(async (url?: string) => {
    // Don't re-open if already exists
    const existing = await WebviewWindow.getByLabel("new-download");
    if (existing) { existing.setFocus(); return; }

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
    const unlisten = listen<string>("browser-download-url", (event) => {
      onUrlDetected(event.payload);
    });
    return () => { unlisten.then(f => f()); };
  }, [onUrlDetected]);

  // Listen for download-created event from New Download window → refresh list
  const queryClient = useQueryClient();
  useEffect(() => {
    const unlisten = listen("download-created", () => {
      queryClient.invalidateQueries({ queryKey: ["downloads"] });
    });
    return () => { unlisten.then(f => f()); };
  }, [queryClient]);

  const openDownloadDetailsWindow = useCallback(async (id: number) => {
    const existing = await WebviewWindow.getByLabel("download-details");
    if (existing) { existing.setFocus(); return; }

    const base = window.location.origin + window.location.pathname.replace(/\/+$/, "");
    const win = new WebviewWindow("download-details", {
      url: `${base}?view=download-details&id=${id}`,
      width: 520,
      height: 560,
      title: t("properties.title"),
    });
    win.once("tauri://created", async () => {
      await win.setAlwaysOnTop(true).catch(() => {});
      await win.setFocus().catch(() => {});
    });
    win.once("tauri://error", (e) => console.error("Failed to open details:", e));
  }, []);

  async function sendDownloadNotification(id: number, title: string) {
    try {
      const { sendNotification, isPermissionGranted, requestPermission } =
        await import("@tauri-apps/plugin-notification");
      let ok = await isPermissionGranted();
      if (!ok) {
        const perm = await requestPermission();
        ok = perm === "granted";
      }
      if (ok) {
        const items = await invoke("list_downloads");
        const itemsArr = items as Array<{ file_name: string; id: number }>;
        const item = itemsArr.find((d) => d.id === id);
        sendNotification({ title, body: item?.file_name ?? `Download #${id}` });
      }
    } catch {}
  }

  // Listen for download started → system notification
  useEffect(() => {
    const unlisten = listen<number>("download-started", (event) => {
      sendDownloadNotification(event.payload, "Download Started");
    });
    return () => { unlisten.then(f => f()); };
  }, []);

  // Listen for download completed → system notification + details window
  useEffect(() => {
    const unlisten = listen<number>("download-completed", async (event) => {
      await sendDownloadNotification(event.payload, "Download Complete");
      openDownloadDetailsWindow(event.payload);
    });
    return () => { unlisten.then(f => f()); };
  }, [openDownloadDetailsWindow]);

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

  const openDownloadProgressWindow = useCallback(async (id: number) => {
    const base = window.location.origin + window.location.pathname.replace(/\/+$/, "");
    const win = new WebviewWindow(`progress-${id}`, {
      url: `${base}?view=new-download&downloadId=${id}`,
      width: 520,
      height: 480,
      title: "Downloading...",
    });
    win.once("tauri://error", (e) => console.error("Failed to open progress window:", e));
  }, []);

  const handleResumeSelected = async () => {
    for (const id of selectedIds) {
      try {
        await resumeDownload.mutateAsync(id);
        openDownloadProgressWindow(id);
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
        hasSelection={selectedIds.size > 0}
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
        <AboutDialog onClose={() => setDialog(null)} />
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
