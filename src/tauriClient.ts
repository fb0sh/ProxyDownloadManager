import { invoke } from "@tauri-apps/api/core";
import type { DownloadItem, ProbeInfo, Settings, UpdateInfo } from "./types";

interface IconData {
  rgba: string;
  width: number;
  height: number;
}

interface ProxyTestResult {
  ok: boolean;
  latency_ms: number;
  status?: number;
  error?: string;
}

export const tauriClient = {
  listDownloads: async () => {
    const items: DownloadItem[] = await invoke<DownloadItem[]>("list_downloads");
    return items.map((item) => ({
      ...item,
      status: item.status || "queued",
    })) as DownloadItem[];
  },
  startDownload: (opts: {
    url: string;
    filename: string;
    proxyName: string;
    connections: number;
    savePath: string;
    headers?: Record<string, string>;
    rateLimitBps?: number;
    startPaused?: boolean;
  }) =>
    invoke<number>("start_download", {
      url: opts.url,
      filename: opts.filename,
      proxyName: opts.proxyName,
      connections: opts.connections,
      savePath: opts.savePath,
      headers: opts.headers ?? {},
      rateLimitBps: opts.rateLimitBps ?? 0,
      startPaused: opts.startPaused ?? false,
    }),
  probeUrl: (url: string, headers: Record<string, string> = {}, proxyName = "") =>
    invoke<ProbeInfo>("probe_url", { url, headers, proxyName }),
  pauseDownload: (id: number) => invoke<void>("pause_download", { id }),
  resumeDownload: (id: number) => invoke<void>("resume_download", { id }),
  deleteDownload: (id: number, deleteFile: boolean) =>
    invoke<void>("delete_download", { id, deleteFile }),
  redownloadDownload: (id: number) => invoke<number>("redownload_download", { id }),
  setDownloadConnections: (id: number, connections: number) =>
    invoke<void>("set_download_connections", { id, connections }),
  setDownloadRateLimit: (id: number, rateLimitBps: number) =>
    invoke<void>("set_download_rate_limit", { id, rateLimitBps }),
  setGlobalRateLimit: (rateLimitBps: number) =>
    invoke<void>("set_global_rate_limit", { rateLimitBps }),
  refreshDownloadUrl: (id: number, url: string, headers: Record<string, string> = {}) =>
    invoke<void>("refresh_download_url", { id, url, headers }),

  getSettings: () => invoke<Settings>("get_settings"),
  saveSettings: (settings: Settings) => invoke<void>("save_settings", { settings }),

  exitApp: () => invoke<void>("exit_app"),
  openFile: (path: string) => invoke<void>("open_file", { path }),
  readLogs: (maxLines: number = 50) => invoke<string[]>("read_logs", { maxLines }),
  getExtensionsDir: () => invoke<string>("get_extensions_dir"),
  openExtensionsFolder: () => invoke<void>("open_extensions_folder"),
  getFileIcon: (fileName: string) => invoke<IconData>("get_file_icon", { fileName }),
  checkUpdate: (proxyName: string) => invoke<UpdateInfo>("check_update", { proxyName }),
  testProxy: (proxyName: string) => invoke<ProxyTestResult>("test_proxy", { proxyName }),
};
