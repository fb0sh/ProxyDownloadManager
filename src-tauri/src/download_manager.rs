use crate::engine::EngineHooks;
use crate::event_bus::{EventBus, FrontendEvent};
use crate::event_handler::{transform_event, EventAction};
use crate::logger::Logger;
use crate::services::settings_service::SettingsService;
use crate::state::ledger::ProgressLedger;
use crate::types::*;
use crate::worker::{Admission, WorkerPool};
use std::sync::{Arc, Mutex};

pub struct DownloadManager {
    ledger: Arc<ProgressLedger>,
    pub(crate) worker_pool: WorkerPool,
    logger: Mutex<Logger>,
    pub(crate) settings: Arc<SettingsService>,
    bus: Arc<EventBus>,
}

impl DownloadManager {
    pub fn new(
        ledger: Arc<ProgressLedger>,
        worker_pool: WorkerPool,
        logger: Logger,
        settings: Arc<SettingsService>,
        bus: Arc<EventBus>,
    ) -> Self {
        Self {
            ledger,
            worker_pool,
            logger: Mutex::new(logger),
            settings,
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

    fn make_hooks(&self) -> EngineHooks {
        let save_ledger = self.ledger.clone();
        let invalidate_ledger = self.ledger.clone();
        EngineHooks {
            save_resume_state: Box::new(move |id, state| {
                save_ledger.save_resume_state(id, state);
            }),
            invalidate_for_restart: Box::new(move |id| {
                invalidate_ledger.invalidate_for_restart(id);
            }),
        }
    }

    pub fn clear_client_pool(&self) {
        self.worker_pool.clear_clients();
    }

    /// Handle an event from the download engine. Uses EventTransformer
    /// to map engine events → structured actions, then applies them.
    pub fn handle_event(&self, event: Event) {
        let id = event.download_id;

        let url_info = self
            .ledger
            .get_item(id)
            .ok()
            .flatten()
            .map(|item| format!(" url={}", item.url))
            .unwrap_or_default();

        self.log_info(&format!("Event: {:?} id={}{}", event.kind, id, url_info));

        let action = transform_event(&event);

        match action {
            EventAction::DownloadStarted(dl_id) => {
                self.ledger.on_started(dl_id);
                self.bus
                    .emit(FrontendEvent::DownloadStarted, serde_json::json!(dl_id));
            }
            EventAction::DownloadCompleted(dl_id) => {
                let file_name = self
                    .ledger
                    .get_item(dl_id)
                    .ok()
                    .flatten()
                    .map(|item| item.file_name)
                    .unwrap_or_default();
                self.ledger.on_completed(dl_id);
                self.bus.emit(
                    FrontendEvent::DownloadCompleted,
                    serde_json::json!({ "id": dl_id, "file_name": file_name }),
                );
            }
            EventAction::DownloadErrored(dl_id, msg) => {
                // State first, then the event: the frontend invalidates its
                // cache on DownloadError, and the refetch must see Failed.
                self.ledger.on_error(dl_id, msg.clone());
                let url = url_info.trim_start_matches(" url=").to_string();
                self.bus.emit(
                    FrontendEvent::DownloadError,
                    serde_json::json!({ "id": dl_id, "url": url, "message": msg }),
                );
            }
            EventAction::UpdateProgress {
                id: dl_id,
                downloaded,
                part_downloaded,
                reset_to_single,
            } => {
                self.ledger.record_progress(
                    dl_id,
                    downloaded,
                    part_downloaded.clone(),
                    reset_to_single,
                );
                let mut payload = serde_json::json!({ "id": dl_id, "downloaded": downloaded });
                if let Some(parts) = part_downloaded {
                    payload["parts"] = serde_json::json!(parts);
                }
                if reset_to_single {
                    payload["reset_to_single"] = serde_json::json!(true);
                }
                self.bus.emit(FrontendEvent::DownloadProgress, payload);
            }
            EventAction::Noop => {}
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
        headers: std::collections::HashMap<String, String>,
        rate_limit_bps: u64,
        start_paused: bool,
    ) -> PdmResult<u64> {
        self.log_info(&format!("Download start url={} proxy={}", url, proxy_name));
        self.execute_download(DownloadSpec {
            url,
            file_name: filename,
            save_path,
            proxy_name,
            connections,
            headers,
            rate_limit_bps,
            start_paused,
        })
        .await
    }

    /// Redownload an existing download with a new ID.
    pub async fn redownload_download(&self, id: u64) -> PdmResult<u64> {
        let existing = self
            .ledger
            .get_item(id)?
            .ok_or_else(|| format!("Download {} not found", id))?;
        self.log_info(&format!("Redownload start id={} url={}", id, existing.url));
        // save_path on the item is the full file path; the pipeline expects a
        // directory (passing the file path nested a second copy inside it).
        let save_dir = std::path::Path::new(&existing.save_path)
            .parent()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default();
        self.execute_download(DownloadSpec {
            url: existing.url,
            file_name: existing.file_name,
            save_path: save_dir,
            proxy_name: existing.proxy_name,
            connections: existing.connections,
            headers: existing.headers,
            rate_limit_bps: existing.rate_limit_bps,
            start_paused: false,
        })
        .await
    }

    /// Pause a download: cancel workers → persist state → emit event.
    pub async fn pause_download(&self, id: u64) -> PdmResult<()> {
        self.log_info(&format!("Pause id={}", id));
        self.worker_pool.cancel_and_wait(id).await;
        self.ledger.on_paused(id)?;
        self.bus
            .emit(FrontendEvent::DownloadPaused, serde_json::json!({ "id": id }));
        Ok(())
    }

    /// Resume a paused download: the ledger reconciles progress and returns a
    /// complete plan; nothing is patched afterwards.
    pub async fn resume_download(&self, id: u64) -> PdmResult<()> {
        self.log_info(&format!("Resume id={}", id));

        let plan = self.ledger.begin_resume(id)?;
        log::info!(
            "[ProxyDM] resume id={} tasks={} downloaded={}/{}",
            id,
            plan.tasks.len(),
            plan.downloaded,
            plan.item.total_size
        );

        // Fully downloaded but never finalized (pause landed right after the
        // last task): finish it here instead of letting the engine error on an
        // empty task list and degrade into a full re-download.
        if plan.tasks.is_empty()
            && plan.item.total_size > 0
            && plan.downloaded >= plan.item.total_size
        {
            let pdm_path = crate::engine::file_io::pdm_path(&plan.item.save_path);
            if std::path::Path::new(&pdm_path).exists() {
                let _ = std::fs::rename(&pdm_path, &plan.item.save_path);
            }
            self.ledger.on_completed(id);
            self.bus.emit(
                FrontendEvent::DownloadCompleted,
                serde_json::json!({ "id": id, "file_name": plan.item.file_name }),
            );
            return Ok(());
        }

        let settings = self.settings.get();
        let proxy_url = self
            .settings
            .resolve_proxy_url(&plan.item.proxy_name)
            .unwrap_or_default();
        let cfg = plan.into_engine_config(
            &proxy_url,
            &settings.user_agent,
            settings.global_rate_limit,
            settings.max_retries,
        );

        match self.worker_pool.add_with_id(cfg, id, self.make_hooks()).await {
            Ok(Admission::Started) => {}
            Ok(Admission::Queued) => {
                // All slots busy: the row waits as Queued and starts on its own.
                self.ledger.mark_queued(id);
            }
            Err(e) => {
                // No worker was spawned — roll the row back to Paused, or the
                // begin_resume status guard would reject every retry.
                let _ = self.ledger.on_paused(id);
                return Err(e);
            }
        }
        self.bus
            .emit(FrontendEvent::DownloadResumed, serde_json::json!({ "id": id }));
        Ok(())
    }

    /// Delete a download: cancel → delete DB/gob → optionally delete files.
    pub async fn delete_download(&self, id: u64, delete_file: bool) -> PdmResult<()> {
        self.log_info(&format!("Delete id={} delete_file={}", id, delete_file));

        let save_path = if delete_file {
            self.ledger
                .get_item(id)
                .ok()
                .flatten()
                .map(|item| item.save_path)
        } else {
            None
        };

        self.worker_pool.cancel_and_wait(id).await;
        self.ledger.on_deleted(id)?;

        if let Some(path) = save_path {
            crate::engine::file_io::remove_download_files(&path);
        }
        Ok(())
    }

    /// Cancel a download without deleting records.
    pub async fn cancel_download(&self, id: u64) {
        self.worker_pool.cancel(id).await;
        self.bus.emit(
            FrontendEvent::DownloadCancelled,
            serde_json::json!({ "id": id }),
        );
    }

    pub async fn probe_url(
        &self,
        url: String,
        headers: std::collections::HashMap<String, String>,
        proxy_name: String,
    ) -> PdmResult<ProbeInfo> {
        let pool = self.worker_pool.pool_ref();
        let headers = crate::headers::filter_headers(&headers);
        let settings = self.settings.get();
        let proxy_url = self.settings.resolve_proxy_url(&proxy_name);
        let default_url = self.settings.resolve_proxy_url(&settings.default_proxy);
        let fallback = if proxy_url != default_url {
            default_url.as_deref()
        } else {
            None
        };
        let user_agents = self.settings.build_user_agents();
        let result = crate::probe::probe_then_default_proxy(
            &url,
            &headers,
            proxy_url.as_deref(),
            fallback,
            pool.as_ref(),
            &user_agents,
        )
        .await?;
        let suggested = crate::engine::chunk::compute_connection_count(
            result.file_size,
            0,
            settings.max_connections,
        );
        let mut info = ProbeInfo {
            url: url.clone(),
            final_url: result.final_url,
            file_name: result.file_name,
            file_size: result.file_size,
            content_type: result.content_type.clone(),
            supports_range: result.supports_range,
            etag: result.etag,
            last_modified: result.last_modified,
            suggested_connections: suggested,
            is_hls: result.is_hls,
            hls_variants: vec![],
        };
        if result.is_hls {
            if let Ok(text) = crate::engine::hls::fetch_text(
                &url,
                &headers,
                proxy_url.as_deref(),
                pool.as_ref(),
                &settings.user_agent,
            )
            .await
            {
                if let Ok(p) = crate::engine::hls::parse_playlist(&text, &url) {
                    info.hls_variants = p.variants;
                    if p.drm {
                        return Err(PdmError::Unsupported(
                            "DRM-protected HLS is not supported".into(),
                        ));
                    }
                }
            }
        }
        Ok(info)
    }

    pub async fn set_runtime_connections(&self, id: u64, connections: u32) -> PdmResult<()> {
        let n = connections.min(crate::engine::chunk::MAX_CONNECTIONS).max(1);
        self.ledger.update_connections(id, n)?;
        let _ = self.worker_pool.set_connections(id, n).await;
        Ok(())
    }

    pub async fn set_runtime_rate_limit(&self, id: u64, rate_limit_bps: u64) -> PdmResult<()> {
        self.ledger.update_rate_limit(id, rate_limit_bps)?;
        let _ = self.worker_pool.set_download_rate_limit(id, rate_limit_bps).await;
        Ok(())
    }

    pub fn set_global_rate_limit(&self, bps: u64) {
        self.worker_pool.set_global_rate_limit(bps);
    }

    pub async fn refresh_url(
        &self,
        id: u64,
        new_url: String,
        headers: std::collections::HashMap<String, String>,
    ) -> PdmResult<()> {
        let headers = crate::headers::filter_headers(&headers);
        let existing = self
            .ledger
            .get_item(id)?
            .ok_or(PdmError::NotFound(id))?;
        let pool = self.worker_pool.pool_ref();
        let proxy_url = self.settings.resolve_proxy_url(&existing.proxy_name);
        let user_agents = self.settings.build_user_agents();
        let probed = crate::probe::probe(
            &new_url,
            &headers,
            proxy_url.as_deref(),
            &pool,
            &user_agents,
        )
        .await?;
        self.ledger.refresh_source(
            id,
            new_url,
            headers,
            probed.etag,
            probed.last_modified,
            probed.content_type,
            probed.file_size,
        )?;
        Ok(())
    }

    // ── Shared pipeline: probe → plan chunks → disk check → DB insert → spawn worker. ──
    async fn execute_download(&self, spec: DownloadSpec) -> PdmResult<u64> {
        let pool = self.worker_pool.pool_ref();
        let headers = crate::headers::filter_headers(&spec.headers);
        let settings = self.settings.get();
        let proxy_url_str = self.settings.resolve_proxy_url(&spec.proxy_name);
        let default_url = self.settings.resolve_proxy_url(&settings.default_proxy);
        let fallback = if proxy_url_str != default_url {
            default_url.clone()
        } else {
            None
        };
        let user_agents = self.settings.build_user_agents();

        let probed = crate::probe::probe_then_default_proxy(
            &spec.url,
            &headers,
            proxy_url_str.as_deref(),
            fallback.as_deref(),
            pool.as_ref(),
            &user_agents,
        )
        .await;
        let outcome = match probed {
            Ok(r) => {
                let name = if spec.file_name.is_empty() {
                    r.file_name
                } else {
                    spec.file_name.clone()
                };
                crate::probe::ProbeOutcome {
                    file_name: crate::filename::sanitize(&name),
                    file_size: r.file_size,
                    supports_range: r.supports_range,
                    content_type: r.content_type,
                    etag: r.etag,
                    last_modified: r.last_modified,
                    final_url: r.final_url,
                    is_hls: r.is_hls,
                }
            }
            Err(_) => {
                crate::probe::probe_with_fallback(
                    &spec.url,
                    &headers,
                    proxy_url_str.as_deref(),
                    &pool,
                    &user_agents,
                    &spec.file_name,
                )
                .await
            }
        };

        let file_name = outcome.file_name;
        let file_size = outcome.file_size;
        let supports_range = outcome.supports_range;

        if file_size > 0 {
            self.log_info(&format!(
                "Probe ok url={} size={} range={} name={}",
                spec.url, file_size, supports_range, file_name
            ));
        } else {
            self.log_warn(&format!(
                "Probe failed, forcing blind download url={}",
                spec.url
            ));
        }

        let connections = crate::engine::chunk::compute_connection_count(
            file_size,
            spec.connections,
            settings.max_connections,
        );

        let save_dir = if spec.save_path.is_empty() {
            settings.download_dir.clone()
        } else {
            spec.save_path.clone()
        };
        let candidate = std::path::Path::new(&save_dir).join(&file_name);
        let candidate_str = candidate.to_string_lossy().to_string();

        if let Some(dup) = self.ledger.find_active_duplicate(&spec.url, &candidate_str)? {
            return Err(PdmError::DuplicateDownload(dup));
        }

        let full_path = apply_conflict_policy(
            &save_dir,
            &file_name,
            settings.file_conflict,
        )?;

        crate::engine::chunk::check_disk_space(&full_path, file_size)?;

        let id = self.worker_pool.next_id();
        let plan = crate::engine::chunk::plan_chunks(
            file_size,
            connections,
            supports_range && !outcome.is_hls,
            settings.max_connections,
        );

        let item = DownloadItem {
            id,
            url: spec.url.clone(),
            file_name,
            save_path: full_path,
            total_size: file_size,
            downloaded: 0,
            status: if spec.start_paused {
                DownloadStatus::Paused
            } else {
                DownloadStatus::Connecting
            },
            parts: plan.parts,
            proxy_name: spec.proxy_name,
            connections,
            resumable: Some(supports_range),
            created_at: now_str(),
            last_try: String::new(),
            headers,
            final_url: if outcome.final_url.is_empty() {
                spec.url
            } else {
                outcome.final_url
            },
            content_type: outcome.content_type,
            etag: outcome.etag,
            last_modified: outcome.last_modified,
            rate_limit_bps: spec.rate_limit_bps,
            ..Default::default()
        };
        self.ledger.insert_item(&item)?;

        if spec.start_paused {
            return Ok(id);
        }

        if outcome.is_hls {
            return self.spawn_hls(item, proxy_url_str).await;
        }

        let cfg = item.to_engine_config(
            &proxy_url_str.unwrap_or_default(),
            &settings.user_agent,
            settings.global_rate_limit,
            settings.max_retries,
        );
        match self.worker_pool.add_with_id(cfg, id, self.make_hooks()).await? {
            Admission::Queued => self.ledger.mark_queued(id),
            Admission::Started => {}
        }

        Ok(id)
    }

    async fn spawn_hls(&self, item: DownloadItem, proxy_url: Option<String>) -> PdmResult<u64> {
        let id = item.id;
        let pool = self.worker_pool.pool_ref();
        let headers = item.headers.clone();
        let save_path = item.save_path.clone();
        let url = if item.final_url.is_empty() {
            item.url.clone()
        } else {
            item.final_url.clone()
        };
        let ua = self.settings.get().user_agent;
        let conns = item.connections.max(1);
        let ledger = self.ledger.clone();
        let bus = self.bus.clone();
        tauri::async_runtime::spawn(async move {
            ledger.on_started(id);
            let result = crate::engine::hls::download_hls(
                &url,
                &save_path,
                &headers,
                proxy_url.as_deref(),
                pool.as_ref(),
                &ua,
                conns,
                |done, total| {
                    let _ = done;
                    let _ = total;
                },
            )
            .await;
            match result {
                Ok(()) => {
                    ledger.on_completed(id);
                    bus.emit(
                        crate::event_bus::FrontendEvent::DownloadCompleted,
                        serde_json::json!({ "id": id, "file_name": item.file_name }),
                    );
                }
                Err(e) => {
                    ledger.on_error(id, e.to_string());
                    bus.emit(
                        crate::event_bus::FrontendEvent::DownloadError,
                        serde_json::json!({ "id": id, "url": url, "message": e.to_string() }),
                    );
                }
            }
        });
        Ok(id)
    }
}

pub fn apply_conflict_policy(
    dir: &str,
    filename: &str,
    policy: crate::types::FileConflictPolicy,
) -> PdmResult<String> {
    let trimmed = dir.trim_end_matches(['/', '\\']);
    let dir = if trimmed.is_empty() || trimmed.ends_with(':') {
        dir
    } else {
        trimmed
    };
    let dir_path = std::path::Path::new(dir);
    let candidate = dir_path.join(filename);
    if !candidate.exists() {
        return Ok(candidate.to_string_lossy().to_string());
    }
    match policy {
        crate::types::FileConflictPolicy::Rename => Ok(unique_filename(dir, filename)),
        crate::types::FileConflictPolicy::Overwrite => {
            Ok(candidate.to_string_lossy().to_string())
        }
        crate::types::FileConflictPolicy::Skip | crate::types::FileConflictPolicy::Ask => {
            Err(PdmError::FileExists(candidate.to_string_lossy().to_string()))
        }
    }
}

pub fn unique_filename(dir: &str, filename: &str) -> String {
    // Trim trailing separators, but keep them for bare roots ("/" or "C:\"),
    // where trimming would yield "" or a drive-relative "C:".
    let trimmed = dir.trim_end_matches(['/', '\\']);
    let dir = if trimmed.is_empty() || trimmed.ends_with(':') {
        dir
    } else {
        trimmed
    };
    let dir = std::path::Path::new(dir);
    let candidate = dir.join(filename);
    if !candidate.exists() {
        return candidate.to_string_lossy().to_string();
    }
    let (stem, ext) = match filename.rfind('.') {
        Some(dot) => (&filename[..dot], &filename[dot..]),
        None => (filename, ""),
    };
    let mut n = 1;
    loop {
        let candidate = dir.join(format!("{}.{}{}", stem, n, ext));
        if !candidate.exists() {
            return candidate.to_string_lossy().to_string();
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
    headers: std::collections::HashMap<String, String>,
    rate_limit_bps: u64,
    start_paused: bool,
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
    fn conflict_rename_adds_number() {
        let dir = std::env::temp_dir().join("pdm_test_unique_2");
        let _ = std::fs::create_dir_all(&dir);
        std::fs::write(dir.join("test.zip"), b"x").unwrap();
        let result = apply_conflict_policy(
            dir.to_str().unwrap(),
            "test.zip",
            crate::types::FileConflictPolicy::Rename,
        )
        .unwrap();
        assert!(result.ends_with("test.1.zip"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn conflict_ask_errors() {
        let dir = std::env::temp_dir().join("pdm_test_unique_3");
        let _ = std::fs::create_dir_all(&dir);
        std::fs::write(dir.join("test.zip"), b"x").unwrap();
        let err = apply_conflict_policy(
            dir.to_str().unwrap(),
            "test.zip",
            crate::types::FileConflictPolicy::Ask,
        )
        .unwrap_err();
        assert!(matches!(err, PdmError::FileExists(_)));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
