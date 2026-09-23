import { useCallback } from "react";
import { WebviewWindow } from "@tauri-apps/api/webviewWindow";
import { t } from "../i18n";
import type { PendingDownloadRequest } from "../types";

export function useWindowManager() {
  const openNewDownload = useCallback(async (payload?: string | PendingDownloadRequest) => {
    const existing = await WebviewWindow.getByLabel("new-download");
    if (existing) {
      existing.setFocus();
      if (payload) {
        try { await existing.emit("new-download-request", payload); } catch { /* window may have closed */ }
      }
      return;
    }

    const url = typeof payload === "string" ? payload : payload?.url;
    const base = window.location.origin + window.location.pathname.replace(/\/+$/, "");
    const params = new URLSearchParams();
    params.set("view", "new-download");
    if (url) params.set("url", url);

    const win = new WebviewWindow("new-download", {
      url: `${base}?${params.toString()}`,
      width: 640,
      height: 560,
      title: t("newDownload.title"),
    });
    win.once("tauri://created", async () => {
      if (payload) {
        try { await win.emit("new-download-request", payload); } catch { /* window may have closed */ }
      }
      await win.show().catch(() => {});
      await win.unminimize().catch(() => {});
      await win.center().catch(() => {});
      await win.setAlwaysOnTop(true).catch(() => {});
      await win.setFocus().catch(() => {});
      const { UserAttentionType } = await import("@tauri-apps/api/window");
      await win.requestUserAttention(UserAttentionType.Critical).catch(() => {});
    });
    win.once("tauri://error", (e) => {
      console.error("Failed to open new download window:", e);
    });
  }, []);

  const openDetails = useCallback(async (id: number) => {
    const existing = await WebviewWindow.getByLabel("download-details");
    if (existing) { existing.setFocus(); return; }

    const base = window.location.origin + window.location.pathname.replace(/\/+$/, "");
    const win = new WebviewWindow("download-details", {
      url: `${base}?view=download-details&id=${id}`,
      width: 520,
      height: 560,
      title: t("properties.title"),
    });
    win.once("tauri://created", async () => {
      await win.setAlwaysOnTop(true).catch(() => {});
      await win.setFocus().catch(() => {});
    });
    win.once("tauri://error", (e) => console.error("Failed to open details:", e));
  }, []);

  return { openNewDownload, openDetails };
}
