# ProxyDM — Proxy Download Manager

A desktop download manager built with **Rust** and **egui**. Supports multi-threaded downloads with HTTP/SOCKS5 proxy configuration, pause/resume, and persistent state.

![screenshot](https://img.shields.io/badge/platform-macOS%20%7C%20Windows%20%7C%20Linux-blue)
![Rust](https://img.shields.io/badge/rust-1.85%2B-orange)

## Features

- **Multi-threaded downloads** — automatically splits files into parts (one thread per part)
- **Pause / Resume** — resumes from where it stopped via HTTP Range headers
- **Proxy support** — HTTP and SOCKS5 proxies per download or globally
- **Persistent state** — SQLite database for downloads, TOML for settings
- **Speed tracker** — real‑time EWMA speed & ETA display
- **Multi‑select** — checkbox selection, batch resume/stop/delete
- **Context menus** — continue, stop, redownload, open file, properties
- **Download details** — per‑part progress, merge status, URL hyperlink
- **System file icons** — native icons on macOS via NSWorkspace
- **Cross‑platform** — runs on macOS, Windows, and Linux

## Quick Start

```bash
# Build (release, optimized for size)
cargo build --release

# Run
./target/release/proxydm
```

## Project Structure

```
src/
  main.rs      — Entry point, module declarations
  app.rs       — ProxyDownloadManager struct + lifecycle methods
  types.rs     — Data types, enums, constants, pure helpers
  persist.rs   — SQLite (downloads) + TOML (settings) persistence
  download.rs  — Multi‑thread download engine (coordinator + part threads)
  icons.rs     — System file icon cache (macOS native / cross‑platform fallback)
  ui.rs        — egui UI rendering (toolbar, sidebar, table, dialogs)
```

## Controls

| Button | Action |
|--------|--------|
| 📥 New Download | Open dialog to add a URL |
| ▶ Resume | Resume selected (or all) paused/failed downloads |
| ⏹ Stop | Stop selected (or all) active downloads |
| 🗑 Delete | Delete selected (or all) downloads |
| ⚙ Settings | Configure download directory, proxies, thread count |
| ℹ About | App info |

**Left sidebar** — filter by All / Completed / Incomplete.

**Table** — click row to toggle selection, double‑click for detail window.

## Data Storage

| Location | Content |
|----------|---------|
| `~/Downloads/.pdm/pdm.toml` | Settings (proxies, download dir, threads) |
| `~/Downloads/.pdm/downloads.db` | Download records (SQLite) |
| `~/Downloads/.pdm/parts/` | Temporary part files during download |

## Build Optimisation

The release profile is configured for **minimal binary size**:

```toml
[profile.release]
opt-level = "z"
lto = true
codegen-units = 1
strip = "symbols"
panic = "abort"
```

Typical binary size: **~14 MB** (macOS arm64).

## License

MIT
