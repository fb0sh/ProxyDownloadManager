import { useEffect, useState } from "react";
import { flushSync } from "react-dom";
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

export type DetailPendingAction = "pause" | "resume" | "openFile" | "openFolder" | "copyUrl" | null;

export function useDownloadDetail(id: number | undefined) {
  const item = useDownload(id);
  const queryClient = useQueryClient();
  const pauseDownload = usePauseDownload();
  const resumeDownload = useResumeDownload();
  const [urlCopied, setUrlCopied] = useState(false);
  /** Which action is in-flight; null = interactive. */
  const [pendingAction, setPendingAction] = useState<DetailPendingAction>(null);

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

  /**
   * Click → flushSync disable (gray) immediately → await work →
   * clear pending only after success (or failure so user can retry).
   * @returns true if work succeeded
   */
  const runAction = async (
    action: NonNullable<DetailPendingAction>,
    work: () => Promise<void>,
  ): Promise<boolean> => {
    if (pendingAction !== null) return false;
    flushSync(() => {
      setPendingAction(action);
    });
    try {
      await work();
      setPendingAction(null);
      return true;
    } catch (e) {
      console.error(`[ProxyDM] details ${action} failed:`, e);
      setPendingAction(null);
      return false;
    }
  };

  const handleCopyUrl = () =>
    runAction("copyUrl", async () => {
      await navigator.clipboard.writeText(item?.url ?? "");
      setUrlCopied(true);
      setTimeout(() => setUrlCopied(false), 2000);
    });

  const handleOpenFile = () =>
    runAction("openFile", async () => {
      if (!item) throw new Error("no item");
      await openFile(item.save_path);
    });

  const handleOpenFolder = () =>
    runAction("openFolder", async () => {
      if (!item) throw new Error("no item");
      await openFolder(item.save_path);
    });

  const handlePause = () =>
    runAction("pause", async () => {
      if (id === undefined) throw new Error("no id");
      await pauseDownload.mutateAsync(id);
    });

  const handleResume = () =>
    runAction("resume", async () => {
      if (id === undefined) throw new Error("no id");
      await resumeDownload.mutateAsync(id);
    });

  return {
    item,
    urlCopied,
    pendingAction,
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
