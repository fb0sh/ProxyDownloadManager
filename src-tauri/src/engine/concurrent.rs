use crate::network::pool::NetworkPool;
use crate::network::limiter::MultiLimiter;
use crate::engine::chunk::{self, ChunkQueue};
use crate::engine::file_io::{create_output_file, finalize_file};
use crate::engine::task_download::{download_task, TaskResult};
use crate::types::{Task, Event, EventKind, EngineConfig, PdmError, PdmResult};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::mpsc;

pub struct ConcurrentDownloader {
    pool: Arc<NetworkPool>,
    event_tx: mpsc::UnboundedSender<Event>,
}

impl ConcurrentDownloader {
    pub fn new(pool: Arc<NetworkPool>, event_tx: mpsc::UnboundedSender<Event>) -> Self {
        Self { pool, event_tx }
    }

    pub async fn download(&self, cfg: &EngineConfig, limiter: Arc<MultiLimiter>, cancel: Arc<AtomicBool>, on_resume: &crate::engine::OnResumeState) -> PdmResult<()> {
        // Always recompute tasks from cfg.downloaded to ensure resume_offset
        // matches actual bytes written. The saved resume_tasks may not include
        // the in-progress task that was lost on cancel.
        let (tasks, resume_offset) = if cfg.is_resume && cfg.downloaded > 0 {
            let remaining = cfg.total_size.saturating_sub(cfg.downloaded);
            let num_conns = 4.max(cfg.connections);
            log::info!("[ProxyDM] concurrent id={} resume from {} bytes, {} remaining", cfg.id, cfg.downloaded, remaining);
            let base = cfg.downloaded;
            let tasks: Vec<Task> = chunk::compute_chunks(remaining, num_conns, 0)
                .into_iter()
                .map(|t| Task { offset: t.offset + base, length: t.length })
                .collect();
            (tasks, base)
        } else if cfg.is_resume {
            // Fallback: downloaded=0 but is_resume=true (gob lost or corrupted)
            let remaining = cfg.total_size;
            let num_conns = 4.max(cfg.connections);
            log::info!("[ProxyDM] concurrent id={} resume with no progress, recomputing from scratch", cfg.id);
            (chunk::compute_chunks(remaining, num_conns, 0), 0)
        } else {
            (chunk::compute_chunks(cfg.total_size, cfg.connections.max(1), 0), 0)
        };
        let bytes_written = Arc::new(AtomicU64::new(resume_offset));

        let num_conns = if cfg.connections > 0 {
            cfg.connections.min(32)
        } else {
            let sqrt = (cfg.total_size as f64 / 1024.0 / 1024.0).sqrt() as u32;
            sqrt.max(1).min(32)
        };

        if tasks.is_empty() {
            return Err(PdmError::Other(format!("No tasks to download for id={}", cfg.id)));
        }

        let num_workers = num_conns.min(tasks.len() as u32).max(1);
        log::info!("[ProxyDM] concurrent id={} workers={} chunks={} total_size={} is_resume={}",
            cfg.id, num_workers, tasks.len(), cfg.total_size, cfg.is_resume);

        let queue = Arc::new(ChunkQueue::new(tasks));

        let file = create_output_file(&cfg.save_path, cfg.total_size).await?;
        let file = Arc::new(file);
        log::info!("[ProxyDM] concurrent id={} file created: {}.pdm", cfg.id, cfg.save_path);

        let client = self.pool.get_client(if cfg.proxy_url.is_empty() { None } else { Some(&cfg.proxy_url) })?;

        let mut handles = Vec::new();
        let download_id = cfg.id;

        // Spawn periodic progress reporter
        let reporter_stop = Arc::new(AtomicBool::new(false));
        let progress_cancel = reporter_stop.clone();
        let progress_tx = self.event_tx.clone();
        let progress_bytes = bytes_written.clone();
        let reporter_handle = tokio::spawn(async move {
            loop {
                if progress_cancel.load(Ordering::Relaxed) { break; }
                let size = progress_bytes.load(Ordering::Relaxed);
                let _ = progress_tx.send(Event {
                    kind: EventKind::DownloadProgress,
                    download_id,
                    data: Some(format!("{}", size)),
                });
                tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            }
        });

        // Spawn workers
        for _worker_id in 0..num_workers {
            let queue = queue.clone();
            let file = file.clone();
            let client = client.clone();
            let cancel = cancel.clone();
            let limiter = limiter.clone();
            let event_tx = self.event_tx.clone();
            let url = cfg.url.clone();
            let max_retries = cfg.max_retries;
            let user_agent = cfg.user_agent.clone();
            let cancel_for_task = cancel.clone();
            let bytes_written = bytes_written.clone();

            let handle = tokio::spawn(async move {
                let mut retries_left = max_retries;
                loop {
                    if cancel.load(Ordering::Relaxed) {
                        return;
                    }
                    let task = match queue.pop() {
                        Some(t) => t,
                        None => break,
                    };

                    let result = download_task(
                        &url, &client, &*file, &task, &cancel_for_task, &limiter, &user_agent, &bytes_written,
                    ).await;

                    match result {
                        TaskResult::Complete => {
                            retries_left = max_retries;
                        }
                        TaskResult::Partial { remaining } => {
                            log::info!("[ProxyDM] task offset={} partial, re-queueing {} bytes", task.offset, remaining.length);
                            queue.push(remaining);
                            retries_left = max_retries;
                        }
                        TaskResult::Cancelled => {
                            return;
                        }
                        TaskResult::Fatal(msg) => {
                            let attempt = max_retries.saturating_sub(retries_left) + 1;
                            let backoff_secs = 2u64.pow(attempt.min(5) as u32).min(30);
                            tokio::time::sleep(std::time::Duration::from_secs(backoff_secs)).await;
                            if retries_left > 0 {
                                retries_left -= 1;
                                queue.push(task);
                            } else {
                                log::error!("retries exhausted for offset={}, stopping", task.offset);
                                // Set cancel flag so other workers stop and the
                                // concurrent downloader enters the cancel path
                                // (saves resume state instead of degrading to single)
                                cancel.store(true, Ordering::Relaxed);
                                let _ = event_tx.send(crate::types::Event {
                                    kind: crate::types::EventKind::DownloadErrored,
                                    download_id,
                                    data: Some(format!("Retries exhausted: {}", msg)),
                                });
                                return;
                            }
                        }
                    }
                }
            });
            handles.push(handle);
        }

        for h in handles {
            let _ = h.await;
        }
        log::info!("[ProxyDM] concurrent id={} all workers done", cfg.id);

        reporter_stop.store(true, Ordering::Relaxed);
        let _ = reporter_handle.await;

        let _ = file.sync_all();

        if cancel.load(Ordering::Relaxed) {
            let remaining_tasks = queue.drain();
            let saved = crate::types::DownloadState {
                url: cfg.url.clone(),
                id: cfg.id,
                file_name: cfg.file_name.clone(),
                save_path: cfg.save_path.clone(),
                total_size: cfg.total_size,
                downloaded: bytes_written.load(Ordering::Relaxed),
                tasks: remaining_tasks,
                proxy_name: cfg.proxy_name.clone(),
                workers: num_workers,
            };
            on_resume(cfg.id, &saved);
            return Err(PdmError::Cancelled);
        }

        if !queue.is_empty() || bytes_written.load(Ordering::Relaxed) < cfg.total_size {
            let downloaded = bytes_written.load(Ordering::Relaxed);
            return Err(PdmError::Other(format!("Download incomplete: {}/{} bytes", downloaded, cfg.total_size)));
        }

        finalize_file(&cfg.save_path).await?;

        let _ = self.event_tx.send(Event {
            kind: EventKind::DownloadCompleted,
            download_id: cfg.id,
            data: None,
        });

        Ok(())
    }
}
