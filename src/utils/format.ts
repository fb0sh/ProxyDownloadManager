import type { DownloadStatus } from "../types";

// --- Status utilities ---

/** Check if a DownloadStatus represents a failed state. */
export function isFailed(status: DownloadStatus): boolean {
  return typeof status === "object" && "failed" in status;
}

/** Extract the error message from a failed DownloadStatus, or undefined. */
export function getErrorMessage(status: DownloadStatus): string | undefined {
  return typeof status === "object" && "failed" in status ? status.failed : undefined;
}

/** Normalize a DownloadStatus to a string for display. */
export function statusString(status: DownloadStatus): string {
  return typeof status === "object" && "failed" in status ? "failed" : status;
}

// --- Formatting utilities ---

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
    // invalid timestamp — return raw value
    return ts;
  }
}

export type StatusVariant = "success" | "danger" | "attention" | "accent" | "default";

export function statusColor(s: DownloadStatus): StatusVariant {
  if (isFailed(s)) return "danger";
  switch (s) {
    case "completed": return "success";
    case "paused": return "attention";
    case "downloading": return "accent";
    default: return "default";
  }
}
