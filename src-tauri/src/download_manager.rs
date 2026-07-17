use crate::types::*;
use crate::state::db::Db;
use crate::state::facade::DownloadStateFacade;
use crate::worker::WorkerPool;
use crate::config;
use crate::log::Logger;
use crate::state::runtime::DownloadManagerState;
use std::sync::Mutex;

pub struct DownloadManager {
    pub facade: DownloadStateFacade,
    pub worker_pool: WorkerPool,
    pub logger: Mutex<Logger>,
}

impl DownloadManager {
    pub fn new(
        db: Db,
        worker_pool: WorkerPool,
        logger: Logger,
        runtime: DownloadManagerState,
    ) -> Self {
        Self {
            facade: DownloadStateFacade::new(db, runtime),
            worker_pool,
            logger: Mutex::new(logger),
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

    /// Handle an event from the download engine.
    pub fn handle_event(&self, event: Event) -> Vec<EmittedEvent> {
        let id = event.download_id;
        let mut emitted = Vec::new();

        let url_info = self.facade.get_item(id)
            .ok()
            .flatten()
            .map(|item| format!(" url={}", item.url))
            .unwrap_or_default();

        self.log_info(&format!("Event: {:?} id={}{}", event.kind, id, url_info));

        if matches!(event.kind, EventKind::DownloadErrored) {
            let msg = event.data.clone().unwrap_or_default();
            let url = url_info.trim_start_matches(" url=").to_string();
            emitted.push(EmittedEvent {
                name: "download-error".to_string(),
                payload: serde_json::json!({ "id": id, "url": url, "message": msg }),
            });
        }

        match event.kind {
            EventKind::DownloadStarted => {
                self.facade.on_started(id);
                emitted.push(EmittedEvent {
                    name: "download-started".to_string(),
                    payload: serde_json::json!(id),
                });
            }
            EventKind::DownloadCompleted => {
                self.facade.on_completed(id);
                emitted.push(EmittedEvent {
                    name: "download-completed".to_string(),
                    payload: serde_json::json!(id),
                });
            }
            EventKind::DownloadErrored => {
                let msg = event.data.unwrap_or_default();
                self.facade.on_error(id, msg);
            }
            EventKind::DownloadProgress => {
                if let Some(data) = &event.data {
                    if let Ok(downloaded) = data.parse::<u64>() {
                        self.facade.update_progress(id, downloaded);
                        emitted.push(EmittedEvent {
                            name: "download-progress".to_string(),
                            payload: serde_json::json!({ "id": id, "downloaded": downloaded }),
                        });
                    }
                }
            }
            _ => {}
        }

        emitted
    }

    /// Start a new download: probe → compute chunks → DB insert → spawn worker.
    pub async fn start_download(
        &self,
        url: String,
        filename: String,
        save_path: String,
        proxy_name: String,
        connections: u32,
    ) -> Result<u64, String> {
        self.log_info(&format!("Download start url={} proxy={}", url, proxy_name));

        let pool = self.worker_pool.pool_ref();
        let headers = std::collections::HashMap::new();
        let proxy_url_str = resolve_proxy_url(&proxy_name);
        let proxy_opt = proxy_url_str.as_deref();

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
                self.log_info(&format!("Probe ok url={} size={} range={} name={}", url, r.file_size, r.supports_range, r.file_name));
                let name = if filename.is_empty() { r.file_name } else { filename };
                (name, r.file_size, r.supports_range)
            }
            Err(e) => {
                self.log_warn(&format!("Probe failed, forcing blind download url={} err={}", url, e));
                let name = if filename.is_empty() {
                    crate::filename::from_url(&url).unwrap_or_else(|| "download".to_string())
                } else {
                    filename
                };
                (name, 0, false)
            }
        };

        let settings = config::load();
        let max_conns = settings.max_connections.max(1).min(32);

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

        if file_size > 0 {
            let pdm_path = format!("{}.pdm", full_path);
            if let Some(parent) = std::path::Path::new(&pdm_path).parent() {
                if let Ok(available) = fs2::available_space(parent) {
                    let needed = file_size + (2u64 * 1024 * 1024);
                    if available < needed {
                        return Err(format!(
                            "Insufficient disk space: need {}, available {}",
                            needed, available
                        ));
                    }
                }
            }
        }

        let id = self.worker_pool.next_id();

        let parts = if supports_range && file_size > 0 {
            let num_conns = if connections > 0 { connections.min(32) } else { 1 };
            let min_chunk = 2u64 * 1024 * 1024;
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

        let cfg = DownloadConfig {
            url,
            output_path: full_path.clone(),
            save_path: full_path.clone(),
            id,
            file_name,
            is_resume: false,
            headers: std::collections::HashMap::new(),
            proxy_name: proxy_url_str.clone().unwrap_or_default(),
            total_size: file_size,
            supports_range,
            rate_limit_bps: 0,
            connections,
            max_retries: 3,
            user_agent: settings.user_agent.clone(),
            resume_tasks: vec![],
        };
        self.worker_pool.add_with_id(cfg, id).await?;

        Ok(id)
    }

    /// Redownload an existing download with a new ID.
    pub async fn redownload_download(&self, id: u64) -> Result<u64, String> {
        let items = self.facade.list_items()?;
        let existing = items.iter().find(|i| i.id == id)
            .ok_or_else(|| format!("Download {} not found", id))?.clone();

        self.log_info(&format!("Redownload start id={} url={}", id, existing.url));

        let pool = self.worker_pool.pool_ref();
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

        let cfg = DownloadConfig {
            url: existing.url,
            output_path: existing.save_path.clone(),
            save_path: existing.save_path,
            id: new_id,
            file_name: existing.file_name,
            is_resume: false,
            headers: std::collections::HashMap::new(),
            proxy_name: proxy_url,
            total_size: file_size,
            supports_range,
            rate_limit_bps: 0,
            connections: existing.connections,
            max_retries: 3,
            user_agent: settings.user_agent,
            resume_tasks: vec![],
        };
        self.worker_pool.add_with_id(cfg, new_id).await?;

        Ok(new_id)
    }

    /// Pause a download: cancel workers → flush → save gob → update DB.
    pub async fn pause_download(&self, id: u64) -> Result<(), String> {
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
    pub async fn resume_download(&self, id: u64) -> Result<(), String> {
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
                supports_range,
                rate_limit_bps: 0,
                connections: saved_state.workers,
                max_retries: 3,
                user_agent: config::load().user_agent,
                resume_tasks: saved_state.tasks,
            };
            self.worker_pool.add_with_id(cfg, id).await?;
        } else {
            if let Ok(Some(item)) = self.facade.get_item(id) {
                let mut updated = item.clone();
                updated.downloaded = 0;
                updated.status = DownloadStatus::Downloading;
                updated.last_try = now_str();
                let _ = self.facade.update_item(&updated);

                let settings = config::load();
                let proxy_url = resolve_proxy_url(&item.proxy_name).unwrap_or_default();
                let cfg = DownloadConfig {
                    url: item.url,
                    output_path: item.save_path.clone(),
                    save_path: item.save_path,
                    id,
                    file_name: item.file_name,
                    is_resume: false,
                    headers: std::collections::HashMap::new(),
                    proxy_name: proxy_url,
                    total_size: item.total_size,
                    supports_range: item.resumable.unwrap_or(true),
                    rate_limit_bps: 0,
                    connections: item.connections,
                    max_retries: 3,
                    user_agent: settings.user_agent,
                    resume_tasks: vec![],
                };
                self.worker_pool.add_with_id(cfg, id).await?;
            }
        }
        Ok(())
    }

    /// Delete a download: cancel → delete DB/gob → optionally delete files.
    pub async fn delete_download(&self, id: u64, delete_file: bool) -> Result<(), String> {
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
}

/// An event to be emitted to the frontend.
pub struct EmittedEvent {
    pub name: String,
    pub payload: serde_json::Value,
}

pub fn resolve_proxy_url(proxy_name: &str) -> Option<String> {
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
        assert!(resolve_proxy_url("").is_none());
    }

    #[test]
    fn test_resolve_proxy_url_missing() {
        assert!(resolve_proxy_url("nonexistent").is_none());
    }
}
