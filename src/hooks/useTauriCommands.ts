// src/hooks/useTauriCommands.ts — plain function, not a React hook
import { invoke } from "@tauri-apps/api/core";
import type { DownloadItem, Settings } from "../types";

export function tauriCommands() {
  return {
    listDownloads: async () => {
      const items: any[] = await invoke<DownloadItem[]>("list_downloads");
      return items.map((item) => ({
        ...item,
        status: item.status || "queued",
      })) as DownloadItem[];
    },
    startDownload: (url: string, filename: string, proxyName: string, connections: number, savePath: string) =>
      invoke<number>("start_download", { url, filename, proxyName, connections, savePath }),
    pauseDownload: (id: number) => invoke<void>("pause_download", { id }),
    resumeDownload: (id: number) => invoke<void>("resume_download", { id }),
    deleteDownload: (id: number, deleteFile: boolean) =>
      invoke<void>("delete_download", { id, deleteFile }),
    getSettings: () => invoke<Settings>("get_settings"),
    saveSettings: (settings: Settings) => invoke<void>("save_settings", { settings }),
    redownloadDownload: (id: number) => invoke<number>("redownload_download", { id }),
  };
}
