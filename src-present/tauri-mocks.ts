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
    url: "https://github.com/fb0sh/ProxyDownloadManager/releases/latest/download/ProxyDownloadManager_aarch64.dmg",
    file_name: "ProxyDownloadManager_aarch64.dmg",
    save_path: "/Downloads/ProxyDownloadManager_aarch64.dmg",
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

/** Same categories as the app fallback, drawn as SVG so the demo matches the list and details icons. */
function showcaseIcon(body: string): string {
  return (
    "data:image/svg+xml;utf8," +
    encodeURIComponent(
      `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="#a3a3a3" stroke-width="1.6" stroke-linejoin="round" stroke-linecap="round">${body}</svg>`,
    )
  );
}

const PAGE =
  `<path d="M7 3.5h7.2L18 7.2V20a1.5 1.5 0 0 1-1.5 1.5h-9A1.5 1.5 0 0 1 6 20V5A1.5 1.5 0 0 1 7 3.5Z"/>` +
  `<path d="M14 3.8V7.5h3.6"/>`;

const SHOWCASE_ICONS: Record<string, string> = {
  generic: showcaseIcon(PAGE),
  document: showcaseIcon(`${PAGE}<path d="M9 11.5h6M9 14.5h5.2M9 17.5h4"/>`),
  pdf: showcaseIcon(`${PAGE}<path d="M8.6 12h6.8v4.4H8.6z"/>`),
  executable: showcaseIcon(`<rect x="5" y="5" width="14" height="14" rx="1.5"/><path d="M9.2 12h5.2M12 9.4l2.6 2.6L12 14.6"/>`),
  archive: showcaseIcon(
    `<path d="M5.5 9.2h13v9.3a1 1 0 0 1-1 1h-11a1 1 0 0 1-1-1V9.2Z"/>` +
      `<path d="M9 9.2V7.4A1 1 0 0 1 10 6.4h4a1 1 0 0 1 1 1v1.8"/>` +
      `<path d="M11.2 12.2h1.6M11.2 14.6h1.6M11.2 17h1.6"/>`,
  ),
  video: showcaseIcon(`<rect x="4.5" y="6.5" width="15" height="11" rx="1.5"/><path d="M10.6 9.4v5.2l4.2-2.6z" fill="#a3a3a3" stroke="none"/>`),
  audio: showcaseIcon(`<path d="M8 10.5v3h2.2l3 2.4V8.1l-3 2.4H8Z"/><path d="M15.2 10.2a2.4 2.4 0 0 1 0 3.6"/>`),
  image: showcaseIcon(`<rect x="4.5" y="5.5" width="15" height="13" rx="1.5"/><circle cx="9" cy="10" r="1.2"/><path d="M6.6 16.2 10 12.8l2.4 2.2 2.2-2 3.2 3.2"/>`),
  installer: showcaseIcon(`${PAGE}<path d="M12 11v6M9.4 14.6 12 17.2l2.6-2.6"/>`),
  disk: showcaseIcon(`<circle cx="12" cy="12" r="7.2"/><circle cx="12" cy="12" r="2.2"/>`),
};

function showcaseCategory(fileName: string, mimeType: string): string {
  const base = (fileName.split(/[/\\]/).pop() ?? fileName).trim();
  const dot = base.lastIndexOf(".");
  const ext = dot > 0 ? base.slice(dot + 1).toLowerCase() : "";
  switch (ext) {
    case "exe":
    case "msi":
    case "bat":
    case "cmd":
    case "com":
    case "appimage":
    case "bin":
      return "executable";
    case "deb":
    case "rpm":
    case "pkg":
    case "apk":
      return "installer";
    case "dmg":
    case "iso":
    case "img":
      return "disk";
    case "zip":
    case "7z":
    case "rar":
    case "tar":
    case "gz":
    case "tgz":
    case "bz2":
    case "xz":
    case "zst":
      return "archive";
    case "mp4":
    case "mkv":
    case "webm":
    case "avi":
    case "mov":
    case "m4v":
    case "flv":
    case "wmv":
      return "video";
    case "mp3":
    case "flac":
    case "wav":
    case "ogg":
    case "aac":
    case "m4a":
    case "opus":
      return "audio";
    case "png":
    case "jpg":
    case "jpeg":
    case "gif":
    case "webp":
    case "bmp":
    case "svg":
    case "ico":
    case "heic":
      return "image";
    case "pdf":
      return "pdf";
    case "txt":
    case "md":
    case "log":
    case "doc":
    case "docx":
    case "odt":
    case "rtf":
    case "xls":
    case "xlsx":
    case "csv":
    case "ppt":
    case "pptx":
      return "document";
    default:
      break;
  }
  const mime = mimeType.toLowerCase();
  if (mime.startsWith("video/")) return "video";
  if (mime.startsWith("audio/")) return "audio";
  if (mime.startsWith("image/")) return "image";
  if (mime.includes("pdf")) return "pdf";
  if (mime.includes("zip") || mime.includes("archive") || mime.includes("compressed")) return "archive";
  if (mime.includes("executable") || mime.includes("x-msdos")) return "executable";
  if (mime.startsWith("text/") || mime.includes("document") || mime.includes("word")) return "document";
  return "generic";
}

function showcaseIconBatch(requests: { id?: number; fileName?: string; mimeType?: string }[]) {
  const icons: { key: string; mime_type: string; data: string }[] = [];
  const seen = new Set<string>();
  const matches = requests.map((req) => {
    const category = showcaseCategory(req.fileName ?? "", req.mimeType ?? "");
    const key = `present:${category}`;
    if (!seen.has(key)) {
      seen.add(key);
      icons.push({ key, mime_type: "image/svg+xml", data: SHOWCASE_ICONS[category] });
    }
    return { id: req.id ?? 0, key };
  });
  return { icons, matches };
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
    case "set_download_proxy": {
      const item = mockDownloads.find((d) => d.id === args?.id);
      if (item) item.proxy_name = args?.proxyName ?? "";
      return;
    }
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
        latest_version: "0.20.2",
        current_version: "0.20.2",
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
    case "get_file_icon": {
      const batch = showcaseIconBatch(args?.request ? [args.request] : []);
      return batch.icons[0] ?? { key: "present:generic", mime_type: "image/svg+xml", data: SHOWCASE_ICONS.generic };
    }
    case "get_file_icons":
      return showcaseIconBatch(Array.isArray(args?.requests) ? args.requests : []);
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
  setTitle = async (_title: string) => {};
  setSize = async (_size: unknown) => {};
  requestUserAttention = async () => {};
  // The real app closes its own OS window here. In the showcase that window is
  // a panel inside the demo frame, so hand the request to DemoWindow instead.
  close = async () => {
    window.dispatchEvent(new CustomEvent("demo-window-close"));
  };
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
