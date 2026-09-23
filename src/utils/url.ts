import type { DownloadItem } from "../types";

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
  } catch { /* invalid URL — not a download link */ }
    return false;
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
  } catch { /* malformed URL */ }
  return "";
}

export type StatusFilter = "all" | "downloading" | "completed" | "incomplete";
export type TypeFilter = "all" | "archive" | "video" | "audio" | "document" | "other";

const TYPE_EXTS: Record<Exclude<TypeFilter, "all" | "other">, string[]> = {
  archive: [".zip", ".tar", ".gz", ".7z", ".rar", ".iso", ".bz2", ".xz"],
  video: [".mp4", ".mkv", ".avi", ".mov", ".webm", ".ts", ".m3u8"],
  audio: [".mp3", ".flac", ".aac", ".wav", ".ogg", ".m4a"],
  document: [".pdf", ".doc", ".docx", ".xls", ".xlsx", ".ppt", ".pptx", ".txt"],
};

export function fileTypeOf(name: string): TypeFilter {
  const lower = name.toLowerCase();
  for (const [kind, exts] of Object.entries(TYPE_EXTS) as [Exclude<TypeFilter, "all" | "other">, string[]][]) {
    if (exts.some((e) => lower.endsWith(e))) return kind;
  }
  return "other";
}

export function applyFilter(
  items: DownloadItem[],
  f: StatusFilter,
  query = "",
  type: TypeFilter = "all",
) {
  let next = items;
  if (f === "completed") next = next.filter((d) => d.status === "completed");
  else if (f === "downloading") {
    next = next.filter((d) => {
      const s = typeof d.status === "string" ? d.status : "failed";
      return s === "downloading" || s === "connecting" || s === "retrying" || s === "merging";
    });
  } else if (f === "incomplete") {
    next = next.filter((d) => d.status !== "completed");
  }
  if (type !== "all") next = next.filter((d) => fileTypeOf(d.file_name) === type);
  const q = query.trim().toLowerCase();
  if (q) next = next.filter((d) => d.file_name.toLowerCase().includes(q) || d.url.toLowerCase().includes(q));
  return next;
}
