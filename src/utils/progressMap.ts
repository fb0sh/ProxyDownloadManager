import type { DownloadPart } from "../types";

/** Percent fill for one Progress Map cell (0–100). */
export function partPercent(downloaded: number, start: number, end: number): number {
  const len = Math.max(0, end - start);
  if (len <= 0) return 0;
  return Math.min(100, Math.floor((Math.min(downloaded, len) / len) * 100));
}

export function partPercentFromPart(part: DownloadPart): number {
  return partPercent(part.downloaded, part.start, part.end);
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
