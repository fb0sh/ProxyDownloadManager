import { useState, useCallback, useEffect } from "react";
import Layout from "./components/Layout";
import DeleteDialog from "./components/dialogs/DeleteDialog";
import SettingsDialog from "./components/dialogs/SettingsDialog";
import PropertiesDialog from "./components/dialogs/PropertiesDialog";
import AboutDialog from "./components/dialogs/AboutDialog";
import LogDialog from "./components/dialogs/LogDialog";
import ExtensionDialog from "./components/dialogs/ExtensionDialog";
import { useClipboardDetection } from "./hooks/useClipboard";
import { usePauseDownload, useResumeDownload, useDownloads, useSettings, useRedownloadDownload } from "./query/downloadQueries";
import { useSettingsStore } from "./stores/settingsStore";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { WebviewWindow } from "@tauri-apps/api/webviewWindow";
import { setLanguage, t } from "./i18n";
import type { DownloadItem } from "./types";

type Dialog =
  | { type: "delete"; ids: number[] }
  | { type: "settings" }
  | { type: "properties"; id: number }
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
    win.once("tauri://created", () => {
      win.setAlwaysOnTop(true);
      win.setFocus();
      // Turn off always-on-top after a brief moment so the user can interact
      // with other windows without the dialog blocking them.
      setTimeout(() => { win.setAlwaysOnTop(false).catch(() => {}); }, 3000);
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

  const handleResumeSelected = () => {
    for (const id of selectedIds) {
      resumeDownload.mutate(id);
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

  const handleProperties = (id: number) => {
    setDialog({ type: "properties", id });
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
      {dialog?.type === "properties" && (
        <PropertiesDialog
          id={dialog.id}
          onClose={() => setDialog(null)}
        />
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
