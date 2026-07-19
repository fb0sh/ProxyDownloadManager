import { useEffect, useMemo, useState } from "react";
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
import { AppProvider, useAppContext, type AppActions, type Dialog } from "./contexts/AppContext";

function AppInner() {
  const { dialog, setDialog, selectedIds, setSelectedIds } = useAppContext();
  const { openNewDownload } = useWindowManager();
  const { settings: loadedSettings } = useSettings();
  const queryClient = useQueryClient();
  const { data: downloads = [] } = useDownloads();

  useEffect(() => {
    if (loadedSettings) {
      setLanguage(loadedSettings.language || "en");
    }
  }, [loadedSettings]);

  useClipboardDetection();
  useDownloadEvents({ queryClient });

  const clearSelection = () => setSelectedIds(new Set());

  const selectedForRedownload = selectedIds.size === 1
    ? downloads.find((d) => selectedIds.has(d.id) && (d.status === "completed" || isFailed(d.status)))
    : undefined;

  return (
    <>
      <Layout onRedownloadItem={selectedForRedownload} />

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

// Takes state setters as params — no useAppContext() dependency,
// so this can be called at the App level before AppProvider renders.
function useActions({ setDialog, setSelectedIds, selectedIds }: {
  setDialog: (d: Dialog) => void;
  setSelectedIds: (ids: Set<number>) => void;
  selectedIds: Set<number>;
}): AppActions {
  const pauseDownload = usePauseDownload();
  const resumeDownload = useResumeDownload();
  const redownloadDownload = useRedownloadDownload();
  const { data: downloads = [] } = useDownloads();
  const { openNewDownload, openDetails } = useWindowManager();

  return useMemo(() => ({
    onNewDownload: () => openNewDownload(),
    onExtension: () => setDialog({ type: "extension" }),
    onLog: () => setDialog({ type: "log" }),
    onSettings: () => setDialog({ type: "settings" }),
    onAbout: () => setDialog({ type: "about" }),
    onQuit: async () => {
      try { await invoke("exit_app"); } catch { window.close(); }
    },
    onResumeSelected: async () => {
      const items = downloads.filter((d) => selectedIds.has(d.id) && d.status === "paused");
      await Promise.all(items.map((d) => resumeDownload.mutateAsync(d.id)));
      setSelectedIds(new Set());
    },
    onPauseSelected: () => {
      const items = downloads.filter((d) => selectedIds.has(d.id) && d.status === "downloading");
      for (const d of items) pauseDownload.mutate(d.id);
      setSelectedIds(new Set());
    },
    onDeleteSelected: () => {
      if (selectedIds.size === 0) return;
      setDialog({ type: "delete", ids: Array.from(selectedIds) });
    },
    onStop: (id: number) => pauseDownload.mutate(id),
    onDelete: (ids: number[]) => setDialog({ type: "delete", ids }),
    onProperties: (id: number) => openDetails(id),
    onRedownload: async (item: DownloadItem) => {
      try { await redownloadDownload.mutateAsync(item.id); } catch {}
    },
  }), [openNewDownload, openDetails, pauseDownload, resumeDownload,
       redownloadDownload, downloads, selectedIds, setSelectedIds, setDialog]);
}

export default function App() {
  const [dialog, setDialog] = useState<Dialog>(null);
  const [selectedIds, setSelectedIds] = useState<Set<number>>(new Set());
  const [filter, setFilter] = useState<"all" | "completed" | "incomplete">("all");

  const actions = useActions({ setDialog, setSelectedIds, selectedIds });

  return (
    <AppProvider
      dialog={dialog} setDialog={setDialog}
      selectedIds={selectedIds} setSelectedIds={setSelectedIds}
      filter={filter} setFilter={setFilter}
      actions={actions}
    >
      <AppInner />
    </AppProvider>
  );
}
