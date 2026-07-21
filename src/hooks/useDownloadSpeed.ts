import { useEffect, useMemo, useRef, useState } from "react";
import { formatBytes } from "../utils/format";
import { t } from "../i18n";
import type { DownloadItem } from "../types";

export interface SpeedInfo {
  display: string;
  bps: number;
}

type SpeedMap = Map<number, SpeedInfo>;

const WINDOW_SIZE = 12;
/** Any positive progress over ~0.3s is enough to estimate speed. */
const MIN_BYTES_DELTA = 1;
const MIN_DT_SEC = 0.3;
const SAMPLE_MIN_INTERVAL_MS = 400;

function computeBps(
  samples: Array<{ downloaded: number; time: number }>,
  prevBps: number | undefined,
): number | null {
  if (samples.length < 2) return null;
  const first = samples[0]!;
  const last = samples[samples.length - 1]!;
  const dt = (last.time - first.time) / 1000;
  const dBytes = last.downloaded - first.downloaded;
  if (dt < MIN_DT_SEC || dBytes < MIN_BYTES_DELTA) {
    return prevBps !== undefined && prevBps > 0 ? Math.round(prevBps * 0.92) : null;
  }
  const rawBps = Math.round(dBytes / dt);
  if (prevBps !== undefined && prevBps > 0) {
    return Math.round(prevBps * 0.5 + rawBps * 0.5);
  }
  return rawBps;
}

/**
 * Rolling download speed for active items.
 * Samples only when `downloaded` advances (or on a min interval) so parent
 * re-renders with a new array identity don't poison the window with flat zeros.
 */
export function useDownloadSpeed(downloads: DownloadItem[]): SpeedMap {
  const samplesRef = useRef<Map<number, Array<{ downloaded: number; time: number }>>>(new Map());
  const prevBpsRef = useRef<Map<number, number>>(new Map());
  const [speeds, setSpeeds] = useState<SpeedMap>(() => new Map());

  // Stable signature: only recompute when progress/status of tracked rows change.
  const signature = useMemo(
    () =>
      downloads
        .map((d) => `${d.id}:${d.status}:${d.downloaded}`)
        .join("|"),
    [downloads],
  );

  useEffect(() => {
    const next = new Map<number, SpeedInfo>();
    const now = Date.now();
    const liveIds = new Set<number>();

    for (const item of downloads) {
      if (item.status !== "downloading") {
        samplesRef.current.delete(item.id);
        prevBpsRef.current.delete(item.id);
        continue;
      }
      liveIds.add(item.id);

      if (!samplesRef.current.has(item.id)) {
        samplesRef.current.set(item.id, []);
      }
      const samples = samplesRef.current.get(item.id)!;
      const last = samples[samples.length - 1];
      const advanced = !last || item.downloaded !== last.downloaded;
      const intervalOk = !last || now - last.time >= SAMPLE_MIN_INTERVAL_MS;
      if (advanced || intervalOk) {
        samples.push({ downloaded: item.downloaded, time: now });
        if (samples.length > WINDOW_SIZE) samples.shift();
      }

      const prev = prevBpsRef.current.get(item.id);
      const bps = computeBps(samples, prev);
      if (bps !== null && bps > 0) {
        prevBpsRef.current.set(item.id, bps);
        next.set(item.id, {
          display: formatBytes(bps) + t("downloadRow.speed"),
          bps,
        });
      }
    }

    // Drop samples for ids no longer in the list
    for (const id of [...samplesRef.current.keys()]) {
      if (!liveIds.has(id)) {
        samplesRef.current.delete(id);
        prevBpsRef.current.delete(id);
      }
    }

    setSpeeds(next);
    // eslint-disable-next-line react-hooks/exhaustive-deps -- signature captures downloads content
  }, [signature]);

  // Tick every second so time-based ETA/speed can update even if bytes stall.
  useEffect(() => {
    const timer = window.setInterval(() => {
      const next = new Map<number, SpeedInfo>();
      for (const [id, samples] of samplesRef.current) {
        const prev = prevBpsRef.current.get(id);
        const bps = computeBps(samples, prev);
        if (bps !== null && bps > 1024) {
          prevBpsRef.current.set(id, bps);
          next.set(id, {
            display: formatBytes(bps) + t("downloadRow.speed"),
            bps,
          });
        }
      }
      setSpeeds(next);
    }, 1000);
    return () => window.clearInterval(timer);
  }, []);

  return speeds;
}

export function formatETA(seconds: number): string {
  if (seconds <= 0 || !isFinite(seconds)) return "—";
  if (seconds < 60) return `${Math.round(seconds)}s`;
  if (seconds < 3600) return `${Math.floor(seconds / 60)}m ${Math.round(seconds % 60)}s`;
  if (seconds < 86400) return `${Math.floor(seconds / 3600)}h ${Math.floor((seconds % 3600) / 60)}m`;
  return `${Math.floor(seconds / 86400)}d ${Math.floor((seconds % 86400) / 3600)}h`;
}

export function computeETA(item: DownloadItem, bps: number): string {
  if (bps <= 0 || item.total_size === 0) return "—";
  const remaining = item.total_size - item.downloaded;
  if (remaining <= 0) return "—";
  return formatETA(remaining / bps);
}
