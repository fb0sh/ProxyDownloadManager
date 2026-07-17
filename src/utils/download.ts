import type { DownloadItem } from "../types";

// --- URL / filename utilities ---

const DOWNLOAD_EXTENSIONS = [
  ".zip", ".tar", ".gz", ".bz2", ".xz", ".7z", ".rar", ".iso",
  ".pdf", ".doc", ".docx", ".xls", ".xlsx", ".ppt", ".pptx",
  ".mp3", ".mp4", ".avi", ".mkv", ".mov", ".wmv", ".flv",
  ".exe", ".msi", ".dmg", ".pkg", ".deb", ".rpm",
  ".png", ".jpg", ".jpeg", ".gif", ".svg", ".webp",
  ".dll", ".so", ".dylib", ".bin", ".dat",
  ".csv", ".json", ".xml", ".sql", ".db",
  ".apk", ".ipa", ".appimage", ".flatpak", ".snap",
];

export function looksLikeDownloadUrl(text: string): boolean {
  try {
    const url = new URL(text);
    const path = url.pathname.toLowerCase();
    return DOWNLOAD_EXTENSIONS.some((ext) => path.endsWith(ext));
  } catch {
    return false;
  }
}

export function extractFilename(url: string): string {
  try {
    const u = new URL(url);

    // Strategy 1: extract from URL path
    const path = u.pathname;
    const segments = path.split("/").filter(Boolean);
    if (segments.length > 0) {
      const last = segments[segments.length - 1];
      if (last.includes(".") && !last.endsWith(".")) return decodeURIComponent(last);
    }

    // Strategy 2: search ALL query param values for filename=xxx
    for (const [, val] of u.searchParams) {
      if (!val) continue;
      const decoded = decodeURIComponent(val);
      const m = decoded.match(/filename\s*=\*?(?:UTF-8''|"|)([^";\s]+)/i);
      if (m) return m[1];
    }

    // Strategy 3: scan the full URL for the last name.extension pattern
    const pattern = /([^\/?#&=\s]{2,})\.(\w{2,5})(?=[\/?#&\s]|$)/g;
    const matches = [...url.matchAll(pattern)];
    if (matches.length > 0) {
      const last = matches[matches.length - 1];
      return last[1] + "." + last[2];
    }

    return "";
  } catch {
    return "";
  }
}

// --- Filter / display utilities ---

export function applyFilter(items: DownloadItem[], f: "all" | "completed" | "incomplete") {
  if (f === "all") return items;
  if (f === "completed") return items.filter((d) => d.status === "completed");
  return items.filter((d) => d.status === "downloading" || d.status === "paused" || d.status === "queued");
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

export function statusColor(s: string): StatusVariant {
  switch (s) {
    case "completed": return "success";
    case "failed": return "danger";
    case "paused": return "attention";
    case "downloading": return "accent";
    default: return "default";
  }
}

// --- File action utilities ---

import { invoke } from "@tauri-apps/api/core";

export async function openFile(path: string): Promise<void> {
  try {
    await invoke("open_file", { path });
  } catch (e) {
    console.error("[ProxyDM] open file error:", e);
  }
}

export async function openFolder(path: string): Promise<void> {
  try {
    const { revealItemInDir } = await import("@tauri-apps/plugin-opener");
    await revealItemInDir(path);
  } catch {
    try {
      await invoke("open_file", { path: path.replace(/[/\\][^/\\]*$/, "") || "." });
    } catch (e) {
      console.error("[ProxyDM] open folder error:", e);
    }
  }
}
