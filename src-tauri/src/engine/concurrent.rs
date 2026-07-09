use crate::network::pool::NetworkPool;
use crate::network::limiter::MultiLimiter;
use crate::engine::chunk::{self, ChunkQueue};
use crate::types::{Task, Event, EventKind, DownloadConfig};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::error::Error;
use tokio::sync::mpsc;

/// Parse a task error: "TaskError:offset:written:message"
/// Returns (remaining_task, message) if valid.
fn parse_task_error(err: &str, orig: &Task) -> Option<(Task, String)> {
    let parts: Vec<&str> = err.splitn(4, ':').collect();
    if parts.len() < 4 || parts[0] != "TaskError" {
        return None;
    }
    let orig_offset: u64 = parts[1].parse().ok()?;
    let written: u64 = parts[2].parse().ok()?;
    let msg = parts[3].to_string();
    let written = written.min(orig.length);
    let remaining = orig.length.saturating_sub(written);
    if remaining > 0 && written > 0 {
        Some((Task { offset: orig_offset + written, length: remaining }, msg))
    } else {
        // Nothing remaining to re-queue, task is fully consumed
        None
    }
}

pub struct ConcurrentDownloader {
    pool: Arc<NetworkPool>,
    event_tx: mpsc::UnboundedSender<Event>,
}

impl ConcurrentDownloader {
    pub fn new(pool: Arc<NetworkPool>, event_tx: mpsc::UnboundedSender<Event>) -> Self {
        Self { pool, event_tx }
    }

    pub async fn download(&self, cfg: &DownloadConfig, limiter: Arc<MultiLimiter>, cancel: Arc<AtomicBool>) -> Result<(), String> {
        // On resume, load saved state — prefer real engine-saved tasks
        let (tasks, resume_offset): (Vec<Task>, u64) = if cfg.is_resume {
            if let Some(saved) = crate::state::gob::load_state(cfg.id).ok().flatten() {
                if !saved.tasks.is_empty()
                    && !(saved.tasks.len() == 1 && saved.tasks[0].offset == saved.downloaded)
                {
                    // Engine-saved tasks: use them directly
                    eprintln!("[ProxyDM] concurrent id={} resume with {} engine tasks", cfg.id, saved.tasks.len());
                    let off = saved.downloaded;
                    (saved.tasks, off)
                } else {
                    // Legacy/fake single task — recompute from downloaded offset
                    let off = saved.downloaded;
                    let remaining = cfg.total_size.saturating_sub(off);
                    if remaining == 0 {
                        return Err(format!("Download {} already complete", cfg.id));
                    }
                    let num_conns = 4.max(cfg.connections);
                    eprintln!("[ProxyDM] concurrent id={} resume recompute from offset={} ({} tasks from engine)",
                        cfg.id, off, saved.tasks.len());
                    (chunk::compute_chunks(remaining, num_conns, 0)
                        .into_iter()
                        .map(|mut t| { t.offset += off; t })
                        .collect::<Vec<_>>(), off)
                }
            } else {
                return Err(format!("Resume requested but no saved state for id={}", cfg.id));
            }
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
            return Err(format!("No tasks to download for id={}", cfg.id));
        }

        let num_workers = num_conns.min(tasks.len() as u32).max(1);
        eprintln!("[ProxyDM] concurrent id={} workers={} chunks={} total_size={} is_resume={}",
            cfg.id, num_workers, tasks.len(), cfg.total_size, cfg.is_resume);

        let queue = Arc::new(ChunkQueue::new(tasks));

        // Pre-allocate file and use std FileExt::write_at for lock-free concurrent writes
        let file = create_output_file(&cfg.output_path, cfg.total_size).await?;
        let file = Arc::new(file);
        eprintln!("[ProxyDM] concurrent id={} file created: {}.pdm", cfg.id, cfg.output_path);

        let client = self.pool.get_client(if cfg.proxy_name.is_empty() { None } else { Some(&cfg.proxy_name) });

        let mut handles = Vec::new();
        let download_id = cfg.id;

        // Spawn periodic progress reporter (not in handles vec — don't block completion)
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

        // Spawn workers — only as many as we have chunks
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
                    let task = queue.pop();
                    let task = match task {
                        Some(t) => t,
                        None => break,
                    };

                    let result = download_task(
                        &url, &client, &*file, &task, &cancel_for_task, &limiter, &user_agent, &bytes_written,
                    ).await;

                    match result {
                        Ok(_) => {}
                        Err(e) => {
                            // Try to parse as TaskError:offset:written:msg — re-queue only remaining portion
                            if let Some((rest, _msg)) = parse_task_error(&e, &task) {
                                eprintln!("[ProxyDM] task offset={} failed, re-queueing remaining {} bytes", task.offset, rest.length);
                                queue.push(rest);
                                continue;
                            }
                            // Plain error (e.g. Cancelled) — re-queue full task
                            if e == "Cancelled" {
                                return;
                            }
                            // Exponential backoff for network errors
                            let attempt = max_retries.saturating_sub(retries_left) + 1;
                            let backoff_secs = 2u64.pow(attempt.min(5) as u32).min(30);
                            tokio::time::sleep(std::time::Duration::from_secs(backoff_secs)).await;
                            if retries_left > 0 {
                                retries_left -= 1;
                            } else {
                                retries_left = max_retries;
                            }
                            queue.push(task);
                        }
                    }
                }
            });
            handles.push(handle);
        }

        // Wait for all workers to finish
        for h in handles {
            let _ = h.await;
        }
        eprintln!("[ProxyDM] concurrent id={} all workers done", cfg.id);

        // Stop the reporter
        reporter_stop.store(true, Ordering::Relaxed);
        let _ = reporter_handle.await;

        // Sync file to ensure all writes are visible
        let _ = file.sync_all();

        // If user canceled (pause/delete), save remaining tasks for resume
        if cancel.load(Ordering::Relaxed) {
            let remaining_tasks = queue.drain();
            if !remaining_tasks.is_empty() {
                let saved = crate::types::DownloadState {
                    url: cfg.url.clone(),
                    id: cfg.id,
                    file_name: cfg.file_name.clone(),
                    save_path: cfg.save_path.clone(),
                    total_size: cfg.total_size,
                    downloaded: bytes_written.load(Ordering::Relaxed),
                    tasks: remaining_tasks,
                    elapsed_secs: 0,
                    chunk_bitmap: Vec::new(),
                    actual_chunk_size: 0,
                    proxy_name: cfg.proxy_name.clone(),
                    workers: num_workers,
                    min_chunk_size: 0,
                };
                let _ = crate::state::gob::save_state(cfg.id, &saved);
            }
            return Err("Cancelled".to_string());
        }

        // Verify completeness: queue must be empty AND total bytes written match
        if !queue.is_empty() || bytes_written.load(Ordering::Relaxed) < cfg.total_size {
            let downloaded = bytes_written.load(Ordering::Relaxed);
            return Err(format!("Download incomplete: {}/{} bytes", downloaded, cfg.total_size));
        }

        // Rename .pdm to final filename
        finalize_file(&cfg.output_path, &cfg.save_path).await?;

        let _ = self.event_tx.send(Event {
            kind: EventKind::DownloadCompleted,
            download_id: cfg.id,
            data: None,
        });

        Ok(())
    }
}

async fn create_output_file(path: &str, total_size: u64) -> Result<std::fs::File, String> {
    let pdm_path = format!("{}.pdm", path);
    if let Some(parent) = std::path::Path::new(&pdm_path).parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let file = std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .read(true)
        .open(&pdm_path)
        .map_err(|e| format!("Failed to create output file: {}", e))?;
    // Pre-allocate space for the entire file
    if total_size > 0 {
        let _ = file.set_len(total_size);
    }
    Ok(file)
}

async fn finalize_file(output_path: &str, save_path: &str) -> Result<(), String> {
    let pdm_path = format!("{}.pdm", output_path);
    tokio::fs::rename(&pdm_path, save_path)
        .await
        .map_err(|e| format!("Failed to rename file: {}", e))
}

/// Cross-platform write_at: write to a specific offset without seeking.
#[cfg(unix)]
fn write_at(file: &std::fs::File, buf: &[u8], offset: u64) -> std::io::Result<()> {
    use std::os::unix::fs::FileExt;
    FileExt::write_all_at(file, buf, offset)
}

#[cfg(windows)]
fn write_at(file: &std::fs::File, buf: &[u8], offset: u64) -> std::io::Result<()> {
    use std::os::windows::fs::FileExt;
    let mut written = 0;
    while written < buf.len() {
        let n = FileExt::seek_write(file, &buf[written..], offset + written as u64)?;
        written += n;
    }
    Ok(())
}

async fn download_task(
    url: &str,
    client: &reqwest::Client,
    file: &std::fs::File,
    task: &Task,
    cancel: &AtomicBool,
    limiter: &MultiLimiter,
    user_agent: &str,
    bytes_written: &AtomicU64,
) -> Result<(), String> {
    // Pre-declare written counter so error path can report it
    let mut written: u64 = 0;
    let range_end = if task.length == 0 {
        String::new()
    } else {
        format!("{}", task.offset + task.length - 1)
    };
    let range_header = format!("bytes={}-{}", task.offset, range_end);
    let mut req = client
        .get(url)
        .header("Range", &range_header);
    if !user_agent.is_empty() {
        req = req.header("User-Agent", user_agent);
    }
    eprintln!("[ProxyDM] concurrent_task offset={} range_end={}", task.offset, range_end);
    let resp = req.send().await.map_err(|e| {
        let mut msg = format!("Request failed: {}", e);
        let mut src = std::error::Error::source(&e);
        while let Some(s) = src {
            msg.push_str(&format!(": {}", s));
            src = s.source();
        }
        eprintln!("[ProxyDM] concurrent_task REQUEST ERROR offset={}: {}", task.offset, msg);
        msg
    })?;

    if cancel.load(Ordering::Relaxed) {
        return Err(format!("TaskError:{}:{}:Cancelled", task.offset, written));
    }

    let status = resp.status();
    eprintln!("[ProxyDM] concurrent_task offset={} HTTP {} (expected 206 or 200)", task.offset, status);

    // For offset > 0: 200 means server ignored Range — fatal, don't retry same spot
    if status == reqwest::StatusCode::OK {
        if task.offset > 0 {
            return Err(format!("Server ignored Range header (HTTP 200), offset={}", task.offset));
        }
    } else if status != reqwest::StatusCode::PARTIAL_CONTENT {
        return Err(format!("HTTP {}", status));
    }

    let stream = resp.bytes_stream();
    use futures_util::StreamExt;
    let mut stream = std::pin::pin!(stream);
    let base_offset = task.offset;
    let chunk_size = task.length;

    const BUF_SIZE: usize = 1024 * 1024; // 1MB
    let mut buf = Vec::with_capacity(BUF_SIZE);

    // Slow chunk detection: if >30s elapsed and <10% done, abort
    let start_time = std::time::Instant::now();

    loop {
        // Check cancel (responsive Stop even during streaming)
        if cancel.load(Ordering::Relaxed) {
            return Err(format!("TaskError:{}:{}:Cancelled", base_offset, written));
        }

        // Abort slow chunks so other workers can steal remaining work
        let elapsed = start_time.elapsed();
        if elapsed > std::time::Duration::from_secs(30)
            && chunk_size > 0
            && written < chunk_size / 10
        {
            eprintln!("[ProxyDM] slow chunk offset={} written={}/{} after {}s, re-queuing",
                base_offset, written, chunk_size, elapsed.as_secs());
            return Err(format!("TaskError:{}:{}:SlowChunk", base_offset, written));
        }

        let chunk_result = tokio::time::timeout(
            std::time::Duration::from_secs(10), stream.next()
        ).await;
        let chunk = match chunk_result {
            Ok(Some(Ok(c))) => c,
            Ok(Some(Err(e))) => return Err(format!("TaskError:{}:{}:Stream error: {}", base_offset, written, e)),
            Ok(None) => {
                if !buf.is_empty() {
                    write_at(file, &buf, base_offset + written)
                        .map_err(|e| format!("write_at error: {}", e))?;
                    written += buf.len() as u64;
                    bytes_written.fetch_add(buf.len() as u64, Ordering::Relaxed);
                }
                break;
            }
            Err(_elapsed) => {
                if cancel.load(Ordering::Relaxed) {
                    return Err(format!("TaskError:{}:{}:Cancelled", base_offset, written));
                }
                continue;
            }
        };
        limiter.wait_n(chunk.len() as u64);

        buf.extend_from_slice(&chunk);

        if buf.len() >= BUF_SIZE {
            write_at(file, &buf, base_offset + written)
                .map_err(|e| format!("write_at error: {}", e))?;
            written += buf.len() as u64;
            bytes_written.fetch_add(buf.len() as u64, Ordering::Relaxed);
            buf.clear();
        }
    }

    Ok(())
}
