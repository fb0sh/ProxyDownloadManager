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
    let engine_kind = if cfg.supports_range { "concurrent" } else { "single" };
    eprintln!("[ProxyDM] run_download id={} engine={} url={} size={} range={}",
        cfg.id, engine_kind, cfg.url, cfg.total_size, cfg.supports_range);

    let _ = event_tx.send(Event {
        kind: crate::types::EventKind::DownloadStarted,
        download_id: cfg.id,
        data: None,
    });

    let result = if cfg.supports_range {
        let downloader = concurrent::ConcurrentDownloader::new(pool.clone(), event_tx.clone());
        downloader.download(&cfg, limiter, cancel).await
    } else {
        let downloader = single::SingleDownloader::new(pool, event_tx.clone());
        downloader.download(&cfg, limiter, cancel).await
    };

    match &result {
        Ok(_) => eprintln!("[ProxyDM] run_download id={} engine={} OK", cfg.id, engine_kind),
        Err(e) => eprintln!("[ProxyDM] run_download id={} engine={} FAILED: {}", cfg.id, engine_kind, e),
    }
    result
}
