//! Performance integration tests for the download core.
//!
//! These exercise the real engine (`run_download`), the real `NetworkPool`,
//! the real probe and a local HTTP server that can pace bandwidth, keep
//! connections alive and misbehave on purpose.

use crate::caplog;
use crate::engine::chunk;
use crate::engine::file_io;
use crate::engine::{run_download, EngineHooks};
use crate::network::limiter::MultiLimiter;
use crate::network::pool::NetworkPool;
use crate::probe;
use crate::testserver::{ServerConfig, TestServer};
use crate::types::{EngineConfig, EventKind, PdmResult};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;

pub(crate) fn next_id() -> u64 {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    // Keep ids far apart from every other test id scheme in the crate
    // (range_http uses pid * 100_000, engine/mod uses pid * 10_000).
    (std::process::id() as u64) * 10_000_000 + NEXT.fetch_add(1, Ordering::Relaxed)
}

pub(crate) fn save_path(id: u64) -> String {
    std::env::temp_dir()
        .join(format!("pdm_perf_{id}.bin"))
        .to_string_lossy()
        .into_owned()
}

pub(crate) fn noop_hooks() -> EngineHooks {
    EngineHooks {
        save_resume_state: Box::new(|_, _| {}),
        invalidate_for_restart: Box::new(|_| {}),
    }
}

pub(crate) fn engine_cfg(
    url: &str,
    size: u64,
    connections: u32,
    auto: bool,
    desired: Option<Arc<AtomicU32>>,
    auto_flag: Option<Arc<AtomicBool>>,
    id: u64,
) -> EngineConfig {
    let plan = chunk::plan_chunks(size, if auto { 0 } else { connections }, true, 0);
    let part_ranges: Vec<(u64, u64)> = plan.parts.iter().map(|p| (p.start, p.end)).collect();
    EngineConfig {
        url: url.to_string(),
        save_path: save_path(id),
        id,
        file_name: "perf.bin".to_string(),
        is_resume: false,
        headers: HashMap::new(),
        proxy_url: String::new(),
        proxy_name: String::new(),
        total_size: size,
        supports_range: true,
        rate_limit_bps: 0,
        connections,
        max_retries: 4,
        user_agent: "pdm-perf".to_string(),
        resume_tasks: vec![],
        downloaded: 0,
        part_ranges,
        part_downloaded: vec![],
        desired_connections: desired,
        auto_connections: auto,
        auto_flag,
        is_hls: false,
    }
}

/// Run a download for at most `budget`; on timeout, cancel and keep whatever
/// was written. Returns (result, max downloaded from progress events, completed).
pub(crate) async fn run_budget(
    cfg: EngineConfig,
    pool: Arc<NetworkPool>,
    budget: Duration,
) -> (PdmResult<()>, u64, bool) {
    let id = cfg.id;
    let (tx, mut rx) = mpsc::unbounded_channel();
    let limiter = Arc::new(MultiLimiter::new(0, 0));
    let cancel = Arc::new(AtomicBool::new(false));
    let cancel_for_task = cancel.clone();
    let mut handle = tokio::spawn(async move {
        run_download(cfg, pool, tx, limiter, cancel_for_task, noop_hooks()).await
    });

    let result = match tokio::time::timeout(budget, &mut handle).await {
        Ok(joined) => joined.unwrap(),
        Err(_) => {
            cancel.store(true, Ordering::Relaxed);
            handle.await.unwrap()
        }
    };

    let mut max_downloaded = 0u64;
    let mut completed = false;
    while let Ok(ev) = rx.try_recv() {
        match ev.kind {
            EventKind::DownloadCompleted => completed = true,
            EventKind::DownloadProgress { downloaded, .. } => {
                max_downloaded = max_downloaded.max(downloaded);
            }
            _ => {}
        }
    }

    let _ = std::fs::remove_file(save_path(id));
    file_io::remove_temp(id);
    (result, max_downloaded, completed)
}

#[test]
fn one_client_per_network_path() {
    let pool = NetworkPool::new(false);
    for _ in 0..100 {
        pool.get_client(None).unwrap();
    }
    assert_eq!(
        pool.client_creation_count(),
        1,
        "100 direct requests must share one reqwest::Client"
    );

    for _ in 0..100 {
        pool.get_client(Some("http://127.0.0.1:9")).unwrap();
    }
    assert_eq!(
        pool.client_creation_count(),
        2,
        "one proxy must share one reqwest::Client"
    );

    pool.get_client(Some("socks5://127.0.0.1:1080")).unwrap();
    assert_eq!(
        pool.client_creation_count(),
        3,
        "a second path is a second client"
    );
    assert_eq!(pool.cached_client_count(), 3);

    // Only a client-construction setting change clears the pool.
    pool.clear();
    pool.get_client(None).unwrap();
    assert_eq!(pool.client_creation_count(), 4);
}

#[tokio::test]
async fn probe_small_206_is_drained_and_download_reuses_the_connection() {
    let srv = TestServer::start(ServerConfig {
        size: 2 * 1024 * 1024,
        ..Default::default()
    })
    .await;
    let url = srv.url("bench.bin");
    let pool = Arc::new(NetworkPool::new(false));
    let headers = HashMap::new();
    let uas = vec!["pdm-perf".to_string()];

    let probed = probe::probe(&url, &headers, None, &pool, &uas)
        .await
        .expect("probe");
    assert!(probed.supports_range);
    assert_eq!(probed.file_size, 2 * 1024 * 1024);

    let desired = Arc::new(AtomicU32::new(1));
    let cfg = engine_cfg(&url, probed.file_size, 1, false, Some(desired), None, next_id());
    let (result, downloaded, completed) = run_budget(cfg, pool.clone(), Duration::from_secs(20)).await;
    assert!(completed, "{result:?}");
    assert_eq!(downloaded, probed.file_size);

    let stats = srv.stats();
    eprintln!(
        "[reuse] tcp_connections={} requests={} max_requests_on_one_connection={}",
        stats.tcp_connections(),
        stats.requests(),
        stats.max_requests_on_one_connection()
    );
    assert!(stats.requests() >= 2, "expected probe + download requests");
    assert_eq!(
        stats.max_requests_on_one_connection(),
        stats.requests(),
        "probe connection was not reused: {} tcp connections for {} requests",
        stats.tcp_connections(),
        stats.requests()
    );
}

#[tokio::test]
async fn probe_never_drains_a_huge_200() {
    // Server ignores Range: a plain 200 with a huge body, paced so a drain
    // would take many seconds.
    let srv = TestServer::start(ServerConfig {
        size: 64 * 1024 * 1024,
        supports_range: false,
        per_conn_bps: 8 * 1024 * 1024,
        chunk: 64 * 1024,
        ..Default::default()
    })
    .await;
    let pool = Arc::new(NetworkPool::new(false));
    let headers = HashMap::new();
    let uas = vec!["pdm-perf".to_string()];

    let started = Instant::now();
    let probed = probe::probe(&srv.url("huge.bin"), &headers, None, &pool, &uas)
        .await
        .expect("probe");
    let elapsed = started.elapsed();
    let sent = srv.stats().bytes_sent();
    eprintln!("[drain] huge-200 probe elapsed={elapsed:?} bytes_sent={sent}");

    assert!(!probed.supports_range);
    assert!(
        elapsed < Duration::from_secs(3),
        "probe blocked draining a huge 200: {elapsed:?}"
    );
    assert!(
        sent < 16 * 1024 * 1024,
        "probe read too much of a huge 200: {sent} bytes"
    );
}

#[tokio::test]
async fn probe_never_drains_an_oversized_206() {
    // Broken server: answers 206 but with the whole file as the body.
    let srv = TestServer::start(ServerConfig {
        size: 32 * 1024 * 1024,
        supports_range: true,
        force_full_206: true,
        per_conn_bps: 4 * 1024 * 1024,
        chunk: 64 * 1024,
        ..Default::default()
    })
    .await;
    let pool = Arc::new(NetworkPool::new(false));
    let headers = HashMap::new();
    let uas = vec!["pdm-perf".to_string()];

    let started = Instant::now();
    let probed = probe::probe(&srv.url("broken.bin"), &headers, None, &pool, &uas)
        .await
        .expect("probe");
    let elapsed = started.elapsed();
    eprintln!(
        "[drain] oversized-206 probe elapsed={elapsed:?} bytes_sent={}",
        srv.stats().bytes_sent()
    );

    assert!(probed.supports_range);
    assert_eq!(probed.file_size, 32 * 1024 * 1024);
    assert!(
        elapsed < Duration::from_secs(3),
        "probe drained an oversized 206: {elapsed:?}"
    );
}

#[tokio::test]
async fn auto_connections_scale_up_on_a_real_download() {
    // Per-connection cap: more workers really do mean more aggregate
    // throughput, so Auto has a signal to climb.
    let size = 24 * 1024 * 1024;
    let srv = TestServer::start(ServerConfig {
        size,
        per_conn_bps: 512 * 1024,
        chunk: 64 * 1024,
        ..Default::default()
    })
    .await;
    let pool = Arc::new(NetworkPool::new(false));
    let desired = Arc::new(AtomicU32::new(4));
    let auto_flag = Arc::new(AtomicBool::new(true));
    let cfg = engine_cfg(
        &srv.url("auto.bin"),
        size,
        0,
        true,
        Some(desired.clone()),
        Some(auto_flag.clone()),
        next_id(),
    );

    let (result, downloaded, completed) = run_budget(cfg, pool, Duration::from_secs(40)).await;
    assert!(completed, "{result:?}");
    assert_eq!(downloaded, size);

    let final_connections = desired.load(Ordering::Relaxed);
    eprintln!("[auto] final connections = {final_connections}");
    assert!(
        final_connections > 4,
        "Auto never scaled up (final={final_connections})"
    );
    assert!(
        final_connections <= chunk::MAX_CONNECTIONS,
        "exceeded the public maximum: {final_connections}"
    );
}

#[tokio::test]
async fn manual_connections_are_never_modified() {
    let size = 8 * 1024 * 1024;
    let srv = TestServer::start(ServerConfig {
        size,
        per_conn_bps: 256 * 1024,
        chunk: 64 * 1024,
        ..Default::default()
    })
    .await;
    let pool = Arc::new(NetworkPool::new(false));
    // The user explicitly asked for 64 on a slow path. Auto is off.
    let desired = Arc::new(AtomicU32::new(64));
    let auto_flag = Arc::new(AtomicBool::new(false));
    let cfg = engine_cfg(
        &srv.url("manual.bin"),
        size,
        64,
        false,
        Some(desired.clone()),
        Some(auto_flag.clone()),
        next_id(),
    );

    let (result, downloaded, completed) = run_budget(cfg, pool, Duration::from_secs(40)).await;
    assert!(completed, "{result:?}");
    assert_eq!(downloaded, size);
    assert_eq!(
        desired.load(Ordering::Relaxed),
        64,
        "manual 64 was silently changed"
    );
    eprintln!(
        "[manual] desired=64 tcp_connections={} completed={completed}",
        srv.stats().tcp_connections()
    );
}

#[tokio::test]
async fn auto_never_exceeds_the_public_maximum() {
    // Manual is respected at the public maximum even through a proxy-shaped
    // (slow, connection-capped) path.
    let size = 4 * 1024 * 1024;
    let srv = TestServer::start(ServerConfig {
        size,
        per_conn_bps: 128 * 1024,
        chunk: 32 * 1024,
        ..Default::default()
    })
    .await;
    let pool = Arc::new(NetworkPool::new(false));
    let desired = Arc::new(AtomicU32::new(64));
    let cfg = engine_cfg(
        &srv.url("max.bin"),
        size,
        64,
        false,
        Some(desired.clone()),
        Some(Arc::new(AtomicBool::new(false))),
        next_id(),
    );
    let (result, downloaded, completed) = run_budget(cfg, pool, Duration::from_secs(40)).await;
    assert!(completed, "{result:?}");
    assert_eq!(downloaded, size);
    assert_eq!(desired.load(Ordering::Relaxed), 64);
}

/// Local throughput / TTFB benchmark. Run explicitly:
/// `cargo test --lib local_throughput -- --ignored --nocapture --test-threads=1`
#[tokio::test]
#[ignore = "manual benchmark"]
async fn local_throughput_by_connection_count() {
    let cap = caplog::init();
    let size = 64 * 1024 * 1024;
    println!("connections,ttfb_ms,first_progress_ms,seconds,mb_per_s,completed");

    for connections in [1u32, 4, 8, 16, 32, 64] {
        let srv = TestServer::start(ServerConfig {
            size,
            chunk: 128 * 1024,
            ..Default::default()
        })
        .await;
        let pool = Arc::new(NetworkPool::new(false));
        let desired = Arc::new(AtomicU32::new(connections));
        let cfg = engine_cfg(
            &srv.url("local.bin"),
            size,
            connections,
            false,
            Some(desired),
            None,
            next_id(),
        );

        cap.reset();
        let started = Instant::now();
        let (result, downloaded, completed) = run_budget(cfg, pool, Duration::from_secs(60)).await;
        let elapsed = started.elapsed().as_secs_f64();

        let ttfb = cap.at_ms("[startup] first-header").unwrap_or(0);
        let first_progress = cap.at_ms("[startup] first-progress").unwrap_or(0);
        let mbps = downloaded as f64 / elapsed.max(0.001) / (1024.0 * 1024.0);
        println!(
            "{connections},{ttfb},{first_progress},{elapsed:.3},{mbps:.1},{completed} ({result:?})"
        );
    }
}
