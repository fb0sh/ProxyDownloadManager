use crate::network::pool::NetworkPool;
use crate::network::limiter::MultiLimiter;
use crate::engine::chunk::{self, ChunkQueue};
use crate::types::{Task, Event, EventKind, DownloadConfig};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio::io::AsyncWriteExt;
use tokio::fs;

pub struct ConcurrentDownloader {
    pool: Arc<NetworkPool>,
    event_tx: mpsc::UnboundedSender<Event>,
}

impl ConcurrentDownloader {
    pub fn new(pool: Arc<NetworkPool>, event_tx: mpsc::UnboundedSender<Event>) -> Self {
        Self { pool, event_tx }
    }

    pub async fn download(&self, cfg: &DownloadConfig, limiter: Arc<MultiLimiter>) -> Result<(), String> {
        let cancel = Arc::new(AtomicBool::new(false));
        let completed_counter = Arc::new(AtomicU64::new(0));

        let num_conns = if cfg.connections > 0 {
            cfg.connections.min(32)
        } else {
            let sqrt = (cfg.total_size as f64 / 1024.0 / 1024.0).sqrt() as u32;
            sqrt.max(1).min(32)
        };

        let min_chunk = 2u64 * 1024 * 1024; // 2MB
        let tasks = if cfg.is_resume {
            // For resume, tasks come from saved state
            vec![]
        } else {
            chunk::compute_chunks(cfg.total_size, num_conns, min_chunk)
        };

        // Resume path: state loaded from Sub-Plan B (state/gob.rs) merge
        if tasks.is_empty() {
            return Err("Resume not yet implemented in concurrent downloader".to_string());
        }

        let queue = Arc::new(ChunkQueue::new(tasks));
        let file = Arc::new(tokio::sync::Mutex::new(
            create_output_file(&cfg.output_path).await?
        ));

        let client = self.pool.get_client(if cfg.proxy_name.is_empty() { None } else { Some(&cfg.proxy_name) });

        let mut handles = Vec::new();
        let download_id = cfg.id;

        // Spawn periodic progress reporter (not in handles vec — don't block completion)
        let progress_cancel = cancel.clone();
        let progress_tx = self.event_tx.clone();
        let progress_path = cfg.output_path.clone();
        let reporter_handle = tokio::spawn(async move {
            loop {
                if progress_cancel.load(Ordering::Relaxed) { break; }
                let size = get_file_size(&progress_path).await;
                let _ = progress_tx.send(Event {
                    kind: EventKind::DownloadProgress,
                    download_id,
                    data: Some(format!("{}", size)),
                });
                tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            }
        });

        let download_id = cfg.id;
        for _worker_id in 0..num_conns {
            let queue = queue.clone();
            let file = file.clone();
            let client = client.clone();
            let cancel = cancel.clone();
            let limiter = limiter.clone();
            let completed_counter = completed_counter.clone();
            let event_tx = self.event_tx.clone();
            let url = cfg.url.clone();
            let max_retries = cfg.max_retries;
            let out_path = cfg.output_path.clone();
            let cancel_for_task = cancel.clone();

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
                        &url, &client, &file, &task, &cancel_for_task, &limiter, retries_left,
                    ).await;

                    match result {
                        Ok(_) => {
                            completed_counter.fetch_add(1, Ordering::SeqCst);
                        }
                        Err(e) => {
                            if retries_left > 0 {
                                retries_left -= 1;
                                queue.push(task);
                            } else {
                                let _ = event_tx.send(Event {
                                    kind: EventKind::DownloadErrored,
                                    download_id,
                                    data: Some(format!("{} (retries exhausted)", e)),
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

        // Stop the reporter
        cancel.store(true, Ordering::Relaxed);
        let _ = reporter_handle.await;

        // Sync file to ensure all writes are visible to metadata
        {
            let mut f = file.lock().await;
            let _ = f.flush().await;
        }

        // Verify completeness
        let downloaded = get_file_size(&cfg.output_path).await;
        if downloaded < cfg.total_size && !cancel.load(Ordering::Relaxed) {
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

async fn create_output_file(path: &str) -> Result<tokio::fs::File, String> {
    let pdm_path = format!("{}.pdm", path);
    if let Some(parent) = std::path::Path::new(&pdm_path).parent() {
        tokio::fs::create_dir_all(parent).await.map_err(|e| e.to_string())?;
    }
    tokio::fs::File::create(&pdm_path)
        .await
        .map_err(|e| format!("Failed to create output file: {}", e))
}

async fn get_file_size(path: &str) -> u64 {
    let pdm_path = format!("{}.pdm", path);
    tokio::fs::metadata(&pdm_path)
        .await
        .map(|m| m.len())
        .unwrap_or(0)
}

async fn finalize_file(output_path: &str, save_path: &str) -> Result<(), String> {
    let pdm_path = format!("{}.pdm", output_path);
    tokio::fs::rename(&pdm_path, save_path)
        .await
        .map_err(|e| format!("Failed to rename file: {}", e))
}

async fn download_task(
    url: &str,
    client: &reqwest::Client,
    file: &Arc<tokio::sync::Mutex<tokio::fs::File>>,
    task: &Task,
    cancel: &AtomicBool,
    limiter: &MultiLimiter,
    _max_retries: u32, // used by worker loop, not here
) -> Result<(), String> {
    let range_end = if task.length == 0 {
        String::new()
    } else {
        format!("{}", task.offset + task.length - 1)
    };
    let range_header = format!("bytes={}-{}", task.offset, range_end);
    let resp = client
        .get(url)
        .header("Range", &range_header)
        .send()
        .await
        .map_err(|e| format!("Request failed: {}", e))?;

    let status = resp.status();
    if status != reqwest::StatusCode::PARTIAL_CONTENT && status != reqwest::StatusCode::OK {
        return Err(format!("HTTP {}", status));
    }

    let stream = resp.bytes_stream();
    use futures_util::StreamExt;
    use tokio::io::AsyncSeekExt;
    let mut stream = std::pin::pin!(stream);
    let mut written = task.offset;

    while let Some(chunk_result) = stream.next().await {
        if cancel.load(Ordering::Relaxed) {
            return Err("Cancelled".to_string());
        }
        let chunk = chunk_result.map_err(|e| format!("Stream error: {}", e))?;
        limiter.wait_n(chunk.len() as u64);

        // Write each chunk immediately so file size grows incrementally
        let mut f = file.lock().await;
        f.seek(std::io::SeekFrom::Start(written)).await
            .map_err(|e| format!("Seek error: {}", e))?;
        f.write_all(&chunk).await
            .map_err(|e| format!("Write error: {}", e))?;
        // Drop lock so other workers can write
        drop(f);
        written += chunk.len() as u64;
    }

    Ok(())
}
