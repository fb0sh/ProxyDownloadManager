// src/hooks/useClipboard.ts
import { useEffect, useRef, useState } from "react";
import { looksLikeDownloadUrl } from "../utils/download";

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
