# Architecture

Current layout of Proxy Download Manager. Historical notes live in `docs/archive/legacy-design.md` and are not an implementation reference.

## Process

One Tauri 2 app. The React UI talks to Rust only through Tauri commands (`src-tauri/src/cmd.rs`) and the event bus (`event_bus.rs`). List and settings screens read through TanStack Query (`src/query/`).

`EventBus` hands every event to a sink: Tauri's emitter in the app, a recorder in `download_manager/lifecycle_tests.rs`. Those tests drive start, pause, resume, queueing, failure and delete through `DownloadManager` against a local origin, and assert on ledger rows and emitted events.

## Download path

1. `DownloadManager::execute_download` probes the URL, applies the filename conflict policy, and inserts a row.
2. `WorkerPool` admits the task or queues it. Pause removes that worker's cancel token before the ledger is marked paused, so a late error from the old worker is dropped.
3. `run_download` picks an engine from `EngineConfig`:
   - `is_hls` → `HlsDownloader` (playlist resolve, segment files on disk, merge, then finalize)
   - `supports_range` → `ConcurrentDownloader` (one chunk-worker loop for the initial set and for workers added later)
   - otherwise → `SingleDownloader`
4. A ranged transfer that loses `Range` (`RangeLost` or `Incomplete`) is truncated and retried once with `SingleDownloader`. A download that was already single or HLS is not degraded.
5. A task is `Completed` only after the temp file is flushed, synced, and `finalize_file` renames it. The engine emits `DownloadCompleted` after that rename.

## Files

Partial bytes live at `{home}/temp/{id}.pdm`. HLS parts live at `{home}/temp/hls-{id}/`. Two tasks with the same filename do not share a temp file. `home` defaults to `~/.ProxyDM` and can be changed in Settings.

## Progress

Engines emit at most one progress event per 500ms, and skip the emit when bytes and the part snapshot are unchanged. A phase-only payload (`retrying`, `merging`, `connecting`) does not reset downloaded bytes. HLS with an unknown byte size reports segment counts in `downloaded` / `total_size`.

## Connections

`connections = 0` is Auto. The ledger stores 0. The live worker uses `auto_connections(file_size)`, capped by the settings maximum (itself 0 meaning the engine cap of 64).
