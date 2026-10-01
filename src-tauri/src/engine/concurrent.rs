use crate::engine::adaptive::{self, AdaptiveConfig, AdaptiveController};
use crate::engine::chunk::{self, ChunkQueue};
use crate::engine::file_io::{create_output_file, finalize_file};
use crate::engine::part_progress::{remaining_tasks_from_parts, PartProgressTracker, PartRange};
use crate::engine::transfer::{Attempt, Transfer, Want};
use crate::network::limiter::MultiLimiter;
use crate::network::pool::NetworkPool;
use crate::network::protocol::PerfStats;
use crate::types::{EngineConfig, Event, EventKind, PdmError, PdmResult, Phase, Task};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;

pub struct ConcurrentDownloader {
    pool: Arc<NetworkPool>,
    event_tx: mpsc::UnboundedSender<Event>,
}

impl ConcurrentDownloader {
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
        let part_ranges: Vec<PartRange> = if cfg.part_ranges.is_empty() {
            if cfg.total_size > 0 {
                vec![PartRange {
                    start: 0,
                    end: cfg.total_size,
                }]
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
        } else if !part_ranges.is_empty() {
            // The UI progress map was planned from these ranges (Auto is stored
            // as connections=0). Tasks must be those ranges. Planning from
            // connections.max(1) collapses Auto into one full-file request, so
            // every segment stays at 0 B until that single body is written.
            let downloaded = if cfg.part_downloaded.len() == part_ranges.len() {
                cfg.part_downloaded.clone()
            } else {
                vec![0; part_ranges.len()]
            };
            (remaining_tasks_from_parts(&part_ranges, &downloaded), 0)
        } else {
            let count = cfg
                .desired_connections
                .as_ref()
                .map(|a| a.load(Ordering::Relaxed))
                .filter(|n| *n > 0)
                .unwrap_or(if cfg.connections > 0 {
                    cfg.connections
                } else {
                    chunk::auto_connections(cfg.total_size)
                });
            (
                chunk::compute_chunks(cfg.total_size, count.max(1).min(chunk::MAX_CONNECTIONS), 0),
                0,
            )
        };
        let bytes_written = Arc::new(AtomicU64::new(resume_offset));
        let perf = Arc::new(PerfStats::new(cfg.id));

        let parts_tracker = PartProgressTracker::new(part_ranges);
        if !cfg.part_downloaded.is_empty() {
            parts_tracker.seed_from_parts(&cfg.part_downloaded);
        }

        let num_conns = {
            let target = cfg
                .desired_connections
                .as_ref()
                .map(|a| a.load(Ordering::Relaxed))
                .filter(|n| *n > 0)
                .unwrap_or(cfg.connections);
            if target > 0 {
                target.min(chunk::MAX_CONNECTIONS)
            } else {
                chunk::auto_connections(cfg.total_size).min(chunk::MAX_CONNECTIONS)
            }
        };

        if tasks.is_empty() {
            // Incomplete → the degrade whitelist lets Single try instead
            // (e.g. range-capable server with unknown size plans no chunks).
            return Err(PdmError::Incomplete(format!(
                "no tasks planned for id={}",
                cfg.id
            )));
        }

        let num_workers = num_conns.min(tasks.len() as u32).max(1);
        log::info!(
            "[ProxyDM] concurrent id={} workers={} chunks={} total_size={} is_resume={}",
            cfg.id,
            num_workers,
            tasks.len(),
            cfg.total_size,
            cfg.is_resume
        );

        let queue = Arc::new(ChunkQueue::new(tasks));

        let file = create_output_file(cfg.id, &cfg.save_path, cfg.total_size).await?;
        let file = Arc::new(file);
        log::debug!(
            "[ProxyDM] concurrent id={} temp={}",
            cfg.id,
            crate::engine::file_io::temp_path(cfg.id)
        );

        let client = self.pool.get_client(if cfg.proxy_url.is_empty() {
            None
        } else {
            Some(&cfg.proxy_url)
        })?;

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
            let mut last_bytes = u64::MAX;
            let mut last_parts: u64 = u64::MAX;
            loop {
                if progress_cancel.load(Ordering::Relaxed) {
                    break;
                }
                let size = progress_bytes.load(Ordering::Relaxed);
                let part_snap = progress_parts.snapshot();
                let parts_hash = part_snap
                    .iter()
                    .fold(0u64, |acc, n| acc.wrapping_mul(31).wrapping_add(*n));
                if size != last_bytes || parts_hash != last_parts {
                    if size > 0 && (last_bytes == 0 || last_bytes == u64::MAX) {
                        log::debug!("[startup] first-progress id={} bytes={}", download_id, size);
                    }
                    last_bytes = size;
                    last_parts = parts_hash;
                    let _ = progress_tx.send(Event {
                        kind: EventKind::DownloadProgress {
                            downloaded: size,
                            parts: part_snap,
                            reset_to_single: false,
                        },
                        download_id,
                    });
                }
                tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            }
        });

        let transfer = Arc::new(Transfer::new(
            client,
            cfg,
            limiter,
            stop.clone(),
            perf.clone(),
        ));
        let desired = cfg
            .desired_connections
            .clone()
            .unwrap_or_else(|| std::sync::Arc::new(AtomicU32::new(num_workers)));
        let live_workers = std::sync::Arc::new(AtomicU32::new(0));
        let phase = std::sync::Arc::new(PhaseCounts::default());

        let make_worker = || ChunkWorker {
            queue: queue.clone(),
            file: file.clone(),
            transfer: transfer.clone(),
            abort_reason: abort_reason.clone(),
            url: cfg.url.clone(),
            bytes_written: bytes_written.clone(),
            parts: parts_tracker.clone(),
            desired: desired.clone(),
            live_workers: live_workers.clone(),
            event_tx: self.event_tx.clone(),
            download_id,
            expected_total: cfg.total_size,
            phase: phase.clone(),
        };

        for _worker_id in 0..num_workers {
            handles.push(spawn_chunk_worker(make_worker()));
        }

        // Scale-up watcher: extra workers join the same queue and the same loop.
        // JoinHandles are retained so finalize cannot race a writer that is
        // still appending to the temp file.
        let extra_handles: Arc<std::sync::Mutex<Vec<tokio::task::JoinHandle<()>>>> =
            Arc::new(std::sync::Mutex::new(Vec::new()));
        let accept_scale = Arc::new(AtomicBool::new(true));
        let scale_handle = {
            let stop = stop.clone();
            let desired = desired.clone();
            let live_workers = live_workers.clone();
            let prototype = make_worker();
            let extra_handles = extra_handles.clone();
            let accept_scale = accept_scale.clone();
            tokio::spawn(async move {
                while accept_scale.load(Ordering::Relaxed) && !stop.load(Ordering::Relaxed) {
                    let want = desired
                        .load(Ordering::Relaxed)
                        .min(chunk::MAX_CONNECTIONS)
                        .max(1);
                    let have = live_workers.load(Ordering::Relaxed);
                    if want > have {
                        let handle = spawn_chunk_worker(prototype.clone());
                        if let Ok(mut guard) = extra_handles.lock() {
                            guard.push(handle);
                        }
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
                }
            })
        };

        // Adaptive Auto: watches measured throughput and moves `desired` one
        // step at a time. It is dormant while the download is Manual — manual
        // counts are never rewritten — and wakes up if the user switches a live
        // download back to Auto.
        let auto_flag = cfg
            .auto_flag
            .clone()
            .unwrap_or_else(|| Arc::new(AtomicBool::new(cfg.auto_connections)));
        let adaptive_ceiling = adaptive::ceiling_for(cfg.total_size);
        let controller_handle = {
            let stop = stop.clone();
            let desired = desired.clone();
            let auto_flag = auto_flag.clone();
            let perf = perf.clone();
            let bytes_written = bytes_written.clone();
            let live_workers = live_workers.clone();
            tokio::spawn(async move {
                let cfg = AdaptiveConfig::default();
                let mut ctrl = AdaptiveController::new(num_workers, adaptive_ceiling, cfg.clone());
                let mut window_bytes = bytes_written.load(Ordering::Relaxed);
                let mut window_at = Instant::now();
                let mut summary_at = Instant::now();
                loop {
                    if stop.load(Ordering::Relaxed) {
                        break;
                    }
                    tokio::time::sleep(Duration::from_millis(500)).await;
                    if stop.load(Ordering::Relaxed) {
                        break;
                    }
                    let now = Instant::now();
                    let bytes = bytes_written.load(Ordering::Relaxed);

                    if now.duration_since(summary_at) >= PerfStats::summary_interval() {
                        perf.log_summary(live_workers.load(Ordering::Relaxed), bytes);
                        summary_at = now;
                    }

                    let current = desired.load(Ordering::Relaxed).max(1);
                    if !auto_flag.load(Ordering::Relaxed) {
                        ctrl.sync(current, now);
                        window_bytes = bytes;
                        window_at = now;
                        continue;
                    }
                    let elapsed = now.duration_since(window_at);
                    let delta = bytes.saturating_sub(window_bytes);
                    if elapsed < cfg.min_window || delta < cfg.min_bytes {
                        continue;
                    }
                    let bps = delta as f64 / elapsed.as_secs_f64();
                    if let Some(next) = ctrl.observe(now, bps, perf.errors()) {
                        desired.store(next, Ordering::Relaxed);
                    }
                    window_bytes = bytes;
                    window_at = now;
                }
            })
        };

        for h in handles {
            let _ = h.await;
        }
        accept_scale.store(false, Ordering::Relaxed);
        let _ = scale_handle.await;
        let extras = extra_handles
            .lock()
            .map(|mut guard| std::mem::take(&mut *guard))
            .unwrap_or_default();
        for h in extras {
            let _ = h.await;
        }
        log::info!("[ProxyDM] concurrent id={} all workers done", cfg.id);

        stop.store(true, Ordering::Relaxed);
        let _ = controller_handle.await;
        let _ = forwarder_handle.await;
        reporter_stop.store(true, Ordering::Relaxed);
        let _ = reporter_handle.await;

        // Final progress snapshot so the Progress Map reaches 100% without waiting for poll.
        {
            let size = bytes_written.load(Ordering::Relaxed);
            let part_snap = parts_tracker.snapshot();
            let _ = self.event_tx.send(Event {
                kind: EventKind::DownloadProgress {
                    downloaded: size,
                    parts: part_snap,
                    reset_to_single: false,
                },
                download_id,
            });
        }

        let _ = file.sync_all();

        let save_snapshot = || {
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
            return Err(PdmError::Incomplete(format!(
                "{}/{} bytes",
                downloaded, cfg.total_size
            )));
        }

        // Release our handle before the rename — Windows refuses to rename a
        // file that still has an open handle with default share flags.
        drop(file);
        let _ = std::fs::File::open(crate::engine::file_io::temp_path(cfg.id))
            .and_then(|f| f.sync_all());
        finalize_file(cfg.id, &cfg.save_path).await?;

        let _ = self.event_tx.send(Event {
            kind: EventKind::DownloadCompleted,
            download_id: cfg.id,
        });

        Ok(())
    }
}

#[derive(Clone)]
struct ChunkWorker {
    queue: Arc<ChunkQueue>,
    file: Arc<std::fs::File>,
    /// Its stop flag is this engine's: a user pause or an internal abort.
    transfer: Arc<Transfer>,
    abort_reason: Arc<std::sync::Mutex<Option<PdmError>>>,
    url: String,
    bytes_written: Arc<AtomicU64>,
    parts: Arc<PartProgressTracker>,
    desired: Arc<AtomicU32>,
    live_workers: Arc<AtomicU32>,
    event_tx: mpsc::UnboundedSender<Event>,
    download_id: u64,
    expected_total: u64,
    phase: Arc<PhaseCounts>,
}

#[derive(Default)]
struct PhaseCounts {
    working: AtomicU32,
    retrying: AtomicU32,
    last: std::sync::Mutex<Option<Phase>>,
}

/// The download is Downloading while any worker is, and Retrying only when
/// every worker that is not idle waits out a backoff. Reported on change.
fn publish_phase(counts: &PhaseCounts, tx: &mpsc::UnboundedSender<Event>, id: u64) {
    let phase = if counts.working.load(Ordering::Relaxed) > 0 {
        Phase::Downloading
    } else if counts.retrying.load(Ordering::Relaxed) > 0 {
        Phase::Retrying
    } else {
        return;
    };
    let Ok(mut last) = counts.last.lock() else {
        return;
    };
    if *last == Some(phase) {
        return;
    }
    *last = Some(phase);
    let _ = tx.send(Event {
        kind: EventKind::PhaseChanged(phase),
        download_id: id,
    });
}

/// One worker loop for both the initial set and workers added at runtime.
/// A popped task is pushed back before the worker stops on any failure, so
/// the resume snapshot cannot lose it.
fn spawn_chunk_worker(env: ChunkWorker) -> tokio::task::JoinHandle<()> {
    env.live_workers.fetch_add(1, Ordering::Relaxed);
    tokio::spawn(async move {
        let stop = &env.transfer.stop;
        let perf = &env.transfer.perf;
        let mut budget = env.transfer.retry_budget();
        let mut budget_key: Option<(u64, u64)> = None;
        let stop_worker = |live: &AtomicU32| {
            live.fetch_sub(1, Ordering::Relaxed);
        };
        let abort = |task: Task, reason: PdmError| {
            env.queue.push(task);
            if let Ok(mut guard) = env.abort_reason.lock() {
                if guard.is_none() {
                    *guard = Some(reason);
                }
            }
            stop.store(true, Ordering::Relaxed);
        };
        let on_write = |offset: u64, len: u64| {
            if env.bytes_written.fetch_add(len, Ordering::Relaxed) == 0 {
                perf.note_progress();
            }
            env.parts.record_write(offset, len);
        };
        loop {
            if stop.load(Ordering::Relaxed) {
                stop_worker(&env.live_workers);
                return;
            }
            if env.live_workers.load(Ordering::Relaxed) > env.desired.load(Ordering::Relaxed).max(1)
            {
                stop_worker(&env.live_workers);
                return;
            }
            let task = match env.queue.pop_or_steal() {
                Some(t) => t,
                None => match env.queue.split_largest(2 * 1024 * 1024) {
                    Some(t) => t,
                    None => {
                        tokio::time::sleep(std::time::Duration::from_millis(80)).await;
                        if env.queue.is_empty() {
                            stop_worker(&env.live_workers);
                            return;
                        }
                        continue;
                    }
                },
            };
            log::debug!(
                "[ProxyDM] worker pop id={} offset={}",
                env.download_id,
                task.offset
            );
            let key = (task.offset, task.length);
            if budget_key != Some(key) {
                budget.reset();
                budget_key = Some(key);
            }

            env.phase.working.fetch_add(1, Ordering::Relaxed);
            publish_phase(&env.phase, &env.event_tx, env.download_id);
            let result = env
                .transfer
                .attempt(
                    &env.url,
                    &env.file,
                    &Want::Range(task.clone()),
                    env.expected_total,
                    &on_write,
                )
                .await;
            env.phase.working.fetch_sub(1, Ordering::Relaxed);

            let failure = match result {
                Attempt::Complete => {
                    budget.reset();
                    continue;
                }
                Attempt::Partial { remaining } => {
                    log::debug!(
                        "[ProxyDM] task offset={} partial, re-queueing {} bytes",
                        task.offset,
                        remaining.length
                    );
                    if !stop.load(Ordering::Relaxed) {
                        // A transfer that stopped mid-range (body idle or a
                        // dropped stream) is a stall, not a clean pause.
                        perf.note_stall();
                        budget.reset();
                    }
                    env.queue.push(remaining);
                    continue;
                }
                Attempt::RangeLost => {
                    abort(task, PdmError::RangeLost);
                    stop_worker(&env.live_workers);
                    return;
                }
                Attempt::Refused(code) => {
                    abort(task, PdmError::Http(code));
                    stop_worker(&env.live_workers);
                    return;
                }
                Attempt::Failed(error) => error,
            };

            let Some(delay) = budget.take() else {
                log::error!(
                    "[ProxyDM] retries exhausted for offset={}, stopping: {}",
                    task.offset,
                    failure
                );
                abort(task, PdmError::RetriesExhausted(Box::new(failure)));
                stop_worker(&env.live_workers);
                return;
            };
            log::warn!(
                "[ProxyDM] chunk offset={} failed, retrying in {:?}: {}",
                task.offset,
                delay,
                failure
            );
            perf.note_retry();
            env.queue.push(task);
            env.phase.retrying.fetch_add(1, Ordering::Relaxed);
            publish_phase(&env.phase, &env.event_tx, env.download_id);
            let go_on = env.transfer.wait(delay).await;
            env.phase.retrying.fetch_sub(1, Ordering::Relaxed);
            if !go_on {
                stop_worker(&env.live_workers);
                return;
            }
            publish_phase(&env.phase, &env.event_tx, env.download_id);
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_retrying_worker_does_not_flip_the_task_off_downloading() {
        let counts = PhaseCounts::default();
        let (tx, mut rx) = mpsc::unbounded_channel();
        counts.working.store(1, Ordering::Relaxed);
        counts.retrying.store(1, Ordering::Relaxed);
        publish_phase(&counts, &tx, 7);
        assert_eq!(
            rx.try_recv().unwrap().kind,
            EventKind::PhaseChanged(Phase::Downloading)
        );
        publish_phase(&counts, &tx, 7);
        assert!(rx.try_recv().is_err(), "unchanged phase was emitted again");

        counts.working.store(0, Ordering::Relaxed);
        publish_phase(&counts, &tx, 7);
        assert_eq!(
            rx.try_recv().unwrap().kind,
            EventKind::PhaseChanged(Phase::Retrying)
        );
    }
}
