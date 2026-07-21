pub mod chunk;
pub mod concurrent;
pub mod file_io;
pub mod part_progress;
pub mod single;
pub mod task_download;

use crate::types::{EngineConfig, DownloadState, PdmError, PdmResult};
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
    log::info!("[ProxyDM] run_download id={} engine={} url={} size={} range={}",
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
            log::error!("[ProxyDM] Concurrent id={} failed, degrading to Single: {}", cfg.id, e);
            let pdm_path = format!("{}.pdm", cfg.save_path);
            let _ = std::fs::OpenOptions::new()
                .write(true)
                .truncate(true)
                .open(&pdm_path);
            // Progress Map: Concurrent → Single becomes one cell; reset progress.
            let _ = event_tx.send(Event {
                kind: crate::types::EventKind::DownloadProgress,
                download_id: cfg.id,
                data: Some(part_progress::encode_progress_data(0, &[0], true)),
            });
            let fallback: Box<dyn DownloadEngine> = Box::new(single::SingleDownloader::new(pool, event_tx.clone()));
            fallback.download(&cfg, limiter, cancel, &on_cancelled).await
        }
    };

    match &result {
        Ok(_) => log::info!("[ProxyDM] run_download id={} engine={} OK", cfg.id, engine_kind),
        Err(e) => log::error!("[ProxyDM] run_download id={} engine={} FAILED: {}", cfg.id, engine_kind, e),
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::EventKind;
    use crate::network::pool::NetworkPool;
    use std::sync::Arc;
    use std::sync::atomic::Ordering;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    fn test_config(url: &str, supports_range: bool, total_size: u64) -> EngineConfig {
        EngineConfig {
            url: url.to_string(),
            save_path: std::env::temp_dir()
                .join(format!("pdm_engine_test_{}.bin", std::process::id()))
                .to_str()
                .unwrap()
                .to_string(),
            id: 1,
            file_name: "file.bin".to_string(),
            is_resume: false,
            headers: std::collections::HashMap::new(),
            proxy_url: String::new(),
            proxy_name: String::new(),
            total_size,
            supports_range,
            rate_limit_bps: 0,
            connections: 2,
            max_retries: 3,
            user_agent: "test-agent".to_string(),
            resume_tasks: vec![],
            downloaded: 0,
            part_ranges: if total_size > 0 {
                vec![(0, total_size)]
            } else {
                vec![]
            },
            part_downloaded: vec![],
        }
    }

    /// Spawn a mock HTTP server that supports Range requests.
    /// Returns the base URL (http://127.0.0.1:PORT).
    async fn spawn_mock_server(file_data: Vec<u8>, supports_range: bool) -> String {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        tokio::spawn(async move {
            loop {
                let (mut stream, _) = match listener.accept().await {
                    Ok(conn) => conn,
                    Err(_) => return,
                };

                // Read request headers
                let mut buf = vec![0u8; 4096];
                let mut total = Vec::new();
                loop {
                    let n = stream.read(&mut buf).await.unwrap_or(0);
                    if n == 0 { break; }
                    total.extend_from_slice(&buf[..n]);
                    if total.windows(4).any(|w| w == b"\r\n\r\n") { break; }
                }
                let req = String::from_utf8_lossy(&total);
                let has_range = req.contains("Range: bytes=");

                if !supports_range || req.contains("bytes=0-0") {
                    // Probe request: respond with 206 + Content-Range
                    let body = if supports_range && req.contains("bytes=0-0") {
                        b"X".to_vec() // 1 byte for probe
                    } else {
                        file_data.clone()
                    };
                    let content_len = body.len();
                    let status_line = if supports_range && (req.contains("bytes=0-0") || has_range) {
                        let range = if req.contains("bytes=0-0") {
                            (0u64, 0u64)
                        } else {
                            // Parse the Range header
                            let start = req.lines()
                                .find(|l| l.starts_with("Range:"))
                                .and_then(|l| l.split("bytes=").nth(1))
                                .and_then(|r| r.split('-').next())
                                .and_then(|s| s.trim().parse::<u64>().ok())
                                .unwrap_or(0);
                            let end = req.lines()
                                .find(|l| l.starts_with("Range:"))
                                .and_then(|l| l.split("bytes=").nth(1))
                                .and_then(|r| r.split('-').nth(1))
                                .and_then(|s| s.trim().parse::<u64>().ok())
                                .unwrap_or(content_len as u64 - 1);
                            (start, end)
                        };
                        format!(
                            "HTTP/1.1 206 Partial Content\r\nContent-Range: bytes {}-{}/{}\r\nContent-Length: {}\r\n\r\n",
                            range.0, range.1, file_data.len(),
                            range.1 - range.0 + 1
                        )
                    } else {
                        format!(
                            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nContent-Disposition: attachment; filename=test.bin\r\n\r\n",
                            content_len
                        )
                    };
                    let _ = stream.write_all(status_line.as_bytes()).await;
                    let response_body = if supports_range && !req.contains("bytes=0-0") && has_range {
                        // Parse range and send the requested bytes
                        let start = req.lines()
                            .find(|l| l.starts_with("Range:"))
                            .and_then(|l| l.split("bytes=").nth(1))
                            .and_then(|r| r.split('-').next())
                            .and_then(|s| s.trim().parse::<u64>().ok())
                            .unwrap_or(0);
                        let end = req.lines()
                            .find(|l| l.starts_with("Range:"))
                            .and_then(|l| l.split("bytes=").nth(1))
                            .and_then(|r| r.split('-').nth(1))
                            .and_then(|s| s.trim().parse::<u64>().ok())
                            .unwrap_or(file_data.len() as u64 - 1);
                        file_data[start as usize..=end as usize].to_vec()
                    } else if supports_range && req.contains("bytes=0-0") {
                        vec![b'X']
                    } else {
                        body
                    };
                    let _ = stream.write_all(&response_body).await;
                } else {
                    // Regular GET — return full file
                    let body = file_data.clone();
                    let status_line = format!(
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n",
                        body.len()
                    );
                    let _ = stream.write_all(status_line.as_bytes()).await;
                    let _ = stream.write_all(&body).await;
                }
            }
        });

        format!("http://127.0.0.1:{}", addr.port())
    }

    #[test]
    fn test_factory_creates_concurrent_when_range_supported() {
        let pool = Arc::new(NetworkPool::new(false));
        let (tx, _rx) = mpsc::unbounded_channel();
        let cfg = test_config("https://example.com/file.zip", true, 1000);
        let engine = create_engine(&cfg, pool, &tx);
        drop(engine);
    }

    #[test]
    fn test_factory_creates_single_when_no_range() {
        let pool = Arc::new(NetworkPool::new(false));
        let (tx, _rx) = mpsc::unbounded_channel();
        let cfg = test_config("https://example.com/file.zip", false, 1000);
        let engine = create_engine(&cfg, pool, &tx);
        drop(engine);
    }

    #[tokio::test]
    async fn test_run_download_emits_started_event() {
        let pool = Arc::new(NetworkPool::new(false));
        let (tx, mut rx) = mpsc::unbounded_channel();
        let cfg = test_config("https://example.com/file.zip", false, 1000);
        let limiter = Arc::new(MultiLimiter::new(0, 0));
        let cancel = Arc::new(AtomicBool::new(false));
        let on_cancelled: OnResumeState = Box::new(|_, _| {});

        let _ = run_download(cfg, pool, tx, limiter, cancel, on_cancelled).await;

        let event = rx.try_recv();
        assert!(event.is_ok(), "Expected at least one event (DownloadStarted)");
        assert!(matches!(event.unwrap().kind, EventKind::DownloadStarted));
    }

    #[tokio::test]
    async fn test_run_download_single_engine_with_mock_server() {
        // Create a small test file
        let file_data = vec![0xABu8; 4096]; // 4KB of test data
        let server_url = spawn_mock_server(file_data.clone(), false).await;
        let url = format!("{}/test.bin", server_url);

        let pool = Arc::new(NetworkPool::new(false));
        let (tx, _rx) = mpsc::unbounded_channel();
        let cfg = test_config(&url, false, file_data.len() as u64);
        let limiter = Arc::new(MultiLimiter::new(0, 0));
        let cancel = Arc::new(AtomicBool::new(false));
        let on_cancelled: OnResumeState = Box::new(|_, _| {});

        let result = run_download(cfg, pool.clone(), tx, limiter, cancel, on_cancelled).await;

        // Single engine should succeed
        assert!(result.is_ok(), "Single engine download failed: {:?}", result.err());

        // Clean up temp file
        let save_path = std::env::temp_dir()
            .join(format!("pdm_engine_test_{}.bin", std::process::id()));
        let _ = std::fs::remove_file(&save_path);
    }

    #[tokio::test]
    async fn test_run_download_concurrent_engine_with_mock_server() {
        // Create test data large enough for multiple chunks
        // With 2 connections and 4KB alignment, we need enough data
        let file_data = vec![0xCDu8; 4 * 1024 * 1024]; // 4MB for multi-chunk
        let server_url = spawn_mock_server(file_data.clone(), true).await;
        let url = format!("{}/test-large.bin", server_url);

        let pool = Arc::new(NetworkPool::new(false));
        let (tx, _rx) = mpsc::unbounded_channel();
        let cfg = test_config(&url, true, file_data.len() as u64);
        let limiter = Arc::new(MultiLimiter::new(0, 0));
        let cancel = Arc::new(AtomicBool::new(false));
        let on_cancelled: OnResumeState = Box::new(|_, _| {});

        let result = run_download(cfg, pool.clone(), tx, limiter, cancel, on_cancelled).await;

        assert!(result.is_ok(), "Concurrent engine download failed: {:?}", result.err());

        // Clean up
        let save_path = std::env::temp_dir()
            .join(format!("pdm_engine_test_{}.bin", std::process::id()));
        let _ = std::fs::remove_file(&save_path);
        let pdm_path = std::env::temp_dir()
            .join(format!("pdm_engine_test_{}.bin.pdm", std::process::id()));
        let _ = std::fs::remove_file(&pdm_path);
    }

    #[tokio::test]
    async fn test_run_download_emits_progress_events() {
        let file_data = vec![0xEFu8; 8192]; // 8KB
        let server_url = spawn_mock_server(file_data.clone(), false).await;
        let url = format!("{}/test.bin", server_url);

        let pool = Arc::new(NetworkPool::new(false));
        let (tx, mut rx) = mpsc::unbounded_channel();
        let cfg = test_config(&url, false, file_data.len() as u64);
        let limiter = Arc::new(MultiLimiter::new(0, 0));
        let cancel = Arc::new(AtomicBool::new(false));
        let on_cancelled: OnResumeState = Box::new(|_, _| {});

        let _ = run_download(cfg, pool.clone(), tx, limiter, cancel, on_cancelled).await;

        // Collect all events — should include DownloadStarted, progress, and DownloadCompleted
        let mut events = Vec::new();
        while let Ok(event) = rx.try_recv() {
            events.push(event);
        }

        let has_started = events.iter().any(|e| matches!(e.kind, EventKind::DownloadStarted));
        let has_completed = events.iter().any(|e| matches!(e.kind, EventKind::DownloadCompleted));
        let has_progress = events.iter().any(|e| matches!(e.kind, EventKind::DownloadProgress));

        assert!(has_started, "Missing DownloadStarted event");
        assert!(has_completed, "Missing DownloadCompleted event (got {} events)", events.len());
        assert!(has_progress, "Missing DownloadProgress events (got {} events)", events.len());

        // Clean up
        let save_path = std::env::temp_dir()
            .join(format!("pdm_engine_test_{}.bin", std::process::id()));
        let _ = std::fs::remove_file(&save_path);
    }

    #[tokio::test]
    async fn test_run_download_cancel_stops_engine() {
        let file_data = vec![0x11u8; 1024 * 1024]; // 1MB
        let server_url = spawn_mock_server(file_data.clone(), true).await;
        let url = format!("{}/test-cancel.bin", server_url);

        let pool = Arc::new(NetworkPool::new(false));
        let (tx, _rx) = mpsc::unbounded_channel();
        let cfg = test_config(&url, true, file_data.len() as u64);
        let limiter = Arc::new(MultiLimiter::new(0, 0));
        let cancel = Arc::new(AtomicBool::new(false));
        let cancel_clone = cancel.clone();
        let resume_called = Arc::new(AtomicBool::new(false));
        let resume_called_clone = resume_called.clone();

        let on_cancelled: OnResumeState = Box::new(move |_id, _state| {
            resume_called_clone.store(true, Ordering::Relaxed);
        });

        // Cancel after a short delay
        let cancel_handle = tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            cancel_clone.store(true, Ordering::Relaxed);
        });

        let result = run_download(cfg, pool.clone(), tx, limiter, cancel, on_cancelled).await;
        cancel_handle.await.unwrap();

        // Should fail with Cancelled, and on_cancelled callback should have been called
        assert!(result.is_err(), "Expected error after cancel");
        assert!(
            resume_called.load(Ordering::Relaxed),
            "on_cancelled callback was not called"
        );

        // Clean up
        let save_path = std::env::temp_dir()
            .join(format!("pdm_engine_test_{}.bin", std::process::id()));
        let _ = std::fs::remove_file(&save_path);
        let pdm_path = std::env::temp_dir()
            .join(format!("pdm_engine_test_{}.bin.pdm", std::process::id()));
        let _ = std::fs::remove_file(&pdm_path);
    }

}
