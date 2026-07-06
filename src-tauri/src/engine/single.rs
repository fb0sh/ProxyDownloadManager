use crate::network::pool::NetworkPool;
use crate::network::limiter::MultiLimiter;
use crate::types::{Event, EventKind, DownloadConfig};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::error::Error;
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
        let mut req = self.pool
            .get_client(if cfg.proxy_name.is_empty() { None } else { Some(&cfg.proxy_name) })
            .get(&cfg.url);
        if !cfg.user_agent.is_empty() {
            req = req.header("User-Agent", &cfg.user_agent);
        }
        let resp = req
            .send()
            .await
            .map_err(|e| {
                let mut msg = format!("Download request failed: {}", e);
                let mut src = std::error::Error::source(&e);
                while let Some(s) = src {
                    msg.push_str(&format!(": {}", s));
                    src = s.source();
                }
                msg
            })?;

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

        loop {
            // Timeout on stream reads so cancel can be detected promptly
            let chunk_result = tokio::time::timeout(
                std::time::Duration::from_secs(3), stream.next()
            ).await;
            let chunk = match chunk_result {
                Ok(Some(c)) => c,
                Ok(None) => break,
                Err(_elapsed) => {
                    if cancel.load(Ordering::Relaxed) {
                        file.flush().await.ok();
                        return Err("Cancelled".to_string());
                    }
                    continue;
                }
            };
            let chunk = chunk.map_err(|e| format!("Stream error: {}", e))?;
            limiter.wait_n(chunk.len() as u64);
            file.write_all(&chunk).await.map_err(|e| format!("Write error: {}", e))?;
            total += chunk.len() as u64;

            // Periodic progress — report every 128KB
            if total % (buf_size * 4) == 0 || total == 0 {
                let _ = self.event_tx.send(Event {
                    kind: EventKind::DownloadProgress,
                    download_id: cfg.id,
                    data: Some(total.to_string()),
                });
            }
        }

        file.flush().await.map_err(|e| format!("Flush error: {}", e))?;

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
