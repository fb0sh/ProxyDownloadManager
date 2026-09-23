# Neat Download Manager Parity Plan

Status: implemented (see CHANGELOG Unreleased)  
Baseline: ProxyDownloadManager 0.11.0 (Tauri 2 + React 19 + Primer)

This document is the working map for the Neat-parity work in `docs/refact.md`, plus the Primer → shadcn/ui migration.

## 现状

The engine, ledger, and resume path are already stronger than the 0.9 era:

- `ProgressLedger` owns progress; crash recovery marks stale `Downloading` rows `Paused` and reconciles gob + DB.
- Concurrent engine degrades to single only on `RangeLost` / `Incomplete`.
- `.pdm` temp files, cancel snapshots, and 500ms progress events exist.
- Probe already accepts a headers map; `EngineConfig.headers` exists.
- Per-download proxy selection and a token-bucket `MultiLimiter` exist.
- Browser extension can intercept downloads and ACK *any* WebSocket message.

The product is not yet a daily driver at Neat's level because the **browser → engine request context is empty**, connection caps disagree with the UI, runtime controls are missing, and the New Download / table UX is too thin.

## Gap Matrix

| Capability | Now | Target | Phase |
|---|---|---|---|
| Browser intercept sends URL only | `sendReliable(url)` | Structured `DownloadRequest` + ACK | A |
| ACK is "any WS message" | `onmessage → success` | `{request_id, accepted, reason}` | A |
| Desktop offline drops download | cancel only after ACK (good) | keep that guarantee | A |
| Probe / engine headers | `execute_download` uses `HashMap::new()` | Cookie / Referer / Auth / UA on probe + every Range GET | A |
| `PendingDownloadRequest` | url, filename, proxy, connections | + final_url, method, referrer, UA, cookies, headers, tab_url, content_type, content_length | A |
| Crash recovery | Downloading → Paused | also Connecting / Retrying / Merging; keep progress | B |
| Status set | Queued / Downloading / Paused / Completed / Failed(msg) | + Connecting / Retrying / Merging; Failed details | B |
| Connections cap | UI 64, engine `.min(32)` | hard max 64; 0 = Auto by size | C |
| ChunkQueue | FIFO only | split / steal remaining large ranges | C |
| Runtime connections | no | 1↔64 without pause | C |
| Runtime rate limit | limiter bps is fixed at spawn | global + per-task `set_bps` | D |
| Retry policy | generic Fatal retry | classify 4xx vs timeout/5xx; jittered backoff; proxy failover | D |
| Proxy auth | host:port only | username/password; HTTP / HTTPS CONNECT / SOCKS5 | E |
| New Download | URL + path string | async probe, folder picker, Download Later | F |
| Task list | dense Primer table, extra columns | Name / Size / Progress / Speed / ETA / Status; memoized rows | F |
| Speed / ETA | rolling window exists | 3–5s window; row-level re-render | F |
| Refresh URL | none | probe + ETag/size check before resume | G |
| Media capture | context menu only | webRequest sniff + popup | G |
| HLS | none | master → variant → concurrent `.ts` merge | G |
| File conflict | always rename | Ask / Rename / Overwrite / Skip | G |
| Auth redaction | logs print raw WS text | `<redacted>` for Cookie / Authorization | G |
| UI kit | Primer + inline styles | shadcn/ui + Tailwind v4 + Lucide | UI |

Verified (do not re-solve):

1. UI offers 64 connections; backend clamps to 32 — **true** (`chunk.rs`, `concurrent.rs`, `download_manager.rs`).
2. Extension `sendReliable()` sends a raw URL — **true**.
3. `PendingDownloadRequest` cannot carry headers — **true**.
4. `execute_download` probes with empty headers; `to_engine_config` / `into_engine_config` also zero headers — **true**.
5. `ProxyConfig` has protocol/host/port only — **true**.
6. `PropertiesDialog` is read-only — **true**.
7. Extension has no media sniffer — **true**.
8. Refresh URL is unimplemented — **true**.
9. README says 64 threads — **true, and currently a lie**.

## 涉及文件

### Backend
- `src-tauri/src/types/{download,config,engine_config,error}.rs`
- `src-tauri/src/headers.rs` *(new)* — allow-list, hop-by-hop filter, redaction
- `src-tauri/src/retry.rs` *(new)* — retry class, jittered backoff, proxy stats
- `src-tauri/src/ws/server.rs`
- `src-tauri/src/probe.rs`
- `src-tauri/src/engine/{chunk,concurrent,single,task_download}.rs`
- `src-tauri/src/engine/hls.rs` *(new)*
- `src-tauri/src/network/{limiter,pool}.rs`
- `src-tauri/src/worker.rs`
- `src-tauri/src/download_manager.rs`
- `src-tauri/src/state/{db,ledger}.rs`
- `src-tauri/src/cmd.rs`, `lib.rs`
- `src-tauri/src/services/settings_service.rs`
- `src-tauri/src/logger.rs`

### Extension
- `browsers-extension/shared/{background.js,protocol.js,popup.html,popup.js,content.js}`
- `browsers-extension/{chrome,edge,firefox}/manifest.json`
- `browsers-extension/build.sh`

### Frontend
- `src/types.ts`, `src/tauriClient.ts`, `src/downloadEvents.ts`
- `src/components/**`, `src/hooks/**`, `src/i18n/**`
- `src/NewDownloadWindow.tsx`, `src/DownloadDetailsWindow.tsx`
- `src-present/**` (demo must follow the same kit)

## 数据结构变化

`PendingDownloadRequest` gains `protocol_version`, `request_id`, `action`, `final_url`, `method`, `referrer`, `user_agent`, `cookies`, `headers`, `tab_url`, `content_type`, `content_length`. All new fields `serde(default)`. Raw URL and old `{action,url,filename}` still parse.

`DownloadItem` gains persisted request context and fail details:

- `headers`, `final_url`, `content_type`, `etag`, `last_modified`
- `rate_limit_bps`
- `error_code`, `error_message`, `http_status`, `retry_count`, `last_error_at`

`DownloadStatus` adds `Connecting`, `Retrying`, `Merging`. `Failed(String)` stays; extra fail columns carry structured detail. Old DB `failed:…` strings still parse.

`ProxyConfig` adds `username`, `password` (`serde(default)`). Protocol enum adds `https` (HTTP CONNECT via `https://` proxy URL). Map key remains the proxy name.

`Settings` adds `file_conflict` (`rename` default), keeps `max_connections = 0` as Auto, hard cap 64.

`EngineConfig.headers` is filled from the item (no more empty map). Optional `desired_connections: Arc<AtomicU32>` for live worker scaling.

WS ACK:

```json
{ "protocol_version": 1, "request_id": "…", "accepted": true, "reason": "" }
```

## 数据库 migration

SQLite `ALTER TABLE` for new columns, guarded by `PRAGMA table_info`. Existing DBs keep working.

New columns on `downloads`:

| Column | Type | Default |
|---|---|---|
| headers | TEXT | `'{}'` |
| final_url | TEXT | `''` |
| content_type | TEXT | `''` |
| etag | TEXT | `''` |
| last_modified | TEXT | `''` |
| rate_limit_bps | INTEGER | `0` |
| error_code | TEXT | `''` |
| error_message | TEXT | `''` |
| http_status | INTEGER | NULL |
| retry_count | INTEGER | `0` |
| last_error_at | TEXT | `''` |

Status strings `connecting` / `retrying` / `merging` are added in `parse_status` / `status_to_string`. Crash recovery treats those live states like `downloading`.

## Phase A–G

### A — Browser request context + structured WS
Extension builds a filtered header set (Cookie, Referer, Origin, User-Agent, Authorization, Accept, Accept-Language). Hop-by-hop and `Sec-*` dropped. `sendReliable` sends JSON, waits for ACK matching `request_id`. ACK failure keeps the browser download. Desktop `parse_message` accepts v1 JSON and raw URL. `execute_download` / resume pass headers into probe and every GET.

### B — Crash recovery + resume
Startup recovery covers every in-flight status. Failed rows store code/message/status/retry/time. Pause during retry/backoff still snapshots the popped task (already true; keep tests). Add coverage for unknown length, ignore-Range, 99%, queued.

### C — Dynamic segmentation + runtime connections
`MAX_CONNECTIONS = 64`. Auto:

- `< 2 MiB` → 1
- `2–16 MiB` → 4
- `16–128 MiB` → 8
- `128 MiB–1 GiB` → 16
- `> 1 GiB` → 32 (up to 64 if requested)

`ChunkQueue` splits remaining large tasks (min 2 MiB) so idle workers steal. Concurrent pool watches `desired_connections`. Properties can change connections live; value persisted.

### D — Runtime bandwidth + retry/failover
`RateLimiter::set_bps`. Worker pool holds a shared global limiter and a per-task limiter. Toolbar sets global; properties set per-task. Retryable: timeout, reset, temp DNS, 408/429/500/502/503/504, partial stream. Not auto-retried: 400/401/403/404/405. Backoff 1,2,4,8,16,30s + jitter. Proxy group: A → B → C → optional Direct. Per-proxy success/fail/latency recorded in memory.

### E — Proxy authentication
Basic userinfo in the proxy URL. Password never logged. UI: Test, latency, username/password, status. Task can pick Direct / default / named proxy.

### F — UX cleanup
New Download: debounce probe (filename, size, type, server, range, final URL, suggested connections). Folder picker. Download + Download Later. Browser-sourced requests fill URL/filename/headers without showing Cookie/Authorization. Table: search, All / Downloading / Completed / Incomplete, type filter. Context menu + double-click. Memoized rows. Speed window 3–5s.

### G — Media, HLS, refresh, conflicts, occupancy
Refresh URL re-probes; mismatch warns. Extension popup: connected, capture toggle, media count, skip-once (Alt / Delete). HLS: parse master/media, concurrent `.ts`, concat; AES-128/DRM → explicit unsupported. File conflict policy. Progress DB flush 3s (events stay 500ms). Log redaction.

### UI — Primer → shadcn
Tailwind v4, CSS variables, Lucide, shadcn primitives. Dense desktop chrome (toolbar + filter strip + table). Teal conduit accent, 4px radius, no cards-for-everything, no gradients/glass. Remove `@primer/react`, `@primer/primitives`, `@primer/octicons-react`. Demo (`src-present`) uses the same kit.

## 风险

- Header allow-list too strict → 403 on exotic CDNs. Mitigation: allow-list + pass through caller-supplied names that already survived the hop-by-hop filter.
- Storing Cookie/Authorization in SQLite. Mitigation: local-only DB, never toast/log/properties plaintext.
- Live worker shrink while a large segment is in flight. Mitigation: finish or split the current segment, then exit extras.
- HLS variant UX. V1 auto-picks highest bandwidth; UI can pass a variant URI later.
- shadcn + Tauri child windows: each webview loads CSS variables independently.
- Firefox MV3 `webRequest` + `host_permissions` already present; Chrome needs them added.

## 测试方案

Rust: Range / non-Range / ignore-Range, pause/resume, crash recovery, 403/429/500, redirect + Cookie, Referer, Basic Auth, header filter, connection Auto, chunk steal, rate limiter `set_bps`, retry class, proxy URL with userinfo (redacted), HLS playlist parse, unique filename policies, WS v1/v0 parse + ACK.

Frontend: status helpers, filter/search, speed/ETA window, probe payload shape, runtime settings client, memoized row identity, extension protocol (ACK success/fail, header filter, bypass).

Commands that must stay green: `cargo check`, `cargo test`, `npx tsc --noEmit`, `pnpm test`.
