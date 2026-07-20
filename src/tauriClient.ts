import { invoke } from "@tauri-apps/api/core";
import type { DownloadItem, Settings, UpdateInfo } from "./types";

/** Icon data returned by the Rust get_file_icon command. */
interface IconData {
  rgba: string; // base64-encoded raw RGBA bytes
  width: number;
  height: number;
}

/** Proxy test result returned by the Rust test_proxy command. */
interface ProxyTestResult {
  ok: boolean;
  latency_ms: number;
  status?: number;
  error?: string;
}

export const tauriClient = {
  // ── Download CRUD ──

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
  redownloadDownload: (id: number) => invoke<number>("redownload_download", { id }),

  // ── Settings ──

  getSettings: () => invoke<Settings>("get_settings"),
  saveSettings: (settings: Settings) => invoke<void>("save_settings", { settings }),

  // ── Utilities ──

  exitApp: () => invoke<void>("exit_app"),
  openFile: (path: string) => invoke<void>("open_file", { path }),
  readLogs: (maxLines: number = 50) => invoke<string[]>("read_logs", { maxLines }),
  getExtensionsDir: () => invoke<string>("get_extensions_dir"),
  openExtensionsFolder: () => invoke<void>("open_extensions_folder"),
  getFileIcon: (fileName: string) => invoke<IconData>("get_file_icon", { fileName }),
  checkUpdate: (proxyName: string) => invoke<UpdateInfo>("check_update", { proxyName }),
  testProxy: (proxyName: string) => invoke<ProxyTestResult>("test_proxy", { proxyName }),
};
