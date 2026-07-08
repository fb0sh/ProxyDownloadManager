/* =========================================================================
 * Tauri API Mocks — makes src/ components work in the browser
 *
 * Each mock returns data that looks exactly like the real Tauri backend.
 * The invoke handler dispatches by command name, matching the real cmd.rs.
 * ========================================================================= */

import type { DownloadItem, Settings, DownloadStatus, DownloadPart, PartStatus } from "../../src/types";

/* ─── Types ──────────────────────────────────────────────────────────── */

type Listener = (...args: any[]) => void;
const listeners = new Map<string, Set<Listener>>();

/* ─── Mock data ──────────────────────────────────────────────────────── */

const now = Math.floor(Date.now() / 1000);
let nextId = 42;

function mockParts(count: number): DownloadPart[] {
  const parts: DownloadPart[] = [];
  for (let i = 0; i < count; i++) {
    parts.push({
      index: i, start: 0, end: 0, downloaded: 0,
      temp_path: "", status: "pending" as PartStatus, retries: 0,
    });
  }
  return parts;
}

const mockDownloads: DownloadItem[] = [
  {
    id: 1, url: "https://releases.ubuntu.com/24.04/ubuntu-24.04-desktop-amd64.iso",
    file_name: "ubuntu-24.04-desktop-amd64.iso",
    save_path: "/Downloads/ubuntu-24.04-desktop-amd64.iso",
    total_size: 5872025600, downloaded: 3937402880,
    status: "downloading" as DownloadStatus,
    parts: mockParts(8), proxy_name: "", connections: 8,
    resumable: true, merge_progress: 0,
    created_at: String(now - 120), last_try: String(now - 120),
  },
  {
    id: 2, url: "https://nodejs.org/dist/v22.0.0/node-v22.0.0.pkg",
    file_name: "node-v22.0.0.pkg",
    save_path: "/Downloads/node-v22.0.0.pkg",
    total_size: 88080384, downloaded: 88080384,
    status: "completed" as DownloadStatus,
    parts: mockParts(4), proxy_name: "", connections: 4,
    resumable: true, merge_progress: 1,
    created_at: String(now - 600), last_try: String(now - 600),
  },
  {
    id: 3, url: "https://code.visualstudio.com/sha/download?build=stable&os=linux-deb-x64",
    file_name: "vscode_amd64.deb",
    save_path: "/Downloads/vscode_amd64.deb",
    total_size: 117440512, downloaded: 50331648,
    status: "downloading" as DownloadStatus,
    parts: mockParts(4), proxy_name: "clash", connections: 4,
    resumable: true, merge_progress: 0,
    created_at: String(now - 300), last_try: String(now - 300),
  },
  {
    id: 4, url: "https://desktop.docker.com/mac/main/amd64/Docker.dmg",
    file_name: "Docker.dmg",
    save_path: "/Downloads/Docker.dmg",
    total_size: 293601280, downloaded: 228589568,
    status: "paused" as DownloadStatus,
    parts: mockParts(4), proxy_name: "", connections: 4,
    resumable: true, merge_progress: 0,
    created_at: String(now - 900), last_try: String(now - 300),
  },
  {
    id: 5, url: "https://github.com/fb0sh/ProxyDownloadManager/releases/download/v0.5.0/proxydm-0.5.0-x86_64.AppImage",
    file_name: "proxydm-0.5.0-x86_64.AppImage",
    save_path: "/Downloads/proxydm-0.5.0-x86_64.AppImage",
    total_size: 12582912, downloaded: 12582912,
    status: "completed" as DownloadStatus,
    parts: mockParts(2), proxy_name: "", connections: 2,
    resumable: true, merge_progress: 1,
    created_at: String(now - 1800), last_try: String(now - 1800),
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
    clash: { protocol: "socks5" as any, host: "127.0.0.1", port: 7890 },
    "v2ray": { protocol: "http" as any, host: "127.0.0.1", port: 10809 },
  },
  global_rate_limit: 0,
  default_proxy: "",
  home_dir: "/Users/user/.ProxyDM",
  language: "zh",
  danger_accept_invalid_certs: true,
};

/* ─── Progress simulation ───────────────────────────────────────────── */

let progressTimers: ReturnType<typeof setInterval>[] = [];

function startProgressSimulation() {
  // Update "downloading" items progress every 2s
  const timer = setInterval(() => {
    for (const item of mockDownloads) {
      if (item.status === "downloading" && item.downloaded < item.total_size) {
        const increment = Math.floor(Math.random() * 500000) + 100000;
        item.downloaded = Math.min(item.downloaded + increment, item.total_size);
        if (item.downloaded >= item.total_size) {
          item.status = "completed" as DownloadStatus;
          item.merge_progress = 1;
          emit("download-completed", item.id);
        }
      }
    }
  }, 2000);
  progressTimers.push(timer);
}

/* ─── Event system ──────────────────────────────────────────────────── */

function emit(event: string, payload?: any) {
  const set = listeners.get(event);
  if (set) set.forEach(fn => fn({ payload, event }));
}

/* ─── Mock invoke — matches cmd.rs command signatures ──────────────── */

async function invoke(command: string, args?: Record<string, any>): Promise<any> {
  // Simulate a tiny delay
  await new Promise(r => setTimeout(r, 10 + Math.random() * 40));

  switch (command) {
    case "list_downloads":
      return [...mockDownloads];

    case "get_settings":
      return { ...defaultSettings };

    case "save_settings":
      Object.assign(defaultSettings, args?.settings);
      return;

    case "start_download": {
      const id = nextId++;
      const size = Math.floor(Math.random() * 500000000) + 5000000;
      const name = args?.filename || args?.url?.split("/").pop() || `download-${id}`;
      mockDownloads.unshift({
        id,
        url: args?.url || "",
        file_name: name,
        save_path: `${defaultSettings.download_dir}/${name}`,
        total_size: size,
        downloaded: 0,
        status: "downloading" as DownloadStatus,
        parts: mockParts(4),
        proxy_name: args?.proxyName || "",
        connections: args?.connections || 4,
        resumable: true,
        merge_progress: 0,
        created_at: String(Math.floor(Date.now() / 1000)),
        last_try: "",
      });
      return id;
    }

    case "pause_download": {
      const item = mockDownloads.find(d => d.id === args?.id);
      if (item && item.status === "downloading") {
        item.status = "paused" as DownloadStatus;
      }
      return;
    }

    case "resume_download": {
      const item = mockDownloads.find(d => d.id === args?.id);
      if (item && item.status === "paused") {
        item.status = "downloading" as DownloadStatus;
      }
      return;
    }

    case "delete_download": {
      const idx = mockDownloads.findIndex(d => d.id === args?.id);
      if (idx >= 0) mockDownloads.splice(idx, 1);
      return;
    }

    case "redownload_download": {
      const item = mockDownloads.find(d => d.id === args?.id);
      if (item) {
        item.status = "downloading" as DownloadStatus;
        item.downloaded = 0;
        item.merge_progress = 0;
      }
      return args?.id;
    }

    case "cancel_download": {
      const item = mockDownloads.find(d => d.id === args?.id);
      if (item) item.status = "paused" as DownloadStatus;
      return;
    }

    case "read_logs":
      return ["[INFO] ProxyDM started", "[INFO] Download manager initialized"];

    case "file_exists":
      return true;

    case "get_extensions_dir":
      return "/Applications/ProxyDM/extensions";

    case "get_file_icon":
      return { icon: "", rank: 0 };

    case "exit_app":
      return;

    default:
      console.warn("[Mock] Unhandled invoke:", command, args);
      return;
  }
}

/* ─── Exports matching @tauri-apps/api/core ─────────────────────────── */

export { invoke };
export type { Listener };

/* ─── Exports matching @tauri-apps/api/event ────────────────────────── */

export async function listen<T = any>(event: string, handler: (event: { payload: T }) => void): Promise<() => void> {
  if (!listeners.has(event)) listeners.set(event, new Set());
  listeners.get(event)!.add(handler as Listener);

  // Auto-emit initial events for demo feel
  if (event === "download-started") {
    setTimeout(() => handler({ payload: 999 } as any), 100);
  }

  return () => {
    listeners.get(event)?.delete(handler as Listener);
  };
}

export async function emitToListeners(event: string, payload?: any): Promise<void> {
  const set = listeners.get(event);
  if (set) set.forEach(fn => fn({ payload, event }));
}

/* ─── Exports matching @tauri-apps/api/webviewWindow ────────────────── */

export class WebviewWindow {
  static getByLabel = async () => null;
  label: string;
  constructor(label: string, _options?: any) { this.label = label; }
  once = async (_e: string, _cb?: any) => {};
  emit = async (event: string, payload?: any) => emitToListeners(event, payload);
  show = async () => {};
  unminimize = async () => {};
  center = async () => {};
  setAlwaysOnTop = async () => {};
  setFocus = async () => {};
  requestUserAttention = async () => {};
  close = async () => {};
}

/* ─── Exports matching @tauri-apps/api/window ───────────────────────── */

export function getCurrentWindow() {
  return {
    onFocusChanged: async (_handler: any) => {
      const noop = () => {};
      return noop;
    },
    onResized: async () => {},
    onMoved: async () => {},
    onCloseRequested: async () => {},
    show: async () => {},
    hide: async () => {},
    close: async () => {},
    setTitle: async () => {},
    setSize: async () => {},
    setPosition: async () => {},
    center: async () => {},
    minimize: async () => {},
    unminimize: async () => {},
    maximize: async () => {},
    unmaximize: async () => {},
    isMinimized: async () => false,
    isMaximized: async () => false,
    isVisible: async () => true,
  };
}

/* ─── Plugin mocks ──────────────────────────────────────────────────── */

export async function isPermissionGranted() { return true; }
export async function requestPermission() { return "granted"; }
export function sendNotification(_opts: any) {
  // no-op in browser
  console.log("[Mock] Notification:", _opts.title, _opts.body);
}
export function onAction(_cb: any) { return async () => {}; }

export async function readText() { return ""; }
export async function writeText(_text: string) {}

export async function isEnabled() { return false; }
export async function enable() {}
export async function disable() {}

export async function open(_opts: any) {}
export async function save(_opts: any) {}
export async function ask(_msg: string) { return true; }
export async function confirm(_msg: string) { return true; }
export async function message(_msg: string) {}

export async function revealItemInDir(_path: string) {}

export const UserAttentionType = { Critical: 1, Informational: 2 };

/* ─── Start simulation on import ────────────────────────────────────── */

startProgressSimulation();
