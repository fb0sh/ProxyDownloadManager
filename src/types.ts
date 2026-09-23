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
  headers?: Record<string, string>;
  final_url?: string;
  content_type?: string;
  etag?: string;
  last_modified?: string;
  rate_limit_bps?: number;
  error_code?: string;
  error_message?: string;
  http_status?: number | null;
  retry_count?: number;
  last_error_at?: string;
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
  | "queued"
  | "connecting"
  | "retrying"
  | "merging"
  | { failed: string };

export type PartStatus =
  | "pending"
  | "downloading"
  | "completed"
  | "failed";

export type ProxyProtocol = "http" | "https" | "socks5";

export interface ProxyConfig {
  protocol: ProxyProtocol;
  host: string;
  port: number;
  username?: string;
  password?: string;
}

export type FileConflictPolicy = "ask" | "rename" | "overwrite" | "skip";

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
  file_conflict?: FileConflictPolicy;
  proxy_group?: string[];
  proxy_group_fallback_direct?: boolean;
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

export interface ProbeInfo {
  url: string;
  final_url: string;
  file_name: string;
  file_size: number;
  content_type: string;
  supports_range: boolean;
  etag: string;
  last_modified: string;
  suggested_connections: number;
  is_hls: boolean;
  hls_variants: { uri: string; bandwidth: number; resolution: string; codecs: string }[];
}

export interface PendingDownloadRequest {
  protocol_version?: number;
  request_id?: string;
  action?: string;
  url: string;
  final_url?: string;
  filename?: string;
  method?: string;
  referrer?: string;
  user_agent?: string;
  cookies?: string;
  headers?: Record<string, string>;
  tab_url?: string;
  content_type?: string;
  content_length?: number;
  proxy_name?: string;
  connections?: number;
}

/** Structured error type matching Rust's PdmError (tagged union). */
export type PdmError =
  | { kind: "cancelled" }
  | { kind: "http"; value: number }
  | { kind: "client_build"; value: string }
  | { kind: "probe"; value: string }
  | { kind: "not_found"; value: number }
  | { kind: "db"; value: string }
  | { kind: "config"; value: string }
  | { kind: "io"; value: string }
  | { kind: "network"; value: string }
  | { kind: "retries_exhausted"; value: string }
  | { kind: "range_lost" }
  | { kind: "other"; value: string }
  | { kind: "file_exists"; value: string }
  | { kind: "duplicate_download"; value: number }
  | { kind: "resource_mismatch"; value: string }
  | { kind: "hls"; value: string }
  | { kind: "unsupported"; value: string };
