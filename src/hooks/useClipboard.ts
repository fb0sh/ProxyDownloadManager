// src/hooks/useClipboard.ts
import { useEffect, useRef, useState } from "react";
import { readText } from "@tauri-apps/plugin-clipboard-manager";
import { looksLikeDownloadUrl } from "../utils/download";
import { useWindowManager } from "./useWindowManager";

export function useClipboardDetection() {
  const [lastText, setLastText] = useState("");
  const intervalRef = useRef<ReturnType<typeof setInterval> | null>(null);
  const { openNewDownload } = useWindowManager();

  useEffect(() => {
    intervalRef.current = setInterval(async () => {
      try {
        // Plugin, not navigator.clipboard: WebView2 permission-gates the web
        // API without a user gesture, so polling it never works on Windows.
        const text = await readText();
        if (text !== lastText) {
          setLastText(text);
          if (
            (text.startsWith("http://") || text.startsWith("https://") || text.startsWith("ftp://")) &&
            looksLikeDownloadUrl(text)
          ) {
            openNewDownload(text);
          }
        }
      } catch {
        // Clipboard access denied — expected in sandboxed contexts
      }
    }, 2000);

    return () => {
      if (intervalRef.current) clearInterval(intervalRef.current);
    };
  }, [lastText, openNewDownload]);

  return null;
}
