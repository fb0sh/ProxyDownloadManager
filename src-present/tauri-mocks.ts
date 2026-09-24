import type { DownloadItem, Settings, DownloadStatus, DownloadPart, PartStatus } from "../../src/types";

type Listener = (...args: any[]) => void;
const listeners = new Map<string, Set<Listener>>();

const now = Math.floor(Date.now() / 1000);
let nextId = 42;

function mockParts(count: number, total: number, downloaded: number): DownloadPart[] {
  const parts: DownloadPart[] = [];
  const slice = Math.floor(total / Math.max(1, count));
  for (let i = 0; i < count; i++) {
    const start = i * slice;
    const end = i === count - 1 ? total : start + slice;
    const len = end - start;
    const d = Math.min(len, Math.max(0, downloaded - start));
    const status: PartStatus = d >= len && len > 0 ? "completed" : d > 0 ? "downloading" : "pending";
    parts.push({ index: i, start, end, downloaded: d, temp_path: "", status, retries: 0 });
  }
  return parts;
}

const mockDownloads: DownloadItem[] = [
  {
    id: 1,
    url: "https://releases.ubuntu.com/24.04/ubuntu-24.04.2-desktop-amd64.iso",
    file_name: "ubuntu-24.04.2-desktop-amd64.iso",
    save_path: "/Downloads/ubuntu-24.04.2-desktop-amd64.iso",
    total_size: 5872025600,
    downloaded: 3937402880,
    status: "downloading",
    parts: mockParts(8, 5872025600, 3937402880),
    proxy_name: "clash",
    connections: 8,
    resumable: true,
    created_at: String(now - 120),
    last_try: String(now - 120),
  },
  {
    id: 2,
    url: "https://nodejs.org/dist/v22.14.0/node-v22.14.0.pkg",
    file_name: "node-v22.14.0.pkg",
    save_path: "/Downloads/node-v22.14.0.pkg",
    total_size: 88080384,
    downloaded: 88080384,
    status: "completed",
    parts: mockParts(4, 88080384, 88080384),
    proxy_name: "",
    connections: 4,
    resumable: true,
    created_at: String(now - 600),
    last_try: String(now - 600),
  },
  {
    id: 3,
    url: "https://code.visualstudio.com/sha/download?build=stable&os=darwin-arm64",
    file_name: "VSCode-darwin-arm64.zip",
    save_path: "/Downloads/VSCode-darwin-arm64.zip",
    total_size: 217440512,
    downloaded: 90331648,
    status: "downloading",
    parts: mockParts(16, 217440512, 90331648),
    proxy_name: "v2ray",
    connections: 16,
    resumable: true,
    created_at: String(now - 300),
    last_try: String(now - 300),
  },
  {
    id: 4,
    url: "https://desktop.docker.com/mac/main/arm64/Docker.dmg",
    file_name: "Docker.dmg",
    save_path: "/Downloads/Docker.dmg",
    total_size: 593601280,
    downloaded: 228589568,
    status: "paused",
    parts: mockParts(8, 593601280, 228589568),
    proxy_name: "",
    connections: 8,
    resumable: true,
    created_at: String(now - 900),
    last_try: String(now - 300),
  },
  {
    id: 5,
    url: "https://github.com/fb0sh/ProxyDownloadManager/releases/download/v0.13.2/ProxyDownloadManager_0.13.2_aarch64.dmg",
    file_name: "ProxyDownloadManager_0.13.2_aarch64.dmg",
    save_path: "/Downloads/ProxyDownloadManager_0.13.2_aarch64.dmg",
    total_size: 18582912,
    downloaded: 18582912,
    status: "completed",
    parts: mockParts(4, 18582912, 18582912),
    proxy_name: "clash",
    connections: 4,
    resumable: true,
    created_at: String(now - 1800),
    last_try: String(now - 1800),
  },
  {
    id: 6,
    url: "https://cdn.example/private/build.tgz",
    file_name: "build.tgz",
    save_path: "/Downloads/build.tgz",
    total_size: 44040192,
    downloaded: 12000000,
    status: { failed: "HTTP 403" },
    parts: mockParts(4, 44040192, 12000000),
    proxy_name: "",
    connections: 4,
    resumable: true,
    error_code: "auth",
    error_message: "HTTP 403",
    http_status: 403,
    created_at: String(now - 2400),
    last_try: String(now - 200),
  },
];

const defaultSettings: Settings = {
  download_dir: "/Downloads",
  max_connections: 0,
  max_retries: 10,
  user_agent: "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36",
  launch_at_startup: false,
  silent_startup: true,
  proxies: {
    clash: { protocol: "socks5", host: "127.0.0.1", port: 7890 },
    v2ray: { protocol: "http", host: "127.0.0.1", port: 10809 },
  },
  global_rate_limit: 0,
  default_proxy: "clash",
  home_dir: "/Users/user/.ProxyDM",
  language: "zh",
  danger_accept_invalid_certs: true,
  global_shortcut: "Ctrl+Super+J",
  file_conflict: "rename",
};

function startProgressSimulation() {
  setInterval(() => {
    for (const item of mockDownloads) {
      if (item.status === "downloading" && item.downloaded < item.total_size) {
        const increment = Math.floor(Math.random() * 8_000_000) + 400_000;
        item.downloaded = Math.min(item.downloaded + increment, item.total_size);
        item.parts = mockParts(item.connections || 4, item.total_size, item.downloaded);
        if (item.downloaded >= item.total_size) {
          item.status = "completed" as DownloadStatus;
          emit("download-completed", { id: item.id, file_name: item.file_name });
        }
      }
    }
  }, 1200);
}

function emit(event: string, payload?: unknown) {
  const set = listeners.get(event);
  if (set) set.forEach((fn) => fn({ payload, event }));
}

async function invoke(command: string, args?: Record<string, any>): Promise<any> {
  await new Promise((r) => setTimeout(r, 12));

  switch (command) {
    case "list_downloads":
      return mockDownloads.map((d) => ({ ...d, parts: d.parts.map((p) => ({ ...p })) }));
    case "get_settings":
      return { ...defaultSettings, proxies: { ...defaultSettings.proxies } };
    case "save_settings":
      Object.assign(defaultSettings, args?.settings);
      return;
    case "start_download": {
      const id = nextId++;
      const size = Math.floor(Math.random() * 500_000_000) + 5_000_000;
      const name = args?.filename || args?.url?.split("/").pop() || `download-${id}`;
      const connections = args?.connections || 8;
      mockDownloads.unshift({
        id,
        url: args?.url || "",
        file_name: name,
        save_path: `${defaultSettings.download_dir}/${name}`,
        total_size: size,
        downloaded: 0,
        status: "downloading",
        parts: mockParts(connections, size, 0),
        proxy_name: args?.proxyName || defaultSettings.default_proxy,
        connections,
        resumable: true,
        created_at: String(Math.floor(Date.now() / 1000)),
        last_try: "",
      });
      return id;
    }
    case "probe_url":
      return {
        url: args?.url,
        final_url: args?.url,
        file_name: args?.url?.split("/").pop() || "download.bin",
        file_size: 104857600,
        content_type: "application/octet-stream",
        supports_range: true,
        etag: "",
        last_modified: "",
        suggested_connections: 8,
        is_hls: false,
        hls_variants: [],
      };
    case "pause_download": {
      const item = mockDownloads.find((d) => d.id === args?.id);
      if (item && item.status === "downloading") item.status = "paused";
      return;
    }
    case "resume_download": {
      const item = mockDownloads.find((d) => d.id === args?.id);
      if (item && (item.status === "paused" || item.status === "queued")) item.status = "downloading";
      return;
    }
    case "delete_download": {
      const idx = mockDownloads.findIndex((d) => d.id === args?.id);
      if (idx >= 0) mockDownloads.splice(idx, 1);
      return;
    }
    case "redownload_download": {
      const item = mockDownloads.find((d) => d.id === args?.id);
      if (item) {
        item.status = "downloading";
        item.downloaded = 0;
        item.parts = mockParts(item.connections || 4, item.total_size, 0);
      }
      return args?.id;
    }
    case "set_global_rate_limit":
      defaultSettings.global_rate_limit = args?.rateLimitBps ?? 0;
      return;
    case "set_download_connections": {
      const item = mockDownloads.find((d) => d.id === args?.id);
      if (item) {
        item.connections = args?.connections ?? item.connections;
        item.parts = mockParts(item.connections, item.total_size, item.downloaded);
      }
      return;
    }
    case "set_download_rate_limit": {
      const item = mockDownloads.find((d) => d.id === args?.id);
      if (item) item.rate_limit_bps = args?.rateLimitBps ?? 0;
      return;
    }
    case "test_proxy":
      return { ok: true, latency_ms: 42 };
    case "check_update":
      return {
        latest_version: "0.13.2",
        current_version: "0.13.2",
        has_update: false,
        release_url: "https://github.com/fb0sh/ProxyDownloadManager/releases",
        release_notes: "",
        assets: [],
      };
    case "read_logs":
      return ["[INFO] ProxyDM started", "[INFO] Extensions synced", "[INFO] Download manager ready"];
    case "file_exists":
      return true;
    case "get_extensions_dir":
      return "~/Library/Application Support/com.fb0sh.proxydownloadmanager/extensions";
    case "open_extensions_folder":
      return;
    case "get_file_icon":
      return { rgba: "", width: 32, height: 32 };
    case "exit_app":
      return;
    default:
      console.warn("[Mock] Unhandled invoke:", command, args);
      return;
  }
}

export { invoke };

export async function listen<T = unknown>(event: string, handler: (event: { payload: T }) => void): Promise<() => void> {
  if (!listeners.has(event)) listeners.set(event, new Set());
  listeners.get(event)!.add(handler as Listener);
  return () => {
    listeners.get(event)?.delete(handler as Listener);
  };
}

export async function emitToListeners(event: string, payload?: unknown): Promise<void> {
  emit(event, payload);
}

export class WebviewWindow {
  static getByLabel = async () => null;
  label: string;
  constructor(label: string, _options?: unknown) {
    this.label = label;
  }
  once = async (_e: string, _cb?: unknown) => {};
  emit = async (event: string, payload?: unknown) => emitToListeners(event, payload);
  show = async () => {};
  unminimize = async () => {};
  center = async () => {};
  setAlwaysOnTop = async () => {};
  setFocus = async () => {};
  requestUserAttention = async () => {};
  close = async () => {};
}

export function getCurrentWindow() {
  return {
    onFocusChanged: async () => () => {},
    show: async () => {},
    hide: async () => {},
    close: async () => {},
  };
}

export function getCurrentWebviewWindow() {
  return new WebviewWindow("present");
}

export async function isPermissionGranted() {
  return true;
}
export async function requestPermission() {
  return "granted";
}
export function sendNotification(_opts: unknown) {}
export async function readText() {
  return "";
}
export async function writeText(_text: string) {}
export async function open(_opts: unknown) {}
export async function revealItemInDir(_path: string) {}
export const UserAttentionType = { Critical: 1, Informational: 2 };

startProgressSimulation();
