# Proxy Download Manager — Design Document

## Tech Stack
- **Language:** Rust (edition 2024)
- **GUI:** egui 0.34 / eframe 0.34
- **HTTP:** reqwest 0.13 (blocking + socks)
- **System Icons:** file_icon_provider 1.0 (macOS NSWorkspace)
- **Persistence:** serde_json (settings + download state)

## Architecture

### UI Layout

```
+--------------------------------------------------------------------------------+
| 📥 New Download | ▶ Resume | ⏹ Stop | 🗑 Delete  |  ←→  | ⚙ Settings | ℹ About | ❌ Quit |
+-------------------+------------------------------------------------------------+
| 📂 Downloads      |   File Name  |   Size    |  Status  | Proxy | Last Try |
| - 📁 All (N)      |  [system     |  12.34 MB | ⬇ 56%   | 🔌 p1 | 2026-06-07 |
| - ✅ Completed (N)|  icon + name]|           | ✅ Done  |   -   |      -     |
| - ⏳ Incomplete(N)|              |           | ⏸ Paused |   -  |    ...     |
|                   |              |           | ❌ Failed |      |            |
|                   |              |           |           |      |            |
+-------------------+------------------------------------------------------------+
```

### Features Implemented

#### 1. Menubar (Toolbar)
- **Left group:** 📥 New Download, ▶ Resume, ⏹ Stop, 🗑 Delete
- **Right group:** ⚙ Settings, ℹ About, ❌ Quit
- Resume enabled only for Paused/Failed items
- Stop enabled only for actively Downloading items

#### 2. Treeview (Sidebar)
- 📁 All (N) — shows all downloads
- ✅ Completed (N) — only completed
- ⏳ Incomplete (N) — everything else

#### 3. Table Columns
| Column | Width | Content |
|--------|-------|---------|
| File Name | Flexible | System file icon (macOS native via NSWorkspace) + filename |
| Size | 100px | Formatted size (B/KB/MB/GB) |
| Status | 180px | Status text + progress bar for active downloads |
| Proxy | 100px | 🔌 proxy-name or `-` for no proxy |
| Last Try | 120px | Timestamp of last activity |

#### 4. Multi-Thread Downloading
- **Architecture:** Download coordinator thread → N part threads
- **Connection count:** Up to 4 parallel connections (min 1MB per part)
- **Flow:** HEAD probe → check Accept-Ranges → split Range → N concurrent part threads
- **Merging:** All parts complete → merge_temp_part_files → cleanup `.partN` files
- **Fallback:** Single-thread if server doesn't support Range headers
- **Resume:** Each part tracks its own `downloaded` offset; completed parts skipped on resume

#### 5. Real Download Functionality
- HTTP/HTTPS with redirect support
- Chunked 64KB streaming with periodic progress updates (every 256KB)
- Per-part progress tracked independently, summed for total
- Graceful error handling: timeouts → Paused; other errors → Failed
- Background threads (std::thread::spawn) with cancel flags (AtomicBool)

#### 6. Proxy Lists
- Settings window has "Proxy Lists" section
- Named proxy entries: name, protocol (HTTP/SOCKS5), host, port, username, password
- Add / Edit / Delete proxies via popup editor
- Default proxy selector (combo box)
- Per-download proxy selection in New Download dialog
- Proxy column in table shows which proxy each item uses
- Proxy resolution: item.proxy_name → lookup in settings.proxies → build_client

#### 7. Pause / Resume
- **Pause:** Sets all cancel flags → part threads stop → per-part progress saved
- **Resume:** Completed parts skipped, pending parts restart with proper Range headers
- **Stop:** Same as Pause (marks as Paused for later resume)
- **Delete:** Cancels all threads, removes main file + all `.partN` temp files

#### 8. Settings
- Download Directory (text input)
- Proxy Lists with add/edit/delete
- Default Proxy selector
- Settings saved to `proxydm_settings.json`

#### 9. Dialogs
- **New Download:** URL input, optional filename override, proxy selector
- **Settings:** Directory, proxy lists
- **About:** App info

#### 10. Persistence
- Downloads saved to `proxydm_downloads.json` (auto-save every ~60 frames)
- Incomplete downloads marked as Paused on restart
- Settings saved to `proxydm_settings.json`

### Data Types

```rust
enum DownloadStatus { Downloading, Paused, Completed, Failed(String), Queued }

struct DownloadPart {
    index: u32, start: u64, end: u64, downloaded: u64,
    temp_path: String, status: PartStatus
}
enum PartStatus { Pending, Downloading, Completed, Failed(String) }

struct DownloadItem {
    id: u64, url, file_name, save_path, total_size, downloaded,
    status, last_try, created_at, parts: Vec<DownloadPart>,
    connections: u32, proxy_name: String
}

enum ProxyProtocol { Http, Socks5 }
struct ProxyEntry { name, protocol, host, port, username, password }
struct AppSettings { download_dir, proxies: Vec<ProxyEntry>, default_proxy: String }
```

### Dependencies

```toml
[dependencies]
eframe = "0.34"      # egui framework (native)
egui = "0.34"         # GUI toolkit
reqwest = { version = "0.13", features = ["blocking", "socks"] }
chrono = { version = "0.4", features = ["serde"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
anyhow = "1"
file_icon_provider = "1.0.1"  # macOS system file icons
```

### Build & Run

```bash
cargo run --release   # macOS arm64 binary: ~14MB
```
