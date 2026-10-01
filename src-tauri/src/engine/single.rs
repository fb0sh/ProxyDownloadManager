use crate::engine::file_io::{self, length_shortfall};
use crate::engine::transfer::{Fetched, Transfer, Want};
use crate::network::limiter::MultiLimiter;
use crate::network::pool::NetworkPool;
use crate::network::protocol::PerfStats;
use crate::types::{EngineConfig, Event, EventKind, PdmError, PdmResult, Phase, Task};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::mpsc;

pub struct SingleDownloader {
    pool: Arc<NetworkPool>,
    event_tx: mpsc::UnboundedSender<Event>,
}

impl SingleDownloader {
    pub fn new(pool: Arc<NetworkPool>, event_tx: mpsc::UnboundedSender<Event>) -> Self {
        Self { pool, event_tx }
    }

    pub async fn download(
        &self,
        cfg: &EngineConfig,
        limiter: Arc<MultiLimiter>,
        cancel: Arc<AtomicBool>,
        on_resume: &crate::engine::OnResumeState,
    ) -> PdmResult<()> {
        log::info!("[ProxyDM] single id={} url={}", cfg.id, cfg.url);
        let client = self
            .pool
            .get_client(if cfg.proxy_url.is_empty() {
                None
            } else {
                Some(&cfg.proxy_url)
            })
            .map_err(|e| PdmError::ClientBuild(e.to_string()))?;
        let perf = Arc::new(PerfStats::new(cfg.id));
        let transfer = Transfer::new(client, cfg, limiter, cancel, perf.clone());

        file_io::migrate_legacy_temp(cfg.id, &cfg.save_path);
        let pdm_path = file_io::temp_path(cfg.id);
        if let Some(parent) = std::path::Path::new(&pdm_path).parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|e| PdmError::Io(e.to_string()))?;
        }
        let file = std::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .read(true)
            .open(&pdm_path)
            .map_err(|e| PdmError::Io(e.to_string()))?;

        // Ask for the tail only when the bytes before it are on disk. A
        // server that ignores the Range sends everything, and the transfer
        // then replaces the file from byte 0.
        let resume_from = if cfg.is_resume { cfg.downloaded } else { 0 };
        let on_disk = file.metadata().map(|m| m.len()).unwrap_or(0);
        let want = if resume_from > 0 && on_disk >= resume_from {
            Want::Range(Task {
                offset: resume_from,
                length: 0,
            })
        } else {
            Want::Whole
        };

        // This engine writes front to back, so the end of the last write is
        // the byte count; it drops back when a broken body starts over.
        let written = AtomicU64::new(match &want {
            Want::Range(task) => task.offset,
            Want::Whole => 0,
        });
        let report = |total: u64| {
            let _ = self.event_tx.send(Event {
                kind: EventKind::DownloadProgress {
                    downloaded: total,
                    parts: vec![total],
                    reset_to_single: true,
                },
                download_id: cfg.id,
            });
        };
        let on_write = |offset: u64, len: u64| {
            let total = offset + len;
            written.store(total, Ordering::Relaxed);
            // Idempotent: reports the first byte that reached the file.
            perf.note_progress();
            report(total);
        };
        let on_phase = |phase: Phase| {
            let _ = self.event_tx.send(Event {
                kind: EventKind::PhaseChanged(phase),
                download_id: cfg.id,
            });
        };

        let outcome = transfer
            .fetch(&cfg.url, &file, want, cfg.total_size, &on_write, &on_phase)
            .await;
        let total = written.load(Ordering::Relaxed);

        let save_progress = |written: u64| {
            let remaining = cfg.total_size.saturating_sub(written);
            if written > 0 && (cfg.total_size == 0 || remaining > 0) {
                let saved = crate::types::DownloadState {
                    url: cfg.url.clone(),
                    id: cfg.id,
                    file_name: cfg.file_name.clone(),
                    save_path: cfg.save_path.clone(),
                    total_size: cfg.total_size,
                    downloaded: written,
                    tasks: vec![Task {
                        offset: written,
                        length: remaining,
                    }],
                    proxy_name: cfg.proxy_name.clone(),
                    workers: 1,
                };
                on_resume(cfg.id, &saved);
            }
        };

        match outcome {
            Fetched::Complete => {}
            Fetched::Stopped => {
                save_progress(total);
                return Err(PdmError::Cancelled);
            }
            // A 206 that is not the tail that was asked for.
            Fetched::RangeLost => {
                log::warn!(
                    "[ProxyDM] single id={} resume range rejected at {}",
                    cfg.id,
                    resume_from
                );
                return Err(PdmError::Incomplete(format!(
                    "resume rejected at offset {resume_from}"
                )));
            }
            Fetched::Failed(error) => return Err(error),
        }

        file.sync_all().map_err(|e| PdmError::Io(e.to_string()))?;
        drop(file);

        if let Some(missing) = length_shortfall(total, cfg.total_size) {
            log::error!(
                "[ProxyDM] single id={} incomplete, missing {missing} bytes",
                cfg.id
            );
            save_progress(total);
            return Err(PdmError::Incomplete(format!(
                "{total}/{} bytes",
                cfg.total_size
            )));
        }

        file_io::finalize_file(cfg.id, &cfg.save_path)
            .await
            .map_err(PdmError::Io)?;

        log::info!("[ProxyDM] single id={} done total={} bytes", cfg.id, total);

        report(total);

        let _ = self.event_tx.send(Event {
            kind: EventKind::DownloadCompleted,
            download_id: cfg.id,
        });

        Ok(())
    }
}
