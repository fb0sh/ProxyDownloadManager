import { useEffect, useRef, useState } from "react";
import { formatBytes } from "../utils/format";
import { t } from "../i18n";
import type { DownloadItem } from "../types";

export interface SpeedInfo {
  display: string;
  bps: number;
}

type SpeedMap = Map<number, SpeedInfo>;

const WINDOW_SIZE = 10; // rolling average over last 10 samples (~5 seconds)
const MIN_BYTES_DELTA = 512 * 1024; // ignore updates where total delta < 512KB

export function useDownloadSpeed(downloads: DownloadItem[]): SpeedMap {
  const samplesRef = useRef<Map<number, Array<{ downloaded: number; time: number }>>>(new Map());
  const [speeds, setSpeeds] = useState<SpeedMap>(() => new Map());
  // Keep previous bps for smoothing when current sample is stale
  const prevBpsRef = useRef<Map<number, number>>(new Map());

  useEffect(() => {
    const next = new Map<number, SpeedInfo>();

    for (const item of downloads) {
      if (item.status !== "downloading") {
        samplesRef.current.delete(item.id);
        prevBpsRef.current.delete(item.id);
        continue;
      }

      if (!samplesRef.current.has(item.id)) {
        samplesRef.current.set(item.id, []);
      }
      const samples = samplesRef.current.get(item.id)!;
      const now = Date.now();

      // Add current sample
      samples.push({ downloaded: item.downloaded, time: now });
      if (samples.length > WINDOW_SIZE) {
        samples.shift();
      }

      if (samples.length >= 2) {
        const first = samples[0];
        const last = samples[samples.length - 1];
        const dt = (last.time - first.time) / 1000;
        const dBytes = last.downloaded - first.downloaded;

        if (dt > 0.1 && dBytes >= MIN_BYTES_DELTA) {
          const rawBps = Math.round(dBytes / dt);
          // EMA smoothing: blend with previous value to reduce jitter
          const prev = prevBpsRef.current.get(item.id);
          const bps = prev !== undefined
            ? Math.round(prev * 0.6 + rawBps * 0.4)
            : rawBps;
          prevBpsRef.current.set(item.id, bps);
          next.set(item.id, {
            display: formatBytes(bps) + t("downloadRow.speed"),
            bps,
          });
        } else {
          // Not enough data change — carry forward previous speed if available
          const prev = prevBpsRef.current.get(item.id);
          if (prev !== undefined && prev > 0) {
            // Decay speed slightly so it doesn't freeze
            const decayed = Math.round(prev * 0.95);
            if (decayed > 1024) {
              next.set(item.id, {
                display: formatBytes(decayed) + t("downloadRow.speed"),
                bps: decayed,
              });
            }
          }
        }
      }
    }

    setSpeeds(next);
  }, [downloads]);

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
