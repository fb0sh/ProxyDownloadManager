# ProxyDM 分阶段并行实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build complete ProxyDM download manager with Rust backend + React frontend, multi-threaded downloading, per-download proxy, WebSocket browser extension integration, and system tray.

**Architecture:** Tauri 2 app with Rust backend handling all download engine logic (probe, concurrent/single downloader, network pool, rate limiting, state persistence) and WebSocket server (port 18999). React frontend using Primer component library, zustand state management, and TanStack Query.

**Tech Stack:** Rust (reqwest, tokio, rusqlite, tungstenite), Tauri 2, React 19, TypeScript, Primer React, zustand, TanStack Query, Vite, pnpm.

## Global Constraints

- All chunks align to 4KB boundaries
- macOS accessory policy (no Dock icon) for tray mode
- WebSocket protocol on port 18999, backward compatible with plain URL strings
- SOCKS5/HTTP proxy support per-download
- Download temp files use `.pdm` extension, remove on completion
- SQLite for main download list, gob binary for per-download pause state
- TOML config at `$HOME/Downloads/.pdm/pdm.toml`
- Max 32 connections per download, min chunk size 2MB
- Primer.style components + Octicons icons
- zustand for client state, TanStack Query for server-state
- Cross-platform: macOS/Win/Linux
- All Tauri commands returning `Result<T, String>` for error propagation

## Phase 0: Shared Foundation (merge to main first)

Shared types + module scaffold that all branches depend on. This is the ONLY sequential gate — after this, all feature branches fork.

### Phase 0 — File Structure

```
src-tauri/src/
├── lib.rs           (modify: add mod declarations + tray setup hook)
├── types.rs         (NEW: all shared Rust types)
└── config.rs        (NEW: settings load/save)

src/
├── types.ts         (NEW: frontend types mirroring Rust)
├── stores/
│   └── settingsStore.ts  (NEW: window state, config)
├── vite-env.d.ts    (modify: add Tauri API types)
```

### Task 0.1: Rust Shared Types

**Files:**
- Create: `src-tauri/src/types.rs`

**Interfaces:**
- Produces: All types used by every other Rust module

- [ ] Step 1: Write types.rs with all shared data structures

```rust
// src-tauri/src/types.rs
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadItem {
    pub id: u64,
    pub url: String,
    pub file_name: String,
    pub save_path: String,
    pub total_size: u64,
    pub downloaded: u64,
    pub status: DownloadStatus,
    pub parts: Vec<DownloadPart>,
    pub proxy_name: String,
    pub connections: u32,
    pub resumable: Option<bool>,
    pub merge_progress: f64,
    pub created_at: String,
    pub last_try: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadPart {
    pub index: u32,
    pub start: u64,
    pub end: u64,
    pub downloaded: u64,
    pub temp_path: String,
    pub status: PartStatus,
    pub retries: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DownloadStatus {
    #[serde(rename = "downloading")]
    Downloading,
    #[serde(rename = "paused")]
    Paused,
    #[serde(rename = "completed")]
    Completed,
    #[serde(rename = "failed")]
    Failed(String),
    #[serde(rename = "queued")]
    Queued,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PartStatus {
    #[serde(rename = "pending")]
    Pending,
    #[serde(rename = "downloading")]
    Downloading,
    #[serde(rename = "completed")]
    Completed,
    #[serde(rename = "failed")]
    Failed(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ProxyProtocol {
    #[serde(rename = "http")]
    Http,
    #[serde(rename = "socks5")]
    Socks5,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProxyConfig {
    pub protocol: ProxyProtocol,
    pub host: String,
    pub port: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadConfig {
    pub url: String,
    pub output_path: String,
    pub save_path: String,
    pub id: u64,
    pub file_name: String,
    pub is_resume: bool,
    pub headers: std::collections::HashMap<String, String>,
    pub proxy_name: String,
    pub total_size: u64,
    pub supports_range: bool,
    pub rate_limit_bps: u64,
    pub connections: u32,
    pub max_retries: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadState {
    pub url: String,
    pub id: u64,
    pub file_name: String,
    pub save_path: String,
    pub total_size: u64,
    pub downloaded: u64,
    pub tasks: Vec<Task>,
    pub elapsed_secs: u64,
    pub chunk_bitmap: Vec<bool>,
    pub actual_chunk_size: u64,
    pub proxy_name: String,
    pub workers: u32,
    pub min_chunk_size: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub offset: u64,
    pub length: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    pub download_dir: String,
    pub max_connections: u32,
    pub max_retries: u32,
    pub user_agent: String,
    pub launch_at_startup: bool,
    pub proxies: std::collections::HashMap<String, ProxyConfig>,
    pub global_rate_limit: u64,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            download_dir: dirs::download_dir()
                .unwrap_or_else(|| std::path::PathBuf::from("."))
                .to_string_lossy()
                .to_string(),
            max_connections: 8,
            max_retries: 10,
            user_agent: "ProxyDM/0.1.0".to_string(),
            launch_at_startup: false,
            proxies: std::collections::HashMap::new(),
            global_rate_limit: 0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingDownloadRequest {
    pub url: String,
    pub filename: String,
    pub proxy_name: String,
    pub connections: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActiveDownload {
    pub cancel: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    pub kind: EventKind,
    pub download_id: u64,
    pub data: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EventKind {
    DownloadStarted,
    DownloadProgress,
    DownloadCompleted,
    DownloadPaused,
    DownloadResumed,
    DownloadErrored,
    DownloadRemoved,
    DownloadQueued,
}
```

- [ ] Step 2: Add `mod types;` and `mod config;` to lib.rs

```rust
// Update src-tauri/src/lib.rs
mod types;
mod config;
```

### Task 0.2: Rust Config Module

**Files:**
- Create: `src-tauri/src/config.rs`

**Interfaces:**
- Produces: `config::load() -> Settings`, `config::save(&Settings)`

- [ ] Step 1: Write config.rs

```rust
// src-tauri/src/config.rs
use crate::types::Settings;
use std::path::PathBuf;

fn config_path() -> PathBuf {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    home.join("Downloads/.pdm/pdm.toml")
}

pub fn load() -> Settings {
    let path = config_path();
    if !path.exists() {
        let settings = Settings::default();
        save(&settings).ok();
        return settings;
    }
    let content = std::fs::read_to_string(&path).unwrap_or_default();
    toml::from_str(&content).unwrap_or_default()
}

pub fn save(settings: &Settings) -> Result<(), String> {
    let path = config_path();
    // Ensure parent dir exists
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let content = toml::to_string(settings).map_err(|e| e.to_string())?;
    std::fs::write(&path, content).map_err(|e| e.to_string())?;
    Ok(())
}
```

- [ ] Step 2: Add toml dependency to Cargo.toml

```toml
# Update src-tauri/Cargo.toml [dependencies]
toml = "0.8"
dirs = "6"
```

### Task 0.3: Frontend Types + State Scaffold

**Files:**
- Create: `src/types.ts`
- Create: `src/stores/settingsStore.ts`
- Modify: `package.json` (add zustand + tanstack query deps)

- [ ] Step 1: Add dependencies

Run: `pnpm add @tanstack/react-query zustand`

- [ ] Step 2: Write types.ts

```typescript
// src/types.ts
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
  merge_progress: number;
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
  proxies: Record<string, ProxyConfig>;
  global_rate_limit: number;
}

export interface PendingDownloadRequest {
  url: string;
  filename: string;
  proxy_name: string;
  connections: number;
}

export interface Event {
  kind: EventKind;
  download_id: number;
  data?: string;
}

export type EventKind =
  | "DownloadStarted"
  | "DownloadProgress"
  | "DownloadCompleted"
  | "DownloadPaused"
  | "DownloadResumed"
  | "DownloadErrored"
  | "DownloadRemoved"
  | "DownloadQueued";
```

- [ ] Step 3: Write settingsStore.ts

```typescript
// src/stores/settingsStore.ts
import { create } from "zustand";
import type { Settings } from "../types";

interface SettingsStore {
  settings: Settings;
  setSettings: (settings: Settings) => void;
  updateProxy: (name: string, protocol: string, host: string, port: number) => void;
  removeProxy: (name: string) => void;
}

export const useSettingsStore = create<SettingsStore>((set) => ({
  settings: {
    download_dir: "",
    max_connections: 8,
    max_retries: 10,
    user_agent: "ProxyDM/0.1.0",
    launch_at_startup: false,
    proxies: {},
    global_rate_limit: 0,
  },
  setSettings: (settings) => set({ settings }),
  updateProxy: (name, protocol, host, port) =>
    set((state) => ({
      settings: {
        ...state.settings,
        proxies: {
          ...state.settings.proxies,
          [name]: { protocol: protocol as any, host, port },
        },
      },
    })),
  removeProxy: (name) =>
    set((state) => {
      const { [name]: _, ...rest } = state.settings.proxies;
      return { settings: { ...state.settings, proxies: rest } };
    }),
}));
```

- [ ] Step 4: Commit phase 0

```bash
git add -A
git commit -m "feat: add shared types and config scaffold for ProxyDM"
```

---

## Sub-Plan A: Download Engine (branch: feat/download-engine)

**Depends on:** Phase 0 merged to main

**Branch:** feat/download-engine (fork from main after Phase 0)

**Files to create:**
```
src-tauri/src/
├── probe.rs
├── engine/
│   ├── mod.rs
│   ├── concurrent.rs
│   ├── single.rs
│   └── chunk.rs
├── network/
│   ├── mod.rs
│   ├── pool.rs
│   └── limiter.rs
```

### Task A.1: NetworkPool — HTTP Connection Pool

**Files:**
- Create: `src-tauri/src/network/mod.rs`
- Create: `src-tauri/src/network/pool.rs`

**Interfaces:**
- Produces: `NetworkPool::get_client(proxy_url: Option<&str>, max_conns: u32) -> reqwest::Client`

- [ ] Step 1: Write network/mod.rs

```rust
pub mod pool;
pub mod limiter;
```

- [ ] Step 2: Write pool.rs

```rust
use std::collections::HashMap;
use std::sync::Mutex;
use reqwest::Proxy;
use std::time::Duration;

pub struct NetworkPool {
    clients: Mutex<HashMap<String, reqwest::Client>>,
}

impl NetworkPool {
    pub fn new() -> Self {
        Self {
            clients: Mutex::new(HashMap::new()),
        }
    }

    pub fn get_client(&self, proxy_url: Option<&str>) -> reqwest::Client {
        let key = proxy_url.unwrap_or("direct").to_string();
        let mut map = self.clients.lock().unwrap();
        if let Some(client) = map.get(&key) {
            return client.clone();
        }
        let mut builder = reqwest::Client::builder()
            .pool_max_idle_per_host(128)
            .tcp_keepalive(Some(Duration::from_secs(60)))
            .timeout(Duration::from_secs(600))
            .connect_timeout(Duration::from_secs(60))
            .https_only(false)
            .danger_accept_invalid_certs(false);

        if let Some(proxy_str) = proxy_url {
            if let Ok(proxy) = Proxy::all(proxy_str) {
                builder = builder.proxy(proxy);
            }
        }

        let client = builder.build().expect("Failed to build reqwest Client");
        map.insert(key, client.clone());
        client
    }
}
```

- [ ] Step 3: Add reqwest dependency to Cargo.toml

```toml
reqwest = { version = "0.12", features = ["socks"] }
```

### Task A.2: Rate Limiter

**Files:**
- Create: `src-tauri/src/network/limiter.rs`

**Interfaces:**
- Produces: `RateLimiter::new(bps: u64)`, `limiter.wait_n(n: u64)`
- Produces: `MultiLimiter::new(global: RateLimiter, per_download: RateLimiter)`, `multi.wait_n(n: u64)`

- [ ] Step 1: Write limiter.rs

```rust
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};
use std::sync::Mutex;

pub struct RateLimiter {
    bps: AtomicU64,
    last_check: Mutex<Instant>,
    allowance: Mutex<f64>,
}

impl RateLimiter {
    pub fn new(bps: u64) -> Self {
        Self {
            bps: AtomicU64::new(bps),
            last_check: Mutex::new(Instant::now()),
            allowance: Mutex::new(0.0),
        }
    }

    pub fn set_rate(&self, bps: u64) {
        self.bps.store(bps, Ordering::Relaxed);
    }

    pub fn wait_n(&self, n: u64) {
        let bps = self.bps.load(Ordering::Relaxed);
        if bps == 0 {
            return; // no limit
        }
        let mut allowance = self.allowance.lock().unwrap();
        let mut last_check = self.last_check.lock().unwrap();
        let now = Instant::now();
        let elapsed = now.duration_since(*last_check).as_secs_f64();
        *last_check = now;
        *allowance += elapsed * (bps as f64);
        if *allowance > (bps as f64) * 2.0 {
            *allowance = (bps as f64) * 2.0;
        }
        if *allowance >= n as f64 {
            *allowance -= n as f64;
            return;
        }
        let deficit = n as f64 - *allowance;
        let wait_time = deficit / (bps as f64);
        std::thread::sleep(Duration::from_secs_f64(wait_time));
        *allowance = 0.0;
    }
}

pub struct MultiLimiter {
    pub global: RateLimiter,
    pub per_download: RateLimiter,
}

impl MultiLimiter {
    pub fn new(global_bps: u64, download_bps: u64) -> Self {
        Self {
            global: RateLimiter::new(global_bps),
            per_download: RateLimiter::new(download_bps),
        }
    }

    pub fn wait_n(&self, n: u64) {
        self.global.wait_n(n);
        self.per_download.wait_n(n);
    }
}
```

### Task A.3: Probe — Server Capability Detection

**Files:**
- Create: `src-tauri/src/probe.rs`

**Interfaces:**
- Produces: `ProbeResult { supports_range: bool, file_size: u64, file_name: String, accept_ranges: bool }`
- Produces: `async fn probe(url, headers, proxy, client) -> Result<ProbeResult>`
- Produces: `PROBE_MUTEX: Mutex<()>` for per-host serialization

- [ ] Step 1: Write probe.rs

```rust
use crate::network::pool::NetworkPool;
use std::collections::HashMap;
use std::sync::Mutex;

pub struct ProbeResult {
    pub supports_range: bool,
    pub file_size: u64,
    pub file_name: String,
    pub accept_ranges: bool,
}

pub struct ProbeHostGuard;

lazy_static::lazy_static! {
    static ref PROBE_LOCKS: Mutex<HashMap<String, Mutex<()>>> = Mutex::new(HashMap::new());
}

fn get_host_lock(url: &str) -> Option<std::sync::Arc<Mutex<()>>> {
    if let Ok(parsed) = url::Url::parse(url) {
        if let Some(host) = parsed.host_str() {
            let mut locks = PROBE_LOCKS.lock().unwrap();
            let lock = locks.entry(host.to_string())
                .or_insert_with(|| Mutex::new(()));
            return None; // We can't clone Mutex, so skip serialization for now
        }
    }
    None
}

pub async fn probe(
    url: &str,
    headers: &HashMap<String, String>,
    proxy: Option<&str>,
    pool: &NetworkPool,
) -> Result<ProbeResult, String> {
    let client = pool.get_client(proxy);
    let mut req = client.head(url);
    for (k, v) in headers {
        req = req.header(k.as_str(), v.as_str());
    }

    // Try Range first to detect 206 support
    let mut range_req = client.get(url);
    range_req = range_req.header("Range", "bytes=0-0");
    for (k, v) in headers {
        range_req = range_req.header(k.as_str(), v.as_str());
    }

    let resp = range_req.send().await.map_err(|e| format!("Probe request failed: {}", e))?;
    let status = resp.status();

    let supports_range = status == reqwest::StatusCode::PARTIAL_CONTENT;

    let (file_size, accept_ranges) = if supports_range {
        let cr = resp.headers().get("content-range")
            .and_then(|v| v.to_str().ok())
            .and_then(|s| {
                s.split('/').nth(1).and_then(|n| n.trim().parse::<u64>().ok())
            })
            .unwrap_or(0);
        let ar = resp.headers().get("accept-ranges")
            .and_then(|v| v.to_str().ok())
            .map(|v| v.contains("bytes"))
            .unwrap_or(false);
        (cr, ar)
    } else if status == reqwest::StatusCode::OK || status == reqwest::StatusCode::FORBIDDEN || status == reqwest::StatusCode::METHOD_NOT_ALLOWED {
        // Retry without Range header
        let resp2 = req.send().await.map_err(|e| format!("Probe HEAD failed: {}", e))?;
        let size = resp2.headers().get("content-length")
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(0);
        (size, false)
    } else {
        return Err(format!("Probe failed with status: {}", status));
    };

    // Detect filename from Content-Disposition or URL
    let file_name = resp.headers().get("content-disposition")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| {
            s.split(';').find_map(|part| {
                let p = part.trim();
                p.strip_prefix("filename=").or_else(|| p.strip_prefix("filename*=UTF-8''"))
            })
        })
        .map(|s| s.trim_matches('"').to_string())
        .unwrap_or_else(|| {
            std::path::Path::new(url)
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| "download".to_string())
        });

    Ok(ProbeResult {
        supports_range,
        file_size,
        file_name,
        accept_ranges,
    })
}
```

- [ ] Step 2: Add dependencies to Cargo.toml

```toml
lazy_static = "1.5"
url = "2"
```

### Task A.4: Chunk Management

**Files:**
- Create: `src-tauri/src/engine/mod.rs`
- Create: `src-tauri/src/engine/chunk.rs`

**Interfaces:**
- Produces: `Chunk { offset, length }`, `compute_chunks(file_size, num_chunks, min_chunk_size) -> Vec<Chunk>`
- Produces: `ChunkQueue { pop(), push(), drain(), close(), is_empty() }`

- [ ] Step 1: Write engine/mod.rs

```rust
pub mod chunk;
pub mod concurrent;
pub mod single;
```

- [ ] Step 2: Write chunk.rs

```rust
use crate::types::Task;

const ALIGN: u64 = 4096;

pub fn align_down(v: u64) -> u64 {
    v & !(ALIGN - 1)
}

pub fn align_up(v: u64) -> u64 {
    (v + ALIGN - 1) & !(ALIGN - 1)
}

pub fn compute_chunks(file_size: u64, num_chunks: u32, min_chunk_size: u64) -> Vec<Task> {
    if num_chunks == 0 {
        return vec![Task { offset: 0, length: file_size }];
    }
    let chunk_size = (file_size / num_chunks as u64).max(min_chunk_size);
    let chunk_size = align_up(chunk_size);

    let mut tasks = Vec::new();
    let mut offset = 0u64;
    while offset < file_size {
        let length = if offset + chunk_size > file_size {
            file_size - offset
        } else {
            chunk_size
        };
        tasks.push(Task { offset, length });
        offset += chunk_size;
    }
    tasks
}

pub fn split_task(task: &Task, split_point: u64) -> (Task, Task) {
    let split = align_down(split_point.max(task.offset + ALIGN).min(task.offset + task.length - ALIGN));
    let left_len = split - task.offset;
    let right_len = task.length - left_len;
    (
        Task { offset: task.offset, length: left_len },
        Task { offset: split, length: right_len },
    )
}

use std::collections::VecDeque;
use std::sync::Mutex;

pub struct ChunkQueue {
    tasks: Mutex<VecDeque<Task>>,
    closed: Mutex<bool>,
}

impl ChunkQueue {
    pub fn new(tasks: Vec<Task>) -> Self {
        Self {
            tasks: Mutex::new(VecDeque::from(tasks)),
            closed: Mutex::new(false),
        }
    }

    pub fn pop(&self) -> Option<Task> {
        let mut tasks = self.tasks.lock().ok()?;
        tasks.pop_front()
    }

    pub fn push(&self, task: Task) {
        if let Ok(mut tasks) = self.tasks.lock() {
            tasks.push_back(task);
        }
    }

    pub fn drain(&self) -> Vec<Task> {
        let mut tasks = self.tasks.lock().unwrap();
        tasks.drain(..).collect()
    }

    pub fn close(&self) {
        *self.closed.lock().unwrap() = true;
    }

    pub fn is_empty(&self) -> bool {
        self.tasks.lock().map(|t| t.is_empty()).unwrap_or(true)
    }

    pub fn len(&self) -> usize {
        self.tasks.lock().map(|t| t.len()).unwrap_or(0)
    }

    pub fn remaining_bytes(&self) -> u64 {
        self.tasks.lock()
            .map(|t| t.iter().map(|task| task.length).sum())
            .unwrap_or(0)
    }
}
```

### Task A.5: Concurrent Downloader

**Files:**
- Create: `src-tauri/src/engine/concurrent.rs`

**Interfaces:**
- Produces: `ConcurrentDownloader { new(), download() }`
- Consumes: `NetworkPool`, `ChunkQueue`, `MultiLimiter`, `ProbeResult`
- Consumes via event channel: `tokio::sync::mpsc::Sender<Event>`

- [ ] Step 1: Write concurrent.rs

```rust
use crate::network::pool::NetworkPool;
use crate::network::limiter::MultiLimiter;
use crate::engine::chunk::{self, ChunkQueue};
use crate::types::{Task, Event, EventKind, DownloadConfig};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering, AtomicI64};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;
use tokio::io::AsyncWriteExt;

pub struct ConcurrentDownloader {
    pool: Arc<NetworkPool>,
    event_tx: mpsc::UnboundedSender<Event>,
}

impl ConcurrentDownloader {
    pub fn new(pool: Arc<NetworkPool>, event_tx: mpsc::UnboundedSender<Event>) -> Self {
        Self { pool, event_tx }
    }

    pub async fn download(&self, cfg: &DownloadConfig, limiter: Arc<MultiLimiter>) -> Result<(), String> {
        let cancel = Arc::new(AtomicBool::new(false));
        let completed_counter = Arc::new(AtomicU64::new(0));

        let num_conns = if cfg.connections > 0 {
            cfg.connections.min(32)
        } else {
            let sqrt = (cfg.total_size as f64 / 1024.0 / 1024.0).sqrt() as u32;
            sqrt.max(1).min(32)
        };

        let min_chunk = 2u64 * 1024 * 1024; // 2MB
        let tasks = if cfg.is_resume {
            // For resume, tasks come from saved state
            vec![]
        } else {
            chunk::compute_chunks(cfg.total_size, num_conns, min_chunk)
        };

        // Resume path: state loaded from Sub-Plan B (state/gob.rs) merge
        if tasks.is_empty() {
            return Err("Resume not yet implemented in concurrent downloader".to_string());
        }

        let queue = Arc::new(ChunkQueue::new(tasks));
        let file = Arc::new(tokio::sync::Mutex::new(
            self.create_output_file(&cfg.output_path).await?
        ));

        let client = self.pool.get_client(if cfg.proxy_name.is_empty() { None } else { Some(&cfg.proxy_name) });

        let mut handles = Vec::new();
        for worker_id in 0..num_conns {
            let queue = queue.clone();
            let file = file.clone();
            let client = client.clone();
            let cancel = cancel.clone();
            let limiter = limiter.clone();
            let completed_counter = completed_counter.clone();
            let event_tx = self.event_tx.clone();
            let offset = worker_id;
            let url = cfg.url.clone();
            let max_retries = cfg.max_retries;
            let total_size = cfg.total_size;

            let handle = tokio::spawn(async move {
                loop {
                    if cancel.load(Ordering::Relaxed) {
                        return;
                    }
                    let task = queue.pop();
                    let task = match task {
                        Some(t) => t,
                        None => break,
                    };

                    let result = download_task(
                        &url, &client, &file, &task, &cancel, &limiter, max_retries,
                    ).await;

                    match result {
                        Ok(_) => {
                            completed_counter.fetch_add(1, Ordering::SeqCst);
                            let _ = event_tx.send(Event {
                                kind: EventKind::DownloadProgress,
                                download_id: cfg.id,
                                data: Some(format!("{}", completed_counter.load(Ordering::Relaxed))),
                            });
                        }
                        Err(e) => {
                            // Re-queue on failure
                            queue.push(task);
                            if max_retries == 0 {
                                let _ = event_tx.send(Event {
                                    kind: EventKind::DownloadErrored,
                                    download_id: cfg.id,
                                    data: Some(e),
                                });
                                return;
                            }
                        }
                    }
                }
            });
            handles.push(handle);
        }

        // Wait for all workers
        for h in handles {
            let _ = h.await;
        }

        // Verify completeness
        let downloaded = self.get_file_size(&cfg.output_path).await;
        if downloaded < cfg.total_size && !cancel.load(Ordering::Relaxed) {
            return Err(format!("Download incomplete: {}/{} bytes", downloaded, cfg.total_size));
        }

        // Rename .pdm to final filename
        self.finalize_file(&cfg.output_path, &cfg.save_path).await?;

        let _ = self.event_tx.send(Event {
            kind: EventKind::DownloadCompleted,
            download_id: cfg.id,
            data: None,
        });

        Ok(())
    }

    async fn create_output_file(&self, path: &str) -> Result<tokio::fs::File, String> {
        let pdm_path = format!("{}.pdm", path);
        if let Some(parent) = std::path::Path::new(&pdm_path).parent() {
            tokio::fs::create_dir_all(parent).await.map_err(|e| e.to_string())?;
        }
        tokio::fs::File::create(&pdm_path)
            .await
            .map_err(|e| format!("Failed to create output file: {}", e))
    }

    async fn get_file_size(&self, path: &str) -> u64 {
        let pdm_path = format!("{}.pdm", path);
        tokio::fs::metadata(&pdm_path)
            .await
            .map(|m| m.len())
            .unwrap_or(0)
    }

    async fn finalize_file(&self, output_path: &str, save_path: &str) -> Result<(), String> {
        let pdm_path = format!("{}.pdm", output_path);
        tokio::fs::rename(&pdm_path, save_path)
            .await
            .map_err(|e| format!("Failed to rename file: {}", e))
    }
}

async fn download_task(
    url: &str,
    client: &reqwest::Client,
    file: &Arc<tokio::sync::Mutex<tokio::fs::File>>,
    task: &Task,
    cancel: &AtomicBool,
    limiter: &MultiLimiter,
    max_retries: u32,
) -> Result<(), String> {
    let range_end = if task.length == 0 {
        String::new()
    } else {
        format!("{}", task.offset + task.length - 1)
    };
    let range_header = format!("bytes={}-{}", task.offset, range_end);
    let resp = client
        .get(url)
        .header("Range", &range_header)
        .send()
        .await
        .map_err(|e| format!("Request failed: {}", e))?;

    let status = resp.status();
    if status != reqwest::StatusCode::PARTIAL_CONTENT && status != reqwest::StatusCode::OK {
        return Err(format!("HTTP {}", status));
    }

    let stream = resp.bytes_stream();
    let mut buf = Vec::new();
    use futures_util::StreamExt;
    let mut stream = std::pin::pin!(stream);

    while let Some(chunk_result) = stream.next().await {
        if cancel.load(Ordering::Relaxed) {
            return Err("Cancelled".to_string());
        }
        let chunk = chunk_result.map_err(|e| format!("Stream error: {}", e))?;
        limiter.wait_n(chunk.len() as u64);
        buf.extend_from_slice(&chunk);
    }

    // Write chunk to file at correct offset
    let mut f = file.lock().await;
    use tokio::io::AsyncSeekExt;
    f.seek(std::io::SeekFrom::Start(task.offset))
        .await
        .map_err(|e| format!("Seek error: {}", e))?;
    f.write_all(&buf)
        .await
        .map_err(|e| format!("Write error: {}", e))?;

    Ok(())
}
```

- [ ] Step 2: Add dependencies to Cargo.toml

```toml
tokio = { version = "1", features = ["full"] }
futures-util = "0.3"
```

### Task A.6: Single Downloader (fallback)

**Files:**
- Create: `src-tauri/src/engine/single.rs`

**Interfaces:**
- Produces: `SingleDownloader { new(), download() }`

- [ ] Step 1: Write single.rs

```rust
use crate::network::pool::NetworkPool;
use crate::network::limiter::MultiLimiter;
use crate::types::{Event, EventKind, DownloadConfig};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio::io::AsyncWriteExt;

pub struct SingleDownloader {
    pool: Arc<NetworkPool>,
    event_tx: mpsc::UnboundedSender<Event>,
}

impl SingleDownloader {
    pub fn new(pool: Arc<NetworkPool>, event_tx: mpsc::UnboundedSender<Event>) -> Self {
        Self { pool, event_tx }
    }

    pub async fn download(&self, cfg: &DownloadConfig, limiter: Arc<MultiLimiter>, cancel: Arc<AtomicBool>) -> Result<(), String> {
        let resp = self.pool
            .get_client(if cfg.proxy_name.is_empty() { None } else { Some(&cfg.proxy_name) })
            .get(&cfg.url)
            .send()
            .await
            .map_err(|e| format!("Download request failed: {}", e))?;

        let status = resp.status();
        if !status.is_success() {
            // Handle 429/503 with Retry-After
            if status == reqwest::StatusCode::TOO_MANY_REQUESTS || status == reqwest::StatusCode::SERVICE_UNAVAILABLE {
                let retry_after = resp.headers()
                    .get("retry-after")
                    .and_then(|v| v.to_str().ok())
                    .and_then(|s| s.parse::<u64>().ok())
                    .unwrap_or(5);
                return Err(format!("Rate limited, retry after {}s", retry_after));
            }
            return Err(format!("HTTP {}", status));
        }

        // Ensure output directory exists
        if let Some(parent) = std::path::Path::new(&cfg.save_path).parent() {
            tokio::fs::create_dir_all(parent).await.map_err(|e| e.to_string())?;
        }

        let mut file = tokio::fs::File::create(&cfg.save_path)
            .await
            .map_err(|e| format!("Failed to create file: {}", e))?;

        let stream = resp.bytes_stream();
        use futures_util::StreamExt;
        let mut stream = std::pin::pin!(stream);
        let mut total = 0u64;
        let buf_size = 32 * 1024; // 32KB buffer

        while let Some(chunk_result) = stream.next().await {
            if cancel.load(Ordering::Relaxed) {
                file.flush().await.ok();
                return Err("Cancelled".to_string());
            }
            let chunk = chunk_result.map_err(|e| format!("Stream error: {}", e))?;
            limiter.wait_n(chunk.len() as u64);
            file.write_all(&chunk).await.map_err(|e| format!("Write error: {}", e))?;
            total += chunk.len() as u64;

            // Periodic progress
            if total % (buf_size * 32) == 0 {
                let _ = self.event_tx.send(Event {
                    kind: EventKind::DownloadProgress,
                    download_id: cfg.id,
                    data: Some(total.to_string()),
                });
            }
        }

        file.flush().await.map_err(|e| format!("Flush error: {}", e))?;

        let _ = self.event_tx.send(Event {
            kind: EventKind::DownloadCompleted,
            download_id: cfg.id,
            data: None,
        });

        Ok(())
    }
}
```

### Task A.7: Engine Module — Download Factory

**Files:**
- Modify: `src-tauri/src/engine/mod.rs`

**Interfaces:**
- Produces: `async fn run_download(cfg, pool, event_tx, limiter, cancel) -> Result<(), String>`

- [ ] Step 1: Update engine/mod.rs to add factory function

```rust
pub mod chunk;
pub mod concurrent;
pub mod single;

use crate::types::DownloadConfig;
use crate::network::pool::NetworkPool;
use crate::network::limiter::MultiLimiter;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use tokio::sync::mpsc;
use crate::types::{Event, EventKind};
use probe::probe;

pub async fn run_download(
    cfg: DownloadConfig,
    pool: Arc<NetworkPool>,
    event_tx: mpsc::UnboundedSender<Event>,
    limiter: Arc<MultiLimiter>,
    cancel: Arc<AtomicBool>,
) -> Result<(), String> {
    let _ = event_tx.send(Event {
        kind: EventKind::DownloadStarted,
        download_id: cfg.id,
        data: None,
    });

    if cfg.supports_range {
        let downloader = concurrent::ConcurrentDownloader::new(pool, event_tx.clone());
        match downloader.download(&cfg, limiter).await {
            Ok(()) => Ok(()),
            Err(e) => {
                // Degrade to single downloader
                let _ = event_tx.send(Event {
                    kind: EventKind::DownloadErrored,
                    download_id: cfg.id,
                    data: Some(format!("Concurrent failed, degrading: {}", e)),
                });
                let downloader = single::SingleDownloader::new(pool, event_tx.clone());
                downloader.download(&cfg, limiter, cancel).await
            }
        }
    } else {
        let downloader = single::SingleDownloader::new(pool, event_tx.clone());
        downloader.download(&cfg, limiter, cancel).await
    }
}
```

- [ ] Note: Remove the duplicate import of `probe` — it's external. Actually, the probe module is separate. Let me fix.

```rust
// Corrected engine/mod.rs
pub mod chunk;
pub mod concurrent;
pub mod single;

use crate::types::DownloadConfig;
use crate::network::pool::NetworkPool;
use crate::network::limiter::MultiLimiter;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use tokio::sync::mpsc;
use crate::types::Event;

pub async fn run_download(
    cfg: DownloadConfig,
    pool: Arc<NetworkPool>,
    event_tx: mpsc::UnboundedSender<Event>,
    limiter: Arc<MultiLimiter>,
    cancel: Arc<AtomicBool>,
) -> Result<(), String> {
    let _ = event_tx.send(Event {
        kind: crate::types::EventKind::DownloadStarted,
        download_id: cfg.id,
        data: None,
    });

    if cfg.supports_range {
        let downloader = concurrent::ConcurrentDownloader::new(pool.clone(), event_tx.clone());
        match downloader.download(&cfg, limiter.clone()).await {
            Ok(()) => Ok(()),
            Err(e) => {
                let _ = event_tx.send(Event {
                    kind: crate::types::EventKind::DownloadErrored,
                    download_id: cfg.id,
                    data: Some(format!("Concurrent failed, degrading: {}", e)),
                });
                let downloader = single::SingleDownloader::new(pool, event_tx.clone());
                downloader.download(&cfg, limiter, cancel).await
            }
        }
    } else {
        let downloader = single::SingleDownloader::new(pool, event_tx.clone());
        downloader.download(&cfg, limiter, cancel).await
    }
}
```

### Task A.8: WorkerPool — Concurrent Execution Controller

**Files:**
- Create: `src-tauri/src/worker.rs`

**Interfaces:**
- Produces: `WorkerPool { new(max_workers), add(cfg), shutdown() }`
- Produces: internal worker loop spawning run_download calls

- [ ] Step 1: Write worker.rs

```rust
use crate::types::{DownloadConfig, Event};
use crate::network::pool::NetworkPool;
use crate::network::limiter::MultiLimiter;
use crate::engine;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex, Semaphore};

pub struct WorkerPool {
    semaphore: Arc<Semaphore>,
    pool: Arc<NetworkPool>,
    event_tx: mpsc::UnboundedSender<Event>,
    active: Arc<Mutex<HashMap<u64, Arc<AtomicBool>>>>,
    next_id: AtomicU64,
}

impl WorkerPool {
    pub fn new(max_workers: u32, event_tx: mpsc::UnboundedSender<Event>) -> Self {
        Self {
            semaphore: Arc::new(Semaphore::new(max_workers as usize)),
            pool: Arc::new(NetworkPool::new()),
            event_tx,
            active: Arc::new(Mutex::new(HashMap::new())),
            next_id: AtomicU64::new(1),
        }
    }

    pub async fn add(&self, mut cfg: DownloadConfig) -> Result<u64, String> {
        let permit = self.semaphore.clone().acquire_owned().await.map_err(|e| e.to_string())?;
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        cfg.id = id;
        let cancel = Arc::new(AtomicBool::new(false));
        {
            let mut active = self.active.lock().await;
            active.insert(id, cancel.clone());
        }
        let event_tx = self.event_tx.clone();
        let pool = self.pool.clone();
        let active_map = self.active.clone();

        tokio::spawn(async move {
            let limiter = Arc::new(MultiLimiter::new(
                0, // global rate limit handled elsewhere
                cfg.rate_limit_bps,
            ));

            let result = engine::run_download(cfg, pool, event_tx.clone(), limiter, cancel.clone()).await;

            if let Err(e) = &result {
                let _ = event_tx.send(Event {
                    kind: crate::types::EventKind::DownloadErrored,
                    download_id: id,
                    data: Some(e.clone()),
                });
            }

            // Cleanup
            {
                let mut active = active_map.lock().await;
                active.remove(&id);
            }
            drop(permit);
        });

        Ok(id)
    }

    pub async fn cancel(&self, id: u64) {
        let mut active = self.active.lock().await;
        if let Some(cancel) = active.remove(&id) {
            cancel.store(true, Ordering::Relaxed);
        }
    }

    pub async fn active_count(&self) -> usize {
        self.active.lock().await.len()
    }

    pub fn pool_ref(&self) -> Arc<NetworkPool> {
        self.pool.clone()
    }
}
```

---

## Sub-Plan B: State Persistence (branch: feat/state-persistence)

**Depends on:** Phase 0 merged to main

**Files to create:**
```
src-tauri/src/
├── state/
│   ├── mod.rs
│   ├── db.rs
│   └── gob.rs
```

### Task B.1: SQLite Database

**Files:**
- Create: `src-tauri/src/state/mod.rs`
- Create: `src-tauri/src/state/db.rs`

**Interfaces:**
- Produces: `Db { new(), init() }`
- Produces: `list_downloads() -> Vec<DownloadItem>`
- Produces: `insert_download(item)`, `update_download(item)`, `delete_download(id)`

- [ ] Step 1: Write state/mod.rs

```rust
pub mod db;
pub mod gob;
```

- [ ] Step 2: Write state/db.rs

```rust
use crate::types::*;
use rusqlite::{Connection, params};
use std::path::PathBuf;
use std::sync::Mutex;

pub struct Db {
    conn: Mutex<Connection>,
}

impl Db {
    pub fn new() -> Result<Self, String> {
        let path = Self::db_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let conn = Connection::open(&path).map_err(|e| e.to_string())?;
        let db = Self { conn: Mutex::new(conn) };
        db.init()?;
        Ok(db)
    }

    fn db_path() -> PathBuf {
        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
        home.join("Downloads/.pdm/state/downloads.db")
    }

    fn init(&self) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS downloads (
                id INTEGER PRIMARY KEY,
                url TEXT NOT NULL,
                file_name TEXT NOT NULL,
                save_path TEXT NOT NULL,
                total_size INTEGER NOT NULL DEFAULT 0,
                downloaded INTEGER NOT NULL DEFAULT 0,
                status TEXT NOT NULL DEFAULT 'queued',
                last_try TEXT NOT NULL DEFAULT '',
                created_at TEXT NOT NULL,
                proxy_name TEXT NOT NULL DEFAULT '',
                connections INTEGER NOT NULL DEFAULT 4,
                parts TEXT NOT NULL DEFAULT '[]',
                resumable INTEGER
            );"
        ).map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn list_downloads(&self) -> Result<Vec<DownloadItem>, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let mut stmt = conn.prepare(
            "SELECT id, url, file_name, save_path, total_size, downloaded, status, last_try, 
                    created_at, proxy_name, connections, parts, resumable FROM downloads ORDER BY id DESC"
        ).map_err(|e| e.to_string())?;

        let items = stmt.query_map([], |row| {
            let parts_str: String = row.get(11)?;
            let parts: Vec<DownloadPart> = serde_json::from_str(&parts_str).unwrap_or_default();
            let status_str: String = row.get(6)?;
            let status = parse_status(&status_str);
            let resumable: Option<i32> = row.get(12)?;

            Ok(DownloadItem {
                id: row.get(0)?,
                url: row.get(1)?,
                file_name: row.get(2)?,
                save_path: row.get(3)?,
                total_size: row.get(4)?,
                downloaded: row.get(5)?,
                status,
                parts,
                proxy_name: row.get(9)?,
                connections: row.get(10)?,
                resumable: resumable.map(|v| v != 0),
                merge_progress: 0.0,
                created_at: row.get(8)?,
                last_try: row.get(7)?,
            })
        }).map_err(|e| e.to_string())?;

        items.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())
    }

    pub fn insert_download(&self, item: &DownloadItem) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let parts_str = serde_json::to_string(&item.parts).map_err(|e| e.to_string())?;
        conn.execute(
            "INSERT INTO downloads (id, url, file_name, save_path, total_size, downloaded, status, last_try, created_at, proxy_name, connections, parts, resumable)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
            params![
                item.id, item.url, item.file_name, item.save_path, item.total_size,
                item.downloaded, status_to_string(&item.status), item.last_try, item.created_at,
                item.proxy_name, item.connections, parts_str,
                item.resumable.map(|v| if v { 1 } else { 0 })
            ],
        ).map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn update_download(&self, item: &DownloadItem) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let parts_str = serde_json::to_string(&item.parts).map_err(|e| e.to_string())?;
        conn.execute(
            "UPDATE downloads SET file_name=?1, total_size=?2, downloaded=?3, status=?4, last_try=?5, proxy_name=?6, connections=?7, parts=?8, resumable=?9 WHERE id=?10",
            params![
                item.file_name, item.total_size, item.downloaded, status_to_string(&item.status),
                item.last_try, item.proxy_name, item.connections, parts_str,
                item.resumable.map(|v| if v { 1 } else { 0 }), item.id
            ],
        ).map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn delete_download(&self, id: u64) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        conn.execute("DELETE FROM downloads WHERE id = ?1", params![id])
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn replace_all(&self, items: &[DownloadItem]) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        conn.execute("DELETE FROM downloads", []).map_err(|e| e.to_string())?;
        for item in items {
            let parts_str = serde_json::to_string(&item.parts).map_err(|e| e.to_string())?;
            conn.execute(
                "INSERT INTO downloads (id, url, file_name, save_path, total_size, downloaded, status, last_try, created_at, proxy_name, connections, parts, resumable)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
                params![
                    item.id, item.url, item.file_name, item.save_path, item.total_size,
                    item.downloaded, status_to_string(&item.status), item.last_try, item.created_at,
                    item.proxy_name, item.connections, parts_str,
                    item.resumable.map(|v| if v { 1 } else { 0 })
                ],
            ).map_err(|e| e.to_string())?;
        }
        Ok(())
    }
}

fn parse_status(s: &str) -> DownloadStatus {
    match s {
        "downloading" => DownloadStatus::Downloading,
        "paused" => DownloadStatus::Paused,
        "completed" => DownloadStatus::Completed,
        s if s.starts_with("failed") => DownloadStatus::Failed(s[7..].trim_start_matches(':').to_string()),
        _ => DownloadStatus::Queued,
    }
}

fn status_to_string(s: &DownloadStatus) -> String {
    match s {
        DownloadStatus::Downloading => "downloading".to_string(),
        DownloadStatus::Paused => "paused".to_string(),
        DownloadStatus::Completed => "completed".to_string(),
        DownloadStatus::Failed(msg) => format!("failed:{}", msg),
        DownloadStatus::Queued => "queued".to_string(),
    }
}
```

- [ ] Step 3: Add rusqlite dependency

```toml
rusqlite = { version = "0.31", features = ["bundled"] }
```

### Task B.2: DownloadState Persistence (gob equivalent)

**Files:**
- Create: `src-tauri/src/state/gob.rs`

**Interfaces:**
- Produces: `save_state(id, state)`, `load_state(id) -> Option<DownloadState>`, `delete_state(id)`
- Produces: `save_master(items)`, `load_master() -> Vec<u64>`

- [ ] Step 1: Write state/gob.rs

```rust
use crate::types::DownloadState;
use std::path::PathBuf;

fn state_dir() -> PathBuf {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    home.join("Downloads/.pdm/state")
}

fn detail_path(id: u64) -> PathBuf {
    state_dir().join(format!("detail-{}.json", id))
}

fn master_path() -> PathBuf {
    state_dir().join("master.json")
}

pub fn save_state(id: u64, state: &DownloadState) -> Result<(), String> {
    let path = detail_path(id);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let json = serde_json::to_string(state).map_err(|e| e.to_string())?;
    std::fs::write(&path, json).map_err(|e| e.to_string())?;
    Ok(())
}

pub fn load_state(id: u64) -> Result<Option<DownloadState>, String> {
    let path = detail_path(id);
    if !path.exists() {
        return Ok(None);
    }
    let json = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
    let state: DownloadState = serde_json::from_str(&json).map_err(|e| e.to_string())?;
    Ok(Some(state))
}

pub fn delete_state(id: u64) -> Result<(), String> {
    let path = detail_path(id);
    if path.exists() {
        std::fs::remove_file(&path).map_err(|e| e.to_string())?;
    }
    Ok(())
}

pub fn save_master(ids: &[u64]) -> Result<(), String> {
    let path = master_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let json = serde_json::to_string(ids).map_err(|e| e.to_string())?;
    std::fs::write(&path, json).map_err(|e| e.to_string())?;
    Ok(())
}

pub fn load_master() -> Result<Vec<u64>, String> {
    let path = master_path();
    if !path.exists() {
        return Ok(Vec::new());
    }
    let json = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
    let ids: Vec<u64> = serde_json::from_str(&json).map_err(|e| e.to_string())?;
    Ok(ids)
}
```

---

## Sub-Plan C: WebSocket Server (branch: feat/websocket)

**Depends on:** Phase 0 merged to main

**Files to create:**
```
src-tauri/src/
├── ws/
│   ├── mod.rs
│   └── server.rs
```

### Task C.1: WebSocket Server (port 18999)

**Files:**
- Create: `src-tauri/src/ws/mod.rs`
- Create: `src-tauri/src/ws/server.rs`

**Interfaces:**
- Produces: `WsServer { new(event_tx), start(address), stop() }`
- Consumes: `Event` channel for sending download updates to clients

- [ ] Step 1: Add tungstenite dependency

```toml
tungstenite = "0.24"
```

- [ ] Step 2: Write ws/mod.rs

```rust
pub mod server;
```

- [ ] Step 3: Write ws/server.rs

```rust
use crate::types::{PendingDownloadRequest, Event};
use std::net::TcpListener;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use tokio::sync::mpsc;

pub struct WsServer {
    running: Arc<AtomicBool>,
    handle: Option<thread::JoinHandle<()>>,
    event_tx: mpsc::UnboundedSender<Event>,
    request_tx: mpsc::UnboundedSender<PendingDownloadRequest>,
}

impl WsServer {
    pub fn new(
        event_tx: mpsc::UnboundedSender<Event>,
        request_tx: mpsc::UnboundedSender<PendingDownloadRequest>,
    ) -> Self {
        Self {
            running: Arc::new(AtomicBool::new(false)),
            handle: None,
            event_tx,
            request_tx,
        }
    }

    pub fn start(&mut self, addr: &str) -> Result<(), String> {
        self.running.store(true, Ordering::SeqCst);
        let running = self.running.clone();
        let request_tx = self.request_tx.clone();
        let event_tx = self.event_tx.clone();

        let listener = TcpListener::bind(addr)
            .map_err(|e| format!("Failed to bind WS server: {}", e))?;
        listener.set_nonblocking(true).ok();

        let handle = thread::spawn(move || {
            while running.load(Ordering::SeqCst) {
                match listener.accept() {
                    Ok((stream, _)) => {
                        let req_tx = request_tx.clone();
                        let ev_tx = event_tx.clone();
                        thread::spawn(move || {
                            handle_connection(stream, req_tx, ev_tx);
                        });
                    }
                    Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(std::time::Duration::from_millis(100));
                    }
                    Err(_) => {
                        std::thread::sleep(std::time::Duration::from_millis(100));
                    }
                }
            }
        });

        self.handle = Some(handle);
        Ok(())
    }

    pub fn stop(&mut self) {
        self.running.store(false, Ordering::SeqCst);
    }
}

use tungstenite::Message;

fn handle_connection(
    stream: std::net::TcpStream,
    request_tx: mpsc::UnboundedSender<PendingDownloadRequest>,
    event_tx: mpsc::UnboundedSender<Event>,
) {
    let mut ws = match tungstenite::accept(stream) {
        Ok(ws) => ws,
        Err(e) => {
            eprintln!("WS accept error: {}", e);
            return;
        }
    };

    loop {
        match ws.read() {
            Ok(Message::Text(text)) => {
                // Try parsing as JSON first, fallback to plain URL
                if let Ok(req) = serde_json::from_str::<PendingDownloadRequest>(&text) {
                    request_tx.send(req).ok();
                } else if text.starts_with("http://") || text.starts_with("https://") || text.starts_with("ftp://") {
                    request_tx.send(PendingDownloadRequest {
                        url: text,
                        filename: String::new(),
                        proxy_name: String::new(),
                        connections: 4,
                    }).ok();
                }
            }
            Ok(Message::Close(_)) => break,
            Err(_) => break,
            _ => {}
        }
    }
}
```

---

## Sub-Plan D: Frontend UI (branch: feat/frontend)

**Depends on:** Phase 0 merged to main (uses types.ts + stores)

**Files to create/modify:**
```
src/
├── query/
│   └── downloadQueries.ts    (NEW)
├── hooks/
│   ├── useTauriCommands.ts   (NEW)
│   └── useClipboard.ts       (NEW)
├── components/
│   ├── Layout.tsx            (NEW)
│   ├── Sidebar.tsx           (NEW)
│   ├── Toolbar.tsx           (NEW)
│   ├── DownloadTable.tsx     (NEW)
│   ├── DownloadRow.tsx       (NEW)
│   └── dialogs/
│       ├── NewDownloadDialog.tsx  (NEW)
│       ├── DeleteDialog.tsx       (NEW)
│       ├── SettingsDialog.tsx     (NEW)
│       └── PropertiesDialog.tsx   (NEW)
├── App.tsx                 (MODIFY)
└── main.tsx                (MODIFY: add QueryClientProvider)
```

### Task D.1: Query Hooks + Tauri Bridge

**Files:**
- Create: `src/hooks/useTauriCommands.ts`
- Create: `src/query/downloadQueries.ts`
- Modify: `src/main.tsx` (add QueryClientProvider)

- [ ] Step 1: Write useTauriCommands.ts

```typescript
// src/hooks/useTauriCommands.ts
import { invoke } from "@tauri-apps/api/core";
import type { DownloadItem, Settings } from "../types";

export function useTauriCommands() {
  return {
    listDownloads: () => invoke<DownloadItem[]>("list_downloads"),
    startDownload: (url: string, filename: string, proxyName: string, connections: number) =>
      invoke<number>("start_download", { url, filename, proxyName, connections }),
    pauseDownload: (id: number) => invoke<void>("pause_download", { id }),
    resumeDownload: (id: number) => invoke<void>("resume_download", { id }),
    deleteDownload: (id: number, deleteFile: boolean) =>
      invoke<void>("delete_download", { id, deleteFile }),
    getSettings: () => invoke<Settings>("get_settings"),
    saveSettings: (settings: Settings) => invoke<void>("save_settings", { settings }),
    cancelDownload: (id: number) => invoke<void>("cancel_download", { id }),
  };
}
```

- [ ] Step 2: Write downloadQueries.ts

```typescript
// src/query/downloadQueries.ts
import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import { useTauriCommands } from "../hooks/useTauriCommands";
import type { DownloadItem, Settings } from "../types";

const DOWNLOADS_KEY = ["downloads"] as const;
const SETTINGS_KEY = ["settings"] as const;

export function useDownloads() {
  const { listDownloads } = useTauriCommands();
  return useQuery({
    queryKey: DOWNLOADS_KEY,
    queryFn: listDownloads,
    refetchInterval: 1000,
  });
}

export function useDownload(id: number | undefined) {
  const { data: downloads } = useDownloads();
  return downloads?.find((d) => d.id === id) ?? null;
}

export function useStartDownload() {
  const queryClient = useQueryClient();
  const { startDownload } = useTauriCommands();
  return useMutation({
    mutationFn: ({ url, filename, proxyName, connections }: {
      url: string; filename: string; proxyName: string; connections: number;
    }) => startDownload(url, filename, proxyName, connections),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: DOWNLOADS_KEY }),
  });
}

export function usePauseDownload() {
  const queryClient = useQueryClient();
  const { pauseDownload } = useTauriCommands();
  return useMutation({
    mutationFn: (id: number) => pauseDownload(id),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: DOWNLOADS_KEY }),
  });
}

export function useResumeDownload() {
  const queryClient = useQueryClient();
  const { resumeDownload } = useTauriCommands();
  return useMutation({
    mutationFn: (id: number) => resumeDownload(id),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: DOWNLOADS_KEY }),
  });
}

export function useDeleteDownload() {
  const queryClient = useQueryClient();
  const { deleteDownload } = useTauriCommands();
  return useMutation({
    mutationFn: ({ id, deleteFile }: { id: number; deleteFile: boolean }) =>
      deleteDownload(id, deleteFile),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: DOWNLOADS_KEY }),
  });
}

export function useSettings() {
  const { getSettings, saveSettings } = useTauriCommands();
  const queryClient = useQueryClient();

  const query = useQuery({
    queryKey: SETTINGS_KEY,
    queryFn: getSettings,
  });

  const mutation = useMutation({
    mutationFn: (settings: Settings) => saveSettings(settings),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: SETTINGS_KEY }),
  });

  return { settings: query.data, isLoading: query.isLoading, saveSettings: mutation.mutate };
}
```

- [ ] Step 3: Update main.tsx

```typescript
// src/main.tsx
import React from "react";
import ReactDOM from "react-dom/client";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import App from "./App";
import "@primer/primitives/dist/css/functional/themes/light.css";
import { BaseStyles, ThemeProvider } from "@primer/react";

const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      staleTime: 1000,
      retry: 1,
    },
  },
});

function RootLayout() {
  return (
    <QueryClientProvider client={queryClient}>
      <ThemeProvider>
        <BaseStyles>
          <App />
        </BaseStyles>
      </ThemeProvider>
    </QueryClientProvider>
  );
}

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <RootLayout />
  </React.StrictMode>,
);
```

### Task D.2: Clipboard Detection Hook

**Files:**
- Create: `src/hooks/useClipboard.ts`

- [ ] Step 1: Write useClipboard.ts

```typescript
// src/hooks/useClipboard.ts
import { useEffect, useRef, useState } from "react";

const DOWNLOAD_EXTENSIONS = [
  ".zip", ".tar", ".gz", ".bz2", ".xz", ".7z", ".rar", ".iso",
  ".pdf", ".doc", ".docx", ".xls", ".xlsx", ".ppt", ".pptx",
  ".mp3", ".mp4", ".avi", ".mkv", ".mov", ".wmv", ".flv",
  ".exe", ".msi", ".dmg", ".pkg", ".deb", ".rpm",
  ".png", ".jpg", ".jpeg", ".gif", ".svg", ".webp",
  ".dll", ".so", ".dylib", ".bin", ".dat",
  ".csv", ".json", ".xml", ".sql", ".db",
  ".apk", ".ipa", ".appimage", ".flatpak", ".snap",
];

function looksLikeDownloadUrl(text: string): boolean {
  try {
    const url = new URL(text);
    const path = url.pathname.toLowerCase();
    return DOWNLOAD_EXTENSIONS.some((ext) => path.endsWith(ext));
  } catch {
    return false;
  }
}

export function useClipboardDetection(onUrlDetected: (url: string) => void) {
  const [lastText, setLastText] = useState("");
  const intervalRef = useRef<ReturnType<typeof setInterval> | null>(null);

  useEffect(() => {
    intervalRef.current = setInterval(async () => {
      try {
        const text = await navigator.clipboard.readText();
        if (text !== lastText) {
          setLastText(text);
          if (
            (text.startsWith("http://") || text.startsWith("https://") || text.startsWith("ftp://")) &&
            looksLikeDownloadUrl(text)
          ) {
            onUrlDetected(text);
          }
        }
      } catch {
        // Clipboard access denied
      }
    }, 2000);

    return () => {
      if (intervalRef.current) clearInterval(intervalRef.current);
    };
  }, [lastText, onUrlDetected]);

  return null;
}
```

### Task D.3: Layout, Sidebar, Toolbar

**Files:**
- Create: `src/components/Layout.tsx`
- Create: `src/components/Sidebar.tsx`
- Create: `src/components/Toolbar.tsx`

- [ ] Step 1: Write Layout.tsx

```tsx
// src/components/Layout.tsx
import { Box } from "@primer/react";
import Sidebar from "./Sidebar";
import Toolbar from "./Toolbar";
import DownloadTable from "./DownloadTable";

interface LayoutProps {
  onNewDownload: () => void;
  selectedIds: Set<number>;
  onSelectChange: (ids: Set<number>) => void;
}

export default function Layout({ onNewDownload, selectedIds, onSelectChange }: LayoutProps) {
  return (
    <Box display="flex" flexDirection="column" height="100vh">
      <Toolbar onNewDownload={onNewDownload} />
      <Box display="flex" flex={1} overflow="hidden">
        <Box width="200px" borderRight="1px solid" borderColor="border.default" sx={{ flexShrink: 0 }}>
          <Sidebar />
        </Box>
        <Box flex={1} overflow="auto">
          <DownloadTable selectedIds={selectedIds} onSelectChange={onSelectChange} />
        </Box>
      </Box>
    </Box>
  );
}
```

- [ ] Step 2: Write Sidebar.tsx

```tsx
// src/components/Sidebar.tsx
import { Box, Text, NavList } from "@primer/react";
import { useDownloads } from "../query/downloadQueries";
import { DownloadIcon, CheckIcon, PauseIcon, StopIcon } from "@primer/octicons-react";

export default function Sidebar() {
  const { data: downloads = [] } = useDownloads();

  const counts = {
    all: downloads.length,
    completed: downloads.filter((d) => d.status === "completed").length,
    incomplete: downloads.filter(
      (d) => d.status === "downloading" || d.status === "paused" || d.status === "queued"
    ).length,
  };

  return (
    <Box p={3}>
      <Text fontWeight="bold" fontSize={2} mb={2} display="block">
        Filters
      </Text>
      <NavList>
        <NavList.Item aria-current="page" onClick={() => {}}>
          <NavList.LeadingVisual>
            <DownloadIcon />
          </NavList.LeadingVisual>
          All ({counts.all})
        </NavList.Item>
        <NavList.Item onClick={() => {}}>
          <NavList.LeadingVisual>
            <CheckIcon />
          </NavList.LeadingVisual>
          Completed ({counts.completed})
        </NavList.Item>
        <NavList.Item onClick={() => {}}>
          <NavList.LeadingVisual>
            <PauseIcon />
          </NavList.LeadingVisual>
          Incomplete ({counts.incomplete})
        </NavList.Item>
      </NavList>
    </Box>
  );
}
```

- [ ] Step 3: Write Toolbar.tsx

```tsx
// src/components/Toolbar.tsx
import { Box, Button } from "@primer/react";
import { PlusIcon, TriangleRightIcon, StopIcon, TrashIcon } from "@primer/octicons-react";

interface ToolbarProps {
  onNewDownload: () => void;
  onResumeSelected?: () => void;
  onPauseSelected?: () => void;
  onDeleteSelected?: () => void;
  hasSelection: boolean;
}

export default function Toolbar({ onNewDownload, hasSelection }: ToolbarProps) {
  return (
    <Box
      display="flex"
      p={2}
      gap={2}
      borderBottom="1px solid"
      borderColor="border.default"
      bg="canvas.subtle"
    >
      <Button onClick={onNewDownload} leadingVisual={PlusIcon} size="small">
        New
      </Button>
      {hasSelection && (
        <>
          <Button leadingVisual={TriangleRightIcon} size="small">
            Resume
          </Button>
          <Button leadingVisual={StopIcon} size="small">
            Stop
          </Button>
          <Button leadingVisual={TrashIcon} size="small" danger>
            Delete
          </Button>
        </>
      )}
    </Box>
  );
}
```

### Task D.4: DownloadTable + DownloadRow

**Files:**
- Create: `src/components/DownloadTable.tsx`
- Create: `src/components/DownloadRow.tsx`

- [ ] Step 1: Write DownloadTable.tsx

```tsx
// src/components/DownloadTable.tsx
import { Box, Text } from "@primer/react";
import { useDownloads } from "../query/downloadQueries";
import DownloadRow from "./DownloadRow";

interface DownloadTableProps {
  selectedIds: Set<number>;
  onSelectChange: (ids: Set<number>) => void;
}

export default function DownloadTable({ selectedIds, onSelectChange }: DownloadTableProps) {
  const { data: downloads = [], isLoading } = useDownloads();

  if (isLoading) {
    return (
      <Box display="flex" justifyContent="center" p={4}>
        <Text color="fg.muted">Loading...</Text>
      </Box>
    );
  }

  if (downloads.length === 0) {
    return (
      <Box display="flex" justifyContent="center" p={4}>
        <Text color="fg.muted">No downloads yet. Click "New" to start one.</Text>
      </Box>
    );
  }

  const toggleSelect = (id: number) => {
    const next = new Set(selectedIds);
    if (next.has(id)) {
      next.delete(id);
    } else {
      next.add(id);
    }
    onSelectChange(next);
  };

  return (
    <Box>
      {downloads.map((d) => (
        <DownloadRow
          key={d.id}
          item={d}
          selected={selectedIds.has(d.id)}
          onToggleSelect={() => toggleSelect(d.id)}
        />
      ))}
    </Box>
  );
}
```

- [ ] Step 2: Write DownloadRow.tsx

```tsx
// src/components/DownloadRow.tsx
import { Box, Text, Checkbox, ProgressBar } from "@primer/react";
import { FileIcon, DownloadIcon, CheckIcon, PauseIcon, AlertIcon } from "@primer/octicons-react";
import type { DownloadItem } from "../types";

function formatBytes(bytes: number): string {
  if (bytes === 0) return "0 B";
  const k = 1024;
  const sizes = ["B", "KB", "MB", "GB", "TB"];
  const i = Math.floor(Math.log(bytes) / Math.log(k));
  return `${(bytes / Math.pow(k, i)).toFixed(1)} ${sizes[i]}`;
}

interface DownloadRowProps {
  item: DownloadItem;
  selected: boolean;
  onToggleSelect: () => void;
}

export default function DownloadRow({ item, selected, onToggleSelect }: DownloadRowProps) {
  const progress = item.total_size > 0 ? (item.downloaded / item.total_size) * 100 : 0;

  const statusIcon = () => {
    switch (item.status) {
      case "downloading":
        return <DownloadIcon />;
      case "completed":
        return <CheckIcon />;
      case "paused":
        return <PauseIcon />;
      case "failed":
        return <AlertIcon />;
      default:
        return <FileIcon />;
    }
  };

  const statusColor = () => {
    switch (item.status) {
      case "completed":
        return "success.fg";
      case "failed":
        return "danger.fg";
      case "paused":
        return "attention.fg";
      default:
        return "fg.default";
    }
  };

  return (
    <Box
      display="flex"
      alignItems="center"
      p={2}
      gap={2}
      borderBottom="1px solid"
      borderColor="border.muted"
      sx={{
        "&:hover": { bg: "canvas.subtle" },
        cursor: "pointer",
      }}
    >
      <Checkbox checked={selected} onChange={onToggleSelect} />
      <Box sx={{ color: statusColor() }}>{statusIcon()}</Box>
      <Box flex={1} minWidth={0}>
        <Text fontWeight="bold" fontSize={1} display="block" sx={{ overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>
          {item.file_name}
        </Text>
        <Text fontSize={0} color="fg.muted">
          {formatBytes(item.downloaded)} / {formatBytes(item.total_size)}
        </Text>
        {(item.status === "downloading" || item.status === "paused") && (
          <Box mt={1}>
            <ProgressBar progress={Math.round(progress)} />
          </Box>
        )}
      </Box>
      <Box textAlign="right" flexShrink={0}>
        <Text fontSize={0} color="fg.muted" display="block">
          {item.status}
        </Text>
        {item.proxy_name && (
          <Text fontSize={0} color="fg.muted">
            Proxy: {item.proxy_name}
          </Text>
        )}
      </Box>
    </Box>
  );
}
```

### Task D.5: Dialogs

**Files:**
- Create: `src/components/dialogs/NewDownloadDialog.tsx`
- Create: `src/components/dialogs/DeleteDialog.tsx`
- Create: `src/components/dialogs/SettingsDialog.tsx`
- Create: `src/components/dialogs/PropertiesDialog.tsx`

- [ ] Step 1: Write NewDownloadDialog.tsx

```tsx
// src/components/dialogs/NewDownloadDialog.tsx
import { useState } from "react";
import { Dialog, Box, TextInput, Button, FormControl, Select } from "@primer/react";
import { useStartDownload } from "../../query/downloadQueries";
import { useSettingsStore } from "../../stores/settingsStore";

interface NewDownloadDialogProps {
  initialUrl?: string;
  onClose: () => void;
}

export default function NewDownloadDialog({ initialUrl = "", onClose }: NewDownloadDialogProps) {
  const [url, setUrl] = useState(initialUrl);
  const [filename, setFilename] = useState("");
  const [proxyName, setProxyName] = useState("");
  const [connections, setConnections] = useState(4);
  const startDownload = useStartDownload();
  const proxies = useSettingsStore((s) => s.settings.proxies);

  const handleSubmit = async () => {
    if (!url) return;
    await startDownload.mutateAsync({ url, filename, proxyName, connections });
    onClose();
  };

  return (
    <Dialog title="New Download" onClose={onClose} width="large">
      <Box display="flex" flexDirection="column" gap={3} p={3}>
        <FormControl required>
          <FormControl.Label>URL</FormControl.Label>
          <TextInput
            value={url}
            onChange={(e) => setUrl(e.target.value)}
            placeholder="https://example.com/file.zip"
            sx={{ width: "100%" }}
          />
        </FormControl>
        <FormControl>
          <FormControl.Label>Filename (optional)</FormControl.Label>
          <TextInput
            value={filename}
            onChange={(e) => setFilename(e.target.value)}
            placeholder="Auto-detect from URL"
            sx={{ width: "100%" }}
          />
        </FormControl>
        <FormControl>
          <FormControl.Label>Proxy</FormControl.Label>
          <Select value={proxyName} onChange={(e) => setProxyName(e.target.value)}>
            <Select.Option value="">No proxy</Select.Option>
            {Object.keys(proxies).map((name) => (
              <Select.Option key={name} value={name}>{name}</Select.Option>
            ))}
          </Select>
        </FormControl>
        <FormControl>
          <FormControl.Label>Connections</FormControl.Label>
          <TextInput
            type="number"
            value={connections}
            onChange={(e) => setConnections(Number(e.target.value))}
            min={1}
            max={32}
            sx={{ width: "100%" }}
          />
        </FormControl>
        <Box display="flex" justifyContent="flex-end" gap={2}>
          <Button onClick={onClose}>Cancel</Button>
          <Button variant="primary" onClick={handleSubmit} disabled={!url || startDownload.isPending}>
            {startDownload.isPending ? "Starting..." : "Download"}
          </Button>
        </Box>
      </Box>
    </Dialog>
  );
}
```

- [ ] Step 2: Write DeleteDialog.tsx

```tsx
// src/components/dialogs/DeleteDialog.tsx
import { Box, Button, Text } from "@primer/react";
import { Dialog } from "@primer/react/experimental";
import { useDeleteDownload } from "../../query/downloadQueries";

interface DeleteDialogProps {
  ids: number[];
  onClose: () => void;
}

export default function DeleteDialog({ ids, onClose }: DeleteDialogProps) {
  const deleteDownload = useDeleteDownload();

  const handleDelete = async (deleteFile: boolean) => {
    await Promise.all(ids.map((id) => deleteDownload.mutateAsync({ id, deleteFile })));
    onClose();
  };

  return (
    <Dialog title="Delete Download" onClose={onClose}>
      <Box p={3} display="flex" flexDirection="column" gap={3}>
        <Text>
          Delete {ids.length} download{ids.length > 1 ? "s" : ""}?
        </Text>
        <Box display="flex" justifyContent="flex-end" gap={2}>
          <Button onClick={onClose}>Cancel</Button>
          <Button onClick={() => handleDelete(false)}>
            Delete Record Only
          </Button>
          <Button variant="danger" onClick={() => handleDelete(true)}>
            Delete File & Record
          </Button>
        </Box>
      </Box>
    </Dialog>
  );
}
```

- [ ] Step 3: Write SettingsDialog.tsx

```tsx
// src/components/dialogs/SettingsDialog.tsx
import { useState, useEffect } from "react";
import { Box, Button, TextInput, FormControl, Text } from "@primer/react";
import { Dialog } from "@primer/react/experimental";
import { useSettings } from "../../query/downloadQueries";
import type { Settings } from "../../types";

interface SettingsDialogProps {
  onClose: () => void;
}

export default function SettingsDialog({ onClose }: SettingsDialogProps) {
  const { settings: initialSettings, saveSettings } = useSettings();
  const [settings, setSettings] = useState<Settings | null>(null);

  useEffect(() => {
    if (initialSettings) setSettings(initialSettings);
  }, [initialSettings]);

  if (!settings) return null;

  const handleSave = async () => {
    if (settings) {
      await saveSettings(settings);
      onClose();
    }
  };

  return (
    <Dialog title="Settings" onClose={onClose} width="large">
      <Box p={3} display="flex" flexDirection="column" gap={3}>
        <FormControl>
          <FormControl.Label>Download Directory</FormControl.Label>
          <TextInput
            value={settings.download_dir}
            onChange={(e) => setSettings({ ...settings, download_dir: e.target.value })}
            sx={{ width: "100%" }}
          />
        </FormControl>
        <FormControl>
          <FormControl.Label>Max Connections</FormControl.Label>
          <TextInput
            type="number"
            value={settings.max_connections}
            onChange={(e) => setSettings({ ...settings, max_connections: Number(e.target.value) })}
            min={1}
            max={64}
          />
        </FormControl>
        <FormControl>
          <FormControl.Label>Max Retries</FormControl.Label>
          <TextInput
            type="number"
            value={settings.max_retries}
            onChange={(e) => setSettings({ ...settings, max_retries: Number(e.target.value) })}
            min={0}
          />
        </FormControl>
        <FormControl>
          <FormControl.Label>User Agent</FormControl.Label>
          <TextInput
            value={settings.user_agent}
            onChange={(e) => setSettings({ ...settings, user_agent: e.target.value })}
            sx={{ width: "100%" }}
          />
        </FormControl>
        <Box display="flex" justifyContent="flex-end" gap={2}>
          <Button onClick={onClose}>Cancel</Button>
          <Button variant="primary" onClick={handleSave}>
            Save
          </Button>
        </Box>
      </Box>
    </Dialog>
  );
}
```

- [ ] Step 4: Write PropertiesDialog.tsx

```tsx
// src/components/dialogs/PropertiesDialog.tsx
import { Box, Text } from "@primer/react";
import { Dialog } from "@primer/react/experimental";
import { useDownload } from "../../query/downloadQueries";
import type { DownloadItem } from "../../types";

function formatBytes(bytes: number): string {
  if (bytes === 0) return "0 B";
  const k = 1024;
  const sizes = ["B", "KB", "MB", "GB", "TB"];
  const i = Math.floor(Math.log(bytes) / Math.log(k));
  return `${(bytes / Math.pow(k, i)).toFixed(1)} ${sizes[i]}`;
}

interface PropertiesDialogProps {
  id: number;
  onClose: () => void;
}

export default function PropertiesDialog({ id, onClose }: PropertiesDialogProps) {
  const item = useDownload(id);

  if (!item) return null;

  const rows: [string, string][] = [
    ["Name", item.file_name],
    ["URL", item.url],
    ["Status", item.status],
    ["Size", formatBytes(item.total_size)],
    ["Downloaded", formatBytes(item.downloaded)],
    ["Proxy", item.proxy_name || "None"],
    ["Connections", String(item.connections)],
    ["Resumable", item.resumable ? "Yes" : item.resumable === false ? "No" : "Unknown"],
    ["Save Path", item.save_path],
    ["Created", item.created_at],
    ["Last Try", item.last_try || "N/A"],
  ];

  return (
    <Dialog title="Properties" onClose={onClose} width="large">
      <Box p={3} display="flex" flexDirection="column" gap={2}>
        {rows.map(([label, value]) => (
          <Box key={label} display="flex">
            <Text fontWeight="bold" sx={{ minWidth: 120, flexShrink: 0 }}>{label}:</Text>
            <Text sx={{ wordBreak: "break-all" }}>{value}</Text>
          </Box>
        ))}
      </Box>
    </Dialog>
  );
}
```

### Task D.6: Wire App.tsx

- [ ] Step 1: Rewrite App.tsx

```tsx
// src/App.tsx
import { useState, useCallback } from "react";
import Layout from "./components/Layout";
import NewDownloadDialog from "./components/dialogs/NewDownloadDialog";
import DeleteDialog from "./components/dialogs/DeleteDialog";
import SettingsDialog from "./components/dialogs/SettingsDialog";
import { useClipboardDetection } from "./hooks/useClipboard";

type Dialog =
  | { type: "new-download"; url?: string }
  | { type: "delete"; ids: number[] }
  | { type: "settings" }
  | null;

function App() {
  const [dialog, setDialog] = useState<Dialog>(null);
  const [selectedIds, setSelectedIds] = useState<Set<number>>(new Set());

  const onUrlDetected = useCallback((url: string) => {
    setDialog({ type: "new-download", url });
  }, []);

  useClipboardDetection(onUrlDetected);

  return (
    <>
      <Layout
        onNewDownload={() => setDialog({ type: "new-download" })}
        selectedIds={selectedIds}
        onSelectChange={setSelectedIds}
      />

      <button onClick={() => setDialog({ type: "settings" })} style={{ position: "fixed", bottom: 8, right: 8 }}>
        Settings
      </button>

      {dialog?.type === "new-download" && (
        <NewDownloadDialog
          initialUrl={dialog.url ?? ""}
          onClose={() => setDialog(null)}
        />
      )}
      {dialog?.type === "delete" && (
        <DeleteDialog
          ids={dialog.ids}
          onClose={() => setDialog(null)}
        />
      )}
      {dialog?.type === "settings" && (
        <SettingsDialog onClose={() => setDialog(null)} />
      )}
    </>
  );
}

export default App;
```

---

## Sub-Plan E: Tauri Commands + Tray + Integration (branch: feat/tray-commands)

**Depends on:** Phase 0 merged to main, feat/download-engine, feat/state-persistence, feat/websocket all merged

**Files to modify:**
```
src-tauri/src/
├── lib.rs              (MODIFY: add all commands, tray, app state)
├── cmd.rs              (NEW: all Tauri command handlers)
├── tray.rs             (NEW: system tray management)
├── Cargo.toml          (MODIFY: add tauri-plugin-shell)
src-tauri/
├── tauri.conf.json     (MODIFY: window config for tray mode)
```

### Task E.1: Tray Module

**Files:**
- Create: `src-tauri/src/tray.rs`

- [ ] Step 1: Write tray.rs

```rust
// src-tauri/src/tray.rs
use tauri::{
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    menu::{Menu, MenuItem},
    AppHandle, Runtime, Manager,
};

pub fn build_tray<R: Runtime>(app: &AppHandle<R>) -> Result<(), Box<dyn std::error::Error>> {
    let show = MenuItem::with_id(app, "show", "Show ProxyDM", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit", true, Some("CmdOrCtrl+Q"))?;
    let menu = Menu::with_items(app, &[&show, &quit])?;

    TrayIconBuilder::new()
        .icon(app.default_window_icon().unwrap().clone())
        .menu(&menu)
        .on_menu_event(|app, event| {
            match event.id.as_ref() {
                "show" => {
                    if let Some(window) = app.get_webview_window("main") {
                        let _ = window.show();
                        let _ = window.set_focus();
                    }
                }
                "quit" => {
                    app.exit(0);
                }
                _ => {}
            }
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up, ..
            } = event {
                if let Some(app) = tray.app_handle() {
                    if let Some(window) = app.get_webview_window("main") {
                        let _ = window.show();
                        let _ = window.set_focus();
                    }
                }
            }
        })
        .build(app)?;

    Ok(())
}
```

### Task E.2: Tauri Command Handlers

**Files:**
- Create: `src-tauri/src/cmd.rs`

- [ ] Step 1: Write cmd.rs

```rust
use crate::types::*;
use crate::state::db::Db;
use crate::worker::WorkerPool;
use tauri::State;
use std::sync::Arc;

pub struct AppState {
    pub db: Db,
    pub worker_pool: WorkerPool,
}

#[tauri::command]
pub fn list_downloads(state: State<'_, Arc<AppState>>) -> Result<Vec<DownloadItem>, String> {
    state.db.list_downloads()
}

#[tauri::command]
pub async fn start_download(
    state: State<'_, Arc<AppState>>,
    url: String,
    filename: String,
    proxy_name: String,
    connections: u32,
) -> Result<u64, String> {
    // First probe the URL
    let pool = state.worker_pool.pool_ref();
    let headers = std::collections::HashMap::new();
    let proxy = if proxy_name.is_empty() { None } else { Some(&proxy_name as &str) };
    let probe_result = crate::probe::probe(&url, &headers, proxy, &pool).await?;

    let file_name = if filename.is_empty() { probe_result.file_name } else { filename };
    let save_path = format!("{}/{}", 
        crate::config::load().download_dir,
        file_name
    );

    let cfg = DownloadConfig {
        url: url.clone(),
        output_path: save_path.trim_end_matches(&file_name).to_string(),
        save_path,
        id: 0, // will be assigned by WorkerPool
        file_name: file_name.clone(),
        is_resume: false,
        headers,
        proxy_name,
        total_size: probe_result.file_size,
        supports_range: probe_result.supports_range,
        rate_limit_bps: 0,
        connections,
        max_retries: 3,
    };

    let id = state.worker_pool.add(cfg).await?;

    // Insert into DB
    let item = DownloadItem {
        id,
        url,
        file_name,
        save_path: String::new(),
        total_size: probe_result.file_size,
        downloaded: 0,
        status: DownloadStatus::Downloading,
        parts: Vec::new(),
        proxy_name,
        connections,
        resumable: Some(probe_result.supports_range),
        merge_progress: 0.0,
        created_at: chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
        last_try: String::new(),
    };
    state.db.insert_download(&item).ok();

    Ok(id)
}

#[tauri::command]
pub async fn pause_download(state: State<'_, Arc<AppState>>, id: u64) -> Result<(), String> {
    state.worker_pool.cancel(id).await;
    // Update DB status
    if let Ok(mut items) = state.db.list_downloads() {
        if let Some(item) = items.iter_mut().find(|i| i.id == id) {
            item.status = DownloadStatus::Paused;
            state.db.update_download(item).ok();
        }
    }
    Ok(())
}

#[tauri::command]
pub async fn resume_download(state: State<'_, Arc<AppState>>, id: u64) -> Result<(), String> {
    // Load saved state and re-queue
    if let Ok(Some(saved_state)) = crate::state::gob::load_state(id) {
        let cfg = DownloadConfig {
            url: saved_state.url,
            output_path: saved_state.save_path.clone(),
            save_path: saved_state.save_path,
            id,
            file_name: saved_state.file_name,
            is_resume: true,
            headers: std::collections::HashMap::new(),
            proxy_name: saved_state.proxy_name,
            total_size: saved_state.total_size,
            supports_range: true,
            rate_limit_bps: 0,
            connections: saved_state.workers,
            max_retries: 3,
        };
        state.worker_pool.add(cfg).await?;
    }
    Ok(())
}

#[tauri::command]
pub async fn cancel_download(state: State<'_, Arc<AppState>>, id: u64) -> Result<(), String> {
    state.worker_pool.cancel(id).await;
    Ok(())
}

#[tauri::command]
pub async fn delete_download(
    state: State<'_, Arc<AppState>>,
    id: u64,
    delete_file: bool,
) -> Result<(), String> {
    // Look up file path before deleting DB record
    let save_path = if delete_file {
        state.db.list_downloads().ok()
            .and_then(|items| items.into_iter().find(|i| i.id == id))
            .map(|item| item.save_path)
    } else {
        None
    };

    state.worker_pool.cancel(id).await;
    state.db.delete_download(id)?;
    crate::state::gob::delete_state(id)?;

    if let Some(path) = save_path {
        let p = std::path::Path::new(&path);
        // Also try .pdm temp file
        let pdm_path = p.with_extension("pdm");
        if pdm_path.exists() {
            std::fs::remove_file(&pdm_path).ok();
        }
        if p.exists() {
            std::fs::remove_file(p).ok();
        }
    }
    Ok(())
}

#[tauri::command]
pub fn get_settings() -> Result<Settings, String> {
    Ok(crate::config::load())
}

#[tauri::command]
pub fn save_settings(settings: Settings) -> Result<(), String> {
    crate::config::save(&settings)
}
```

- [ ] Step 2: Add chrono dependency

```toml
chrono = "0.4"
```

### Task E.3: Update lib.rs with Full Integration

**Files:**
- Modify: `src-tauri/src/lib.rs`

- [ ] Step 1: Rewrite lib.rs

```rust
mod types;
mod config;
mod probe;
mod engine;
mod network;
mod worker;
mod state;
mod ws;
mod tray;
mod cmd;

use crate::worker::WorkerPool;
use crate::cmd::AppState;
use std::sync::Arc;
use tokio::sync::mpsc;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let (event_tx, _event_rx) = mpsc::unbounded_channel();
    let (request_tx, _request_rx) = mpsc::unbounded_channel();

    let db = crate::state::db::Db::new().expect("Failed to initialize database");
    let worker_pool = WorkerPool::new(8, event_tx.clone());

    let state = Arc::new(AppState { db, worker_pool });

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(state)
        .setup(|app| {
            // Build tray
            crate::tray::build_tray(app.handle())?;

            // Start WebSocket server in background
            let mut ws_server = crate::ws::server::WsServer::new(event_tx, request_tx);
            std::thread::spawn(move || {
                if let Err(e) = ws_server.start("127.0.0.1:18999") {
                    eprintln!("WS server error: {}", e);
                }
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            cmd::list_downloads,
            cmd::start_download,
            cmd::pause_download,
            cmd::resume_download,
            cmd::cancel_download,
            cmd::delete_download,
            cmd::get_settings,
            cmd::save_settings,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
```

### Task E.4: Update tauri.conf.json for Tray Mode

**Files:**
- Modify: `src-tauri/tauri.conf.json`

- [ ] Step 1: Update window configuration

```json
{
  "$schema": "https://schema.tauri.app/config/2",
  "productName": "ProxyDM",
  "version": "0.1.0",
  "identifier": "com.fb0sh.proxydownloadmanager",
  "build": {
    "beforeDevCommand": "pnpm dev",
    "devUrl": "http://localhost:1420",
    "beforeBuildCommand": "pnpm build",
    "frontendDist": "../dist"
  },
  "app": {
    "windows": [
      {
        "title": "ProxyDM",
        "width": 960,
        "height": 600,
        "center": true,
        "visible": true
      }
    ],
    "trayIcon": {
      "iconPath": "icons/icon.png",
      "iconAsTemplate": true
    },
    "security": {
      "csp": null
    }
  },
  "bundle": {
    "active": true,
    "targets": "all",
    "icon": [
      "icons/32x32.png",
      "icons/128x128.png",
      "icons/128x128@2x.png",
      "icons/icon.icns",
      "icons/icon.ico"
    ],
    "macOS": {
      "minimumSystemVersion": "12.0"
    }
  }
}
```

---

## Integration & Verification Plan

After all branches are complete and merged back to main:
1. Build: `cd src-tauri && cargo build`
2. Frontend: `pnpm tauri dev`
3. Test WebSocket: `websocat ws://127.0.0.1:18999`
4. Test download: via UI "New" button
5. Test pause/resume
6. Test proxy config in Settings
