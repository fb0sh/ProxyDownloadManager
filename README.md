# ProxyDM

> Multi-threaded download manager with proxy support — built with Tauri 2 + React 19.

ProxyDM intercepts browser downloads via a companion extension and takes over with multi-threaded downloading, proxy routing, and resume support. Think IDM, but open-source, cross-platform, and proxy-native.

![ProxyDM Screenshot](./docs/screenshot.png)

## Features

- **Multi-threaded downloads** — split files into chunks and download them in parallel (up to 32 connections)
- **Resume broken downloads** — supports HTTP `Range` requests; pick up where you left off
- **Proxy support** — HTTP and SOCKS5 proxies per-download or global
- **Browser extension** — Chrome/Edge/Firefox extension intercepts downloads and sends them to ProxyDM
- **Automatic URL detection** — clipboard monitoring detects download links
- **Redownload** — retry failed or missing files without re-adding the URL
- **Duplicate detection** — skips re-downloading files that already exist on disk; auto-redownload if missing
- **Download logging** — color-coded log levels, viewable in-app
- **IDM-style progress** — smooth animation per-thread progress display
- **System tray** — minimize to tray, background downloads, quick access
- **i18n** — English and Chinese (zh) interfaces
- **Cross-platform** — macOS, Windows, Linux

## Download

Download the latest release for your platform from the [Releases page](https://github.com/fb0sh/ProxyDownloadManager/releases).

| Platform | Format |
|----------|--------|
| macOS    | `.dmg` |
| Windows  | `.exe` / `.msi` |
| Linux    | `.deb` / `.rpm` / `.AppImage` |

## Browser Extension

ProxyDM ships with a companion browser extension that intercepts downloads and hands them to the desktop app.

**Supported browsers:** Chrome, Edge, Firefox

### macOS — Installation

> ProxyDM places the extension files in `~/Library/Application Support/com.fb0sh.proxydownloadmanager/extensions/`. To load them in your browser, you first need to reveal the `~/Library` folder in Finder.

1. Open **Finder**, then click **Go** in the menu bar.
2. Hold the <kbd>Option</kbd> key — **Library** appears in the dropdown menu. Select it.
   > Alternatively: open Finder, press <kbd>⌘⇧G</kbd> (Go to Folder), type `~/Library` and press Enter.
3. Navigate to `Application Support` → `com.fb0sh.proxydownloadmanager` → `extensions`.

#### Chrome / Edge

1. Open the browser and go to `chrome://extensions` (Chrome) or `edge://extensions` (Edge).
2. Enable **Developer mode** (toggle in the top-right corner).
3. Click **Load unpacked** and select the `chrome` (or `edge`) folder inside the `extensions` directory.
4. Enable the extension. Click the toolbar icon to toggle download interception on/off.

#### Firefox

1. Open Firefox and go to `about:debugging#/runtime/this-firefox`.
2. Click **Load Temporary Add-on…** and select the `manifest.json` file inside the `firefox` folder.
3. The extension loads temporarily; Firefox will remind you to re-load it after restart.

> **Note:** Firefox requires unsigned extensions to be loaded as temporary add-ons. Consider signing the extension for permanent installation if deploying to multiple machines.

### Windows / Linux — Installation

1. Open the app and click **Extensions** in the toolbar.
2. Click **Open Folder** to reveal the extensions directory.
3. Follow the same Chrome/Edge/Firefox steps above.

## Development

### Prerequisites

- [Node.js](https://nodejs.org/) (v20+)
- [pnpm](https://pnpm.io/)
- [Rust](https://www.rust-lang.org/) (edition 2021)
- Tauri system dependencies — see [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/)

### Setup

```bash
# Install frontend dependencies
pnpm install

# Start development
pnpm tauri dev
```

The desktop app opens with hot-reload enabled for both the Rust backend and the React frontend.

### Project Structure

```
├── src/                          # React frontend
│   ├── components/
│   │   ├── dialogs/              # Extension, Log, Properties, Settings, etc.
│   │   ├── DownloadTable.tsx
│   │   ├── Layout.tsx
│   │   └── Toolbar.tsx
│   ├── hooks/                    # Custom React hooks
│   ├── i18n/                     # en.ts, zh.ts
│   ├── stores/                   # Zustand stores
│   ├── query/                    # TanStack Query config
│   └── App.tsx                   # Root component
├── src-tauri/                    # Rust backend
│   └── src/
│       ├── cmd.rs                # Tauri commands (IPC handlers)
│       ├── lib.rs                # App setup, plugins, tray
│       ├── engine/               # Concurrent + single-thread download engines
│       ├── worker.rs             # Worker pool
│       ├── network/              # HTTP client pool, rate limiter
│       ├── ws/                   # WebSocket server (browser extension comm)
│       ├── state/                # SQLite DB, gob snapshots, runtime state
│       ├── config.rs             # TOML config loader
│       ├── probe.rs              # URL probing (size, range support)
│       └── types.rs              # Shared types & enums
├── browsers-extension/           # Companion browser extension
│   ├── chrome/
│   ├── edge/
│   └── firefox/
└── docs/                         # Design docs, screenshots
```

## Build

```bash
pnpm tauri build
```

The bundled application (`.dmg` / `.exe` / `.deb`) is output to `src-tauri/target/release/bundle/`.

## Tech Stack

| Layer       | Technology |
|-------------|-----------|
| Desktop     | [Tauri 2](https://v2.tauri.app/) |
| Frontend    | [React 19](https://react.dev/), [TypeScript](https://www.typescriptlang.org/), [Vite](https://vite.dev/) |
| UI          | [Primer React 38](https://primer.style/react/), [Octicons](https://primer.style/octicons/) |
| State       | [Zustand 5](https://github.com/pmndrs/zustand), [TanStack Query 5](https://tanstack.com/query) |
| Backend     | [Rust](https://www.rustlang.org/), [tokio](https://tokio.rs/), [reqwest 0.12](https://docs.rs/reqwest/) |
| Storage     | SQLite via [rusqlite](https://github.com/rusqlite/rusqlite), custom binary snapshots |
| Proxy       | HTTP / SOCKS5 via `reqwest` |
| Extensions  | Chrome MV3, Firefox Manifest V2 |

## Contributing

Contributions are welcome! Here's how to get started:

### Reporting Issues

- **Bug reports** — include the app version, operating system, and steps to reproduce. If possible, attach a log (open ProxyDM → **Log** → copy).
- **Feature requests** — describe the use case and any prior art you have in mind.

### Pull Requests

1. Fork the repo and create your branch from `main`.
2. If adding a feature, open an issue first to discuss the design.
3. Run `pnpm tauri build` to verify the app compiles.
4. Make sure the linter and type checker pass:
   ```bash
   npx tsc --noEmit
   ```
5. Update the README if your change affects the user-facing functionality.
6. Open a PR with a clear title and description.

### Development Conventions

- **Frontend** — React functional components with hooks. Uses Primer React for UI consistency. i18n keys in `src/i18n/`.
- **Backend** — Rust with Tauri commands in `cmd.rs`. Download engine logic in `engine/`. Error strings (not custom error types) in command return values.
- **Browser extension** — MV3 for Chrome/Edge, MV2 for Firefox. The extension communicates with the desktop app via WebSocket (`ws://127.0.0.1:18999`).
- **Commit style** — [Conventional Commits](https://www.conventionalcommits.org/): `feat:`, `fix:`, `docs:`, `refactor:`, etc.

### Localizing

Add a new language:

1. Create `src/i18n/xx.ts` exporting a `Translations` object.
2. Add it to the import map in `src/i18n/index.ts`.
3. Add the language code to `Settings.language` type in both Rust (`types.rs`) and TypeScript (`src/types.ts`).

## License

[MIT](./LICENSE) © fb0sh
