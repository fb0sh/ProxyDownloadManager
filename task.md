# Proxy Download Manager — Design Document

## Tech Stack
- **Language:** Rust (edition 2021)
- **GUI:** egui 0.34 / eframe 0.34
- **HTTP:** reqwest 0.13 (blocking + socks5)
- **Persistence:** rusqlite 0.32 (SQLite) + toml 0.8
- **System Icons:** file_icon_provider 1.0 (macOS NSWorkspace)
- **Clipboard:** arboard 3.4
- **File dialog:** rfd 0.15

## Module Architecture

```
main.rs          Entry point, module declarations
  ├── types.rs   Data types, enums, SpeedTracker, helpers
  ├── persist.rs SQLite + TOML load/save
  ├── download.rs Multi-threaded download engine
  ├── app.rs     ProxyDownloadManager struct + lifecycle
  ├── icons.rs   System file icon cache (cross-platform)
  └── ui.rs      egui UI rendering (impl eframe::App)
```

## UI Layout

```
+--------------------------------------------------------------------------------+
| 📥 New Download | ▶ Resume | ⏹ Stop | 🗑 Delete  |  ←→  | ⚙ Settings | ℹ About | ❌ Quit |
+-------------------+------------------------------------------------------------+
| 📂 Downloads      |  ☐  File Name  |   Size    |  Status  | Speed | Remain | Resume | Proxy | Last Try |
| - 📁 All (N)      |  ☑  [icon]name |  12.34 MB | ⬇ 56%   | 2.1MB/s| 12s   | ✅     | 🔌 p1 | 2026-06-07 |
| - ✅ Completed (N)|  ☐  [icon]name |           | ✅ Done  |  -    |  -    | ✅     |   -   |      -     |
| - ⏳ Incomplete(N)|  ☐  [icon]name |           | ⏸ Paused |  -    |  -    | ❌     |   -  |    ...     |
|                   |  ☐  [icon]name |           | ❌ Failed |       |       |        |      |            |
+-------------------+------------------------------------------------------------+
```

## Features Implemented

### 1. Toolbar
- **Left:** 📥 New Download, ▶ Resume, ⏹ Stop, 🗑 Delete
- **Right:** ⚙ Settings, ℹ About, ❌ Quit
- Resume enabled for Paused/Failed items; Stop for Downloading items
- **Batch mode:** when nothing selected, buttons act on ALL matching items

### 2. Sidebar (Tree View)
- 📁 All (N), ✅ Completed (N), ⏳ Incomplete (N)
- Status message display below

### 3. Table
| Column | Width | Content |
|--------|-------|---------|
| ☐/☑ | 28px | Multi-select checkbox |
| File Name | 192px | System file icon + truncated name |
| Size | 75px | Formatted size |
| Status | 120px | Status text + % for active |
| Speed | 80px | EWMA speed |
| Remain | 80px | ETA |
| Resume | 55px | ✅/❌ badge |
| Proxy | 55px | 🔌 proxy-name or `-` |
| Last Try | 120px | Timestamp |

### 4. Multi-Thread Downloading
- **Architecture:** Coordinator thread → HEAD probe → N part threads
- **Part size:** ~1MB per part, min(connections, total/1MB)
- **Flow:** HEAD → check Accept-Ranges → split Range → concurrent GETs
- **Merging:** All parts complete → merge_temp_part_files → cleanup `.partN`
- **Fallback:** 1 part if server doesn't support Range
- **Resume:** Each part tracked independently; completed parts skipped

### 5. Real Download Functionality
- HTTP/HTTPS with redirect support (up to 10)
- 64KB chunked streaming, progress update every 256KB
- Graceful error handling: timeouts → Paused; other errors → Failed
- Background `std::thread::spawn` with `AtomicBool` cancel flags

### 6. Proxy Lists
- Named proxy entries: name, protocol (HTTP/SOCKS5), host, port, username, password
- Add / Edit / Delete via popup editor
- Default proxy selector, per-download override

### 7. Pause / Resume
- **Pause:** Cancel flags → part threads stop → per-part progress saved
- **Resume:** Completed parts skipped, pending restart with correct Range
- **Stop:** Same as Pause
- **Delete:** Cancel threads, remove main file + all `.partN` files

### 8. Settings
- Download Directory (with browse dialog via rfd)
- Max threads per file (8/16/32/64)
- Proxy Lists with add/edit/delete
- Default Proxy selector
- Cache display + clear button

### 9. Dialogs
| Dialog | Content |
|--------|---------|
| **New Download** | URL (auto-filled from clipboard), filename, proxy, threads |
| **Edit Download** | Modify URL, filename, proxy, threads |
| **Properties** | Full metadata: name, URL, path, size, status, proxy, parts, dates |
| **Confirm Delete** | Delete record only (keep file) or delete file + record |
| **Download Progress** | Auto/manual detail window with progress bar, per-part progress, merge status |
| **About** | App info |

### 10. Selection & Batch Operations
- **Multi-select:** Checkbox in each row; click row to toggle
- **Select all:** Header checkbox to toggle all visible items
- **Batch Resume:** Resume all selected paused/failed items
- **Batch Stop:** Stop all selected downloading items
- **Batch Delete:** Delete all selected items
- When nothing selected: toolbar buttons act on ALL matching items

### 11. Detail Window Rules
- **Toolbar Resume (batch, >1 item):** No detail window
- **Toolbar Resume (single item):** Opens detail window
- **Context menu ▶ Continue:** Opens detail window
- **Double-click row:** Opens detail window
- **Toolbar Stop (any):** No detail window

### 12. Persistence
- Downloads saved to SQLite (`downloads.db`), auto-save every ~60 frames
- Incomplete downloads marked as Paused on restart
- Settings saved to TOML (`pdm.toml`)
- All data in `~/Downloads/.pdm/`

### 13. Cross-Platform
| Action | macOS | Windows | Linux |
|--------|-------|---------|-------|
| Open file | `open` | `explorer` | `xdg-open` |
| Show in folder | `open -R` | `explorer /select,` | `xdg-open` parent dir |
| File icons | NSWorkspace | Fallback (generic) | Fallback (generic) |
| File dialog | rfd (native) | rfd (native) | rfd (native) |

## Data Types

```rust
enum DownloadStatus { Downloading, Paused, Completed, Failed(String), Queued }

struct DownloadPart { index: u32, start, end, downloaded: u64, temp_path, status: PartStatus }
enum PartStatus { Pending, Downloading, Completed, Failed(String) }

struct DownloadItem {
    id: u64, url, file_name, save_path, total_size, downloaded,
    status, last_try, created_at, parts: Vec<DownloadPart>,
    connections: u32, proxy_name: String, resumable: Option<bool>
}

enum ProxyProtocol { Http, Socks5 }
struct ProxyEntry { name, protocol, host, port, username, password }
struct AppSettings { download_dir, proxies, default_proxy, max_connections }
```

## Build & Run

```bash
cargo run --release   # macOS arm64 binary: ~14MB
```

## Performance

- Binary size: ~14 MB (release, stripped, LTO)
- Threads: up to 64 per download, configurable globally or per-download
- Speed tracker: EWMA with α=0.15, updates every 256KB
- Save interval: every 60 frames (~1 second at 60fps)
