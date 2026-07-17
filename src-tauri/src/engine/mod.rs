pub mod chunk;
pub mod concurrent;
pub mod single;

use crate::types::{DownloadConfig, DownloadState};
use crate::network::pool::NetworkPool;
use crate::network::limiter::MultiLimiter;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use tokio::sync::mpsc;
use crate::types::Event;
use async_trait::async_trait;

/// Callback invoked by the engine when a download is cancelled,
/// to persist remaining tasks for resume. Avoids direct gob access.
pub type OnCancelled = Box<dyn Fn(u64, &DownloadState) + Send + Sync>;

/// Trait for download engine implementations.
/// Both ConcurrentDownloader and SingleDownloader implement this,
/// allowing the dispatch logic to be polymorphic.
#[async_trait]
pub trait DownloadEngine: Send + Sync {
    async fn download(
        &self,
        cfg: &DownloadConfig,
        limiter: Arc<MultiLimiter>,
        cancel: Arc<AtomicBool>,
        on_cancelled: &OnCancelled,
    ) -> Result<(), String>;
}

#[async_trait]
impl DownloadEngine for concurrent::ConcurrentDownloader {
    async fn download(
        &self,
        cfg: &DownloadConfig,
        limiter: Arc<MultiLimiter>,
        cancel: Arc<AtomicBool>,
        on_cancelled: &OnCancelled,
    ) -> Result<(), String> {
        self.download(cfg, limiter, cancel, on_cancelled).await
    }
}

#[async_trait]
impl DownloadEngine for single::SingleDownloader {
    async fn download(
        &self,
        cfg: &DownloadConfig,
        limiter: Arc<MultiLimiter>,
        cancel: Arc<AtomicBool>,
        on_cancelled: &OnCancelled,
    ) -> Result<(), String> {
        self.download(cfg, limiter, cancel, on_cancelled).await
    }
}

/// Factory: create the appropriate engine based on config.
fn create_engine(
    cfg: &DownloadConfig,
    pool: Arc<NetworkPool>,
    event_tx: &mpsc::UnboundedSender<Event>,
) -> Box<dyn DownloadEngine> {
    if cfg.supports_range {
        Box::new(concurrent::ConcurrentDownloader::new(pool, event_tx.clone()))
    } else {
        Box::new(single::SingleDownloader::new(pool, event_tx.clone()))
    }
}

pub async fn run_download(
    cfg: DownloadConfig,
    pool: Arc<NetworkPool>,
    event_tx: mpsc::UnboundedSender<Event>,
    limiter: Arc<MultiLimiter>,
    cancel: Arc<AtomicBool>,
    on_cancelled: OnCancelled,
) -> Result<(), String> {
    let engine_kind = if cfg.supports_range { "concurrent" } else { "single" };
    eprintln!("[ProxyDM] run_download id={} engine={} url={} size={} range={}",
        cfg.id, engine_kind, cfg.url, cfg.total_size, cfg.supports_range);

    let _ = event_tx.send(Event {
        kind: crate::types::EventKind::DownloadStarted,
        download_id: cfg.id,
        data: None,
    });

    let engine = create_engine(&cfg, pool.clone(), &event_tx);
    let result = engine.download(&cfg, limiter.clone(), cancel.clone(), &on_cancelled).await;

    // On concurrent failure (not cancelled), degrade to single
    let result = match result {
        Ok(()) => result,
        Err(ref e) if e == "Cancelled" => result,
        Err(e) => {
            eprintln!("[ProxyDM] Concurrent id={} failed, degrading to Single: {}", cfg.id, e);
            let pdm_path = format!("{}.pdm", cfg.output_path);
            let _ = std::fs::OpenOptions::new()
                .write(true)
                .truncate(true)
                .open(&pdm_path);
            let _ = event_tx.send(Event {
                kind: crate::types::EventKind::DownloadProgress,
                download_id: cfg.id,
                data: Some("0".to_string()),
            });
            let fallback: Box<dyn DownloadEngine> = Box::new(single::SingleDownloader::new(pool, event_tx.clone()));
            fallback.download(&cfg, limiter, cancel, &on_cancelled).await
        }
    };

    match &result {
        Ok(_) => eprintln!("[ProxyDM] run_download id={} engine={} OK", cfg.id, engine_kind),
        Err(e) => eprintln!("[ProxyDM] run_download id={} engine={} FAILED: {}", cfg.id, engine_kind, e),
    }
    result
}
