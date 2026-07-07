import { useEffect, useRef, useState } from "react";
import { formatBytes } from "../types";
import { t } from "../i18n";
import type { DownloadItem } from "../types";

export interface SpeedInfo {
  display: string;
  bps: number;
}

type SpeedMap = Map<number, SpeedInfo>;

const WINDOW_SIZE = 5; // rolling average over last 5 samples

export function useDownloadSpeed(downloads: DownloadItem[]): SpeedMap {
  // Store last N (downloaded, timestamp) pairs per download
  const samplesRef = useRef<Map<number, Array<{ downloaded: number; time: number }>>>(new Map());
  const [speeds, setSpeeds] = useState<SpeedMap>(() => new Map());

  useEffect(() => {
    const next = new Map<number, SpeedInfo>();

    for (const item of downloads) {
      if (item.status !== "downloading") {
        samplesRef.current.delete(item.id);
        continue;
      }

      // Get or create sample buffer
      if (!samplesRef.current.has(item.id)) {
        samplesRef.current.set(item.id, []);
      }
      const samples = samplesRef.current.get(item.id)!;
      const now = Date.now();

      // Add current sample
      samples.push({ downloaded: item.downloaded, time: now });
      // Keep only last WINDOW_SIZE samples
      if (samples.length > WINDOW_SIZE) {
        samples.shift();
      }

      // Need at least 2 samples to compute speed
      if (samples.length >= 2) {
        const first = samples[0];
        const last = samples[samples.length - 1];
        const dt = (last.time - first.time) / 1000;
        const dBytes = last.downloaded - first.downloaded;

        if (dt > 0 && dBytes > 0) {
          const bps = Math.round(dBytes / dt);
          next.set(item.id, {
            display: formatBytes(bps) + t("downloadRow.speed"),
            bps,
          });
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
