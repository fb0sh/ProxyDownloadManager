use crate::engine::EngineHooks;
use crate::event_bus::{EventBus, FrontendEvent};
use crate::logger::Logger;
use crate::services::settings_service::SettingsService;
use crate::state::ledger::ProgressLedger;
use crate::types::engine_config::item_is_hls;
use crate::types::*;
use crate::worker::{Admission, PendingPatch, WorkerPool};
use std::sync::{Arc, Mutex};
use std::time::Instant;

pub struct DownloadManager {
    ledger: Arc<ProgressLedger>,
    pub(crate) worker_pool: WorkerPool,
    logger: Mutex<Logger>,
    pub(crate) settings: Arc<SettingsService>,
    bus: Arc<EventBus>,
    probe_cache: crate::probe::ProbeCache,
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
            probe_cache: crate::probe::ProbeCache::new(),
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

    /// Handle one engine report: the ledger records it, then the frontend is
    /// told. State first: the frontend refetches on these events, and the
    /// refetch must see what the report changed.
    pub fn handle_event(&self, event: Event) {
        let id = event.download_id;
        let item = self.ledger.get_item(id).ok().flatten();
        let url_info = item
            .as_ref()
            .map(|item| format!(" url={}", item.url))
            .unwrap_or_default();
        self.log_info(&format!(
            "Event: {} id={}{}",
            event.kind.name(),
            id,
            url_info
        ));

        self.ledger.apply(id, &event.kind);

        let (name, payload) = match event.kind {
            EventKind::DownloadStarted => (FrontendEvent::DownloadStarted, serde_json::json!(id)),
            EventKind::DownloadCompleted => {
                let file_name = item.map(|item| item.file_name).unwrap_or_default();
                (
                    FrontendEvent::DownloadCompleted,
                    serde_json::json!({ "id": id, "file_name": file_name }),
                )
            }
            EventKind::DownloadErrored(error) => {
                let url = item.map(|item| item.url).unwrap_or_default();
                (
                    FrontendEvent::DownloadError,
                    serde_json::json!({ "id": id, "url": url, "message": error.to_string() }),
                )
            }
            EventKind::DownloadProgress {
                downloaded,
                parts,
                reset_to_single,
            } => {
                let mut payload =
                    serde_json::json!({ "id": id, "downloaded": downloaded, "parts": parts });
                if reset_to_single {
                    payload["reset_to_single"] = serde_json::json!(true);
                }
                (FrontendEvent::DownloadProgress, payload)
            }
            EventKind::SegmentProgress { done, total, phase } => (
                FrontendEvent::DownloadProgress,
                serde_json::json!({
                    "id": id,
                    "downloaded": done,
                    "total_size": total,
                    "status": phase.as_str(),
                }),
            ),
            // No byte count: the frontend keeps the one it has.
            EventKind::PhaseChanged(phase) => (
                FrontendEvent::DownloadProgress,
                serde_json::json!({ "id": id, "status": phase.as_str() }),
            ),
        };
        self.bus.emit(name, payload);
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
        self.bus.emit(
            FrontendEvent::DownloadPaused,
            serde_json::json!({ "id": id }),
        );
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
            let save_path = plan.item.save_path.clone();
            let file_name = plan.item.file_name.clone();
            crate::engine::file_io::migrate_legacy_temp(id, &save_path);
            let temp = crate::engine::file_io::temp_path(id);
            if std::path::Path::new(&temp).exists() {
                match crate::engine::file_io::finalize_file(id, &save_path).await {
                    Ok(()) => {
                        self.ledger.on_completed(id);
                        self.bus.emit(
                            FrontendEvent::DownloadCompleted,
                            serde_json::json!({ "id": id, "file_name": file_name }),
                        );
                        return Ok(());
                    }
                    Err(e) => {
                        let _ = self.ledger.on_paused(id);
                        return Err(PdmError::Io(e));
                    }
                }
            }
            if std::path::Path::new(&save_path).exists() {
                self.ledger.on_completed(id);
                self.bus.emit(
                    FrontendEvent::DownloadCompleted,
                    serde_json::json!({ "id": id, "file_name": file_name }),
                );
                return Ok(());
            }
            let _ = self.ledger.on_paused(id);
            return Err(PdmError::Io(format!(
                "partial file missing for download {id}"
            )));
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

        match self
            .worker_pool
            .add_with_id(cfg, id, self.make_hooks())
            .await
        {
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
        self.bus.emit(
            FrontendEvent::DownloadResumed,
            serde_json::json!({ "id": id }),
        );
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
        crate::engine::file_io::remove_temp(id);
        if let Some(path) = save_path {
            let _ = std::fs::remove_file(path);
        }
        Ok(())
    }

    /// Cancel a download without deleting records.
    pub async fn cancel_download(&self, id: u64) {
        let was_queued = self
            .ledger
            .get_item(id)
            .ok()
            .flatten()
            .map(|item| matches!(item.status, DownloadStatus::Queued))
            .unwrap_or(false);
        self.worker_pool.cancel(id).await;
        if was_queued {
            let _ = self.ledger.on_paused(id);
        }
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
        let result = crate::probe::cached_probe(
            &self.probe_cache,
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
        let item = self.ledger.get_item(id)?.ok_or(PdmError::NotFound(id))?;
        let caps = runtime_control_capabilities(&item.status, item.resumable, item_is_hls(&item));
        if !caps.connections {
            return Err(PdmError::Unsupported(
                "connections cannot be changed in this state".into(),
            ));
        }
        let settings = self.settings.get();
        let applied = crate::engine::chunk::compute_connection_count(
            item.total_size,
            connections,
            settings.max_connections,
        );
        // 0 stays Auto only when settings is also Auto. A settings default is
        // a concrete count, so this task does not keep size-detecting.
        let stored = if connections == 0 && settings.max_connections == 0 {
            0
        } else {
            applied
        };
        let previous = item.connections;
        self.ledger.update_connections(id, stored)?;
        if item.status.is_live() {
            // Auto means the engine keeps adapting; an explicit count means the
            // engine stops adapting and holds exactly what the user picked.
            let applied_ok = if stored == 0 {
                self.worker_pool.set_auto(id).await
            } else {
                self.worker_pool.set_connections(id, applied).await
            };
            if !applied_ok {
                let _ = self.ledger.update_connections(id, previous);
                return Err(PdmError::Other(
                    "download is not running, connections were not changed".into(),
                ));
            }
        } else if matches!(item.status, DownloadStatus::Queued) {
            if !self
                .worker_pool
                .configure_pending(
                    id,
                    PendingPatch {
                        connections: Some(stored),
                        ..PendingPatch::default()
                    },
                )
                .await
            {
                let _ = self.ledger.update_connections(id, previous);
                return Err(PdmError::Other(
                    "download is no longer queued, connections were not changed".into(),
                ));
            }
        }
        Ok(())
    }

    /// Change the proxy the download will use.
    /// An active resumable HTTP download is paused, the name is stored, then
    /// the same id resumes so the new client is built from that name. A failed
    /// resume leaves the row paused with its temp file and progress intact.
    pub async fn switch_download_proxy(&self, id: u64, proxy_name: String) -> PdmResult<()> {
        let item = self.ledger.get_item(id)?.ok_or(PdmError::NotFound(id))?;
        let caps = runtime_control_capabilities(&item.status, item.resumable, item_is_hls(&item));
        if !caps.proxy {
            return Err(PdmError::Unsupported(
                "proxy cannot be changed in this state".into(),
            ));
        }
        if item.status.is_live() {
            self.pause_download(id).await?;
            let after = self.ledger.get_item(id)?.ok_or(PdmError::NotFound(id))?;
            if !matches!(after.status, DownloadStatus::Paused) {
                return Err(PdmError::Other(
                    "download could not be paused to switch proxy".into(),
                ));
            }
            self.ledger.update_proxy_name(id, proxy_name)?;
            return self.resume_download(id).await;
        }
        if matches!(item.status, DownloadStatus::Paused) {
            return self.ledger.update_proxy_name(id, proxy_name);
        }
        if matches!(item.status, DownloadStatus::Queued) {
            let previous = item.proxy_name.clone();
            self.ledger.update_proxy_name(id, proxy_name.clone())?;
            let url = self
                .settings
                .resolve_proxy_url(&proxy_name)
                .unwrap_or_default();
            if !self
                .worker_pool
                .configure_pending(
                    id,
                    PendingPatch {
                        proxy_url: Some(url),
                        proxy_name: Some(proxy_name),
                        ..PendingPatch::default()
                    },
                )
                .await
            {
                let _ = self.ledger.update_proxy_name(id, previous);
                return Err(PdmError::Other(
                    "download is no longer queued, proxy was not changed".into(),
                ));
            }
            return Ok(());
        }
        Err(PdmError::Unsupported(
            "proxy cannot be changed in this state".into(),
        ))
    }

    pub async fn set_runtime_rate_limit(&self, id: u64, rate_limit_bps: u64) -> PdmResult<()> {
        let item = self.ledger.get_item(id)?.ok_or(PdmError::NotFound(id))?;
        let caps = runtime_control_capabilities(&item.status, item.resumable, item_is_hls(&item));
        if !caps.rate_limit {
            return Err(PdmError::Unsupported(
                "rate limit cannot be changed in this state".into(),
            ));
        }
        let previous = item.rate_limit_bps;
        self.ledger.update_rate_limit(id, rate_limit_bps)?;
        if item.status.is_live() {
            if !self
                .worker_pool
                .set_download_rate_limit(id, rate_limit_bps)
                .await
            {
                let _ = self.ledger.update_rate_limit(id, previous);
                return Err(PdmError::Other(
                    "download is not running, rate limit was not changed".into(),
                ));
            }
        } else if matches!(item.status, DownloadStatus::Queued) {
            if !self
                .worker_pool
                .configure_pending(
                    id,
                    PendingPatch {
                        rate_limit_bps: Some(rate_limit_bps),
                        ..PendingPatch::default()
                    },
                )
                .await
            {
                let _ = self.ledger.update_rate_limit(id, previous);
                return Err(PdmError::Other(
                    "download is no longer queued, rate limit was not changed".into(),
                ));
            }
        }
        Ok(())
    }

    pub fn set_global_rate_limit(&self, bps: u64) {
        self.worker_pool.set_global_rate_limit(bps);
    }

    /// Change how many downloads may run at once. `0` is unlimited; queued
    /// downloads start immediately when the new limit allows.
    pub async fn set_max_active_downloads(&self, max_active: u32) {
        self.worker_pool.set_max_active(max_active).await;
    }

    pub async fn refresh_url(
        &self,
        id: u64,
        new_url: String,
        headers: std::collections::HashMap<String, String>,
    ) -> PdmResult<()> {
        let headers = crate::headers::filter_headers(&headers);
        let existing = self.ledger.get_item(id)?.ok_or(PdmError::NotFound(id))?;
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
        let startup = Instant::now();

        let probed = crate::probe::cached_probe(
            &self.probe_cache,
            &spec.url,
            &headers,
            proxy_url_str.as_deref(),
            fallback.as_deref(),
            pool.as_ref(),
            &user_agents,
        )
        .await;
        log::debug!("[startup] probe={}ms", startup.elapsed().as_millis());
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

        let requested_connections = spec.connections;
        let connections = crate::engine::chunk::compute_connection_count(
            file_size,
            requested_connections,
            settings.max_connections,
        );

        let save_dir = if spec.save_path.is_empty() {
            settings.download_dir.clone()
        } else {
            spec.save_path.clone()
        };
        let candidate = std::path::Path::new(&save_dir).join(&file_name);
        let candidate_str = candidate.to_string_lossy().to_string();

        if let Some(dup) = self
            .ledger
            .find_active_duplicate(&spec.url, &candidate_str)?
        {
            return Err(PdmError::DuplicateDownload(dup));
        }

        let full_path = apply_conflict_policy(&save_dir, &file_name, settings.file_conflict)?;

        crate::engine::chunk::check_disk_space(&full_path, file_size)?;

        let id = self.worker_pool.next_id();
        let planning = Instant::now();
        let plan = crate::engine::chunk::plan_chunks(
            file_size,
            requested_connections,
            supports_range && !outcome.is_hls,
            settings.max_connections,
        );
        log::debug!("[startup] planning={}ms", planning.elapsed().as_millis());

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
            // 0 stays Auto only when settings is also Auto. A settings default
            // is applied once and stored, so the worker does not size-detect.
            connections: if requested_connections == 0 && settings.max_connections == 0 {
                0
            } else {
                connections
            },
            resumable: Some(supports_range),
            created_at: now_str(),
            last_try: String::new(),
            headers,
            final_url: if outcome.final_url.is_empty() {
                spec.url
            } else {
                outcome.final_url
            },
            content_type: if outcome.is_hls
                && !outcome
                    .content_type
                    .to_ascii_lowercase()
                    .contains("mpegurl")
            {
                "application/vnd.apple.mpegurl".into()
            } else {
                outcome.content_type
            },
            etag: outcome.etag,
            last_modified: outcome.last_modified,
            rate_limit_bps: spec.rate_limit_bps,
            ..Default::default()
        };
        self.ledger.insert_item(&item)?;

        if spec.start_paused {
            return Ok(id);
        }

        let cfg = item.to_engine_config(
            &proxy_url_str.unwrap_or_default(),
            &settings.user_agent,
            settings.global_rate_limit,
            settings.max_retries,
        );
        match self
            .worker_pool
            .add_with_id(cfg, id, self.make_hooks())
            .await?
        {
            Admission::Queued => self.ledger.mark_queued(id),
            Admission::Started => {}
        }
        log::debug!("[startup] workers={}ms", startup.elapsed().as_millis());

        Ok(id)
    }
}

/// Which detail-window controls the engine will actually honor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RuntimeCaps {
    pub proxy: bool,
    pub connections: bool,
    pub rate_limit: bool,
}

pub fn runtime_control_capabilities(
    status: &DownloadStatus,
    resumable: Option<bool>,
    is_hls: bool,
) -> RuntimeCaps {
    let none = RuntimeCaps {
        proxy: false,
        connections: false,
        rate_limit: false,
    };
    match status {
        DownloadStatus::Completed | DownloadStatus::Failed(_) => none,
        // Paused values are stored and used on the next resume.
        DownloadStatus::Paused => RuntimeCaps {
            proxy: true,
            connections: true,
            rate_limit: true,
        },
        // Queued values are written into the pending engine config. A
        // non-range HTTP download never reads the connection count.
        DownloadStatus::Queued => RuntimeCaps {
            proxy: true,
            connections: is_hls || resumable != Some(false),
            rate_limit: true,
        },
        _ => {
            if is_hls || resumable == Some(false) {
                RuntimeCaps {
                    proxy: false,
                    connections: false,
                    rate_limit: true,
                }
            } else {
                RuntimeCaps {
                    proxy: true,
                    connections: true,
                    rate_limit: true,
                }
            }
        }
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
        crate::types::FileConflictPolicy::Overwrite => Ok(candidate.to_string_lossy().to_string()),
        crate::types::FileConflictPolicy::Skip | crate::types::FileConflictPolicy::Ask => Err(
            PdmError::FileExists(candidate.to_string_lossy().to_string()),
        ),
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
mod lifecycle_tests;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runtime_caps_follow_what_the_engine_can_apply() {
        let all = RuntimeCaps {
            proxy: true,
            connections: true,
            rate_limit: true,
        };
        let rate_only = RuntimeCaps {
            proxy: false,
            connections: false,
            rate_limit: true,
        };
        let none = RuntimeCaps {
            proxy: false,
            connections: false,
            rate_limit: false,
        };
        assert_eq!(
            runtime_control_capabilities(&DownloadStatus::Paused, Some(false), true),
            all
        );
        assert_eq!(
            runtime_control_capabilities(&DownloadStatus::Downloading, Some(true), false),
            all
        );
        assert_eq!(
            runtime_control_capabilities(&DownloadStatus::Connecting, None, false),
            all
        );
        assert_eq!(
            runtime_control_capabilities(&DownloadStatus::Downloading, Some(false), false),
            rate_only
        );
        assert_eq!(
            runtime_control_capabilities(&DownloadStatus::Retrying, Some(true), true),
            rate_only
        );
        assert_eq!(
            runtime_control_capabilities(&DownloadStatus::Completed, Some(true), false),
            none
        );
        assert_eq!(
            runtime_control_capabilities(
                &DownloadStatus::Failed("HTTP 403".into()),
                Some(true),
                false
            ),
            none
        );
        let queued_single =
            runtime_control_capabilities(&DownloadStatus::Queued, Some(false), false);
        assert!(queued_single.proxy && queued_single.rate_limit);
        assert!(!queued_single.connections);
        let queued_hls = runtime_control_capabilities(&DownloadStatus::Queued, Some(false), true);
        assert!(queued_hls.connections);
    }

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
