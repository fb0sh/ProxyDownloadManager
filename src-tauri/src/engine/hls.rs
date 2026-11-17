use crate::engine::transfer::{send_headers, Fetched, Transfer, Want};
use crate::network::limiter::MultiLimiter;
use crate::network::pool::NetworkPool;
use crate::network::protocol::PerfStats;
use crate::types::{
    EngineConfig, Event, EventKind, HlsVariantInfo, PdmError, PdmResult, Phase, Task,
};
use std::collections::HashMap;
use std::io::{Read, Seek, SeekFrom, Write};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;
use url::Url;

#[derive(Debug, Clone)]
pub struct HlsInitMap {
    pub uri: String,
    pub byte_range: Option<Task>,
}

#[derive(Debug, Clone)]
pub struct HlsPlaylist {
    pub is_master: bool,
    pub variants: Vec<HlsVariantInfo>,
    pub segments: Vec<HlsSegment>,
    pub init_map: Option<HlsInitMap>,
    pub encrypted: bool,
    pub drm: bool,
}

#[derive(Debug, Clone)]
pub struct HlsSegment {
    pub uri: String,
    pub duration: f64,
}

fn parse_byterange(spec: &str) -> Option<Task> {
    // RFC 8216: BYTERANGE="<n>[@<o>]"
    let s = spec.trim();
    if let Some((len_s, off_s)) = s.split_once('@') {
        let length: u64 = len_s.trim().parse().ok()?;
        let offset: u64 = off_s.trim().parse().ok()?;
        if length > 0 {
            return Some(Task { offset, length });
        }
        None
    } else {
        let length: u64 = s.parse().ok()?;
        if length > 0 {
            Some(Task { offset: 0, length })
        } else {
            None
        }
    }
}

pub fn parse_playlist(text: &str, base: &str) -> PdmResult<HlsPlaylist> {
    if !text.lines().any(|l| l.trim().starts_with("#EXTM3U")) {
        return Err(PdmError::Hls("not an M3U8 playlist".into()));
    }
    let base_url = Url::parse(base).map_err(|e| PdmError::Hls(e.to_string()))?;
    let mut variants = Vec::new();
    let mut segments = Vec::new();
    let mut init_map: Option<HlsInitMap> = None;
    let mut encrypted = false;
    let mut drm = false;
    let mut pending_stream: Option<(u64, String, String)> = None;
    let mut pending_dur: Option<f64> = None;

    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() {
            continue;
        }
        if line.starts_with("#EXT-X-KEY") {
            let method = attr(line, "METHOD")
                .unwrap_or_default()
                .to_ascii_uppercase();
            // RFC 8216 §4.3.2.4: METHOD=NONE explicitly means segments are NOT encrypted.
            if !method.is_empty() && method != "NONE" {
                encrypted = true;
                if method.contains("SAMPLE-AES")
                    || line.contains("com.apple.streamingkeydelivery")
                    || line.contains("urn:uuid:")
                {
                    drm = true;
                }
            }
            continue;
        }
        if line.starts_with("#EXT-X-MAP") {
            if init_map.is_none() {
                if let Some(raw_uri) = attr(line, "URI").filter(|u| !u.is_empty()) {
                    let byte_range = attr(line, "BYTERANGE").and_then(|b| parse_byterange(&b));
                    init_map = Some(HlsInitMap {
                        uri: resolve(&base_url, &raw_uri),
                        byte_range,
                    });
                }
            }
            continue;
        }
        if line.starts_with("#EXT-X-STREAM-INF") {
            let bw = attr(line, "BANDWIDTH")
                .and_then(|s| s.parse().ok())
                .unwrap_or(0);
            let res = attr(line, "RESOLUTION").unwrap_or_default();
            let codecs = attr(line, "CODECS").unwrap_or_default();
            pending_stream = Some((bw, res, codecs));
            continue;
        }
        if line.starts_with("#EXTINF") {
            let dur = line
                .split(':')
                .nth(1)
                .and_then(|s| s.split(',').next())
                .and_then(|s| s.trim().parse().ok())
                .unwrap_or(0.0);
            pending_dur = Some(dur);
            continue;
        }
        if line.starts_with('#') {
            continue;
        }
        let uri = resolve(&base_url, line);
        if let Some((bandwidth, resolution, codecs)) = pending_stream.take() {
            variants.push(HlsVariantInfo {
                uri,
                bandwidth,
                resolution,
                codecs,
            });
        } else {
            segments.push(HlsSegment {
                uri,
                duration: pending_dur.take().unwrap_or(0.0),
            });
        }
    }

    Ok(HlsPlaylist {
        is_master: !variants.is_empty() && segments.is_empty(),
        variants,
        segments,
        init_map,
        encrypted,
        drm,
    })
}

fn attr(line: &str, key: &str) -> Option<String> {
    let k = format!("{}=", key);
    let rest = line.split(&k).nth(1)?;
    if rest.starts_with('"') {
        Some(rest[1..].split('"').next().unwrap_or("").to_string())
    } else {
        Some(rest.split(',').next().unwrap_or("").trim().to_string())
    }
}

fn resolve(base: &Url, href: &str) -> String {
    base.join(href)
        .map(|u| u.to_string())
        .unwrap_or_else(|_| href.to_string())
}

pub async fn fetch_text(
    url: &str,
    headers: &HashMap<String, String>,
    proxy: Option<&str>,
    pool: &NetworkPool,
    user_agent: &str,
) -> PdmResult<String> {
    let client = pool.get_client(proxy)?;
    let req = crate::headers::prepare_request(client.get(url), headers, user_agent, None);
    let resp = send_headers(req, None)
        .await
        .map_err(|e| PdmError::Network(e.to_string()))?;
    if !resp.status().is_success() {
        return Err(PdmError::Http(resp.status().as_u16()));
    }
    resp.text()
        .await
        .map_err(|e| PdmError::Network(e.to_string()))
}

fn reject_unplayable(plist: &HlsPlaylist) -> PdmResult<()> {
    if plist.drm {
        return Err(PdmError::Unsupported(
            "DRM-protected HLS is not supported".into(),
        ));
    }
    if plist.encrypted {
        return Err(PdmError::Unsupported(
            "AES-128 HLS is not supported in this version".into(),
        ));
    }
    if !plist.is_master && plist.segments.is_empty() {
        return Err(PdmError::Hls("media playlist has no segments".into()));
    }
    Ok(())
}

async fn resolve_media(
    media_url: &str,
    headers: &HashMap<String, String>,
    proxy: Option<&str>,
    pool: &NetworkPool,
    user_agent: &str,
) -> PdmResult<HlsPlaylist> {
    let text = fetch_text(media_url, headers, proxy, pool, user_agent).await?;
    let plist = parse_playlist(&text, media_url)?;
    reject_unplayable(&plist)?;
    if !plist.is_master {
        return Ok(plist);
    }
    let variant = plist
        .variants
        .iter()
        .max_by_key(|v| v.bandwidth)
        .ok_or_else(|| PdmError::Hls("master playlist has no variants".into()))?;
    let nested = fetch_text(&variant.uri, headers, proxy, pool, user_agent).await?;
    let media = parse_playlist(&nested, &variant.uri)?;
    reject_unplayable(&media)?;
    if media.segments.is_empty() {
        return Err(PdmError::Hls("media playlist has no segments".into()));
    }
    Ok(media)
}

pub struct HlsDownloader {
    pool: Arc<NetworkPool>,
    event_tx: mpsc::UnboundedSender<Event>,
}

impl HlsDownloader {
    pub fn new(pool: Arc<NetworkPool>, event_tx: mpsc::UnboundedSender<Event>) -> Self {
        Self { pool, event_tx }
    }

    pub async fn download(
        &self,
        cfg: &EngineConfig,
        limiter: Arc<MultiLimiter>,
        cancel: Arc<AtomicBool>,
        _on_resume: &crate::engine::OnResumeState,
    ) -> PdmResult<()> {
        log::info!("[ProxyDM] hls id={} url={}", cfg.id, cfg.url);
        if cancel.load(Ordering::Relaxed) {
            return Err(PdmError::Cancelled);
        }
        let proxy = if cfg.proxy_url.is_empty() {
            None
        } else {
            Some(cfg.proxy_url.as_str())
        };
        let media = resolve_media(
            &cfg.url,
            &cfg.headers,
            proxy,
            self.pool.as_ref(),
            &cfg.user_agent,
        )
        .await?;
        let total = media.segments.len() as u64;
        if total == 0 {
            return Err(PdmError::Hls("media playlist has no segments".into()));
        }

        let part_dir = crate::engine::file_io::hls_part_dir(cfg.id);
        std::fs::create_dir_all(&part_dir)?;

        // Separate internal `stop` from user `cancel` so a fatal segment failure
        // immediately stops sibling workers without masquerading as user pause.
        let stop = Arc::new(AtomicBool::new(false));
        let cancel_mirror = {
            let cancel = cancel.clone();
            let stop = stop.clone();
            tokio::spawn(async move {
                while !stop.load(Ordering::Relaxed) {
                    if cancel.load(Ordering::Relaxed) {
                        stop.store(true, Ordering::Relaxed);
                        break;
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                }
            })
        };

        let transfer = Arc::new(Transfer::new(
            self.pool.get_client(proxy)?,
            cfg,
            limiter,
            stop.clone(),
            Arc::new(PerfStats::new(cfg.id)),
        ));

        // Fetch fMP4 `#EXT-X-MAP` initialization segment first if present.
        let init_map = media.init_map.clone();
        if let Some(ref map) = init_map {
            let init_final = part_dir.join("init.part");
            let already_ok = std::fs::metadata(&init_final)
                .map(|m| m.len() > 0)
                .unwrap_or(false);
            if !already_ok {
                let init_tmp = part_dir.join("init.part.tmp");
                let file = std::fs::File::create(&init_tmp)?;
                let want = match map.byte_range.clone() {
                    Some(task) => Want::Range(task),
                    None => Want::Whole,
                };
                let fetched = transfer
                    .fetch(&map.uri, &file, want, 0, &|_, _| {}, &|_| {})
                    .await;
                drop(file);
                match fetched {
                    Fetched::Complete => {
                        std::fs::rename(&init_tmp, &init_final)?;
                    }
                    Fetched::Stopped => {
                        let _ = std::fs::remove_file(&init_tmp);
                        stop.store(true, Ordering::Relaxed);
                        let _ = cancel_mirror.await;
                        return Err(PdmError::Cancelled);
                    }
                    Fetched::RangeLost => {
                        let _ = std::fs::remove_file(&init_tmp);
                        stop.store(true, Ordering::Relaxed);
                        let _ = cancel_mirror.await;
                        return Err(PdmError::RangeLost);
                    }
                    Fetched::Failed(err) => {
                        let _ = std::fs::remove_file(&init_tmp);
                        stop.store(true, Ordering::Relaxed);
                        let _ = cancel_mirror.await;
                        return Err(err);
                    }
                }
            }
        }

        // Auto uses 8 sliding-window workers for HLS (`total_size` is 0 for M3U8,
        // so `auto_connections(0)` was previously stuck at 4 lockstep barriers).
        let conns = (if cfg.connections == 0 {
            8
        } else {
            cfg.connections
        })
        .clamp(1, 16)
        .min(media.segments.len() as u32) as usize;

        let segments = Arc::new(media.segments);
        let cursor = Arc::new(AtomicUsize::new(0));
        let done_count = Arc::new(AtomicU64::new(0));
        let abort_err: Arc<Mutex<Option<PdmError>>> = Arc::new(Mutex::new(None));
        let mut workers = Vec::with_capacity(conns);

        for _ in 0..conns {
            let segments = segments.clone();
            let cursor = cursor.clone();
            let done_count = done_count.clone();
            let part_dir = part_dir.clone();
            let transfer = transfer.clone();
            let stop = stop.clone();
            let abort_err = abort_err.clone();
            let event_tx = self.event_tx.clone();
            let download_id = cfg.id;

            workers.push(tokio::spawn(async move {
                loop {
                    if stop.load(Ordering::Relaxed) {
                        return;
                    }
                    let i = cursor.fetch_add(1, Ordering::Relaxed);
                    if i >= segments.len() {
                        return;
                    }
                    let final_path = part_dir.join(format!("{i:06}.part"));
                    let cached = std::fs::metadata(&final_path)
                        .map(|m| m.len() > 0)
                        .unwrap_or(false);
                    if !cached {
                        let tmp_path = part_dir.join(format!("{i:06}.part.tmp"));
                        let file = match std::fs::File::create(&tmp_path) {
                            Ok(f) => f,
                            Err(e) => {
                                if let Ok(mut g) = abort_err.lock() {
                                    g.get_or_insert(PdmError::Io(e.to_string()));
                                }
                                stop.store(true, Ordering::Relaxed);
                                return;
                            }
                        };
                        let outcome = transfer
                            .fetch(&segments[i].uri, &file, Want::Whole, 0, &|_, _| {}, &|_| {})
                            .await;
                        drop(file);
                        match outcome {
                            Fetched::Complete => {
                                if let Err(e) = std::fs::rename(&tmp_path, &final_path) {
                                    let _ = std::fs::remove_file(&tmp_path);
                                    if let Ok(mut g) = abort_err.lock() {
                                        g.get_or_insert(PdmError::Io(e.to_string()));
                                    }
                                    stop.store(true, Ordering::Relaxed);
                                    return;
                                }
                            }
                            Fetched::Stopped => {
                                let _ = std::fs::remove_file(&tmp_path);
                                return;
                            }
                            Fetched::RangeLost => {
                                let _ = std::fs::remove_file(&tmp_path);
                                if let Ok(mut g) = abort_err.lock() {
                                    g.get_or_insert(PdmError::RangeLost);
                                }
                                stop.store(true, Ordering::Relaxed);
                                return;
                            }
                            Fetched::Failed(err) => {
                                let _ = std::fs::remove_file(&tmp_path);
                                if let Ok(mut g) = abort_err.lock() {
                                    g.get_or_insert(err);
                                }
                                stop.store(true, Ordering::Relaxed);
                                return;
                            }
                        }
                    }

                    let done = done_count.fetch_add(1, Ordering::Relaxed) + 1;
                    let _ = event_tx.send(Event {
                        kind: EventKind::SegmentProgress {
                            done,
                            total,
                            phase: Phase::Downloading,
                        },
                        download_id,
                    });
                }
            }));
        }

        for w in workers {
            let _ = w.await;
        }
        stop.store(true, Ordering::Relaxed);
        let _ = cancel_mirror.await;

        if cancel.load(Ordering::Relaxed) {
            return Err(PdmError::Cancelled);
        }
        if let Some(err) = abort_err.lock().ok().and_then(|mut g| g.take()) {
            return Err(err);
        }

        let _ = self.event_tx.send(Event {
            kind: EventKind::SegmentProgress {
                done: total,
                total,
                phase: Phase::Merging,
            },
            download_id: cfg.id,
        });

        crate::engine::file_io::migrate_legacy_temp(cfg.id, &cfg.save_path);
        let pdm = crate::engine::file_io::temp_path(cfg.id);
        let seg_count = segments.len();
        let part_dir_merge = part_dir.clone();
        tokio::task::spawn_blocking(move || -> PdmResult<()> {
            if let Some(parent) = std::path::Path::new(&pdm).parent() {
                std::fs::create_dir_all(parent)?;
            }
            let out_file = std::fs::File::create(&pdm)?;
            let mut writer = std::io::BufWriter::with_capacity(512 * 1024, out_file);
            if let Some(map) = init_map {
                let init_path = part_dir_merge.join("init.part");
                let mut init_file = std::fs::File::open(&init_path)?;
                if let Some(range) = map.byte_range {
                    init_file.seek(SeekFrom::Start(range.offset))?;
                    let mut slice = init_file.take(range.length);
                    std::io::copy(&mut slice, &mut writer)?;
                } else {
                    std::io::copy(&mut init_file, &mut writer)?;
                }
            }
            for i in 0..seg_count {
                let path = part_dir_merge.join(format!("{i:06}.part"));
                let mut part = std::fs::File::open(&path)?;
                std::io::copy(&mut part, &mut writer)?;
                drop(part);
                let _ = std::fs::remove_file(&path);
            }
            writer.flush()?;
            let file = writer
                .into_inner()
                .map_err(|e| PdmError::Io(e.error().to_string()))?;
            file.sync_all()?;
            let _ = std::fs::remove_dir_all(&part_dir_merge);
            Ok(())
        })
        .await
        .map_err(|e| PdmError::Hls(e.to_string()))??;

        crate::engine::file_io::finalize_file(cfg.id, &cfg.save_path)
            .await
            .map_err(PdmError::Io)?;

        log::info!("[ProxyDM] hls id={} done segments={total}", cfg.id);
        let _ = self.event_tx.send(Event {
            kind: EventKind::DownloadCompleted,
            download_id: cfg.id,
        });
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_master() {
        let text = r#"#EXTM3U
#EXT-X-STREAM-INF:BANDWIDTH=800000,RESOLUTION=640x360,CODECS="avc1"
low.m3u8
#EXT-X-STREAM-INF:BANDWIDTH=1400000,RESOLUTION=1280x720
high.m3u8
"#;
        let p = parse_playlist(text, "https://cdn.example/master.m3u8").unwrap();
        assert!(p.is_master);
        assert_eq!(p.variants.len(), 2);
        assert!(p.variants[1].uri.ends_with("high.m3u8"));
        assert_eq!(p.variants[1].bandwidth, 1_400_000);
    }

    #[test]
    fn parse_media() {
        let text = r#"#EXTM3U
#EXT-X-TARGETDURATION:10
#EXTINF:9.0,
seg0.ts
#EXTINF:9.0,
seg1.ts
#EXT-X-ENDLIST
"#;
        let p = parse_playlist(text, "https://cdn.example/media.m3u8").unwrap();
        assert!(!p.is_master);
        assert_eq!(p.segments.len(), 2);
        assert!(p.segments[0].uri.ends_with("seg0.ts"));
    }

    #[test]
    fn drm_detected() {
        let text = r#"#EXTM3U
#EXT-X-KEY:METHOD=SAMPLE-AES,URI="skd://x",KEYFORMAT="com.apple.streamingkeydelivery"
#EXTINF:1.0,
a.ts
"#;
        let p = parse_playlist(text, "https://cdn.example/a.m3u8").unwrap();
        assert!(p.drm);
        assert!(p.encrypted);
    }

    #[test]
    fn key_method_none_is_not_encrypted_and_ext_x_map_parses() {
        let text = r#"#EXTM3U
#EXT-X-KEY:METHOD=NONE
#EXT-X-MAP:URI="init.mp4",BYTERANGE="720@0"
#EXTINF:4.0,
seg0.m4s
#EXTINF:4.0,
seg1.m4s
#EXT-X-ENDLIST
"#;
        let p = parse_playlist(text, "https://cdn.example/live/stream.m3u8").unwrap();
        assert!(!p.encrypted, "METHOD=NONE must not be treated as encrypted");
        assert!(!p.drm);
        let map = p.init_map.expect("EXT-X-MAP parsed");
        assert_eq!(map.uri, "https://cdn.example/live/init.mp4");
        assert_eq!(
            map.byte_range,
            Some(Task {
                offset: 0,
                length: 720
            })
        );
        assert_eq!(p.segments.len(), 2);
    }
}
