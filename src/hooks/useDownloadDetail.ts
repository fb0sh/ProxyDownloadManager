import { useEffect, useState } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { listen } from "@tauri-apps/api/event";
import {
  useDownload,
  usePauseDownload,
  useResumeDownload,
} from "../query/downloadQueries";
import { openFile, openFolder } from "../utils/download";
import { patchDownloadProgress } from "./useDownloadEvents";
import { EVENTS } from "../constants/events";
import type { DownloadItem } from "../types";

export function useDownloadDetail(id: number | undefined) {
  const item = useDownload(id);
  const queryClient = useQueryClient();
  const pauseDownload = usePauseDownload();
  const resumeDownload = useResumeDownload();
  const [urlCopied, setUrlCopied] = useState(false);
  const [actionPending, setActionPending] = useState(false);

  // Details is a separate webview — subscribe to progress for live Progress Map.
  useEffect(() => {
    if (id === undefined) return;
    let cancelled = false;
    const unlisteners: Promise<() => void>[] = [];

    unlisteners.push(
      listen<{
        id: number;
        downloaded: number;
        parts?: number[];
        reset_to_single?: boolean;
      }>(EVENTS.DOWNLOAD_PROGRESS, (event) => {
        if (cancelled) return;
        const { id: eventId, downloaded, parts, reset_to_single } = event.payload;
        if (eventId !== id) return;
        queryClient.setQueryData<DownloadItem[]>(["downloads"], (old) =>
          patchDownloadProgress(old, eventId, downloaded, parts, reset_to_single)
        );
      })
    );

    // Pause/resume from this window or main window: refresh item status.
    for (const name of [
      EVENTS.DOWNLOAD_PAUSED,
      EVENTS.DOWNLOAD_RESUMED,
      EVENTS.DOWNLOAD_COMPLETED,
      EVENTS.DOWNLOAD_ERROR,
    ]) {
      unlisteners.push(
        listen(name, () => {
          if (cancelled) return;
          queryClient.invalidateQueries({ queryKey: ["downloads"] });
        })
      );
    }

    return () => {
      cancelled = true;
      unlisteners.forEach((u) => u.then((f) => f()));
    };
  }, [id, queryClient]);

  const handleCopyUrl = async () => {
    try {
      await navigator.clipboard.writeText(item?.url ?? "");
      setUrlCopied(true);
      setTimeout(() => setUrlCopied(false), 2000);
    } catch { /* clipboard not available */ }
  };

  const handleOpenFile = async () => {
    if (!item) return;
    await openFile(item.save_path);
  };

  const handleOpenFolder = async () => {
    if (!item) return;
    await openFolder(item.save_path);
  };

  const handlePause = async () => {
    if (id === undefined || actionPending) return;
    setActionPending(true);
    try {
      await pauseDownload.mutateAsync(id);
    } catch (e) {
      console.error("[ProxyDM] pause from details failed:", e);
    } finally {
      setActionPending(false);
    }
  };

  const handleResume = async () => {
    if (id === undefined || actionPending) return;
    setActionPending(true);
    try {
      await resumeDownload.mutateAsync(id);
    } catch (e) {
      console.error("[ProxyDM] resume from details failed:", e);
    } finally {
      setActionPending(false);
    }
  };

  return {
    item,
    urlCopied,
    actionPending,
    handleCopyUrl,
    handleOpenFile,
    handleOpenFolder,
    handlePause,
    handleResume,
  };
}

export function useDownloadIdFromUrl(): number | undefined {
  const p = new URLSearchParams(window.location.search);
  const idParam = p.get("id");
  return idParam ? Number(idParam) : undefined;
}
