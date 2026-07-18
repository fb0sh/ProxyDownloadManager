use crate::network::pool::NetworkPool;
use crate::network::limiter::MultiLimiter;
use crate::engine::chunk::{self, ChunkQueue};
use crate::types::{Task, Event, EventKind, EngineConfig, PdmError, PdmResult};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::mpsc;

/// Outcome of a single chunk download attempt.
#[derive(Debug, PartialEq)]
enum TaskResult {
    /// Chunk fully downloaded.
    Complete,
    /// Partial progress: remaining bytes should be re-queued.
    Partial { remaining: Task },
    /// User cancelled.
    Cancelled,
    /// Unrecoverable error — don't retry this chunk.
    Fatal(String),
}

pub struct ConcurrentDownloader {
    pool: Arc<NetworkPool>,
    event_tx: mpsc::UnboundedSender<Event>,
}

impl ConcurrentDownloader {
    pub fn new(pool: Arc<NetworkPool>, event_tx: mpsc::UnboundedSender<Event>) -> Self {
        Self { pool, event_tx }
    }

    pub async fn download(&self, cfg: &EngineConfig, limiter: Arc<MultiLimiter>, cancel: Arc<AtomicBool>, on_resume: &crate::engine::OnResumeState) -> PdmResult<()> {
        // On resume, use tasks passed via config instead of reading gob directly
        let (tasks, resume_offset): (Vec<Task>, u64) = if cfg.is_resume && !cfg.resume_tasks.is_empty() {
            eprintln!("[ProxyDM] concurrent id={} resume with {} engine tasks", cfg.id, cfg.resume_tasks.len());
            let off = cfg.resume_tasks.iter().map(|t| t.offset + t.length).max().unwrap_or(0);
            (cfg.resume_tasks.clone(), off)
        } else if cfg.is_resume {
            // No saved tasks — compute from scratch using total_size
            let remaining = cfg.total_size;
            let num_conns = 4.max(cfg.connections);
            eprintln!("[ProxyDM] concurrent id={} resume recompute from scratch", cfg.id);
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
        eprintln!("[ProxyDM] concurrent id={} workers={} chunks={} total_size={} is_resume={}",
            cfg.id, num_workers, tasks.len(), cfg.total_size, cfg.is_resume);

        let queue = Arc::new(ChunkQueue::new(tasks));

        // Pre-allocate file and use std FileExt::write_at for lock-free concurrent writes
        let file = create_output_file(&cfg.save_path, cfg.total_size).await?;
        let file = Arc::new(file);
        eprintln!("[ProxyDM] concurrent id={} file created: {}.pdm", cfg.id, cfg.save_path);

        let client = self.pool.get_client(if cfg.proxy_url.is_empty() { None } else { Some(&cfg.proxy_url) })?;

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
                        TaskResult::Complete => {
                            retries_left = max_retries;
                        }
                        TaskResult::Partial { remaining } => {
                            eprintln!("[ProxyDM] task offset={} partial, re-queueing {} bytes", task.offset, remaining.length);
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
                                eprintln!("[ProxyDM] worker retries exhausted for offset={}, stopping", task.offset);
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

        // If user canceled (pause/delete), save remaining tasks for resume via callback
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
                    proxy_name: cfg.proxy_url.clone(),
                    workers: num_workers,
                };
                on_resume(cfg.id, &saved);
            }
            return Err(PdmError::Cancelled);
        }

        // Verify completeness: queue must be empty AND total bytes written match
        if !queue.is_empty() || bytes_written.load(Ordering::Relaxed) < cfg.total_size {
            let downloaded = bytes_written.load(Ordering::Relaxed);
            return Err(PdmError::Other(format!("Download incomplete: {}/{} bytes", downloaded, cfg.total_size)));
        }

        // Rename .pdm to final filename
        finalize_file(&cfg.save_path, &cfg.save_path).await?;

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
) -> TaskResult {
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
    let resp = match req.send().await {
        Ok(r) => r,
        Err(e) => {
            let mut msg = format!("Request failed: {}", e);
            let mut src = std::error::Error::source(&e);
            while let Some(s) = src {
                msg.push_str(&format!(": {}", s));
                src = s.source();
            }
            eprintln!("[ProxyDM] concurrent_task REQUEST ERROR offset={}: {}", task.offset, msg);
            return TaskResult::Fatal(msg);
        }
    };

    if cancel.load(Ordering::Relaxed) {
        return TaskResult::Cancelled;
    }

    let status = resp.status();
    eprintln!("[ProxyDM] concurrent_task offset={} HTTP {} (expected 206 or 200)", task.offset, status);

    // For offset > 0: 200 means server ignored Range — fatal
    if status == reqwest::StatusCode::OK && task.offset > 0 {
        return TaskResult::Fatal(format!("Server ignored Range header (HTTP 200), offset={}", task.offset));
    }
    if status != reqwest::StatusCode::OK && status != reqwest::StatusCode::PARTIAL_CONTENT {
        return TaskResult::Fatal(format!("HTTP {}", status));
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
            return TaskResult::Cancelled;
        }

        // Abort slow chunks so other workers can steal remaining work
        let elapsed = start_time.elapsed();
        if elapsed > std::time::Duration::from_secs(30)
            && chunk_size > 0
            && written < chunk_size / 10
        {
            eprintln!("[ProxyDM] slow chunk offset={} written={}/{} after {}s, re-queuing",
                base_offset, written, chunk_size, elapsed.as_secs());
            let remaining = chunk_size.saturating_sub(written);
            return TaskResult::Partial {
                remaining: Task { offset: base_offset + written, length: remaining },
            };
        }

        let chunk_result = tokio::time::timeout(
            std::time::Duration::from_secs(10), stream.next()
        ).await;
        let chunk = match chunk_result {
            Ok(Some(Ok(c))) => c,
            Ok(Some(Err(e))) => {
                let remaining = chunk_size.saturating_sub(written);
                if remaining > 0 && written > 0 {
                    return TaskResult::Partial {
                        remaining: Task { offset: base_offset + written, length: remaining },
                    };
                }
                return TaskResult::Fatal(format!("Stream error: {}", e));
            }
            Ok(None) => {
                if !buf.is_empty() {
                    if let Err(e) = write_at(file, &buf, base_offset + written) {
                        return TaskResult::Fatal(format!("write_at error: {}", e));
                    }
                    written += buf.len() as u64;
                    bytes_written.fetch_add(buf.len() as u64, Ordering::Relaxed);
                }
                break;
            }
            Err(_elapsed) => {
                if cancel.load(Ordering::Relaxed) {
                    return TaskResult::Cancelled;
                }
                continue;
            }
        };
        limiter.wait_n(chunk.len() as u64).await;

        buf.extend_from_slice(&chunk);

        if buf.len() >= BUF_SIZE {
            if let Err(e) = write_at(file, &buf, base_offset + written) {
                return TaskResult::Fatal(format!("write_at error: {}", e));
            }
            written += buf.len() as u64;
            bytes_written.fetch_add(buf.len() as u64, Ordering::Relaxed);
            buf.clear();
        }
    }

    TaskResult::Complete
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn task_result_complete_is_not_partial() {
        assert_eq!(TaskResult::Complete, TaskResult::Complete);
        assert_ne!(TaskResult::Complete, TaskResult::Cancelled);
    }

    #[test]
    fn task_result_partial_has_remaining() {
        let remaining = Task { offset: 3000, length: 2000 };
        let r = TaskResult::Partial { remaining: remaining.clone() };
        if let TaskResult::Partial { remaining } = r {
            assert_eq!(remaining.offset, 3000);
            assert_eq!(remaining.length, 2000);
        } else {
            panic!("expected Partial");
        }
    }

    #[test]
    fn task_result_fatal_contains_message() {
        let r = TaskResult::Fatal("HTTP 403".to_string());
        if let TaskResult::Fatal(msg) = r {
            assert_eq!(msg, "HTTP 403");
        } else {
            panic!("expected Fatal");
        }
    }
}
