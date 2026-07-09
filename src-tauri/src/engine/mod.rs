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
        let conc_result = downloader.download(&cfg, limiter.clone(), cancel.clone()).await;
        match conc_result {
            Ok(()) => conc_result,
            Err(ref e) if e == "Cancelled" => conc_result,
            Err(e) => {
                eprintln!("[ProxyDM] Concurrent id={} failed, degrading to Single: {}", cfg.id, e);
                // SessionReset: truncate .pdm so Single starts clean
                let pdm_path = format!("{}.pdm", cfg.output_path);
                let _ = std::fs::OpenOptions::new()
                    .write(true)
                    .truncate(true)
                    .open(&pdm_path);
                // Reset progress to 0 for the frontend
                let _ = event_tx.send(Event {
                    kind: crate::types::EventKind::DownloadProgress,
                    download_id: cfg.id,
                    data: Some("0".to_string()),
                });
                // Retry with single downloader
                let downloader = single::SingleDownloader::new(pool, event_tx.clone());
                downloader.download(&cfg, limiter, cancel).await
            }
        }
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
