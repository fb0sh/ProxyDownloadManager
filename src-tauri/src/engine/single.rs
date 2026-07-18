use crate::network::pool::NetworkPool;
use crate::network::limiter::MultiLimiter;
use crate::types::{PdmError, PdmResult, Event, EventKind, EngineConfig};
use std::sync::atomic::{AtomicBool, Ordering};
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

    pub async fn download(&self, cfg: &EngineConfig, limiter: Arc<MultiLimiter>, cancel: Arc<AtomicBool>, on_resume: &crate::engine::OnResumeState) -> PdmResult<()> {
        log::info!("[ProxyDM] single id={} url={}", cfg.id, cfg.url);
        let mut req = self.pool
            .get_client(if cfg.proxy_url.is_empty() { None } else { Some(&cfg.proxy_url) })
            .map_err(|e| PdmError::ClientBuild(e.to_string()))?
            .get(&cfg.url);
        if !cfg.user_agent.is_empty() {
            req = req.header("User-Agent", &cfg.user_agent);
        }
        let resp = req
            .send()
            .await
            .map_err(|e| PdmError::Network(e.to_string()))?;
        log::info!("[ProxyDM] single id={} HTTP {} size={}", cfg.id, resp.status(),
            resp.headers().get("content-length").and_then(|v| v.to_str().ok()).unwrap_or("?"));

        if cancel.load(Ordering::Relaxed) {
            return Err(PdmError::Cancelled);
        }

        let status = resp.status();
        if !status.is_success() {
            // Handle 429/503 with Retry-After
            if status == reqwest::StatusCode::TOO_MANY_REQUESTS || status == reqwest::StatusCode::SERVICE_UNAVAILABLE {
                let retry_after = resp.headers()
                    .get("retry-after")
                    .and_then(|v| v.to_str().ok())
                    .and_then(|s| s.parse::<u64>().ok())
                    .unwrap_or(5);
                return Err(PdmError::Other(format!("Rate limited, retry after {}s", retry_after)));
            }
            return Err(PdmError::Http(status.as_u16()));
        }

        // Ensure output directory exists
        if let Some(parent) = std::path::Path::new(&cfg.save_path).parent() {
            tokio::fs::create_dir_all(parent).await.map_err(|e| PdmError::Io(e.to_string()))?;
        }

        // Use std::fs::File (no tokio overhead for sequential write)
        // Write to .pdm temp file for crash safety, rename on completion
        let pdm_path = format!("{}.pdm", cfg.save_path);
        use std::io::Write;
        let mut file = std::fs::File::create(&pdm_path)
            .map_err(|e| PdmError::Io(e.to_string()))?;

        let stream = resp.bytes_stream();
        use futures_util::StreamExt;
        let mut stream = std::pin::pin!(stream);
        let mut total = 0u64;
        const BUF_SIZE: usize = 1024 * 1024; // 1MB buffer
        let mut buf = Vec::with_capacity(BUF_SIZE);

        // Helper: save current progress for resume via callback
        let save_progress = |written: u64, total_size: u64, id: u64, cfg: &EngineConfig| {
            let remaining = total_size.saturating_sub(written);
            if remaining > 0 {
                let saved = crate::types::DownloadState {
                    url: cfg.url.clone(),
                    id,
                    file_name: cfg.file_name.clone(),
                    save_path: cfg.save_path.clone(),
                    total_size,
                    downloaded: written,
                    tasks: vec![crate::types::Task { offset: written, length: remaining }],
                    proxy_name: cfg.proxy_url.clone(),
                    workers: 1,
                };
                on_resume(id, &saved);
            }
        };

        loop {
            // Check cancel between chunks for responsive pause
            if cancel.load(Ordering::Relaxed) {
                if !buf.is_empty() {
                    let _ = file.write_all(&buf);
                    total += buf.len() as u64;
                    buf.clear();
                }
                save_progress(total, cfg.total_size, cfg.id, cfg);
                return Err(PdmError::Cancelled);
            }

            let chunk_result = tokio::time::timeout(
                std::time::Duration::from_secs(10), stream.next()
            ).await;
            let chunk = match chunk_result {
                Ok(Some(c)) => c,
                Ok(None) => break,
                Err(_elapsed) => {
                    if cancel.load(Ordering::Relaxed) {
                        if !buf.is_empty() {
                            let _ = file.write_all(&buf);
                            total += buf.len() as u64;
                            buf.clear();
                        }
                        save_progress(total, cfg.total_size, cfg.id, cfg);
                        return Err(PdmError::Cancelled);
                    }
                    continue;
                }
            };
            let chunk = chunk.map_err(|e| PdmError::Network(e.to_string()))?;
            limiter.wait_n(chunk.len() as u64).await;
            buf.extend_from_slice(&chunk);

            if buf.len() >= BUF_SIZE {
                file.write_all(&buf).map_err(|e| PdmError::Io(e.to_string()))?;
                total += buf.len() as u64;
                buf.clear();

                let _ = self.event_tx.send(Event {
                    kind: EventKind::DownloadProgress,
                    download_id: cfg.id,
                    data: Some(total.to_string()),
                });
            }
        }

        // Flush remainder
        if !buf.is_empty() {
            file.write_all(&buf).map_err(|e| PdmError::Io(e.to_string()))?;
            total += buf.len() as u64;
        }
        file.flush().map_err(|e| PdmError::Io(e.to_string()))?;
        drop(file);

        // Rename .pdm to final filename (matches concurrent engine convention)
        tokio::fs::rename(&pdm_path, &cfg.save_path).await
            .map_err(|e| PdmError::Io(e.to_string()))?;

        log::info!("[ProxyDM] single id={} done total={} bytes", cfg.id, total);

        // Final progress update so UI reaches 100%
        let _ = self.event_tx.send(Event {
            kind: EventKind::DownloadProgress,
            download_id: cfg.id,
            data: Some(total.to_string()),
        });

        let _ = self.event_tx.send(Event {
            kind: EventKind::DownloadCompleted,
            download_id: cfg.id,
            data: None,
        });

        Ok(())
    }
}
