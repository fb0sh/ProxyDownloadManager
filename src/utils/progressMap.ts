import type { DownloadPart, DownloadStatus } from "../types";

/** Percent fill for one Progress Map cell (0–100). */
export function partPercent(downloaded: number, start: number, end: number): number {
  const len = Math.max(0, end - start);
  if (len <= 0) return 0;
  return Math.min(100, Math.floor((Math.min(downloaded, len) / len) * 100));
}

export function partPercentFromPart(part: DownloadPart): number {
  return partPercent(part.downloaded, part.start, part.end);
}

/**
 * Progress Map (进度地图) cell percents with the status rules applied:
 * Completed → every cell 100% (even if the last part event was lost);
 * Paused/Queued freeze at recorded progress; Failed keeps what was fetched.
 */
export function cellPercents(parts: DownloadPart[], status: DownloadStatus): number[] {
  if (status === "completed") return parts.map(() => 100);
  return parts.map(partPercentFromPart);
}

/**
 * Overall percent (0–100), the ONE formula for table, dialog and details:
 * floor-based so 100% is never shown early; Completed always reads 100.
 */
export function overallPercent(
  downloaded: number,
  totalSize: number,
  status: DownloadStatus,
): number {
  if (status === "completed") return 100;
  if (totalSize <= 0) return 0;
  return Math.min(100, Math.floor((Math.min(downloaded, totalSize) / totalSize) * 100));
}

/** Apply per-part downloaded[] onto DownloadPart[] (fixed ranges). */
export function applyPartDownloaded(
  parts: DownloadPart[],
  partDownloaded: number[],
  totalSize: number,
  resetToSingle?: boolean,
): DownloadPart[] {
  if (resetToSingle) {
    const d = partDownloaded[0] ?? 0;
    return [
      {
        index: 0,
        start: 0,
        end: totalSize,
        downloaded: d,
        temp_path: "",
        status: d >= totalSize && totalSize > 0 ? "completed" : d > 0 ? "downloading" : "pending",
        retries: 0,
      },
    ];
  }
  if (parts.length === 0 && partDownloaded.length > 0) {
    return [
      {
        index: 0,
        start: 0,
        end: totalSize,
        downloaded: partDownloaded[0] ?? 0,
        temp_path: "",
        status: "downloading",
        retries: 0,
      },
    ];
  }
  return parts.map((p, i) => {
    if (i >= partDownloaded.length) return p;
    const downloaded = partDownloaded[i]!;
    const len = p.end - p.start;
    const status =
      downloaded >= len && len > 0
        ? ("completed" as const)
        : downloaded > 0
          ? ("downloading" as const)
          : p.status;
    return { ...p, downloaded, status };
  });
}
