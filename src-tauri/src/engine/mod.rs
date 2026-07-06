pub mod chunk;
pub mod concurrent;
pub mod single;

use crate::types::DownloadConfig;
use crate::network::pool::NetworkPool;
use crate::network::limiter::MultiLimiter;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use tokio::sync::mpsc;
use crate::types::Event;

pub async fn run_download(
    cfg: DownloadConfig,
    pool: Arc<NetworkPool>,
    event_tx: mpsc::UnboundedSender<Event>,
    limiter: Arc<MultiLimiter>,
    cancel: Arc<AtomicBool>,
) -> Result<(), String> {
    let _ = event_tx.send(Event {
        kind: crate::types::EventKind::DownloadStarted,
        download_id: cfg.id,
        data: None,
    });

    if cfg.supports_range {
        let downloader = concurrent::ConcurrentDownloader::new(pool.clone(), event_tx.clone());
        match downloader.download(&cfg, limiter.clone()).await {
            Ok(()) => Ok(()),
            Err(e) => {
                let _ = event_tx.send(Event {
                    kind: crate::types::EventKind::DownloadErrored,
                    download_id: cfg.id,
                    data: Some(format!("Concurrent failed, degrading: {}", e)),
                });
                let downloader = single::SingleDownloader::new(pool, event_tx.clone());
                downloader.download(&cfg, limiter, cancel).await
            }
        }
    } else {
        let downloader = single::SingleDownloader::new(pool, event_tx.clone());
        downloader.download(&cfg, limiter, cancel).await
    }
}
