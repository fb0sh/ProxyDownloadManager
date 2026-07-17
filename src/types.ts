export interface DownloadItem {
  id: number;
  url: string;
  file_name: string;
  save_path: string;
  total_size: number;
  downloaded: number;
  status: DownloadStatus;
  parts: DownloadPart[];
  proxy_name: string;
  connections: number;
  resumable: boolean | null;
  created_at: string;
  last_try: string;
}

export interface DownloadPart {
  index: number;
  start: number;
  end: number;
  downloaded: number;
  temp_path: string;
  status: PartStatus;
  retries: number;
}

export type DownloadStatus =
  | "downloading"
  | "paused"
  | "completed"
  | "failed"
  | "queued";

export type PartStatus =
  | "pending"
  | "downloading"
  | "completed"
  | "failed";

export type ProxyProtocol = "http" | "socks5";

export interface ProxyConfig {
  protocol: ProxyProtocol;
  host: string;
  port: number;
}

export interface Settings {
  download_dir: string;
  max_connections: number;
  max_retries: number;
  user_agent: string;
  launch_at_startup: boolean;
  silent_startup: boolean;
  proxies: Record<string, ProxyConfig>;
  global_rate_limit: number;
  default_proxy: string;
  home_dir: string;
  language: string;
  danger_accept_invalid_certs: boolean;
  global_shortcut: string;
}

export interface AssetInfo {
  name: string;
  url: string;
  recommended: boolean;
}

export interface UpdateInfo {
  latest_version: string;
  current_version: string;
  has_update: boolean;
  release_url: string;
  release_notes: string;
  assets: AssetInfo[];
}

export function formatBytes(bytes: number): string {
  if (bytes === 0) return "0 B";
  const k = 1024;
  const sizes = ["B", "KB", "MB", "GB", "TB"];
  const i = Math.floor(Math.log(bytes) / Math.log(k));
  return `${parseFloat((bytes / Math.pow(k, i)).toFixed(1))} ${sizes[i]}`;
}
