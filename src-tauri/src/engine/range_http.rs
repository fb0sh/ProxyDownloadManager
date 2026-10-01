//! Local HTTP/1.1 range downloads. These assert the body, not just `Ok`,
//! because a server that ignores Range can still complete via the single-file fallback.

use super::{file_io, run_download, EngineHooks};
use crate::network::limiter::MultiLimiter;
use crate::network::pool::NetworkPool;
use crate::types::{DownloadState, EngineConfig, Event, EventKind};
use std::collections::HashMap;
use std::io::Read;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::mpsc;

#[derive(Clone, Copy)]
pub(crate) enum BodyMode {
    Exact,
    IgnoreRange,
    BadContentRange,
    ShortFirst,
}

struct Hit {
    ranges: Vec<String>,
    encodings: Vec<String>,
    if_ranges: Vec<String>,
}

pub(crate) struct Srv {
    hits: Mutex<Vec<Hit>>,
    short_left: AtomicUsize,
    progressed: AtomicBool,
    pub(crate) release: AtomicBool,
    pub(crate) stall: AtomicBool,
}

fn header_values<'a>(req: &'a str, name: &str) -> Vec<&'a str> {
    let prefix = format!("{name}:");
    req.lines()
        .filter_map(|line| {
            let line = line.trim();
            if line.len() >= prefix.len() && line[..prefix.len()].eq_ignore_ascii_case(&prefix) {
                Some(line[prefix.len()..].trim())
            } else {
                None
            }
        })
        .collect()
}

fn first_range(req: &str) -> Option<(u64, Option<u64>)> {
    let raw = header_values(req, "range").into_iter().next()?;
    let spec = raw.split(',').next()?.trim();
    let spec = spec
        .strip_prefix("bytes=")
        .or_else(|| spec.strip_prefix("bytes ="))?;
    let (start, end) = spec.split_once('-')?;
    let start = start.trim().parse().ok()?;
    let end = if end.trim().is_empty() {
        None
    } else {
        Some(end.trim().parse().ok()?)
    };
    Some((start, end))
}

async fn read_headers(stream: &mut tokio::net::TcpStream) -> Option<String> {
    let mut total = Vec::new();
    let mut buf = [0u8; 4096];
    loop {
        let n = stream.read(&mut buf).await.unwrap_or(0);
        if n == 0 {
            return None;
        }
        total.extend_from_slice(&buf[..n]);
        if total.windows(4).any(|w| w == b"\r\n\r\n") || total.len() > 64 * 1024 {
            break;
        }
    }
    Some(String::from_utf8_lossy(&total).into_owned())
}

async fn write_generated(stream: &mut tokio::net::TcpStream, start: u64, len: u64) {
    let mut off = 0u64;
    let mut buf = [0u8; 32 * 1024];
    while off < len {
        let n = ((len - off) as usize).min(buf.len());
        for (i, slot) in buf[..n].iter_mut().enumerate() {
            *slot = (start + off + i as u64) as u8;
        }
        if stream.write_all(&buf[..n]).await.is_err() {
            return;
        }
        off += n as u64;
    }
}

async fn write_full(stream: &mut tokio::net::TcpStream, size: u64) {
    let hdr = format!(
        "HTTP/1.1 200 OK\r\nContent-Length: {size}\r\nContent-Disposition: attachment; filename=file.bin\r\nConnection: close\r\n\r\n"
    );
    if stream.write_all(hdr.as_bytes()).await.is_err() {
        return;
    }
    write_generated(stream, 0, size).await;
}

pub(crate) async fn spawn_server(size: u64, mode: BodyMode, stall: bool) -> (String, Arc<Srv>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let srv = Arc::new(Srv {
        hits: Mutex::new(Vec::new()),
        short_left: AtomicUsize::new(if matches!(mode, BodyMode::ShortFirst) {
            1
        } else {
            0
        }),
        progressed: AtomicBool::new(false),
        release: AtomicBool::new(false),
        stall: AtomicBool::new(stall),
    });
    let shared = srv.clone();
    tokio::spawn(async move {
        loop {
            let Ok((mut stream, _)) = listener.accept().await else {
                return;
            };
            let srv = shared.clone();
            let mode = mode;
            tokio::spawn(async move {
                let Some(req) = read_headers(&mut stream).await else {
                    return;
                };
                let hit = Hit {
                    ranges: header_values(&req, "range")
                        .into_iter()
                        .map(str::to_string)
                        .collect(),
                    encodings: header_values(&req, "accept-encoding")
                        .into_iter()
                        .map(str::to_string)
                        .collect(),
                    if_ranges: header_values(&req, "if-range")
                        .into_iter()
                        .map(str::to_string)
                        .collect(),
                };
                let range = first_range(&req);
                srv.hits.lock().unwrap().push(hit);

                if matches!(mode, BodyMode::IgnoreRange) {
                    write_full(&mut stream, size).await;
                    return;
                }
                if matches!(mode, BodyMode::BadContentRange) && range.is_some() {
                    let _ = stream
                        .write_all(
                            b"HTTP/1.1 206 Partial Content\r\nContent-Range: bytes 5-6/9\r\nContent-Length: 2\r\nConnection: close\r\n\r\nNO",
                        )
                        .await;
                    return;
                }
                let Some((start, end)) = range else {
                    write_full(&mut stream, size).await;
                    return;
                };
                if size == 0 || start >= size {
                    let _ = stream
                        .write_all(
                            b"HTTP/1.1 416 Range Not Satisfiable\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                        )
                        .await;
                    return;
                }
                let end = end.unwrap_or(size - 1).min(size - 1);
                if end < start {
                    return;
                }
                let len = end - start + 1;
                let mut send_len = len;
                if len > 8192 && srv.short_left.load(Ordering::Relaxed) > 0 {
                    srv.short_left.fetch_sub(1, Ordering::Relaxed);
                    send_len = 4096;
                }
                let hdr = format!(
                    "HTTP/1.1 206 Partial Content\r\nContent-Range: bytes {start}-{end}/{size}\r\nContent-Length: {len}\r\nConnection: close\r\n\r\n"
                );
                if stream.write_all(hdr.as_bytes()).await.is_err() {
                    return;
                }
                if srv.stall.load(Ordering::Relaxed) {
                    let head = 8192.min(len);
                    write_generated(&mut stream, start, head).await;
                    srv.progressed.store(true, Ordering::Relaxed);
                    while srv.stall.load(Ordering::Relaxed) && !srv.release.load(Ordering::Relaxed)
                    {
                        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
                    }
                    if srv.stall.load(Ordering::Relaxed) {
                        return;
                    }
                    write_generated(&mut stream, start + head, len - head).await;
                    return;
                }
                write_generated(&mut stream, start, send_len).await;
            });
        }
    });
    (format!("http://127.0.0.1:{port}/file.bin"), srv)
}

fn split_parts(size: u64, n: u32) -> Vec<(u64, u64)> {
    let n = n.max(1);
    let mut parts = Vec::with_capacity(n as usize);
    let mut start = 0u64;
    for i in 0..n {
        let end = if i + 1 == n {
            size
        } else {
            (size / n as u64) * (i as u64 + 1)
        };
        if end > start {
            parts.push((start, end));
            start = end;
        }
    }
    parts
}

fn make_cfg(
    url: &str,
    size: u64,
    connections: u32,
    parts: Vec<(u64, u64)>,
    headers: HashMap<String, String>,
) -> EngineConfig {
    static NEXT: AtomicUsize = AtomicUsize::new(1);
    let id = (std::process::id() as u64)
        .saturating_mul(100_000)
        .saturating_add(NEXT.fetch_add(1, Ordering::Relaxed) as u64);
    EngineConfig {
        url: url.to_string(),
        save_path: std::env::temp_dir()
            .join(format!("pdm_range_{id}.bin"))
            .to_string_lossy()
            .into_owned(),
        id,
        file_name: "file.bin".into(),
        is_resume: false,
        headers,
        proxy_url: String::new(),
        proxy_name: String::new(),
        total_size: size,
        supports_range: true,
        rate_limit_bps: 0,
        connections,
        max_retries: 4,
        user_agent: "pdm-range-test".into(),
        resume_tasks: vec![],
        downloaded: 0,
        part_downloaded: vec![0; parts.len()],
        part_ranges: parts,
        desired_connections: None,
        auto_connections: false,
        auto_flag: None,
        is_hls: false,
    }
}

fn quiet_hooks() -> EngineHooks {
    EngineHooks {
        save_resume_state: Box::new(|_, _| {}),
        invalidate_for_restart: Box::new(|_| {}),
    }
}

fn max_downloaded(rx: &mut mpsc::UnboundedReceiver<Event>) -> u64 {
    let mut max_dl = 0u64;
    while let Ok(ev) = rx.try_recv() {
        if !matches!(ev.kind, EventKind::DownloadProgress) {
            continue;
        }
        let Some(data) = ev.data else { continue };
        let Ok(v) = serde_json::from_str::<serde_json::Value>(&data) else {
            continue;
        };
        if let Some(n) = v.get("downloaded").and_then(|x| x.as_u64()) {
            max_dl = max_dl.max(n);
        }
    }
    max_dl
}

pub(crate) fn assert_file(path: &str, size: u64) {
    let file = std::fs::File::open(path).unwrap_or_else(|e| panic!("open {path}: {e}"));
    assert_eq!(file.metadata().unwrap().len(), size, "{path}");
    let mut reader = std::io::BufReader::new(file);
    let mut buf = [0u8; 64 * 1024];
    let mut off = 0u64;
    loop {
        let n = reader.read(&mut buf).unwrap();
        if n == 0 {
            break;
        }
        for (i, byte) in buf[..n].iter().enumerate() {
            let at = off + i as u64;
            assert_eq!(*byte, at as u8, "mismatch at {at}");
        }
        off += n as u64;
    }
    assert_eq!(off, size);
}

async fn finish(cfg: EngineConfig) -> (Result<(), crate::types::PdmError>, u64) {
    let pool = Arc::new(NetworkPool::new(false));
    let (tx, mut rx) = mpsc::unbounded_channel();
    let limiter = Arc::new(MultiLimiter::new(0, 0));
    let cancel = Arc::new(AtomicBool::new(false));
    let result = run_download(cfg, pool, tx, limiter, cancel, quiet_hooks()).await;
    (result, max_downloaded(&mut rx))
}

fn replay_headers() -> HashMap<String, String> {
    let mut headers = HashMap::new();
    headers.insert("Range".into(), "bytes=0-0".into());
    headers.insert("If-Range".into(), "\"etag\"".into());
    headers.insert("Accept-Encoding".into(), "gzip".into());
    headers.insert("Cookie".into(), "sid=1".into());
    headers
}

fn hits_of(srv: &Srv) -> Vec<Hit> {
    srv.hits
        .lock()
        .unwrap()
        .iter()
        .map(|h| Hit {
            ranges: h.ranges.clone(),
            encodings: h.encodings.clone(),
            if_ranges: h.if_ranges.clone(),
        })
        .collect()
}

fn assert_engine_owns_headers(hits: &[Hit]) {
    assert!(!hits.is_empty(), "server saw no requests");
    for hit in hits {
        assert_eq!(hit.ranges.len(), 1, "ranges {:?}", hit.ranges);
        let range = hit.ranges[0].trim();
        assert!(range.starts_with("bytes="), "{range}");
        assert_ne!(range, "bytes=0-0");
        assert_eq!(hit.encodings.len(), 1, "encodings {:?}", hit.encodings);
        assert_eq!(hit.encodings[0].trim(), "identity");
        assert!(hit.if_ranges.is_empty(), "if-range {:?}", hit.if_ranges);
    }
}

async fn ranged_case(connections: u32, size: u64) {
    let (url, srv) = spawn_server(size, BodyMode::Exact, false).await;
    let parts = split_parts(size, connections);
    assert_eq!(parts.len(), connections as usize);
    let cfg = make_cfg(&url, size, connections, parts, replay_headers());
    let path = cfg.save_path.clone();
    let id = cfg.id;
    let (result, downloaded) = finish(cfg).await;
    assert!(result.is_ok(), "{:?}", result.err());
    assert!(downloaded > 0, "progress stayed at 0");
    assert_file(&path, size);
    let hits = hits_of(&srv);
    assert_engine_owns_headers(&hits);
    let starts: std::collections::HashSet<u64> = hits
        .iter()
        .filter_map(|h| first_range(&format!("range: {}", h.ranges[0])))
        .map(|(start, _)| start)
        .collect();
    assert_eq!(starts.len(), connections as usize, "starts {starts:?}");
    let _ = std::fs::remove_file(&path);
    file_io::remove_temp(id);
}

#[tokio::test]
async fn range_one_four_and_thirty_two_connections() {
    ranged_case(1, 256 * 1024).await;
    ranged_case(4, 1024 * 1024).await;
    ranged_case(32, 2 * 1024 * 1024).await;
}

#[tokio::test]
async fn auto_connections_follow_planned_parts() {
    // connections stored as 0 is Auto. The engine must still request each
    // planned part. The old path built one task via connections.max(1).
    let size = 8 * 1024 * 1024;
    let (url, srv) = spawn_server(size, BodyMode::Exact, false).await;
    let parts = split_parts(size, 4);
    let cfg = make_cfg(&url, size, 0, parts, replay_headers());
    let path = cfg.save_path.clone();
    let (result, downloaded) = finish(cfg).await;
    assert!(result.is_ok(), "{:?}", result.err());
    assert!(downloaded > 0);
    assert_file(&path, size);
    let hits = hits_of(&srv);
    assert_engine_owns_headers(&hits);
    let starts: std::collections::HashSet<u64> = hits
        .iter()
        .filter_map(|h| first_range(&format!("range: {}", h.ranges[0])))
        .map(|(s, _)| s)
        .collect();
    assert_eq!(
        starts.len(),
        4,
        "auto planned one full-file range: {starts:?}"
    );
    let _ = std::fs::remove_file(&path);
}

#[tokio::test]
async fn hundred_megabyte_range_download_writes_every_byte() {
    let size = 100 * 1024 * 1024;
    let (url, _srv) = spawn_server(size, BodyMode::Exact, false).await;
    let parts = split_parts(size, 4);
    let cfg = make_cfg(&url, size, 4, parts, HashMap::new());
    let path = cfg.save_path.clone();
    let (result, downloaded) = finish(cfg).await;
    assert!(result.is_ok(), "{:?}", result.err());
    assert!(downloaded > 0);
    assert_file(&path, size);
    let _ = std::fs::remove_file(&path);
}

#[tokio::test]
async fn ignored_range_degrades_without_a_corrupt_file() {
    let size = 256 * 1024;
    let (url, srv) = spawn_server(size, BodyMode::IgnoreRange, false).await;
    let parts = split_parts(size, 4);
    let cfg = make_cfg(&url, size, 4, parts, HashMap::new());
    let path = cfg.save_path.clone();
    let (result, downloaded) = finish(cfg).await;
    assert!(result.is_ok(), "{:?}", result.err());
    assert!(downloaded > 0);
    assert_file(&path, size);
    let hits = hits_of(&srv);
    assert!(hits.iter().any(|h| !h.ranges.is_empty()));
    assert!(
        hits.iter().any(|h| h.ranges.is_empty()),
        "single-file fallback never ran"
    );
    let _ = std::fs::remove_file(&path);
}

#[tokio::test]
async fn malformed_content_range_does_not_keep_the_bad_body() {
    let size = 128 * 1024;
    let (url, _srv) = spawn_server(size, BodyMode::BadContentRange, false).await;
    let parts = split_parts(size, 4);
    let cfg = make_cfg(&url, size, 4, parts, HashMap::new());
    let path = cfg.save_path.clone();
    let (result, _) = finish(cfg).await;
    assert!(
        result.is_ok(),
        "fallback should still save the real file: {:?}",
        result.err()
    );
    assert_file(&path, size);
    let _ = std::fs::remove_file(&path);
}

#[tokio::test]
async fn short_stream_is_resumed_by_the_remainder() {
    let size = 256 * 1024;
    let (url, srv) = spawn_server(size, BodyMode::ShortFirst, false).await;
    let parts = split_parts(size, 1);
    let cfg = make_cfg(&url, size, 1, parts, HashMap::new());
    let path = cfg.save_path.clone();
    let (result, downloaded) = finish(cfg).await;
    assert!(result.is_ok(), "{:?}", result.err());
    assert!(downloaded > 0);
    assert_file(&path, size);
    let hits = hits_of(&srv);
    assert!(
        hits.len() >= 2,
        "short body was not retried: {}",
        hits.len()
    );
    let _ = std::fs::remove_file(&path);
}

#[tokio::test]
async fn pause_keeps_bytes_and_resume_finishes_the_same_file() {
    let size = 256 * 1024;
    let (url, srv) = spawn_server(size, BodyMode::Exact, true).await;
    let parts = split_parts(size, 2);
    let cfg = make_cfg(&url, size, 2, parts, HashMap::new());
    let saved: Arc<Mutex<Option<DownloadState>>> = Arc::new(Mutex::new(None));
    let saved_hook = saved.clone();
    let hooks = EngineHooks {
        save_resume_state: Box::new(move |_id, state| {
            *saved_hook.lock().unwrap() = Some(state.clone());
        }),
        invalidate_for_restart: Box::new(|_| {}),
    };
    let pool = Arc::new(NetworkPool::new(false));
    let (tx, mut rx) = mpsc::unbounded_channel();
    let limiter = Arc::new(MultiLimiter::new(0, 0));
    let cancel = Arc::new(AtomicBool::new(false));
    let cancel_flag = cancel.clone();
    let release = srv.clone();
    tokio::spawn(async move {
        while !release.progressed.load(Ordering::Relaxed) {
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
        cancel_flag.store(true, Ordering::Relaxed);
        tokio::time::sleep(std::time::Duration::from_millis(250)).await;
        release.release.store(true, Ordering::Relaxed);
    });
    let path = cfg.save_path.clone();
    let id = cfg.id;
    let first = run_download(cfg, pool, tx, limiter, cancel, hooks).await;
    let partial = max_downloaded(&mut rx);
    assert!(
        matches!(first, Err(crate::types::PdmError::Cancelled)),
        "expected pause, got {first:?}"
    );
    assert!(partial > 0, "pause saved no bytes");
    let snap = saved.lock().unwrap().clone().expect("resume snapshot");
    assert!(!snap.tasks.is_empty());
    assert!(snap.downloaded > 0);

    srv.stall.store(false, Ordering::Relaxed);
    let mut resume = make_cfg(&url, size, 2, split_parts(size, 2), HashMap::new());
    resume.id = id;
    resume.save_path = path.clone();
    resume.is_resume = true;
    resume.resume_tasks = snap.tasks;
    resume.downloaded = snap.downloaded;
    let (result, downloaded) = finish(resume).await;
    assert!(result.is_ok(), "{:?}", result.err());
    assert!(downloaded > 0);
    assert_file(&path, size);
    let _ = std::fs::remove_file(&path);
    file_io::remove_temp(id);
}
