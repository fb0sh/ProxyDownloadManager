# ProxyDM

Multi-threaded download manager built with Tauri 2 + React 19 + Primer React.

## Features

- Multi-threaded downloads (configurable connections per task)
- Resume broken downloads (supports Range requests)
- Proxy support (HTTP/SOCKS5)
- Automatic URL detection from clipboard
- Redownload failed/missing files
- Download logs with level coloring
- Duplicate URL detection (auto-redownload if file missing, notification if exists)
- IDM-style progress display with smooth animation
- Right-click context menu (Stop, Delete, Open, Open in Folder, Redownload, Details)
- System tray integration

## UI

GitHub-style design using Primer React components. Dialogs use bordered card sections with uppercase headers, consistent spacing, and status badges.

## Development

```bash
pnpm install
pnpm tauri dev
```

## Build

```bash
pnpm tauri build
```

## Tech Stack

- **Frontend**: React 19, Primer React 38, TanStack Query 5, Zustand 5
- **Backend**: Rust, Tauri 2, tokio, reqwest 0.12
- **Storage**: SQLite (rusqlite), custom binary state snapshots
