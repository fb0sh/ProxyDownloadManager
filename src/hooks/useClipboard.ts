// src/hooks/useClipboard.ts
import { useEffect, useRef, useState } from "react";
import { looksLikeDownloadUrl } from "../utils/download";
import { useWindowManager } from "./useWindowManager";

export function useClipboardDetection() {
  const [lastText, setLastText] = useState("");
  const intervalRef = useRef<ReturnType<typeof setInterval> | null>(null);
  const { openNewDownload } = useWindowManager();

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
