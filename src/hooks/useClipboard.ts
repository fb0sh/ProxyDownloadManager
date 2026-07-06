// src/hooks/useClipboard.ts
import { useEffect, useRef, useState } from "react";

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

function looksLikeDownloadUrl(text: string): boolean {
  try {
    const url = new URL(text);
    const path = url.pathname.toLowerCase();
    return DOWNLOAD_EXTENSIONS.some((ext) => path.endsWith(ext));
  } catch {
    return false;
  }
}

export function useClipboardDetection(onUrlDetected: (url: string) => void) {
  const [lastText, setLastText] = useState("");
  const intervalRef = useRef<ReturnType<typeof setInterval> | null>(null);

  useEffect(() => {
    intervalRef.current = setInterval(async () => {
      try {
        const text = await navigator.clipboard.readText();
        if (text !== lastText) {
          setLastText(text);
          if (
            (text.startsWith("http://") || text.startsWith("https://") || text.startsWith("ftp://")) &&
            looksLikeDownloadUrl(text)
          ) {
            onUrlDetected(text);
          }
        }
      } catch {
        // Clipboard access denied
      }
    }, 2000);

    return () => {
      if (intervalRef.current) clearInterval(intervalRef.current);
    };
  }, [lastText, onUrlDetected]);

  return null;
}
