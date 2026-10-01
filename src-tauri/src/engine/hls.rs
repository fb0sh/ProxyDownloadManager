use crate::engine::transfer::{send_headers, Fetched, Transfer, Want};
use crate::network::limiter::MultiLimiter;
use crate::network::pool::NetworkPool;
use crate::network::protocol::PerfStats;
use crate::types::{EngineConfig, Event, EventKind, HlsVariantInfo, PdmError, PdmResult, Phase};
use std::collections::HashMap;
use std::io::Write;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::mpsc;
use url::Url;

#[derive(Debug, Clone)]
pub struct HlsPlaylist {
    pub is_master: bool,
    pub variants: Vec<HlsVariantInfo>,
    pub segments: Vec<HlsSegment>,
    pub encrypted: bool,
    pub drm: bool,
}

#[derive(Debug, Clone)]
pub struct HlsSegment {
    pub uri: String,
    pub duration: f64,
}

pub fn parse_playlist(text: &str, base: &str) -> PdmResult<HlsPlaylist> {
    if !text.lines().any(|l| l.trim().starts_with("#EXTM3U")) {
        return Err(PdmError::Hls("not an M3U8 playlist".into()));
    }
    let base_url = Url::parse(base).map_err(|e| PdmError::Hls(e.to_string()))?;
    let mut variants = Vec::new();
    let mut segments = Vec::new();
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
            encrypted = true;
            if line.contains("SAMPLE-AES") || line.contains("com.apple.streamingkeydelivery") {
                drm = true;
            }
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
        let transfer = Arc::new(Transfer::new(
            self.pool.get_client(proxy)?,
            cfg,
            limiter,
            cancel.clone(),
            Arc::new(PerfStats::new(cfg.id)),
        ));
        let conns = (if cfg.connections == 0 {
            crate::engine::chunk::auto_connections(cfg.total_size)
        } else {
            cfg.connections
        })
        .clamp(1, 16) as usize;
        let mut next = 0usize;
        while next < media.segments.len() {
            if cancel.load(Ordering::Relaxed) {
                return Err(PdmError::Cancelled);
            }
            let end = (next + conns).min(media.segments.len());
            let mut joins = Vec::new();
            for i in next..end {
                let uri = media.segments[i].uri.clone();
                let path = part_dir.join(format!("{i:06}.part"));
                let transfer = transfer.clone();
                joins.push(tokio::spawn(async move {
                    let file = std::fs::File::create(&path)?;
                    match transfer
                        .fetch(&uri, &file, Want::Whole, 0, &|_, _| {}, &|_| {})
                        .await
                    {
                        Fetched::Complete => Ok(()),
                        Fetched::Stopped => Err(PdmError::Cancelled),
                        Fetched::RangeLost => Err(PdmError::RangeLost),
                        Fetched::Failed(error) => Err(error),
                    }
                }));
            }
            for join in joins {
                let result: PdmResult<()> = join.await.map_err(|e| PdmError::Hls(e.to_string()))?;
                if let Err(error) = result {
                    if cancel.load(Ordering::Relaxed) {
                        return Err(PdmError::Cancelled);
                    }
                    return Err(error);
                }
            }
            next = end;
            let _ = self.event_tx.send(Event {
                kind: EventKind::SegmentProgress {
                    done: next as u64,
                    total,
                    phase: Phase::Downloading,
                },
                download_id: cfg.id,
            });
        }

        if cancel.load(Ordering::Relaxed) {
            return Err(PdmError::Cancelled);
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
        if let Some(parent) = std::path::Path::new(&pdm).parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut file = std::fs::File::create(&pdm)?;
        for i in 0..media.segments.len() {
            let path = part_dir.join(format!("{i:06}.part"));
            let mut part = std::fs::File::open(&path)?;
            std::io::copy(&mut part, &mut file)?;
            drop(part);
            let _ = std::fs::remove_file(&path);
        }
        file.flush()?;
        file.sync_all()?;
        drop(file);
        let _ = std::fs::remove_dir_all(&part_dir);
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
}
