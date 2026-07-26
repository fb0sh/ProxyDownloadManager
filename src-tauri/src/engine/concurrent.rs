use crate::network::pool::NetworkPool;
use crate::network::limiter::MultiLimiter;
use crate::engine::chunk::{self, ChunkQueue};
use crate::engine::file_io::{create_output_file, finalize_file};
use crate::engine::part_progress::{encode_progress_data, PartProgressTracker, PartRange};
use crate::engine::task_download::{download_task, TaskResult};
use crate::types::{Event, EventKind, EngineConfig, PdmError, PdmResult};
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
        let part_ranges: Vec<PartRange> = if cfg.part_ranges.is_empty() {
            if cfg.total_size > 0 {
                vec![PartRange { start: 0, end: cfg.total_size }]
            } else {
                vec![]
            }
        } else {
            cfg.part_ranges
                .iter()
                .map(|&(start, end)| PartRange { start, end })
                .collect()
        };

        // The resume plan (ledger `begin_resume`) is authoritative: tasks,
        // per-part progress and the total arrive mutually consistent, so the
        // engine no longer re-derives or reconciles them.
        let (tasks, resume_offset) = if cfg.is_resume {
            log::info!(
                "[ProxyDM] concurrent id={} resume with {} tasks, downloaded={}",
                cfg.id,
                cfg.resume_tasks.len(),
                cfg.downloaded
            );
            (cfg.resume_tasks.clone(), cfg.downloaded)
        } else {
            (chunk::compute_chunks(cfg.total_size, cfg.connections.max(1), 0), 0)
        };
        let bytes_written = Arc::new(AtomicU64::new(resume_offset));

        let parts_tracker = PartProgressTracker::new(part_ranges);
        if !cfg.part_downloaded.is_empty() {
            parts_tracker.seed_from_parts(&cfg.part_downloaded);
        }

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

        // `cancel` means exactly one thing: user pause. Internal aborts (retry
        // exhaustion, range loss) use `stop` + `abort_reason` instead, so the
        // outcome ladder below can tell the three apart. Workers only watch
        // `stop`; a small forwarder mirrors the external pause flag into it.
        let stop = Arc::new(AtomicBool::new(false));
        let abort_reason: Arc<std::sync::Mutex<Option<PdmError>>> =
            Arc::new(std::sync::Mutex::new(None));
        let forwarder_handle = {
            let cancel = cancel.clone();
            let stop = stop.clone();
            tokio::spawn(async move {
                while !stop.load(Ordering::Relaxed) {
                    if cancel.load(Ordering::Relaxed) {
                        stop.store(true, Ordering::Relaxed);
                        break;
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                }
            })
        };

        // Spawn periodic progress reporter
        let reporter_stop = Arc::new(AtomicBool::new(false));
        let progress_cancel = reporter_stop.clone();
        let progress_tx = self.event_tx.clone();
        let progress_bytes = bytes_written.clone();
        let progress_parts = parts_tracker.clone();
        let reporter_handle = tokio::spawn(async move {
            loop {
                if progress_cancel.load(Ordering::Relaxed) { break; }
                let size = progress_bytes.load(Ordering::Relaxed);
                let part_snap = progress_parts.snapshot();
                let _ = progress_tx.send(Event {
                    kind: EventKind::DownloadProgress,
                    download_id,
                    data: Some(encode_progress_data(size, &part_snap, false)),
                });
                tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            }
        });

        // Spawn workers
        for _worker_id in 0..num_workers {
            let queue = queue.clone();
            let file = file.clone();
            let client = client.clone();
            let stop = stop.clone();
            let abort_reason = abort_reason.clone();
            let limiter = limiter.clone();
            let url = cfg.url.clone();
            let max_retries = cfg.max_retries;
            let user_agent = cfg.user_agent.clone();
            let stop_for_task = stop.clone();
            let bytes_written = bytes_written.clone();
            let parts = parts_tracker.clone();

            // On any abort the popped task goes back into the queue first, so
            // the drain-based resume snapshot always covers remaining work.
            let abort = move |queue: &ChunkQueue,
                              task: crate::types::Task,
                              reason: PdmError,
                              stop: &AtomicBool,
                              abort_reason: &std::sync::Mutex<Option<PdmError>>| {
                queue.push(task);
                if let Ok(mut guard) = abort_reason.lock() {
                    if guard.is_none() {
                        *guard = Some(reason);
                    }
                }
                stop.store(true, Ordering::Relaxed);
            };

            let handle = tokio::spawn(async move {
                let mut retries_left = max_retries;
                loop {
                    if stop.load(Ordering::Relaxed) {
                        return;
                    }
                    let task = match queue.pop() {
                        Some(t) => t,
                        None => break,
                    };

                    let result = download_task(
                        &url, &client, &*file, &task, &stop_for_task, &limiter, &user_agent, &bytes_written,
                        Some(parts.clone()),
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
                        TaskResult::RangeNotSupported => {
                            abort(&queue, task, PdmError::RangeLost, &stop, &abort_reason);
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
                                abort(
                                    &queue,
                                    task,
                                    PdmError::RetriesExhausted(msg),
                                    &stop,
                                    &abort_reason,
                                );
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

        stop.store(true, Ordering::Relaxed);
        let _ = forwarder_handle.await;
        reporter_stop.store(true, Ordering::Relaxed);
        let _ = reporter_handle.await;

        // Final progress snapshot so the Progress Map reaches 100% without waiting for poll.
        {
            let size = bytes_written.load(Ordering::Relaxed);
            let part_snap = parts_tracker.snapshot();
            let _ = self.event_tx.send(Event {
                kind: EventKind::DownloadProgress,
                download_id,
                data: Some(encode_progress_data(size, &part_snap, false)),
            });
        }

        let _ = file.sync_all();

        let mut save_snapshot = || {
            let saved = crate::types::DownloadState {
                url: cfg.url.clone(),
                id: cfg.id,
                file_name: cfg.file_name.clone(),
                save_path: cfg.save_path.clone(),
                total_size: cfg.total_size,
                downloaded: bytes_written.load(Ordering::Relaxed),
                tasks: queue.drain(),
                proxy_name: cfg.proxy_name.clone(),
                workers: num_workers,
            };
            on_resume(cfg.id, &saved);
        };

        // Outcome ladder: user pause wins over any internal abort.
        if cancel.load(Ordering::Relaxed) {
            save_snapshot();
            return Err(PdmError::Cancelled);
        }
        let aborted = abort_reason.lock().ok().and_then(|mut g| g.take());
        if let Some(err) = aborted {
            save_snapshot();
            return Err(err);
        }

        if !queue.is_empty() || bytes_written.load(Ordering::Relaxed) < cfg.total_size {
            let downloaded = bytes_written.load(Ordering::Relaxed);
            return Err(PdmError::Other(format!("Download incomplete: {}/{} bytes", downloaded, cfg.total_size)));
        }

        // Release our handle before the rename — Windows refuses to rename a
        // file that still has an open handle with default share flags.
        drop(file);
        finalize_file(&cfg.save_path).await?;

        let _ = self.event_tx.send(Event {
            kind: EventKind::DownloadCompleted,
            download_id: cfg.id,
            data: None,
        });

        Ok(())
    }
}
