use crate::types::*;
use crate::state::db::Db;
use crate::state::facade::DownloadStateFacade;
use crate::worker::WorkerPool;
use crate::engine::OnResumeState;
use crate::config;
use crate::log::Logger;
use crate::state::runtime::DownloadManagerState;
use crate::services::{chunk_planner, probe_service};
use std::sync::{Arc, Mutex};

pub struct DownloadManager {
    pub(crate) facade: Arc<DownloadStateFacade>,
    pub(crate) worker_pool: WorkerPool,
    logger: Mutex<Logger>,
    settings: Mutex<Settings>,
}

/// Flags indicating which Tauri-specific side effects the caller should trigger.
pub struct SettingsChangeFlags {
    pub tls_changed: bool,
    pub shortcut_changed: bool,
    pub old_shortcut: String,
    pub new_shortcut: String,
    pub launch_at_startup: bool,
    pub silent_startup: bool,
}

impl DownloadManager {
    pub fn new(
        db: Db,
        worker_pool: WorkerPool,
        logger: Logger,
        runtime: DownloadManagerState,
    ) -> Self {
        let settings = config::load();
        Self {
            facade: Arc::new(DownloadStateFacade::new(db, runtime)),
            worker_pool,
            logger: Mutex::new(logger),
            settings: Mutex::new(settings),
        }
    }

    pub fn get_settings(&self) -> Settings {
        self.settings.lock().map(|s| s.clone()).unwrap_or_default()
    }

    pub fn reload_settings(&self) {
        if let Ok(mut s) = self.settings.lock() {
            *s = config::load();
        }
    }

    /// Resolve a proxy name to a URL using cached settings.
    pub fn resolve_proxy_url(&self, proxy_name: &str) -> Option<String> {
        let settings = self.get_settings();
        resolve_proxy_url_from(proxy_name, &settings)
    }

    pub fn log_info(&self, msg: &str) {
        if let Ok(l) = self.logger.lock() {
            l.info(msg);
        }
    }

    /// Save settings: persist to disk, reload cache, clear pool if TLS changed.
    /// Returns change flags for the caller to handle Tauri-specific side effects.
    pub fn save_settings(&self, new_settings: &Settings) -> PdmResult<SettingsChangeFlags> {
        let old = self.get_settings();
        let tls_changed = old.danger_accept_invalid_certs != new_settings.danger_accept_invalid_certs;
        let shortcut_changed = old.global_shortcut != new_settings.global_shortcut;

        config::save(new_settings)?;
        self.reload_settings();

        if tls_changed {
            eprintln!("[ProxyDM] TLS cert validation changed, clearing client pool");
            self.clear_client_pool();
        }

        Ok(SettingsChangeFlags {
            tls_changed,
            shortcut_changed,
            old_shortcut: old.global_shortcut,
            new_shortcut: new_settings.global_shortcut.clone(),
            launch_at_startup: new_settings.launch_at_startup,
            silent_startup: new_settings.silent_startup,
        })
    }

    pub fn log_warn(&self, msg: &str) {
        if let Ok(l) = self.logger.lock() {
            l.warn(msg);
        }
    }

    fn make_resume_callback(&self) -> OnResumeState {
        let facade = self.facade.clone();
        Box::new(move |id, state| { facade.save_gob(id, state); })
    }

    // ── Delegate methods (encapsulate facade/worker_pool access) ──

    pub fn list_items(&self) -> PdmResult<Vec<DownloadItem>> {
        self.facade.list_items()
    }

    pub fn update_item(&self, item: &DownloadItem) -> PdmResult<()> {
        self.facade.update_item(item)
    }

    pub fn flush(&self) -> usize {
        self.facade.flush()
    }

    pub fn clear_client_pool(&self) {
        self.worker_pool.clear_clients();
    }

    /// Build the user-agent fallback list: configured UA + browser defaults.
    fn build_user_agents(&self) -> Vec<String> {
        let settings = self.get_settings();
        probe_service::build_user_agents(&settings.user_agent)
    }

    /// Handle an event from the download engine. Emits frontend events via the bus.
    pub fn handle_event(&self, event: Event, bus: &crate::event_bus::EventBus) {
        use crate::event_bus::FrontendEvent;
        let id = event.download_id;

        let url_info = self.facade.get_item(id)
            .ok()
            .flatten()
            .map(|item| format!(" url={}", item.url))
            .unwrap_or_default();

        self.log_info(&format!("Event: {:?} id={}{}", event.kind, id, url_info));

        if matches!(event.kind, EventKind::DownloadErrored) {
            let msg = event.data.clone().unwrap_or_default();
            let url = url_info.trim_start_matches(" url=").to_string();
            bus.emit(FrontendEvent::DownloadError, serde_json::json!({ "id": id, "url": url, "message": msg }));
        }

        match event.kind {
            EventKind::DownloadStarted => {
                self.facade.on_started(id);
                bus.emit(FrontendEvent::DownloadStarted, serde_json::json!(id));
            }
            EventKind::DownloadCompleted => {
                let file_name = self.facade.get_item(id)
                    .ok()
                    .flatten()
                    .map(|item| item.file_name)
                    .unwrap_or_default();
                self.facade.on_completed(id);
                bus.emit(FrontendEvent::DownloadCompleted, serde_json::json!({ "id": id, "file_name": file_name }));
            }
            EventKind::DownloadErrored => {
                let msg = event.data.unwrap_or_default();
                self.facade.on_error(id, msg);
            }
            EventKind::DownloadProgress => {
                if let Some(data) = &event.data {
                    if let Ok(downloaded) = data.parse::<u64>() {
                        self.facade.update_progress(id, downloaded);
                        bus.emit(FrontendEvent::DownloadProgress, serde_json::json!({ "id": id, "downloaded": downloaded }));
                    }
                }
            }
            _ => {}
        }
    }

    /// Start a new download: probe → compute chunks → DB insert → spawn worker.
    pub async fn start_download(
        &self,
        url: String,
        filename: String,
        save_path: String,
        proxy_name: String,
        connections: u32,
    ) -> PdmResult<u64> {
        self.log_info(&format!("Download start url={} proxy={}", url, proxy_name));

        let pool = self.worker_pool.pool_ref();
        let headers = std::collections::HashMap::new();
        let proxy_url_str = self.resolve_proxy_url(&proxy_name);
        let proxy_opt = proxy_url_str.as_deref();

        let settings = self.get_settings();
        let user_agents = self.build_user_agents();

        let outcome = probe_service::probe_with_fallback(
            &url, &headers, proxy_opt, &pool, &user_agents, &filename,
        ).await;

        let file_name = outcome.file_name;
        let file_size = outcome.file_size;
        let supports_range = outcome.supports_range;

        if file_size > 0 {
            self.log_info(&format!("Probe ok url={} size={} range={} name={}", url, file_size, supports_range, file_name));
        } else {
            self.log_warn(&format!("Probe failed, forcing blind download url={}", url));
        }

        let max_conns = settings.max_connections.max(1).min(32);

        let connections = chunk_planner::compute_connection_count(file_size, connections, max_conns);

        let save_dir = if save_path.is_empty() {
            settings.download_dir
        } else {
            save_path
        };
        let full_path = unique_filename(&save_dir, &file_name);

        chunk_planner::check_disk_space(&full_path, file_size)?;

        let id = self.worker_pool.next_id();

        let plan = chunk_planner::plan_chunks(file_size, connections, supports_range, settings.max_connections);
        let parts = plan.parts;

        let item = DownloadItem {
            id,
            url: url.clone(),
            file_name: file_name.clone(),
            save_path: full_path.clone(),
            total_size: file_size,
            downloaded: 0,
            status: DownloadStatus::Downloading,
            parts,
            proxy_name: proxy_name.clone(),
            connections,
            resumable: Some(supports_range),
            created_at: now_str(),
            last_try: String::new(),
        };
        self.facade.insert_item(&item)?;

        let mut cfg = item.to_engine_config(&proxy_url_str.clone().unwrap_or_default(), &settings.user_agent, false, settings.max_retries);
        cfg.id = id;
        self.worker_pool.add_with_id(cfg, id, self.make_resume_callback()).await?;

        Ok(id)
    }

    /// Redownload an existing download with a new ID.
    pub async fn redownload_download(&self, id: u64) -> PdmResult<u64> {
        let items = self.facade.list_items()?;
        let existing = items.iter().find(|i| i.id == id)
            .ok_or_else(|| format!("Download {} not found", id))?.clone();

        self.log_info(&format!("Redownload start id={} url={}", id, existing.url));

        let pool = self.worker_pool.pool_ref();
        let headers = std::collections::HashMap::new();
        let proxy_url = self.resolve_proxy_url(&existing.proxy_name).unwrap_or_default();
        let proxy_opt = if proxy_url.is_empty() { None } else { Some(proxy_url.as_str()) };
        let settings = self.get_settings();
        let user_agents = self.build_user_agents();

        let probe_result = crate::probe::probe(&existing.url, &headers, proxy_opt, &pool, &user_agents).await;

        let (file_size, supports_range) = match probe_result {
            Ok(r) => {
                self.log_info(&format!("Probe ok url={} size={} range={}", existing.url, r.file_size, r.supports_range));
                (r.file_size, r.supports_range)
            }
            Err(e) => {
                self.log_warn(&format!("Probe failed, blind redownload url={} err={}", existing.url, e));
                (0, false)
            }
        };

        let new_id = self.worker_pool.next_id();

        let new_item = DownloadItem {
            id: new_id,
            url: existing.url.clone(),
            file_name: existing.file_name.clone(),
            save_path: existing.save_path.clone(),
            total_size: file_size,
            downloaded: 0,
            status: DownloadStatus::Downloading,
            parts: vec![],
            proxy_name: existing.proxy_name.clone(),
            connections: existing.connections,
            resumable: Some(supports_range),
            created_at: now_str(),
            last_try: String::new(),
        };
        self.facade.insert_item(&new_item)?;

        let cfg = new_item.to_engine_config(&proxy_url, &settings.user_agent, false, settings.max_retries);
        self.worker_pool.add_with_id(cfg, new_id, self.make_resume_callback()).await?;

        Ok(new_id)
    }

    /// Pause a download: cancel workers → flush → save gob → update DB.
    pub async fn pause_download(&self, id: u64) -> PdmResult<()> {
        self.log_info(&format!("Pause id={}", id));
        self.worker_pool.cancel_and_wait(id).await;
        self.facade.flush();

        if let Ok(Some(mut item)) = self.facade.get_item(id) {
            if matches!(item.status, DownloadStatus::Downloading) {
                if item.resumable != Some(true) {
                    self.facade.save_gob_for_non_resumable(&item);
                }
                item.status = DownloadStatus::Paused;
                let _ = self.facade.update_item(&item);
            }
        }
        Ok(())
    }

    /// Resume a paused download.
    pub async fn resume_download(&self, id: u64) -> PdmResult<()> {
        self.log_info(&format!("Resume id={}", id));

        if let Some(saved_state) = self.facade.load_gob(id) {
            let supports_range = if let Ok(Some(mut item)) = self.facade.get_item(id) {
                item.status = DownloadStatus::Downloading;
                item.last_try = now_str();
                let _ = self.facade.update_item(&item);
                item.resumable.unwrap_or(true)
            } else {
                true
            };

            let proxy_url = self.resolve_proxy_url(&saved_state.proxy_name).unwrap_or_default();
            let s = self.get_settings();
            let cfg = saved_state.to_engine_config(&proxy_url, &s.user_agent, supports_range, s.max_retries);
            self.worker_pool.add_with_id(cfg, id, self.make_resume_callback()).await?;
        } else {
            if let Ok(Some(item)) = self.facade.get_item(id) {
                let mut updated = item.clone();
                updated.downloaded = 0;
                updated.status = DownloadStatus::Downloading;
                updated.last_try = now_str();
                let _ = self.facade.update_item(&updated);

                let settings = self.get_settings();
                let proxy_url = self.resolve_proxy_url(&item.proxy_name).unwrap_or_default();
                let cfg = item.to_engine_config(&proxy_url, &settings.user_agent, false, settings.max_retries);
                self.worker_pool.add_with_id(cfg, id, self.make_resume_callback()).await?;
            }
        }
        Ok(())
    }

    /// Delete a download: cancel → delete DB/gob → optionally delete files.
    pub async fn delete_download(&self, id: u64, delete_file: bool) -> PdmResult<()> {
        self.log_info(&format!("Delete id={} delete_file={}", id, delete_file));

        let save_path = if delete_file {
            self.facade.list_items().ok()
                .and_then(|items| items.into_iter().find(|i| i.id == id))
                .map(|item| item.save_path)
        } else {
            None
        };

        self.worker_pool.cancel_and_wait(id).await;
        self.facade.delete_item(id)?;
        self.facade.delete_gob(id);

        if let Some(path) = save_path {
            let p = std::path::Path::new(&path);
            let pdm_path = std::path::PathBuf::from(format!("{}.pdm", path));
            if pdm_path.exists() {
                let _ = std::fs::remove_file(&pdm_path);
            }
            if p.exists() {
                let _ = std::fs::remove_file(p);
            }
        }
        Ok(())
    }

    /// Cancel a download without deleting records.
    pub async fn cancel_download(&self, id: u64) {
        self.worker_pool.cancel(id).await;
    }

    /// Check for application updates from GitHub.
    pub async fn check_update(&self, proxy_name: &str) -> PdmResult<serde_json::Value> {
        let proxy_url = self.resolve_proxy_url(proxy_name);
        let pool = self.worker_pool.pool_ref();
        let client = pool.get_client(proxy_url.as_deref())?;

        let resp = client
            .get("https://api.github.com/repos/fb0sh/ProxyDownloadManager/releases/latest")
            .header("User-Agent", concat!("ProxyDM/", env!("CARGO_PKG_VERSION")))
            .header("Accept", "application/vnd.github.v3+json")
            .timeout(std::time::Duration::from_secs(30))
            .send()
            .await
            .map_err(|e| format!("Failed to check update: {}", e))?;

        if !resp.status().is_success() {
            return Err(PdmError::Other(format!("GitHub API responded with status {}", resp.status())));
        }

        let body = resp.text().await
            .map_err(|e| format!("Failed to read response: {}", e))?;
        Ok(serde_json::from_str(&body)
            .map_err(|e| format!("Failed to parse release info: {}", e))?)
    }

    /// Test proxy connectivity.
    pub async fn test_proxy(&self, proxy_name: &str) -> PdmResult<serde_json::Value> {
        let proxy_url = self.resolve_proxy_url(proxy_name);
        let pool = self.worker_pool.pool_ref();
        let client = pool.get_client(proxy_url.as_deref())?;

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
}

/// Resolve a proxy name to a URL using the given settings.
pub(crate) fn resolve_proxy_url_from(proxy_name: &str, settings: &Settings) -> Option<String> {
    if proxy_name.is_empty() {
        return None;
    }
    let proxy = settings.proxies.get(proxy_name)?;
    let protocol = match proxy.protocol {
        ProxyProtocol::Http => "http",
        ProxyProtocol::Socks5 => "socks5",
    };
    Some(format!("{}://{}:{}", protocol, proxy.host, proxy.port))
}

pub fn now_str() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let dur = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    format!("{}", dur.as_secs())
}

pub fn unique_filename(dir: &str, filename: &str) -> String {
    let dir = dir.trim_end_matches('/');
    let candidate = format!("{}/{}", dir, filename);
    if !std::path::Path::new(&candidate).exists() {
        return candidate;
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_unique_filename_no_conflict() {
        let dir = std::env::temp_dir().join("pdm_test_unique_1");
        let _ = std::fs::create_dir_all(&dir);
        let result = unique_filename(dir.to_str().unwrap(), "test.zip");
        assert!(result.ends_with("test.zip"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_resolve_proxy_url_empty() {
        let settings = Settings::default();
        assert!(resolve_proxy_url_from("", &settings).is_none());
    }

    #[test]
    fn test_resolve_proxy_url_missing() {
        let settings = Settings::default();
        assert!(resolve_proxy_url_from("nonexistent", &settings).is_none());
    }
}
