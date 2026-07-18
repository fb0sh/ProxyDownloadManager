use crate::types::*;
use crate::state::db::Db;
use crate::state::facade::DownloadStateFacade;
use crate::worker::WorkerPool;
use crate::engine::OnResumeState;
use crate::event_bus::{EventBus, FrontendEvent};
use crate::logger::Logger;
use crate::state::runtime::DownloadManagerState;
use crate::services::{settings_service::SettingsService, network_service::NetworkService};
use std::sync::{Arc, Mutex};

pub struct DownloadManager {
    pub(crate) facade: Arc<DownloadStateFacade>,
    pub(crate) worker_pool: WorkerPool,
    logger: Mutex<Logger>,
    pub(crate) settings: Arc<SettingsService>,
    pub(crate) network: Arc<NetworkService>,
    bus: Arc<EventBus>,
}

impl DownloadManager {
    pub fn new(
        db: Db,
        worker_pool: WorkerPool,
        logger: Logger,
        runtime: DownloadManagerState,
        settings: Arc<SettingsService>,
        network: Arc<NetworkService>,
        bus: Arc<EventBus>,
    ) -> Self {
        Self {
            facade: Arc::new(DownloadStateFacade::new(db, runtime)),
            worker_pool,
            logger: Mutex::new(logger),
            settings,
            network,
            bus,
        }
    }

    pub fn log_info(&self, msg: &str) {
        if let Ok(l) = self.logger.lock() {
            l.info(msg);
        }
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

    /// Handle an event from the download engine. Emits frontend events via the bus.
    pub fn handle_event(&self, event: Event) {
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
            self.bus.emit(FrontendEvent::DownloadError, serde_json::json!({ "id": id, "url": url, "message": msg }));
        }

        match event.kind {
            EventKind::DownloadStarted => {
                self.facade.on_started(id);
                self.bus.emit(FrontendEvent::DownloadStarted, serde_json::json!(id));
            }
            EventKind::DownloadCompleted => {
                let file_name = self.facade.get_item(id)
                    .ok()
                    .flatten()
                    .map(|item| item.file_name)
                    .unwrap_or_default();
                self.facade.on_completed(id);
                self.bus.emit(FrontendEvent::DownloadCompleted, serde_json::json!({ "id": id, "file_name": file_name }));
            }
            EventKind::DownloadErrored => {
                let msg = event.data.unwrap_or_default();
                self.facade.on_error(id, msg);
            }
            EventKind::DownloadProgress => {
                if let Some(data) = &event.data {
                    if let Ok(downloaded) = data.parse::<u64>() {
                        self.facade.update_progress(id, downloaded);
                        self.bus.emit(FrontendEvent::DownloadProgress, serde_json::json!({ "id": id, "downloaded": downloaded }));
                    }
                }
            }
            _ => {}
        }
    }

    /// Start a new download.
    pub async fn start_download(
        &self,
        url: String,
        filename: String,
        save_path: String,
        proxy_name: String,
        connections: u32,
    ) -> PdmResult<u64> {
        self.log_info(&format!("Download start url={} proxy={}", url, proxy_name));
        self.execute_download(DownloadSpec {
            url, file_name: filename, save_path, proxy_name, connections,
        }).await
    }

    /// Redownload an existing download with a new ID.
    pub async fn redownload_download(&self, id: u64) -> PdmResult<u64> {
        let existing = self.facade.get_item(id)?
            .ok_or_else(|| format!("Download {} not found", id))?;
        self.log_info(&format!("Redownload start id={} url={}", id, existing.url));
        self.execute_download(DownloadSpec {
            url: existing.url,
            file_name: existing.file_name,
            save_path: existing.save_path,
            proxy_name: existing.proxy_name,
            connections: existing.connections,
        }).await
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
        self.bus.emit(FrontendEvent::DownloadPaused, serde_json::json!({ "id": id }));
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

            let proxy_url = self.settings.resolve_proxy_url(&saved_state.proxy_name).unwrap_or_default();
            let s = self.settings.get();
            let mut cfg = saved_state.to_engine_config(&proxy_url, &s.user_agent, supports_range, s.max_retries);
            cfg.rate_limit_bps = s.global_rate_limit;
            self.worker_pool.add_with_id(cfg, id, self.make_resume_callback()).await?;
        } else {
            if let Ok(Some(item)) = self.facade.get_item(id) {
                let mut updated = item.clone();
                updated.downloaded = 0;
                updated.status = DownloadStatus::Downloading;
                updated.last_try = now_str();
                let _ = self.facade.update_item(&updated);

                let settings = self.settings.get();
                let proxy_url = self.settings.resolve_proxy_url(&item.proxy_name).unwrap_or_default();
                let mut cfg = item.to_engine_config(&proxy_url, &settings.user_agent, false, settings.max_retries);
                cfg.rate_limit_bps = settings.global_rate_limit;
                self.worker_pool.add_with_id(cfg, id, self.make_resume_callback()).await?;
            }
        }
        self.bus.emit(FrontendEvent::DownloadResumed, serde_json::json!({ "id": id }));
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
        self.bus.emit(FrontendEvent::DownloadCancelled, serde_json::json!({ "id": id }));
    }

    /// Delegate check_update to the network service.
    pub async fn check_update(&self, proxy_name: &str) -> PdmResult<serde_json::Value> {
        let proxy_url = self.settings.resolve_proxy_url(proxy_name);
        self.network.check_update(proxy_url.as_deref()).await
    }

    /// Delegate test_proxy to the network service.
    pub async fn test_proxy(&self, proxy_name: &str) -> PdmResult<serde_json::Value> {
        let proxy_url = self.settings.resolve_proxy_url(proxy_name);
        self.network.test_proxy(proxy_url.as_deref()).await
    }

    /// Shared pipeline: probe → plan chunks → disk check → DB insert → spawn worker.
    async fn execute_download(&self, spec: DownloadSpec) -> PdmResult<u64> {
        let pool = self.worker_pool.pool_ref();
        let headers = std::collections::HashMap::new();
        let proxy_url_str = self.settings.resolve_proxy_url(&spec.proxy_name);
        let proxy_opt = proxy_url_str.as_deref();
        let settings = self.settings.get();
        let user_agents = self.settings.build_user_agents();

        let outcome = crate::probe::probe_with_fallback(
            &spec.url, &headers, proxy_opt, &pool, &user_agents, &spec.file_name,
        ).await;

        let file_name = outcome.file_name;
        let file_size = outcome.file_size;
        let supports_range = outcome.supports_range;

        if file_size > 0 {
            self.log_info(&format!("Probe ok url={} size={} range={} name={}", spec.url, file_size, supports_range, file_name));
        } else {
            self.log_warn(&format!("Probe failed, forcing blind download url={}", spec.url));
        }

        let max_conns = settings.max_connections.max(1).min(32);
        let connections = crate::engine::chunk::compute_connection_count(file_size, spec.connections, max_conns);

        let save_dir = if spec.save_path.is_empty() {
            settings.download_dir
        } else {
            spec.save_path
        };
        let full_path = unique_filename(&save_dir, &file_name);

        crate::engine::chunk::check_disk_space(&full_path, file_size)?;

        let id = self.worker_pool.next_id();
        let plan = crate::engine::chunk::plan_chunks(file_size, connections, supports_range, settings.max_connections);

        let item = DownloadItem {
            id,
            url: spec.url,
            file_name,
            save_path: full_path,
            total_size: file_size,
            downloaded: 0,
            status: DownloadStatus::Downloading,
            parts: plan.parts,
            proxy_name: spec.proxy_name,
            connections,
            resumable: Some(supports_range),
            created_at: now_str(),
            last_try: String::new(),
        };
        self.facade.insert_item(&item)?;

        let mut cfg = item.to_engine_config(&proxy_url_str.unwrap_or_default(), &settings.user_agent, false, settings.max_retries);
        cfg.rate_limit_bps = settings.global_rate_limit;
        self.worker_pool.add_with_id(cfg, id, self.make_resume_callback()).await?;

        Ok(id)
    }
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

/// Spec for the shared download pipeline.
struct DownloadSpec {
    url: String,
    file_name: String,
    save_path: String,
    proxy_name: String,
    connections: u32,
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
}
