import { useState, useCallback, useEffect } from "react";
import Layout from "./components/Layout";
import NewDownloadDialog from "./components/dialogs/NewDownloadDialog";
import DeleteDialog from "./components/dialogs/DeleteDialog";
import SettingsDialog from "./components/dialogs/SettingsDialog";
import PropertiesDialog from "./components/dialogs/PropertiesDialog";
import AboutDialog from "./components/dialogs/AboutDialog";
import LogDialog from "./components/dialogs/LogDialog";
import { useClipboardDetection } from "./hooks/useClipboard";
import { usePauseDownload, useResumeDownload, useDownloads, useSettings, useRedownloadDownload } from "./query/downloadQueries";
import { useSettingsStore } from "./stores/settingsStore";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { setLanguage, t } from "./i18n";
import type { DownloadItem } from "./types";

type Dialog =
  | { type: "new-download"; url?: string }
  | { type: "delete"; ids: number[] }
  | { type: "settings" }
  | { type: "properties"; id: number }
  | { type: "about" }
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

  const onUrlDetected = useCallback((url: string) => {
    setDialog({ type: "new-download", url });
  }, []);

  useClipboardDetection(onUrlDetected);

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
        onNewDownload={() => setDialog({ type: "new-download" })}
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

      {dialog?.type === "new-download" && (
        <NewDownloadDialog
          initialUrl={dialog.url ?? ""}
          onClose={() => setDialog(null)}
        />
      )}
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
      {dialog?.type === "log" && (
        <LogDialog onClose={() => setDialog(null)} />
      )}
    </>
  );
}

export default App;
