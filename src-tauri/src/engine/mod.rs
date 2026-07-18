pub mod chunk;
pub mod concurrent;
pub mod single;

use crate::types::{EngineConfig, DownloadState, PdmError, PdmResult};
use crate::types::engine_config;
use crate::network::pool::NetworkPool;
use crate::network::limiter::MultiLimiter;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use tokio::sync::mpsc;
use crate::types::Event;
use async_trait::async_trait;

/// Callback invoked by the engine when a download is cancelled,
/// to persist remaining tasks for resume. Avoids direct gob access.
pub type OnResumeState = Box<dyn Fn(u64, &DownloadState) + Send + Sync>;

/// Trait for download engine implementations.
/// Both ConcurrentDownloader and SingleDownloader implement this,
/// allowing the dispatch logic to be polymorphic.
#[async_trait]
pub trait DownloadEngine: Send + Sync {
    async fn download(
        &self,
        cfg: &EngineConfig,
        limiter: Arc<MultiLimiter>,
        cancel: Arc<AtomicBool>,
        on_cancelled: &OnResumeState,
    ) -> PdmResult<()>;
}

#[async_trait]
impl DownloadEngine for concurrent::ConcurrentDownloader {
    async fn download(
        &self,
        cfg: &EngineConfig,
        limiter: Arc<MultiLimiter>,
        cancel: Arc<AtomicBool>,
        on_cancelled: &OnResumeState,
    ) -> PdmResult<()> {
        self.download(cfg, limiter, cancel, on_cancelled).await
    }
}

#[async_trait]
impl DownloadEngine for single::SingleDownloader {
    async fn download(
        &self,
        cfg: &EngineConfig,
        limiter: Arc<MultiLimiter>,
        cancel: Arc<AtomicBool>,
        on_cancelled: &OnResumeState,
    ) -> PdmResult<()> {
        self.download(cfg, limiter, cancel, on_cancelled).await
    }
}

/// Factory: create the appropriate engine based on config.
fn create_engine(
    cfg: &EngineConfig,
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
    cfg: EngineConfig,
    pool: Arc<NetworkPool>,
    event_tx: mpsc::UnboundedSender<Event>,
    limiter: Arc<MultiLimiter>,
    cancel: Arc<AtomicBool>,
    on_cancelled: OnResumeState,
) -> PdmResult<()> {
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
        Err(ref e) if matches!(e, PdmError::Cancelled) => result,
        Err(e) => {
            eprintln!("[ProxyDM] Concurrent id={} failed, degrading to Single: {}", cfg.id, e);
            let pdm_path = format!("{}.pdm", cfg.save_path);
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::EventKind;
    use crate::network::pool::NetworkPool;
    use std::sync::Arc;

    fn test_config(supports_range: bool) -> EngineConfig {
        EngineConfig {
            url: "https://example.com/file.zip".to_string(),
            save_path: "/tmp/file.zip".to_string(),
            id: 1,
            file_name: "file.zip".to_string(),
            is_resume: false,
            headers: std::collections::HashMap::new(),
            proxy_url: String::new(),
            total_size: 1000,
            supports_range,
            rate_limit_bps: 0,
            connections: 4,
            max_retries: 3,
            user_agent: "test".to_string(),
            resume_tasks: vec![],
        }
    }

    #[test]
    fn test_factory_creates_concurrent_when_range_supported() {
        let pool = Arc::new(NetworkPool::new(false));
        let (tx, _rx) = mpsc::unbounded_channel();
        let cfg = test_config(true);
        let engine = create_engine(&cfg, pool, &tx);
        // Concurrent engine handles range requests
        // We can't directly inspect the type, but we can verify it doesn't panic
        drop(engine);
    }

    #[test]
    fn test_factory_creates_single_when_no_range() {
        let pool = Arc::new(NetworkPool::new(false));
        let (tx, _rx) = mpsc::unbounded_channel();
        let cfg = test_config(false);
        let engine = create_engine(&cfg, pool, &tx);
        drop(engine);
    }

    #[tokio::test]
    async fn test_run_download_emits_started_event() {
        let pool = Arc::new(NetworkPool::new(false));
        let (tx, mut rx) = mpsc::unbounded_channel();
        let cfg = test_config(false);
        let limiter = Arc::new(MultiLimiter::new(0, 0));
        let cancel = Arc::new(AtomicBool::new(false));
        let on_cancelled: OnResumeState = Box::new(|_, _| {});

        // run_download will fail because the URL is unreachable,
        // but it should still emit DownloadStarted before failing
        let _ = run_download(cfg, pool, tx, limiter, cancel, on_cancelled).await;

        // Check that at least one event was sent (DownloadStarted)
        let event = rx.try_recv();
        assert!(event.is_ok(), "Expected at least one event (DownloadStarted)");
        assert!(matches!(event.unwrap().kind, EventKind::DownloadStarted));
    }
}
