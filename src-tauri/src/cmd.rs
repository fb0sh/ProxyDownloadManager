use crate::types::*;
use crate::state::db::Db;
use crate::worker::WorkerPool;
use crate::config;
use crate::log::Logger;
use crate::icons::{IconCache, IconData};
use crate::state::runtime::DownloadManagerState;
use std::process::Command as StdCommand;
use tauri::State;
use std::sync::Arc;
use std::sync::Mutex;
use tauri::Emitter;

pub struct AppState {
    pub db: Db,
    pub worker_pool: WorkerPool,
    pub logger: Mutex<Logger>,
    pub app_handle: tauri::AppHandle,
    pub runtime: DownloadManagerState,
}

impl AppState {
    pub async fn handle_event(&self, event: Event) {
        let id = event.download_id;
        let data_suffix = event.data.as_ref().map(|d| format!(" data={}", d)).unwrap_or_default();

        // Get URL for logging (single query, not full scan)
        let url_info = if let Ok(Some(item)) = self.db.get_by_id(id) {
            format!(" url={}", item.url)
        } else { String::new() };

        let log_msg = format!("Event: {:?} id={}{}{}", event.kind, id, url_info, data_suffix);
        if let Ok(l) = self.logger.lock() {
            l.info(&log_msg);
        }

        // Notify frontend on error
        if matches!(event.kind, EventKind::DownloadErrored) {
            let msg = event.data.clone().unwrap_or_default();
            let url = url_info.trim_start_matches(" url=").to_string();
            let _ = self.app_handle.emit("download-error", serde_json::json!({
                "id": id,
                "url": url,
                "message": msg,
            }));
        }

        match event.kind {
            EventKind::DownloadStarted => {
                self.runtime.register(id);
                let _ = self.app_handle.emit("download-started", id);
            }
            EventKind::DownloadCompleted => {
                self.runtime.remove(id);
                if let Ok(Some(mut item)) = self.db.get_by_id(id) {
                    item.status = DownloadStatus::Completed;
                    for part in item.parts.iter_mut() {
                        if !matches!(part.status, PartStatus::Completed) {
                            part.status = PartStatus::Completed;
                        }
                    }
                    let _ = self.db.update_download(&item);
                }
                let _ = self.app_handle.emit("download-completed", id);
            }
            EventKind::DownloadErrored => {
                self.runtime.remove(id);
                if let Ok(Some(mut item)) = self.db.get_by_id(id) {
                    if matches!(item.status, DownloadStatus::Paused) {
                        return;
                    }
                    item.status = DownloadStatus::Failed(event.data.unwrap_or_default());
                    for part in item.parts.iter_mut() {
                        if matches!(part.status, PartStatus::Pending | PartStatus::Downloading) {
                            part.status = PartStatus::Failed(format!("download failed"));
                        }
                    }
                    let _ = self.db.update_download(&item);
                }
            }
            EventKind::DownloadProgress => {
                if let Some(data) = &event.data {
                    if let Ok(downloaded) = data.parse::<u64>() {
                        self.runtime.update_progress(id, downloaded);
                        // Real-time event so frontend doesn't wait for DB flush
                        let _ = self.app_handle.emit("download-progress", serde_json::json!({
                            "id": id,
                            "downloaded": downloaded,
                        }));
                    }
                }
            }
            _ => {}
        }
    }
}

fn resolve_proxy_url(proxy_name: &str) -> Option<String> {
    if proxy_name.is_empty() {
        return None;
    }
    let settings = config::load();
    let proxy = settings.proxies.get(proxy_name)?;
    let protocol = match proxy.protocol {
        ProxyProtocol::Http => "http",
        ProxyProtocol::Socks5 => "socks5",
    };
    Some(format!("{}://{}:{}", protocol, proxy.host, proxy.port))
}

fn now_str() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let dur = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    // Format as ISO-like datetime string
    let secs = dur.as_secs();
    // Simple formatting: days since epoch
    format!("{}", secs)
}

/// Return a unique file path in `dir` for `filename`.
/// If `dir/filename` exists, try `dir/name.1.ext`, `dir/name.2.ext`, etc.
fn unique_filename(dir: &str, filename: &str) -> String {
    let dir = dir.trim_end_matches('/');
    let candidate = format!("{}/{}", dir, filename);
    if !std::path::Path::new(&candidate).exists() {
        return candidate;
    }
    // Split into stem and extension
    let (stem, ext) = match filename.rfind('.') {
        Some(dot) => (&filename[..dot], &filename[dot..]),
        None => (filename, ""),
    };
    let mut n = 1;
    loop {
        let candidate = format!("{}/{}.{}{}", dir, stem, n, ext);
        if !std::path::Path::new(&candidate).exists() {
            return candidate;
        }
        n += 1;
    }
}

#[tauri::command]
pub async fn redownload_download(
    state: State<'_, Arc<AppState>>,
    id: u64,
) -> Result<u64, String> {
    let items = state.db.list_downloads()?;
    let existing = items.iter().find(|i| i.id == id)
        .ok_or_else(|| format!("Download {} not found", id))?.clone();

    {
        if let Ok(l) = state.logger.lock() {
            l.info(&format!("Redownload start id={} url={}", id, existing.url));
        }
    }

    let mut updated = existing.clone();
    updated.downloaded = 0;
    updated.status = DownloadStatus::Downloading;
    updated.last_try = now_str();
    state.db.update_download(&updated)?;
    // Reset progress immediately so frontend doesn't bounce
    state.runtime.register(id);
    let _ = state.app_handle.emit("download-progress", serde_json::json!({
        "id": id,
        "downloaded": 0u64,
    }));

    let pool = state.worker_pool.pool_ref();
    let headers = std::collections::HashMap::new();
    let proxy_url = resolve_proxy_url(&existing.proxy_name).unwrap_or_default();
    let proxy_opt = if proxy_url.is_empty() { None } else { Some(proxy_url.as_str()) };
    let settings = config::load();
    let user_agents = vec![
        settings.user_agent.clone(),
        "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/125.0.0.0 Safari/537.36".to_string(),
        "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/125.0.0.0 Safari/537.36".to_string(),
        "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:127.0) Gecko/20100101 Firefox/127.0".to_string(),
    ];

    let probe_result = crate::probe::probe(&existing.url, &headers, proxy_opt, &pool, &user_agents).await;

    let (file_size, supports_range) = match probe_result {
        Ok(r) => {
            if let Ok(l) = state.logger.lock() {
                l.info(&format!("Probe ok url={} size={} range={}", existing.url, r.file_size, r.supports_range));
            }
            (r.file_size, r.supports_range)
        }
        Err(e) => {
            if let Ok(l) = state.logger.lock() {
                l.warn(&format!("Probe failed, blind redownload url={} err={}", existing.url, e));
            }
            (0, false)
        }
    };

    let cfg = DownloadConfig {
        url: existing.url.clone(),
        output_path: existing.save_path.clone(),
        save_path: existing.save_path,
        id,
        file_name: existing.file_name,
        is_resume: false,
        headers: std::collections::HashMap::new(),
        proxy_name: proxy_url,
        total_size: file_size,
        supports_range,
        rate_limit_bps: 0,
        connections: existing.connections,
        max_retries: 3,
        user_agent: settings.user_agent.clone(),
    };

    state.worker_pool.add_with_id(cfg, id).await
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
    save_path: String,
    proxy_name: String,
    connections: u32,
) -> Result<u64, String> {
    // Log download start with URL
    {
        if let Ok(l) = state.logger.lock() {
            l.info(&format!("Download start url={} proxy={}", url, proxy_name));
        }
    }

    let pool = state.worker_pool.pool_ref();
    let headers = std::collections::HashMap::new();

    let proxy_url_str = resolve_proxy_url(&proxy_name);
    let proxy_opt = proxy_url_str.as_deref();

    // Build UA rotation list: config UA > Chrome > Firefox
    let settings = config::load();
    let user_agents = vec![
        settings.user_agent.clone(),
        "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/125.0.0.0 Safari/537.36".to_string(),
        "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/125.0.0.0 Safari/537.36".to_string(),
        "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:127.0) Gecko/20100101 Firefox/127.0".to_string(),
    ];

    let probe_result = crate::probe::probe(&url, &headers, proxy_opt, &pool, &user_agents).await;

    let (file_name, file_size, supports_range) = match probe_result {
        Ok(r) => {
            if let Ok(l) = state.logger.lock() {
                l.info(&format!("Probe ok url={} size={} range={} name={}", url, r.file_size, r.supports_range, r.file_name));
            }
            let name = if filename.is_empty() { r.file_name } else { filename };
            (name, r.file_size, r.supports_range)
        }
        Err(e) => {
            // Probe completely failed → force single download, unknown size
            if let Ok(l) = state.logger.lock() {
                l.warn(&format!("Probe failed, forcing blind download url={} err={}", url, e));
            }
            let name = if filename.is_empty() {
                std::path::Path::new(&url)
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| "download".to_string())
            } else {
                filename
            };
            (name, 0, false)
        }
    };

    let settings = config::load();
    let max_conns = settings.max_connections.max(1).min(32);

    // Connection count: 0 = auto (based on file size), >0 = user-specified
    let connections = if connections > 0 {
        connections.min(32)
    } else if file_size == 0 {
        max_conns.min(2)
    } else if file_size < 100 * 1024 * 1024 {
        max_conns.min(2)
    } else if file_size < 1024 * 1024 * 1024 {
        max_conns.min(4)
    } else if file_size < 10u64 * 1024 * 1024 * 1024 {
        max_conns.min(8)
    } else {
        max_conns.min(16)
    };
    let save_dir = if save_path.is_empty() {
        settings.download_dir
    } else {
        save_path
    };
    let full_path = unique_filename(&save_dir, &file_name);

    // Check available disk space before starting
    if file_size > 0 {
        let pdm_path = format!("{}.pdm", full_path);
        if let Some(parent) = std::path::Path::new(&pdm_path).parent() {
            if let Ok(available) = fs2::available_space(parent) {
                let needed = file_size + (2u64 * 1024 * 1024); // extra 2MB margin
                if available < needed {
                    return Err(format!(
                        "Insufficient disk space: need {}, available {}",
                        needed, available
                    ));
                }
            }
        }
    }

    let cfg = DownloadConfig {
        url: url.clone(),
        output_path: full_path.clone(),
        save_path: full_path.clone(),
        id: 0,
        file_name: file_name.clone(),
        is_resume: false,
        headers: std::collections::HashMap::new(),
        proxy_name: proxy_url_str.clone().unwrap_or_default(),
        total_size: file_size,
        supports_range,
        rate_limit_bps: 0,
        connections,
        max_retries: 3,
        user_agent: settings.user_agent.clone(),
    };

    let id = state.worker_pool.add(cfg).await?;

    // Compute initial chunk layout for thread progress display
    let parts = if supports_range && file_size > 0 {
        let num_conns = if connections > 0 { connections.min(32) } else { 1 };
        let min_chunk = 2u64 * 1024 * 1024; // 2MB
        let tasks = crate::engine::chunk::compute_chunks(file_size, num_conns, min_chunk);
        tasks.iter().enumerate().map(|(i, t)| DownloadPart {
            index: i as u32,
            start: t.offset,
            end: t.offset + t.length,
            downloaded: 0,
            temp_path: String::new(),
            status: PartStatus::Pending,
            retries: 0,
        }).collect()
    } else {
        vec![DownloadPart {
            index: 0,
            start: 0,
            end: file_size,
            downloaded: 0,
            temp_path: String::new(),
            status: PartStatus::Pending,
            retries: 0,
        }]
    };

    let item = DownloadItem {
        id,
        url,
        file_name,
        save_path: full_path,
        total_size: file_size,
        downloaded: 0,
        status: DownloadStatus::Downloading,
        parts,
        proxy_name,
        connections,
        resumable: Some(supports_range),
        merge_progress: 0.0,
        created_at: now_str(),
        last_try: String::new(),
    };
    let _ = state.db.insert_download(&item);

    Ok(id)
}

#[tauri::command]
pub async fn pause_download(state: State<'_, Arc<AppState>>, id: u64) -> Result<(), String> {
    state.worker_pool.cancel(id).await;
    if let Ok(Some(mut item)) = state.db.get_by_id(id) {
        if matches!(item.status, DownloadStatus::Downloading) {
            // Save state for resume: compute remaining tasks from incomplete parts
            let mut tasks = Vec::new();
            for part in &item.parts {
                let remaining = part.end - part.downloaded;
                if remaining > 0 {
                    tasks.push(Task {
                        offset: part.start + part.downloaded,
                        length: remaining,
                    });
                }
            }
            if !tasks.is_empty() {
                let saved = crate::state::gob::DownloadState {
                    url: item.url.clone(),
                    id: item.id,
                    file_name: item.file_name.clone(),
                    save_path: item.save_path.clone(),
                    total_size: item.total_size,
                    downloaded: item.downloaded,
                    tasks,
                    elapsed_secs: 0,
                    chunk_bitmap: Vec::new(),
                    actual_chunk_size: 0,
                    proxy_name: item.proxy_name.clone(),
                    workers: item.connections,
                    min_chunk_size: 0,
                };
                let _ = crate::state::gob::save_state(id, &saved);
            }
            item.status = DownloadStatus::Paused;
            let _ = state.db.update_download(&item);
        }
    }
    Ok(())
}

#[tauri::command]
pub async fn resume_download(state: State<'_, Arc<AppState>>, id: u64) -> Result<(), String> {
    if let Ok(Some(saved_state)) = crate::state::gob::load_state(id) {
        // Update DB status to Downloading before spawn
        if let Ok(mut items) = state.db.list_downloads() {
            if let Some(item) = items.iter_mut().find(|i| i.id == id) {
                item.status = DownloadStatus::Downloading;
                item.last_try = now_str();
                let _ = state.db.update_download(item);
            }
        }

        let proxy_url = resolve_proxy_url(&saved_state.proxy_name).unwrap_or_default();
        let cfg = DownloadConfig {
            url: saved_state.url,
            output_path: saved_state.save_path.clone(),
            save_path: saved_state.save_path,
            id,
            file_name: saved_state.file_name,
            is_resume: true,
            headers: std::collections::HashMap::new(),
            proxy_name: proxy_url,
            total_size: saved_state.total_size,
            supports_range: true,
            rate_limit_bps: 0,
            connections: saved_state.workers,
            max_retries: 3,
            user_agent: config::load().user_agent,
        };
        state.worker_pool.add_with_id(cfg, id).await?;
    } else {
        // No saved state — fall back to redownload
        return redownload_download(state, id).await.map(|_| ());
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
        let pdm_path = p.with_extension("pdm");
        if pdm_path.exists() {
            let _ = std::fs::remove_file(&pdm_path);
        }
        if p.exists() {
            let _ = std::fs::remove_file(p);
        }
    }
    Ok(())
}

#[tauri::command]
pub fn get_settings() -> Result<Settings, String> {
    Ok(crate::config::load())
}

#[tauri::command]
pub fn save_settings(state: State<'_, Arc<AppState>>, settings: Settings) -> Result<(), String> {
    let old = crate::config::load();
    let tls_changed = old.danger_accept_invalid_certs != settings.danger_accept_invalid_certs;
    crate::config::save(&settings)?;
    if tls_changed {
        state.worker_pool.clear_clients();
    }
    Ok(())
}

#[tauri::command]
pub fn exit_app(app: tauri::AppHandle) {
    app.exit(0);
}

#[tauri::command]
pub fn read_logs(max_lines: Option<usize>) -> Result<Vec<String>, String> {
    crate::log::read_logs(max_lines.unwrap_or(30))
}

#[tauri::command]
pub fn file_exists(path: String) -> bool {
    std::path::Path::new(&path).exists()
}

#[tauri::command]
pub fn get_file_icon(
    icon_cache: State<'_, IconCache>,
    file_name: String,
) -> IconData {
    icon_cache.get(&file_name)
}

/// Open a file with the system default application, bypassing opener plugin scope.
/// Safe because the user explicitly clicked Open.
#[tauri::command]
pub fn open_file(path: String) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    let status = StdCommand::new("open").arg(&path).status();
    #[cfg(target_os = "windows")]
    let status = StdCommand::new("cmd").args(["/c", "start", "", &path]).status();
    #[cfg(target_os = "linux")]
    let status = StdCommand::new("xdg-open").arg(&path).status();

    match status {
        Ok(s) if s.success() => Ok(()),
        Ok(s) => Err(format!("exit code: {}", s)),
        Err(e) => Err(e.to_string()),
    }
}

#[tauri::command]
pub async fn test_proxy(
    _state: State<'_, Arc<AppState>>,
    proxy_name: String,
) -> Result<serde_json::Value, String> {
    let proxy_url = resolve_proxy_url(&proxy_name);
    let settings = crate::config::load();
    let pool = crate::network::pool::NetworkPool::new(settings.danger_accept_invalid_certs);
    let client = pool.get_client(proxy_url.as_deref());

    let start = std::time::Instant::now();
    match client
        .get("https://www.google.com")
        .timeout(std::time::Duration::from_secs(10))
        .send()
        .await
    {
        Ok(resp) => {
            let latency_ms = start.elapsed().as_millis() as u64;
            let ok = resp.status().is_success();
            let status = resp.status().as_u16();
            Ok(serde_json::json!({
                "ok": ok,
                "latency_ms": latency_ms,
                "status": status,
            }))
        }
        Err(e) => {
            let latency_ms = start.elapsed().as_millis() as u64;
            Ok(serde_json::json!({
                "ok": false,
                "latency_ms": latency_ms,
                "error": format!("{}", e),
            }))
        }
    }
}
