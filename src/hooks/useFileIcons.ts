import { useState, useEffect, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { DownloadItem } from "../types";

/** Extract file extension from a filename, lowercased. */
function ext(name: string): string {
  const i = name.lastIndexOf(".");
  return i > 0 ? name.slice(i + 1).toLowerCase() : "";
}

interface IconData {
  rgba: string; // base64-encoded raw RGBA bytes
  width: number;
  height: number;
}

/** Map of extension → <img> data URL (rendered via canvas) */
export type IconMap = Map<string, string>;

/** 32×32 gray document icon (hand-drawn RGBA, matches Rust fallback).
 *  Generated once via canvas at module load. */
const FALLBACK = (() => {
  const size = 32;
  const canvas = document.createElement("canvas");
  canvas.width = size;
  canvas.height = size;
  const ctx = canvas.getContext("2d")!;
  const img = ctx.createImageData(size, size);
  for (let y = 0; y < size; y++) {
    for (let x = 0; x < size; x++) {
      const i = (y * size + x) * 4;
      const border = x < 2 || y < 2 || x >= size - 2 || y >= size - 2;
      const folded = x >= size - 8 && y < 8 && (x - (size - 8)) < (8 - y) + 3;
      if (border || folded) {
        img.data[i] = 180;     // R
        img.data[i + 1] = 180; // G
        img.data[i + 2] = 180; // B
        img.data[i + 3] = 255; // A
      } else {
        img.data[i + 3] = 0;   // transparent
      }
    }
  }
  ctx.putImageData(img, 0, 0);
  return canvas.toDataURL("image/png");
})();

function rgbaToDataURL(rgbaBase64: string, width: number, height: number): string {
  try {
    // Decode base64 → RGBA bytes
    const binary = atob(rgbaBase64);
    const bytes = new Uint8Array(binary.length);
    for (let i = 0; i < binary.length; i++) {
      bytes[i] = binary.charCodeAt(i);
    }

    // Render on an offscreen canvas → PNG data URL
    const canvas = document.createElement("canvas");
    canvas.width = width;
    canvas.height = height;
    const ctx = canvas.getContext("2d");
    if (!ctx) return FALLBACK;

    const imageData = ctx.createImageData(width, height);
    imageData.data.set(bytes);
    ctx.putImageData(imageData, 0, 0);
    return canvas.toDataURL("image/png");
  } catch {
    return FALLBACK;
  }
}

/**
 * Loads native OS file-type icons for all unique extensions in the download list.
 * Returns a Map<extension_string, data_url> that can be looked up by filename.
 */
export function useFileIcons(downloads: DownloadItem[]): IconMap {
  const [icons, setIcons] = useState<IconMap>(() => new Map());
  const loadedRef = useRef<Set<string>>(new Set());
  const convertRef = useRef(rgbaToDataURL);

  useEffect(() => {
    // Collect unique extensions not yet loaded
    const needed = new Set<string>();
    for (const d of downloads) {
      const e = ext(d.file_name);
      if (e && !loadedRef.current.has(e)) needed.add(e);
    }
    if (needed.size === 0) return;

    let cancelled = false;

    (async () => {
      const next = new Map(icons);
      for (const e of needed) {
        if (cancelled) break;
        try {
          const data = await invoke<IconData>("get_file_icon", {
            fileName: `file.${e}`,
          });
          const url = convertRef.current(data.rgba, data.width, data.height);
          next.set(e, url);
        } catch {
          next.set(e, FALLBACK);
        }
      }
      if (!cancelled) {
        loadedRef.current = new Set([...loadedRef.current, ...needed]);
        setIcons(next);
      }
    })();

    return () => {
      cancelled = true;
    };
  }, [downloads]);

  return icons;
}

/** Look up icon for a filename, return fallback if missing. */
export function iconFor(icons: IconMap, fileName: string): string {
  const e = ext(fileName);
  return icons.get(e) ?? FALLBACK;
}
