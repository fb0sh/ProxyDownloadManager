# ProxyDM 完整设计文档

> 基于现有 Rust 代码库与 Surge 下载引擎设计合并而成

---

## 一、项目概述

ProxyDM 是**多线程下载管理器**，支持浏览器扩展集成、每下载独立代理、系统托盘后台运行、暂停/恢复。Rust 后端 + Tauri/HTML 前端。

### 核心差异特性

| 特性 | 说明 |
|------|------|
| 每下载独立代理 | 每个下载可指定不同 HTTP/SOCKS5 代理 |
| 浏览器扩展集成 | Edge 扩展通过 WebSocket (port 18999) 通信 |
| 后台托盘模式 | 无 Dock 图标，系统托盘常驻 |
| 暂停/恢复 | 分块位图 + gob 序列化状态持久化 |

---

## 二、系统架构

```
┌─────────────────────────────────────────────────────────┐
│                    前端 (Tauri + HTML/CSS)               │
│  Toolbar | Table | Dialogs | Clipboard Detection        │
└────────────────────────┬────────────────────────────────┘
                         │ Tauri Commands
┌────────────────────────▼────────────────────────────────┐
│                    Rust Backend                          │
│                                                         │
│  ┌─────────────┐  ┌──────────────┐  ┌────────────────┐ │
│  │ WebSocket   │  │ DownloadMgr  │  │  System Tray   │ │
│  │ Server      │◄─┤ (调度/状态)   │──┤  & Window      │ │
│  │ (port 18999)│  │              │  │  Management    │ │
│  └─────────────┘  └──────┬───────┘  └────────────────┘ │
│                          │                              │
│  ┌───────────────────────▼──────────────────────────────┐│
│  │              下载引擎层                               ││
│  │  ┌──────────────────┐  ┌──────────────────────────┐ ││
│  │  │ ConcurrentDler   │  │ SingleDler (fallback)    │ ││
│  │  │ 多线程分块下载    │  │ 单线程回退               │ ││
│  │  └────────┬─────────┘  └──────────────────────────┘ ││
│  │           │                                          ││
│  │  ┌────────▼─────────┐  ┌──────────────────────────┐ ││
│  │  │ Probe 能力探测    │  │ NetworkPool 传输连接池    │ ││
│  │  └──────────────────┘  └──────────────────────────┘ ││
│  └──────────────────────────────────────────────────────┘│
│                                                         │
│  ┌─────────────┐  ┌──────────────┐  ┌────────────────┐ │
│  │ State 持久化 │  │ WorkerPool   │  │ RateLimiter    │ │
│  │ SQLite + gob │  │ 并发控制      │  │ 限速 (双层)    │ │
│  └─────────────┘  └──────────────┘  └────────────────┘ │
└─────────────────────────────────────────────────────────┘
```

### 双进程新下载设计

当浏览器扩展拦截下载且应用处于后台托盘模式时，主窗口隐藏于屏幕外 (1x1 at -10000,-10000)。下载交互有两种路径：

1. **主路径**：WebSocket 收到 URL → 设置 `ws_focus=true` + `ws_url` → 主窗口重绘 → 显示 "New Download" 对话框
2. **回退路径**：托盘模式 → 生成独立 `--new-download-window` 子进程 → 用户填表单 → 通过 WebSocket 返回主进程

### 数据流

```
Edge Extension ──WebSocket──→ ws_server (port 18999)
                                    │
                          ┌─────────┴──────────┐
                          │ process_message()   │
                          │  "action:start" →   │
                          │    spawn_for_request│
                          │    (standalone win) │
                          │                    │
                          │  other → focus=     │
                          │    true + url=xxx   │
                          │    (inline dialog)  │
                          └─────────┬──────────┘
                                    │
                          ┌─────────▼──────────┐
                          │  app.rs:           │
                          │  add_new_download() │
                          │  start_download()   │
                          └─────────┬──────────┘
                                    │
                          ┌─────────▼──────────┐
                          │  下载引擎           │
                          │  Probe → WorkerPool │
                          │  → Concurrent/Single│
                          └────────────────────┘
```

---

## 三、下载全流程

```
请求进入 (Tauri Command / WebSocket / Clipboard)
   │
   ▼
[探测阶段] ProbeServerWithProxy(url, headers)
   │  GET + Range: bytes=0-0
   │  206 → SupportsRange=true, fileSize 从 Content-Range 解析
   │  200 → SupportsRange=false, fileSize 从 Content-Length 解析
   │  403/405 → 重试不带 Range 请求
   │  从 Content-Disposition 或 URL 路径检测文件名
   │  同一 host 串行化探测，防止限流
   │
   ▼
[入队阶段] WorkerPool.Add(cfg)
   │  配置限流器 (global + per-download)
   │  入队
   │  发 ID 到 task channel
   │
   ▼
[调度阶段] WorkerPool.worker() → RunDownload(cfg)
   │
   ├─ SupportsRange=true ──────────────────────────────┐
   │   │                                                │
   │   ▼                                                │
   │  ConcurrentDownloader.Download()                   │
   │   │                                                │
   │   ├─ bootstrapMetadata() → Range: bytes=0-0        │
   │   ├─ LoadState() → 检查 isResume                    │
   │   ├─ getInitialConnections(剩余大小)                │
   │   ├─ determineChunkSize()                          │
   │   ├─ prewarmConnections()                          │
   │   ├─ startHelpers() → 3 个辅助协程                 │
   │   ├─ executeWorkers() → N 个 worker                │
   │   │     └─ 每个 worker: pop task → Range → WriteAt │
   │   ├─ 暂停 → handlePause() → 保存 state → ErrPaused │
   │   └─ 完成 → syncFile()                             │
   │                                                    │
   └─ SupportsRange=false ──────────────────────────────┘
       │
       ▼
      SingleDownloader.Download()
         │  GET 完整文件
         │  io.CopyBuffer 顺序写入
         │  不支持暂停/恢复
         │  429/503 Retry-After 退避 (最多 6 次)
   │
   ▼
[完成阶段]
   成功 → FinalizeSession() → 计算平均速度 → 通知前端
   暂停 → ErrPaused → worker 移除 active 记录 → 保存状态
   错误 → 判断降级条件 → Concurrent→Single 或 上报错误
```

---

## 四、下载引擎

### 4.1 引擎选择

| 条件 | 引擎 |
|------|------|
| 服务器支持 Range (206) | ConcurrentDownloader |
| 不支持 Range (200) | SingleDownloader |
| Concurrent 失败降级 | SingleDownloader |

### 4.2 ConcurrentDownloader

**连接数计算** (getInitialConnections):

```
workers = sqrt(fileSizeMB)          // 平方根启发式
workers = clamp(workers, 1, 32)     // 单下载上限 32
maxChunks = fileSize / MinChunkSize(2MB)
workers = min(workers, maxChunks)
```

API 参数 `workers > 0` 直接覆盖，跳过 sqrt 计算。

**分块策略** (determineChunkSize):

- **并行模式**（默认）：`chunkSize = fileSize / numConns`，4KB 对齐
- **顺序模式**：固定 `MinChunkSize = 2MB`，严格按 offset 顺序写
- **恢复模式**：直接复用暂停时保存的 task 列表，不重新分块

### 4.3 Worker 执行模型

```
for {
    task = queue.Pop()  // 阻塞等待
    if task == nil → return (队列关闭)

    for retry := 0; retry < maxRetries; retry++ {
        err = downloadTask(ctx, file, task, buf, client)

        if err == nil → break
        if 限流错误 → penalize host + 退避 + 切 proxy
        if 通用错误 → 切 proxy + 指数退避
    }

    if err != nil → return err
}
```

**downloadTask** (单个 chunk 下载):

1. 发 `Range: bytes=offset-end` GET 请求
2. Read HTTP body → `WriteAt(file, buf, offset)` 写入正确偏移
3. 每读数据更新 `LastActivity`（防健康检测误杀）
4. 限速时通过 MultiLimiter.WaitN() 控制
5. 按 batch (1MB 或 200ms) flush 进度上报
6. 每 2s 滑动窗口计算 EMA 速度

### 4.4 辅助协程

下载运行 3 个后台协程：

| 协程 | 周期 | 职责 |
|------|------|------|
| balancer | 200ms | 工作窃取 + 对冲请求 |
| completionMonitor | 50ms | 检测下载完成条件 |
| healthMonitor | 1s | 检测慢 worker / 卡死 worker |

**完成条件**：队列空 AND (全部 worker idle 或已下载字节 ≥ 文件大小)。

### 4.5 动态负载均衡

**工作窃取** (StealWork)：idle worker 出现时，从最忙 worker 偷一半剩余 chunk。对半分割，4KB 对齐。

```
[worker A: 0-100MB] → split → [worker A: 0-50MB] [idle: 50-100MB]
```

实现：遍历 active tasks 找 `RemainingBytes()` 最大者 → `StopAt.Store(newStopAt)` 通知原始 worker 停 → 被偷 chunk 入队。

**对冲请求** (HedgeWork)：chunk 太小无法窃取时，创建 duplicate task，两个 worker 同时下载。通过 `SharedMaxOffset` (atomic.Int64) 去重上报进度。先完成者赢。

### 4.6 SingleDownloader

| 特性 | 说明 |
|------|------|
| 适用场景 | 不支持 Range、403/405 拒绝分片、Concurrent 降级 |
| 限制 | 不支持暂停/恢复，中断后从头开始 |
| 实现 | 32KB buffer → `io.CopyBuffer` → 429/503 Retry-After 退避 |

### 4.7 降级机制

```
Concurrent 失败 且 !暂停 && !取消 && !超时
  → SessionReset() 清进度
  → Truncate 清临时文件
  → SingleDownloader.Download() 重新下载
```

---

## 五、断点续传

### 5.1 状态持久化

双文件结构：

```
~/.pdm/state/
├── master.gob        ← 全局主列表 (所有下载索引)
└── detail-<id>.gob   ← 单下载详细状态
```

**DownloadState 字段**：

| 字段 | 说明 |
|------|------|
| URL, ID, FileName, SavePath | 下载标识 |
| TotalSize, Downloaded | 总大小 / 已下载 |
| Tasks | 剩余未完成任务列表 |
| Elapsed | 已用时间 |
| ChunkBitmap | chunk 完成位图 |
| ActualChunkSize | bitmap 对应 chunk 大小 |
| ProxyName | 使用的代理 |
| Workers, MinChunkSize | 连接数/分块参数 |

### 5.2 暂停流程

```
handlePause()
  1. 收集 active task 剩余部分
  2. DrainRemaining() 取出队列中剩余 task
  3. computedDownloaded = fileSize - remainingBytes
  4. 保存 ChunkBitmap 快照
  5. 构建 DownloadState (tasks, mirrors, bitmap, elapsed)
  6. 保存到磁盘
  7. 返回 ErrPaused 阻止完成回调
```

边界：`remainingBytes == 0` 时直接完成（暂停命中完成边界）。

### 5.3 恢复流程

**热路径**（同 session）：

```
ExtractPausedConfig(id) → 从内存取 config
hydrateConfigFromDisk(state) → 从磁盘覆盖最新状态
cfg.IsResume = true → 重新入队
```

**冷路径**（跨 session）：

```
state.LoadState(url, savePath) → 读 detail gob
buildResumeConfig() → 恢复进度/时间/worker数/限速
→ 重新入队
```

恢复后执行：
- 恢复 `Downloaded` / `Elapsed`
- 恢复 `ChunkBitmap` 并重算进度
- 直接复用 saved Tasks，不重新分块
- `getEffectiveSizeForWorkers = 剩余大小`
- 连接数按剩余大小重算

### 5.4 文件命名

- 进行中：`<filename>.pdm`（临时文件名）
- 完成：移除 `.pdm` 后缀 → 最终文件名

---

## 六、代理与网络

### 6.1 NetworkPool

共享传输连接池，key 为 `(proxyURL, maxConns)` 二元组。

| 参数 | 值 | 说明 |
|------|-----|------|
| MaxIdleConns | 512 | 全局空闲连接上限 |
| MaxIdleConnsPerHost | 128 | 每 host 空闲上限 |
| MaxConnsPerHost | 512 | 每 host 总连接上限 |
| DisableCompression | true | 块下载不兼容压缩 |
| ForceAttemptHTTP2 | false | 禁用 HTTP/2 |
| TLSHandshakeTimeout | 10s | TLS 超时 |
| IdleConnTimeout | 90s | 空闲超时 |
| ConnectTimeout | 60s | 连接超时 |
| ReadTimeout | 600s | 读取超时 (10min) |
| RedirectPolicy | 10 | 最多 10 次重定向 |

设计要点：
- 相同配置复用同一 `reqwest Client`，避免重复 DNS + TLS
- 引用计数管理
- 代理全链路透传：Probe 和下载使用同一 NetworkPool + 同一代理

### 6.2 代理配置

- 每下载可指定独立代理（HTTP/HTTPS/SOCKS5）
- 下载级 → 默认代理 → 无代理（三级回退）
- SOCKS5 支持

### 6.3 连接预热

下载前预建 N 个 TCP+TLS 连接：

- 发 `Range: bytes=0-0` 完成握手
- 等待至少 N 个就绪或 10s 超时
- 预建连接进入 idle 池，worker 直接复用

---

## 七、容错设计

### 7.1 多级重试

| 错误类型 | 策略 |
|----------|------|
| 限流 (429/503) | 退避 + 切 proxy，最多 6 次 |
| 通用错误 | 切 proxy，最多 3 次 |
| 单 proxy 退避 | `1<<attempt * 200ms` |

### 7.2 健康检测

每秒检查所有 active task：

```
StallTimeout (3s) 无活动 → 判定卡死
速度 < avgSpeed * 30% 持续 > 5s → 判定慢

判定通过:
  taskCancel() → worker 返回 ctx.Err()
  remaining 重新入队 → 换 proxy
```

保护机制：每次 TCP socket 读到数据立即更新 `LastActivity`，不等 buffer 填满。

### 7.3 线程安全

- `completed_counter` (AtomicU32)：线程退出时 ALWAYS 自增（通过 catch_unwind guard）
- 避免 coordinator 在 panic 时永久 stall
- 每 256KB 进度同步到共享状态
- 每个 read 循环检查 cancel flag

### 7.4 现有问题与修复优先级

| # | 问题 | 说明 |
|---|------|------|
| 1 | Stall 检测过于激进 | 5s无进度在慢连接触发 premature merge。Fix：使用最小进度阈值 (如 1MB) |
| 2 | WS server 单线程 accept | 一个慢连接阻塞全部。Fix：spawn handler per connection |
| 3 | completed_counter race on resume | 预 seed counter + 运行中线程 = premature completion。workaround 通过 stall 检测 |
| 4 | 删除时 part 清理竞态 | delete_download 不等线程完全停止就删文件 |
| 5 | 无队列管理 | 不能重排序、无优先级、无定时下载 |

---

## 八、速率限制

两层限流器级联：

```
MultiLimiter
  ├── GlobalLimiter     → 全局限速
  └── DownloadLimiter   → 单下载限速
```

- 每个下载独立 token bucket
- 默认继承全局默认值
- 支持 `SetRateLimit(id, bps)` / `ClearRateLimit(id)` 单独覆盖
- pause 时保存 rate limit，resume 时恢复

---

## 九、事件系统

所有模块间通过 channel 传递事件，不直接耦合：

| 事件 | 触发点 | 说明 |
|------|--------|------|
| DownloadStarted | RunDownload 开始 | 通知前端开始 |
| DownloadProgress | worker 定期 flush | 更新进度条 |
| DownloadCompleted | 下载成功 | 通知完成 |
| DownloadPaused | handlePause | 通知暂停 |
| DownloadResumed | Resume 重新入队 | 通知恢复 |
| DownloadErrored | 下载失败 | 通知错误 |
| DownloadRemoved | Cancel / Delete | 通知移除 |
| DownloadQueued | 入队 | 通知排队 |

---

## 十、前端 UI 设计

### 10.1 布局

```
┌──────────────────────────────────────────┐
│  Toolbar: New | Resume | Stop | Delete   │
│                    Settings | About | Quit│
├──────────┬───────────────────────────────┤
│ Sidebar   │  Table (sortable by date)    │
│ All (N)   │  ☐ | Icon | Name | Size     │
│ Completed │  Status | Speed | ETA       │
│ Incomplete│  Resume | Proxy | Last Try   │
│           │                              │
│ Total: N  │  Context menu on right-click │
└──────────┴───────────────────────────────┘
```

### 10.2 对话框

| 对话框 | 内容 |
|--------|------|
| New Download | URL + filename + proxy + threads + Download/Cancel |
| Delete Confirm | "Delete Record Only" vs "Delete File & Record" |
| Edit | URL + filename + proxy + threads + Save/Cancel |
| Properties | 只读全部字段 + 文件元数据 |
| Detail Window | 进度条 + 信息卡 + parts 列表 + 动作按钮 |
| Settings | 下载目录、最大线程、最大重试、UA、缓存、启动项、代理列表编辑器 |
| About | 版本信息 |

### 10.3 Clipboard 检测

每 30 帧读取剪贴板。匹配 `http://|https://|ftp://` 且通过 `looks_like_download_url()` 启发式检测（~100 种扩展名）→ 打开 New Download 对话框。

### 10.4 自动保存

每 60 帧 flush 下载列表到 SQLite。

---

## 十一、WebSocket 协议

### 11.1 浏览器扩展协议

Edge 扩展 → `ws://127.0.0.1:18999/` → 发送：

```json
{
  "action": "start",
  "url": "https://example.com/file.zip",
  "filename": "file.zip",
  "proxy_name": "my-proxy",
  "connections": 8,
  "referrer": "optional",
  "tab_title": "optional"
}
```

旧版兼容：纯 URL 字符串也支持。

### 11.2 处理逻辑

- `action: "start"` → 序列化 `PendingDownloadRequest` → 存入 `ws_url` → 设 `ws_focus=true`
- 其他 → 生成独立 `new_download_window` 子进程。若失败 → 回退到 inline dialog

---

## 十二、跨平台窗口管理

### 12.1 三级焦点策略

1. `egui::Context::send_viewport_cmd(Focus)` — 跨平台，线程安全
2. 平台原生：
   - **macOS**: `osascript` → `set frontmost of every process whose unix id is PID to true`
   - **Windows**: `AttachThreadInput` + `SetForegroundWindow` + `BringWindowToTop`
   - **Linux**: `wmctrl -a <app_name>` → 回退 X11 (`_NET_ACTIVE_WINDOW` → `XSetInputFocus`)
3. macOS 特殊：`activateIgnoringOtherApps(true)`

### 12.2 托盘模式

- 托盘菜单：Show ProxyDM / Quit
- `hide_main_window()`: 窗口移出屏幕 (1x1, -10000,-10000)
- `show_main_window()`: 960x600 → 屏幕居中 → Focus → 激活应用
- macOS: `NSApplicationActivationPolicy::Accessory` — 无 Dock 图标

### 12.3 macOS 原生

- `set_accessory_policy()`: 无 Dock 图标、无 Cmd+Tab
- 文件图标：`NSWorkspace` + 每扩展名 cache → 回退程序化灰色文档图标
- 启动：`auto_launch` crate → LaunchAgent

---

## 十三、持久化

### 13.1 SQLite Schema

```sql
CREATE TABLE downloads (
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
    parts TEXT NOT NULL DEFAULT '[]'
);
ALTER TABLE downloads ADD COLUMN resumable INTEGER;
```

Status 字符串：`downloading | paused | completed | failed:msg | queued`
Parts 序列化为 JSON 字符串。
保存策略：全量替换（DELETE ALL + INSERT ALL, 每 ~60 帧）。

### 13.2 设置文件

TOML 格式，路径 `$HOME/Downloads/.pdm/pdm.toml`：

```toml
download_dir = "$HOME/Downloads"
max_connections = 8
max_retries = 10
user_agent = "..."
launch_at_startup = false

[proxies]
my-proxy = { protocol = "socks5", host = "127.0.0.1", port = 1080 }
```

### 13.3 日志

`$HOME/Downloads/.pdm/logs/proxydm.log`
- 缓冲写入，每 2048 字节 flush
- 格式 `[2026-07-03 14:30:00.123]`

---

## 十四、关键数据结构

### 下载项 (DownloadItem)

```rust
struct DownloadItem {
    id: u64,
    url: String,
    file_name: String,
    save_path: String,
    total_size: u64,
    downloaded: u64,
    status: DownloadStatus,
    parts: Vec<DownloadPart>,
    proxy_name: String,
    connections: u32,
    resumable: Option<bool>,
    merge_progress: f64,
    created_at: String,
    last_try: String,
}

struct DownloadPart {
    index: u32,
    start: u64,
    end: u64,
    downloaded: u64,
    temp_path: String,
    status: PartStatus,
    retries: u32,
}

enum DownloadStatus {
    Downloading, Paused, Completed, Failed(String), Queued,
}

enum PartStatus {
    Pending, Downloading, Completed, Failed(String),
}

enum ProxyProtocol {
    Http, Socks5,
}
```

### 运行时状态

```rust
struct ActiveDownload {
    cancels: Vec<Arc<AtomicBool>>,
    completed_parts: Arc<AtomicU32>,
}

struct PendingDownloadRequest {
    url: String,
    filename: String,
    proxy_name: String,
    connections: u32,
}
```

### 引擎配置 (DownloadConfig)

```rust
struct DownloadConfig {
    url: String,
    output_path: String,
    save_path: String,
    id: u64,
    file_name: String,
    is_resume: bool,
    headers: HashMap<String, String>,
    proxy_name: String,
    total_size: u64,
    supports_range: bool,
    rate_limit_bps: u64,
    connections: u32,
    max_retries: u32,
}
```

### 分块任务 (Task)

```rust
struct Task {
    offset: u64,
    length: u64,
    shared_max_offset: Arc<AtomicI64>,  // 对冲去重
}
```

### 速度追踪 (SpeedTracker)

EWMA 平滑，alpha=0.15，3s 采样窗口。

---

## 十五、运行时参数

| 参数 | 默认值 | 说明 |
|------|--------|------|
| MaxConnections | 32 | 单下载最大连接数 |
| MinChunkSize | 2MB | 最小 chunk |
| WorkerBufSize | 512KB | 读缓冲区 |
| MaxRetries | 3 | 任务重试次数 |
| SlowThreshold | 0.30 | 慢 worker 判定阈值 |
| StallTimeout | 3s | 卡死判定超时 |
| SpeedEmaAlpha | 0.3 | 速度平滑系数 |
| ConnectTimeout | 60s | 连接超时 |
| ReadTimeout | 600s | 读取超时 |
| TCPKeepalive | 60s | TCP 保活 |
| PoolIdleTimeout | 90s | 连接池空闲超时 |

---

## 十六、设计原则

1. **Rust 后端不变** — 下载引擎、持久化、类型定义是纯逻辑，直接复用
2. **UI 层可替换** — Tauri + HTML/CSS/React 前端通过 Tauri Commands 调用后端
3. **WebSocket 保持** — Port 18999 协议是扩展契约，向后兼容
4. **macOS 优先** — Accessory 策略、系统托盘、文件图标。然后 Windows/Linux
5. **每下载独立代理** — 核心差异化特性，非附属
6. **后台模式** — 应用常驻托盘，无需窗口。Tauri 内建 `app.setHidden()` + tray
7. **Think-in-Code** — 数据处理在沙箱中完成，仅派生结果进入上下文
8. **4KB 对齐** — 所有 chunk 对齐 4KB 边界，减少跨页 I/O 开销
9. 前端组件 全部使用 Primer.style 可以上 context7查询或 https://primer.style/product/components
10. 所有图标使用 https://primer.style/octicons/
11. 前端样式使用 组件库的默认样式
12. MacOS 窗口快捷键使用https://docs.rs/objc2/latest/objc2/
13. 要跨平台 Win/Linux/MacOS
14. 如果要用，请先使用 zustand/tanstack query router
