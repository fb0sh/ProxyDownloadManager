import { useEffect, useState } from "react";
import { flushSync } from "react-dom";
import { useQueryClient } from "@tanstack/react-query";
import {
  useDownload,
  usePauseDownload,
  useResumeDownload,
} from "../query/downloadQueries";
import { openFile, openFolder } from "../utils/download";
import { subscribeDownloadEvents } from "../downloadEvents";

export type DetailPendingAction = "pause" | "resume" | "openFile" | "openFolder" | "copyUrl" | null;

export function useDownloadDetail(id: number | undefined) {
  const item = useDownload(id);
  const queryClient = useQueryClient();
  const pauseDownload = usePauseDownload();
  const resumeDownload = useResumeDownload();
  const [urlCopied, setUrlCopied] = useState(false);
  /** Which action is in-flight; null = interactive. */
  const [pendingAction, setPendingAction] = useState<DetailPendingAction>(null);

  // Details window is a separate webview (own JS realm, own QueryClient) —
  // it consumes the same seam with the default cache policy, no handlers.
  useEffect(() => {
    if (id === undefined) return;
    return subscribeDownloadEvents(queryClient);
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
      try {
        // Plugin first — WebView2 permission-gates navigator.clipboard.
        const { writeText } = await import("@tauri-apps/plugin-clipboard-manager");
        await writeText(item?.url ?? "");
      } catch {
        await navigator.clipboard.writeText(item?.url ?? "");
      }
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
