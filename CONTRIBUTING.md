# Contributing to ProxyDM

Thanks for your interest! ProxyDM is a small project, so every contribution counts.

## Table of Contents

- [Code of Conduct](#code-of-conduct)
- [Getting Started](#getting-started)
- [Development Workflow](#development-workflow)
- [Project Architecture](#project-architecture)
- [Coding Guidelines](#coding-guidelines)
- [Pull Request Process](#pull-request-process)
- [Browser Extension](#browser-extension)

## Code of Conduct

Be respectful, constructive, and assume good faith. This is a solo project opened to the public — keep it friendly.

## Getting Started

1. Fork the repository.
2. Install prerequisites:
   - [Rust](https://www.rust-lang.org/tools/install) (latest stable)
   - [Node.js](https://nodejs.org/) v20+
   - [pnpm](https://pnpm.io/installation)
   - Platform-specific [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/)
3. Clone your fork:
   ```bash
   git clone https://github.com/<your-username>/ProxyDownloadManager.git
   cd ProxyDownloadManager
   ```
4. Install dependencies:
   ```bash
   pnpm install
   ```
5. Start development:
   ```bash
   pnpm tauri dev
   ```

## Development Workflow

```bash
# One-shot: compile frontend + backend and open the app
pnpm tauri dev

# Frontend only (hot-reload in browser, no Tauri window)
pnpm dev

# Type-check frontend
npx tsc --noEmit

# Run frontend tests
pnpm test

# Build for production
pnpm tauri build
```

The Rust backend can also be checked independently:

```bash
cd src-tauri
cargo check       # fast type-check
cargo clippy      # lint
```

### Debugging

- Rust `eprintln!()` output appears in the terminal where `pnpm tauri dev` is running.
- Frontend `console.log()` appears in the WebView console (enable devtools via `tauri.conf.json`).
- Download logs are viewable inside the app: **Log** button in the toolbar.
- The WebSocket server listens on `ws://127.0.0.1:18999` for browser extension communication.

## Project Architecture

### Data Flow

```
Browser Extension                Desktop App
┌─────────────────┐             ┌──────────────────────────┐
│ Click download   │ WebSocket   │ WS Server (ws/server.rs) │
│ → cancel browser │ ──────────► │ → emit Tauri event       │
│   download       │             │ → NewDownload window     │
└─────────────────┘             └──────────────────────────┘
                                         │
                                         ▼
                                 ┌──────────────────┐
                                 │ cmd::start_download│
                                 │ → probe URL       │
                                 │ → spawn workers   │
                                 └──────────────────┘
                                         │
                                         ▼
                                 ┌──────────────────┐
                                 │ Worker Pool      │
                                 │ (worker.rs)      │
                                 │ ┌─ concurrent.rs │
                                 │ └─ single.rs     │
                                 └──────────────────┘
```

### Key Modules (Rust)

| Module | Responsibility |
|--------|---------------|
| `cmd.rs` | Tauri command handlers — the IPC boundary between frontend and backend |
| `engine/` | Download engines: `concurrent.rs` (multi-thread with Range), `single.rs` (single-connection fallback), `chunk.rs` (chunk computation) |
| `worker.rs` | Worker pool managing active downloads, cancellation, and rate limiting |
| `network/` | Reqwest HTTP client pool (per-proxy, TLS config), rate limiter |
| `ws/` | WebSocket server for browser extension communication |
| `state/` | SQLite database (`db.rs`), binary state snapshots (`gob.rs`), runtime progress (`runtime.rs`) |
| `config.rs` | TOML config loader/saver (~/.ProxyDM/ProxyDM.toml) |
| `probe.rs` | URL probing — detect file size, Range support, content-type |
| `types.rs` | Shared type definitions used across all modules |

### Key Modules (Frontend)

| Module | Responsibility |
|--------|---------------|
| `App.tsx` | Root component, event listeners, global state sync |
| `components/Layout.tsx` | Main layout shell |
| `components/Toolbar.tsx` | Top toolbar with action buttons |
| `components/DownloadTable.tsx` | Download list with context menu |
| `components/dialogs/` | All dialogs: Extension, Log, NewDownload, Properties, Settings, etc. |
| `i18n/` | Translation files (`en.ts`, `zh.ts` + index) |
| `stores/` | Zustand stores for UI state |

## Coding Guidelines

### General

- **Simple over clever** — prefer straightforward code. No speculative abstractions.
- **Match existing style** — consistency matters more than personal preference.
- **Touch only what you must** — don't reformat unrelated code or fix pre-existing issues unless they're directly in your change's path.

### Rust

- Return `Result<_, String>` from Tauri commands (error strings are shown to the frontend).
- Use `eprintln!()` for debug logging in the backend; use `Logger` (`log.rs`) for persistent download logs.
- Prefer `tokio::spawn` for async tasks; use `std::thread::spawn` only for long-running blocking work (e.g., the WebSocket server).
- Import style: group `std` → external crates → `crate::`.

### TypeScript / React

- Use functional components with hooks.
- Primer React components for UI consistency (Button, Dialog, Text, etc.).
- i18n: translate user-facing strings via `t('key')` from `src/i18n/`.
- State: Zustand for global UI state, TanStack Query for server-state (future).
- CSS: inline styles or Primer CSS variables; avoid separate CSS files for new components.

### Commit Messages

Use [Conventional Commits](https://www.conventionalcommits.org/):

```
feat: add download rate limiter
fix: handle connection timeout on slow proxies
docs: update browser extension installation guide
refactor: extract chunk computation into engine/chunk.rs
i18n: add Japanese translation
```

## Pull Request Process

1. **Open an issue first** for features or significant changes — discuss the design before coding.
2. Create a branch from `main` with a descriptive name:
   ```
   feat/rate-limiter
   fix/proxy-timeout
   docs/extension-guide
   ```
3. Keep PRs focused — one feature/fix per PR. Large changes should be broken into smaller PRs.
4. Update the README if your change adds or changes user-facing functionality.
5. Verify the app builds:
   ```bash
   pnpm tauri build
   ```
6. Verify types:
   ```bash
   npx tsc --noEmit
   cd src-tauri && cargo check
   ```
7. Open the PR with:
   - A clear title following conventional commits
   - A description explaining what changed and why
   - Screenshots for UI changes

## Browser Extension

The companion extension lives in `browsers-extension/`:

```
browsers-extension/
├── chrome/          # Manifest V3
│   ├── manifest.json
│   ├── background.js
│   ├── content.js
│   └── icons/
├── edge/            # Identical to Chrome (different store)
│   ├── manifest.json
│   ├── background.js
│   ├── content.js
│   └── icons/
└── firefox/         # Manifest V2
    ├── manifest.json
    ├── background.js
    └── icons/
```

### How it works

1. The extension's `background.js` connects to ProxyDM's WebSocket server at `ws://127.0.0.1:18999`.
2. When the user clicks a download link or uses the right-click menu, the extension intercepts the download, cancels the browser's native download, and sends the URL to ProxyDM.
3. ProxyDM opens the New Download window pre-filled with the URL.

Most changes to the extension logic live in `browsers-extension/chrome/background.js`. Keep Edge and Chrome in sync (they share identical code). Firefox uses Manifest V2 and may have minor differences.
