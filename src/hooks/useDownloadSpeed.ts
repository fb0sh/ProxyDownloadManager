import { useEffect, useRef, useState } from "react";
import { formatBytes } from "../types";
import { t } from "../i18n";
import type { DownloadItem } from "../types";

export interface SpeedInfo {
  display: string;
  bps: number;
}

type SpeedMap = Map<number, SpeedInfo>;

export function useDownloadSpeed(downloads: DownloadItem[]): SpeedMap {
  const refs = useRef<Map<number, { downloaded: number; time: number }>>(new Map());
  const [speeds, setSpeeds] = useState<SpeedMap>(() => new Map());

  useEffect(() => {
    const next = new Map<number, SpeedInfo>();

    for (const item of downloads) {
      if (item.status !== "downloading") {
        refs.current.delete(item.id);
        continue;
      }

      const now = Date.now();
      const prev = refs.current.get(item.id);

      if (prev && prev.time > 0 && prev.downloaded > 0) {
        const dt = (now - prev.time) / 1000;
        if (dt > 0) {
          const bps = Math.round((item.downloaded - prev.downloaded) / dt);
          if (bps > 0) {
            next.set(item.id, {
              display: formatBytes(bps) + t("downloadRow.speed"),
              bps,
            });
          }
        }
      }

      refs.current.set(item.id, { downloaded: item.downloaded, time: now });
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
