import { useEffect, useMemo, useState } from "react";
import Layout from "./components/Layout";
import DialogRenderer from "./components/DialogRenderer";
import { useDialog, type DialogActions as DialogActionTypes } from "./hooks/useDialog";
import { useSelection, type SelectionActions } from "./hooks/useSelection";
import { useClipboardDetection } from "./hooks/useClipboard";
import { usePauseDownload, useResumeDownload, useDownloads, useSettings, useRedownloadDownload } from "./query/downloadQueries";
import { useDownloadEvents } from "./hooks/useDownloadEvents";
import { useWindowManager } from "./hooks/useWindowManager";
import { isFailed } from "./utils/download";
import { useQueryClient } from "@tanstack/react-query";
import { setLanguage } from "./i18n";
import { tauriClient } from "./tauriClient";
import type { DownloadItem } from "./types";
import { AppProvider, useAppContext, type AppActions } from "./contexts/AppContext";

function AppInner() {
  const { dialog, dialogActions, selectedIds, selectionActions } = useAppContext();
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

  const selectedForRedownload = selectedIds.size === 1
    ? downloads.find((d) => selectedIds.has(d.id) && (d.status === "completed" || isFailed(d.status)))
    : undefined;

  const handleDialogClose = () => {
    dialogActions.closeDialog();
    selectionActions.clearSelection();
  };

  return (
    <>
      <Layout onRedownloadItem={selectedForRedownload} />
      <DialogRenderer dialog={dialog} onClose={handleDialogClose} onDownloadUpdate={(url) => openNewDownload(url)} />
    </>
  );
}

/** Builds the action callbacks using focused hooks instead of raw setters. */
function useActions(dialogActs: DialogActionTypes, selectActs: SelectionActions, selectedIds: Set<number>): AppActions {
  const pauseDownload = usePauseDownload();
  const resumeDownload = useResumeDownload();
  const redownloadDownload = useRedownloadDownload();
  const { data: downloads = [] } = useDownloads();
  const { openNewDownload, openDetails } = useWindowManager();

  return useMemo(() => ({
    onNewDownload: () => openNewDownload(),
    onExtension: () => dialogActs.openExtension(),
    onLog: () => dialogActs.openLog(),
    onSettings: () => dialogActs.openSettings(),
    onAbout: () => dialogActs.openAbout(),
    onQuit: async () => {
      try { await tauriClient.exitApp(); } catch { window.close(); }
    },
    onResumeSelected: async () => {
      const items = downloads.filter((d) => selectedIds.has(d.id) && d.status === "paused");
      await Promise.all(items.map((d) => resumeDownload.mutateAsync(d.id)));
      selectActs.clearSelection();
    },
    onPauseSelected: () => {
      const items = downloads.filter((d) => selectedIds.has(d.id) && d.status === "downloading");
      for (const d of items) pauseDownload.mutate(d.id);
      selectActs.clearSelection();
    },
    onDeleteSelected: () => {
      if (selectedIds.size === 0) return;
      dialogActs.openDelete(Array.from(selectedIds));
    },
    onStop: (id: number) => pauseDownload.mutate(id),
    onDelete: (ids: number[]) => dialogActs.openDelete(ids),
    onProperties: (id: number) => openDetails(id),
    onRedownload: async (item: DownloadItem) => {
      try { await redownloadDownload.mutateAsync(item.id); } catch (e) { console.error("[ProxyDM] redownload failed:", e); }
    },
  }), [openNewDownload, openDetails, pauseDownload, resumeDownload,
       redownloadDownload, downloads, selectedIds, dialogActs, selectActs]);
}

export default function App() {
  const dialog = useDialog();
  const selection = useSelection();
  const [filter, setFilter] = useState<"all" | "completed" | "incomplete">("all");

  const actions = useActions(dialog, selection, selection.selectedIds);

  return (
    <AppProvider
      dialog={dialog.dialog}
      dialogActions={dialog}
      selectedIds={selection.selectedIds}
      selectionActions={selection}
      filter={filter}
      setFilter={setFilter}
      actions={actions}
    >
      <AppInner />
    </AppProvider>
  );
}
