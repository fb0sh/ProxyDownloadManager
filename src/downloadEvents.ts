// The event half of the backend seam (tauriClient covers commands): event
// names, typed wire payloads, the subscription, and the one cache-write
// policy live here. Every webview subscribes through this module.
import { listen } from "@tauri-apps/api/event";
import type { QueryClient } from "@tanstack/react-query";
import { EVENTS } from "./constants/events";
import type { DownloadItem } from "./types";
import { applyPartDownloaded } from "./utils/progressMap";

/** Wire payloads (field names are Rust snake_case, mirrored by event_handler.rs tests). */
export interface ProgressPayload {
  id: number;
  downloaded: number;
  /** Per-part downloaded BYTES aligned with DownloadItem.parts — not DownloadPart[]. */
  parts?: number[];
  /** Concurrent → Single degrade: the Progress Map collapses to one cell. */
  reset_to_single?: boolean;
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
  onBrowserDownloadUrl?: (url: string) => void;
  onCreated?: () => void;
}

/** Patch a download-list cache with updated progress for a single download. */
export function patchDownloadProgress(
  cache: DownloadItem[] | undefined,
  id: number,
  downloaded: number,
  partDownloaded?: number[],
  resetToSingle?: boolean,
): DownloadItem[] | undefined {
  if (!cache) return cache;
  return cache.map((d) => {
    if (d.id !== id) return d;
    const parts =
      partDownloaded !== undefined
        ? applyPartDownloaded(d.parts, partDownloaded, d.total_size, resetToSingle)
        : d.parts;
    return { ...d, downloaded, parts };
  });
}

/**
 * The one subscription to backend download events, with the one cache-write
 * policy: progress events patch the ["downloads"] cache in place; lifecycle
 * events invalidate it. Returns an unsubscribe function.
 */
export function subscribeDownloadEvents(
  queryClient: QueryClient,
  handlers: DownloadEventHandlers = {},
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
        queryClient.setQueryData<DownloadItem[]>(["downloads"], (old) =>
          patchDownloadProgress(old, p.id, p.downloaded, p.parts, p.reset_to_single),
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
    listen<string>(
      EVENTS.BROWSER_DOWNLOAD_URL,
      guard((url) => handlers.onBrowserDownloadUrl?.(url)),
    ),
  );

  return () => {
    cancelled = true;
    unlisteners.forEach((u) => u.then((f) => f()));
  };
}
