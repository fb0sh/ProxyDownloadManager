// The event half of the backend seam (tauriClient covers commands): event
// names, typed wire payloads, the subscription, and the one cache-write
// policy live here. Every webview subscribes through this module.
import { listen } from "@tauri-apps/api/event";
import type { QueryClient } from "@tanstack/react-query";
import { EVENTS } from "./constants/events";
import type { DownloadItem, PendingDownloadRequest } from "./types";
import { applyPartDownloaded } from "./utils/progressMap";

/** Wire payloads (field names are Rust snake_case), built in download_manager.rs `handle_event` and pinned by its lifecycle tests. */
export interface ProgressPayload {
  id: number;
  /** Omitted on phase-only events (retrying / merging). Must not be treated as zero. */
  downloaded?: number;
  /** Per-part downloaded BYTES aligned with DownloadItem.parts — not DownloadPart[]. */
  parts?: number[];
  /** Concurrent → Single degrade: the Progress Map collapses to one cell. */
  reset_to_single?: boolean;
  total_size?: number;
  /** Engine phase: downloading | retrying | merging. Not every status: `connecting` is set by the command layer, not reported by an engine. */
  status?: string;
}

export interface CompletedPayload {
  id: number;
  file_name: string;
}

export interface ErrorPayload {
  id: number;
  url: string;
  message: string;
}

/** Domain reactions (notifications, window opening) — the caller's business. */
export interface DownloadEventHandlers {
  onStarted?: (id: number) => void;
  onCompleted?: (payload: CompletedPayload) => void;
  onError?: (payload: ErrorPayload) => void;
  onBrowserDownloadUrl?: (req: string | PendingDownloadRequest) => void;
  onCreated?: () => void;
}

const LIVE_PHASES = new Set(["connecting", "retrying", "merging", "downloading"]);

function isTerminalStatus(status: DownloadItem["status"]): boolean {
  const s = typeof status === "object" && "failed" in status ? "failed" : status;
  return s === "paused" || s === "completed" || s === "failed";
}

/** Patch a download-list cache with updated progress for a single download. */
export function patchDownloadProgress(
  cache: DownloadItem[] | undefined,
  id: number,
  downloaded?: number,
  partDownloaded?: number[],
  resetToSingle?: boolean,
  extra?: { totalSize?: number; status?: string },
): DownloadItem[] | undefined {
  if (!cache) return cache;
  return cache.map((d) => {
    if (d.id !== id) return d;
    const totalSize = extra?.totalSize && d.total_size === 0 ? extra.totalSize : d.total_size;
    const parts =
      partDownloaded !== undefined
        ? applyPartDownloaded(d.parts, partDownloaded, totalSize, resetToSingle)
        : d.parts;
    const nextDownloaded =
      typeof downloaded === "number" && Number.isFinite(downloaded) ? downloaded : d.downloaded;
    const phase = extra?.status;
    const status =
      phase && LIVE_PHASES.has(phase) && !isTerminalStatus(d.status)
        ? (phase as DownloadItem["status"])
        : d.status;
    return { ...d, downloaded: nextDownloaded, parts, total_size: totalSize, status };
  });
}

export interface SubscribeOptions {
  /** Patch progress only for this id — the details window watches one
   * download; other downloads' 500ms progress events would only cause
   * wasted re-renders there (the 1s poll covers them). */
  progressId?: number;
}

/**
 * The one subscription to backend download events, with the one cache-write
 * policy: progress events patch the ["downloads"] cache in place; lifecycle
 * events invalidate it. Returns an unsubscribe function.
 */
export function subscribeDownloadEvents(
  queryClient: QueryClient,
  handlers: DownloadEventHandlers = {},
  options: SubscribeOptions = {},
): () => void {
  let cancelled = false;
  const unlisteners: Promise<() => void>[] = [];
  const guard =
    <T>(fn: (payload: T) => void) =>
    (event: { payload: T }) => {
      if (!cancelled) fn(event.payload);
    };
  const invalidate = () =>
    queryClient.invalidateQueries({ queryKey: ["downloads"] });

  unlisteners.push(
    listen<ProgressPayload>(
      EVENTS.DOWNLOAD_PROGRESS,
      guard((p) => {
        if (options.progressId !== undefined && p.id !== options.progressId) return;
        queryClient.setQueryData<DownloadItem[]>(["downloads"], (old) =>
          patchDownloadProgress(old, p.id, p.downloaded, p.parts, p.reset_to_single, {
            totalSize: p.total_size,
            status: p.status,
          }),
        );
      }),
    ),
  );

  for (const name of [EVENTS.DOWNLOAD_PAUSED, EVENTS.DOWNLOAD_RESUMED, EVENTS.DOWNLOAD_CANCELLED]) {
    unlisteners.push(listen(name, guard(invalidate)));
  }

  unlisteners.push(
    listen(
      EVENTS.DOWNLOAD_CREATED,
      guard(() => {
        invalidate();
        handlers.onCreated?.();
      }),
    ),
  );

  unlisteners.push(
    listen<number>(
      EVENTS.DOWNLOAD_STARTED,
      guard((id) => handlers.onStarted?.(id)),
    ),
  );

  unlisteners.push(
    listen<CompletedPayload>(
      EVENTS.DOWNLOAD_COMPLETED,
      guard((p) => {
        invalidate();
        handlers.onCompleted?.(p);
      }),
    ),
  );

  unlisteners.push(
    listen<ErrorPayload>(
      EVENTS.DOWNLOAD_ERROR,
      guard((p) => {
        invalidate();
        handlers.onError?.(p);
      }),
    ),
  );

  unlisteners.push(
    listen<string | PendingDownloadRequest>(
      EVENTS.BROWSER_DOWNLOAD_URL,
      guard((payload) => handlers.onBrowserDownloadUrl?.(payload)),
    ),
  );

  return () => {
    cancelled = true;
    unlisteners.forEach((u) => u.then((f) => f()));
  };
}
