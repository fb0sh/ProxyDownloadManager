use crate::network::pool::NetworkPool;
use crate::network::limiter::MultiLimiter;
use crate::types::{Event, EventKind, DownloadConfig};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio::io::AsyncWriteExt;

pub struct SingleDownloader {
    pool: Arc<NetworkPool>,
    event_tx: mpsc::UnboundedSender<Event>,
}

impl SingleDownloader {
    pub fn new(pool: Arc<NetworkPool>, event_tx: mpsc::UnboundedSender<Event>) -> Self {
        Self { pool, event_tx }
    }

    pub async fn download(&self, cfg: &DownloadConfig, limiter: Arc<MultiLimiter>, cancel: Arc<AtomicBool>) -> Result<(), String> {
        let resp = self.pool
            .get_client(if cfg.proxy_name.is_empty() { None } else { Some(&cfg.proxy_name) })
            .get(&cfg.url)
            .send()
            .await
            .map_err(|e| format!("Download request failed: {}", e))?;

        let status = resp.status();
        if !status.is_success() {
            // Handle 429/503 with Retry-After
            if status == reqwest::StatusCode::TOO_MANY_REQUESTS || status == reqwest::StatusCode::SERVICE_UNAVAILABLE {
                let retry_after = resp.headers()
                    .get("retry-after")
                    .and_then(|v| v.to_str().ok())
                    .and_then(|s| s.parse::<u64>().ok())
                    .unwrap_or(5);
                return Err(format!("Rate limited, retry after {}s", retry_after));
            }
            return Err(format!("HTTP {}", status));
        }

        // Ensure output directory exists
        if let Some(parent) = std::path::Path::new(&cfg.save_path).parent() {
            tokio::fs::create_dir_all(parent).await.map_err(|e| e.to_string())?;
        }

        let mut file = tokio::fs::File::create(&cfg.save_path)
            .await
            .map_err(|e| format!("Failed to create file: {}", e))?;

        let stream = resp.bytes_stream();
        use futures_util::StreamExt;
        let mut stream = std::pin::pin!(stream);
        let mut total = 0u64;
        let buf_size = 32 * 1024; // 32KB buffer

        while let Some(chunk_result) = stream.next().await {
            if cancel.load(Ordering::Relaxed) {
                file.flush().await.ok();
                return Err("Cancelled".to_string());
            }
            let chunk = chunk_result.map_err(|e| format!("Stream error: {}", e))?;
            limiter.wait_n(chunk.len() as u64);
            file.write_all(&chunk).await.map_err(|e| format!("Write error: {}", e))?;
            total += chunk.len() as u64;

            // Periodic progress
            if total % (buf_size * 32) == 0 {
                let _ = self.event_tx.send(Event {
                    kind: EventKind::DownloadProgress,
                    download_id: cfg.id,
                    data: Some(total.to_string()),
                });
            }
        }

        file.flush().await.map_err(|e| format!("Flush error: {}", e))?;

        let _ = self.event_tx.send(Event {
            kind: EventKind::DownloadCompleted,
            download_id: cfg.id,
            data: None,
        });

        Ok(())
    }
}
