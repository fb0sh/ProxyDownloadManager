import type { DownloadStatus } from "../types";

export function isFailed(status: DownloadStatus): boolean {
  return typeof status === "object" && "failed" in status;
}

export function getErrorMessage(status: DownloadStatus): string | undefined {
  return typeof status === "object" && "failed" in status ? status.failed : undefined;
}

export function statusString(status: DownloadStatus): string {
  return typeof status === "object" && "failed" in status ? "failed" : status;
}

export function isActiveStatus(status: DownloadStatus): boolean {
  const s = statusString(status);
  return s === "downloading" || s === "connecting" || s === "retrying" || s === "merging";
}

export function isIncompleteStatus(status: DownloadStatus): boolean {
  const s = statusString(status);
  return s !== "completed";
}

export function formatBytes(bytes: number): string {
  if (bytes === 0) return "0 B";
  const k = 1024;
  const sizes = ["B", "KB", "MB", "GB", "TB"];
  const i = Math.floor(Math.log(bytes) / Math.log(k));
  return `${parseFloat((bytes / Math.pow(k, i)).toFixed(1))} ${sizes[i]}`;
}

export function formatTimestamp(ts: string): string {
  if (!ts) return "—";
  const secs = Number(ts);
  if (!Number.isFinite(secs) || secs <= 0) return ts;
  try {
    const d = new Date(secs * 1000);
    const pad = (n: number) => String(n).padStart(2, "0");
    return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())} ${pad(d.getHours())}:${pad(d.getMinutes())}:${pad(d.getSeconds())}`;
  } catch {
    return ts;
  }
}

export type StatusVariant = "success" | "danger" | "attention" | "accent" | "default";

export function statusColor(s: DownloadStatus): StatusVariant {
  if (isFailed(s)) return "danger";
  switch (s) {
    case "completed": return "success";
    case "paused": return "attention";
    case "downloading":
    case "connecting":
    case "retrying":
    case "merging":
      return "accent";
    default: return "default";
  }
}

export function formatRateLimit(bps: number): string {
  if (!bps) return "Unlimited";
  if (bps >= 1024 * 1024) return `${(bps / (1024 * 1024)).toFixed(bps % (1024 * 1024) === 0 ? 0 : 1)} MB/s`;
  if (bps >= 1024) return `${Math.round(bps / 1024)} KB/s`;
  return `${bps} B/s`;
}
