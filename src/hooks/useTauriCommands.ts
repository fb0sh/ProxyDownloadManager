// src/hooks/useTauriCommands.ts
import { invoke } from "@tauri-apps/api/core";
import type { DownloadItem, Settings } from "../types";

export function useTauriCommands() {
  return {
    listDownloads: () => invoke<DownloadItem[]>("list_downloads"),
    startDownload: (url: string, filename: string, proxyName: string, connections: number, savePath: string) =>
      invoke<number>("start_download", { url, filename, proxyName, connections, savePath }),
    pauseDownload: (id: number) => invoke<void>("pause_download", { id }),
    resumeDownload: (id: number) => invoke<void>("resume_download", { id }),
    deleteDownload: (id: number, deleteFile: boolean) =>
      invoke<void>("delete_download", { id, deleteFile }),
    getSettings: () => invoke<Settings>("get_settings"),
    saveSettings: (settings: Settings) => invoke<void>("save_settings", { settings }),
    cancelDownload: (id: number) => invoke<void>("cancel_download", { id }),
    redownloadDownload: (id: number) => invoke<number>("redownload_download", { id }),
  };
}
