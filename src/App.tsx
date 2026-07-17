import { useState, useCallback, useEffect } from "react";
import Layout from "./components/Layout";
import DeleteDialog from "./components/dialogs/DeleteDialog";
import SettingsDialog from "./components/dialogs/SettingsDialog";
import AboutDialog from "./components/dialogs/AboutDialog";
import LogDialog from "./components/dialogs/LogDialog";
import ExtensionDialog from "./components/dialogs/ExtensionDialog";
import { useClipboardDetection } from "./hooks/useClipboard";
import { usePauseDownload, useResumeDownload, useDownloads, useSettings, useRedownloadDownload } from "./query/downloadQueries";
import { useDownloadEvents } from "./hooks/useDownloadEvents";
import { useQueryClient } from "@tanstack/react-query";
import { invoke } from "@tauri-apps/api/core";
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
  const queryClient = useQueryClient();

  useEffect(() => {
    if (loadedSettings) {
      setLanguage(loadedSettings.language || "en");
    }
  }, [loadedSettings]);

  const openNewDownloadWindow = useCallback(async (url?: string) => {
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

  useClipboardDetection(useCallback((url: string) => openNewDownloadWindow(url), [openNewDownloadWindow]));

  useDownloadEvents({ queryClient, openNewDownloadWindow, openDownloadDetailsWindow });

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
        onProperties={(id) => openDownloadDetailsWindow(id)}
        onRedownloadItem={selectedForRedownload}
        onRedownload={handleRedownload}
        selectedIds={selectedIds}
        onSelectChange={setSelectedIds}
        filter={filter}
        onFilterChange={setFilter}
      />

      {dialog?.type === "delete" && (
        <DeleteDialog ids={dialog.ids} onClose={() => { setDialog(null); clearSelection(); }} />
      )}
      {dialog?.type === "settings" && (
        <SettingsDialog onClose={() => setDialog(null)} />
      )}
      {dialog?.type === "about" && (
        <AboutDialog onClose={() => setDialog(null)} onDownloadUpdate={(url) => openNewDownloadWindow(url)} />
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
