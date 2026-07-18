import { useEffect } from "react";
import Layout from "./components/Layout";
import DeleteDialog from "./components/dialogs/DeleteDialog";
import SettingsDialog from "./components/dialogs/SettingsDialog";
import AboutDialog from "./components/dialogs/AboutDialog";
import LogDialog from "./components/dialogs/LogDialog";
import ExtensionDialog from "./components/dialogs/ExtensionDialog";
import { useClipboardDetection } from "./hooks/useClipboard";
import { usePauseDownload, useResumeDownload, useDownloads, useSettings, useRedownloadDownload } from "./query/downloadQueries";
import { useDownloadEvents } from "./hooks/useDownloadEvents";
import { useWindowManager } from "./hooks/useWindowManager";
import { isFailed } from "./utils/download";
import { useQueryClient } from "@tanstack/react-query";
import { invoke } from "@tauri-apps/api/core";
import { setLanguage } from "./i18n";
import type { DownloadItem } from "./types";
import { useAppContext } from "./contexts/AppContext";

function App() {
  const { dialog, setDialog, selectedIds, setSelectedIds } = useAppContext();

  const pauseDownload = usePauseDownload();
  const resumeDownload = useResumeDownload();
  const redownloadDownload = useRedownloadDownload();
  const { data: downloads = [] } = useDownloads();
  const { settings: loadedSettings } = useSettings();
  const queryClient = useQueryClient();
  const { openNewDownload, openDetails } = useWindowManager();

  useEffect(() => {
    if (loadedSettings) {
      setLanguage(loadedSettings.language || "en");
    }
  }, [loadedSettings]);

  useClipboardDetection();

  useDownloadEvents({ queryClient });

  const handleQuit = async () => {
    try { await invoke("exit_app"); } catch { window.close(); }
  };

  const handlePauseSelected = () => {
    for (const id of selectedIds) pauseDownload.mutate(id);
  };

  const handleResumeSelected = async () => {
    for (const id of selectedIds) {
      try { await resumeDownload.mutateAsync(id); } catch {}
    }
  };

  const handleDeleteSelected = () => {
    setDialog({ type: "delete", ids: Array.from(selectedIds) });
  };

  const handleStop = (id: number) => pauseDownload.mutate(id);

  const handleDelete = (ids: number[]) => setDialog({ type: "delete", ids });

  const handleRedownload = async (item: DownloadItem) => {
    try { await redownloadDownload.mutateAsync(item.id); } catch {}
  };

  const clearSelection = () => setSelectedIds(new Set());

  const selectedForRedownload = selectedIds.size === 1
    ? downloads.find((d) => selectedIds.has(d.id) && (d.status === "completed" || isFailed(d.status)))
    : undefined;

  return (
    <>
      <Layout
        onNewDownload={() => openNewDownload()}
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
        onProperties={(id) => openDetails(id)}
        onRedownloadItem={selectedForRedownload}
        onRedownload={handleRedownload}
      />

      {dialog?.type === "delete" && (
        <DeleteDialog ids={dialog.ids} onClose={() => { setDialog(null); clearSelection(); }} />
      )}
      {dialog?.type === "settings" && (
        <SettingsDialog onClose={() => setDialog(null)} />
      )}
      {dialog?.type === "about" && (
        <AboutDialog onClose={() => setDialog(null)} onDownloadUpdate={(url) => openNewDownload(url)} />
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
