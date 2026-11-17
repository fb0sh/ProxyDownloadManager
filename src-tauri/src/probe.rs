use crate::network::pool::NetworkPool;
use crate::types::{PdmError, PdmResult};
use std::collections::HashMap;
use std::error::Error;
use std::sync::Mutex;
use std::time::{Duration, Instant};

#[derive(Debug, Clone)]
pub struct ProbeResult {
    pub supports_range: bool,
    pub file_size: u64,
    pub file_name: String,
    pub content_type: String,
    pub etag: String,
    pub last_modified: String,
    pub final_url: String,
    pub is_hls: bool,
}

fn effective_user_agents(headers: &HashMap<String, String>, user_agents: &[String]) -> Vec<String> {
    // When the caller (e.g. browser extension) already captured the browser's exact
    // User-Agent in `headers`, `apply_headers` pins it on every request. Looping 5
    // fallback UAs would replay the exact same wire request 5 times.
    let explicit_ua = headers
        .iter()
        .any(|(k, v)| k.eq_ignore_ascii_case("user-agent") && !v.is_empty());
    if explicit_ua {
        return vec![String::new()];
    }
    let mut out: Vec<String> = Vec::with_capacity(user_agents.len() + 1);
    for ua in user_agents.iter().chain(std::iter::once(&String::new())) {
        if !out.contains(ua) {
            out.push(ua.clone());
        }
    }
    out
}

fn parse_empty_416_total(resp: &reqwest::Response) -> Option<u64> {
    resp.headers()
        .get("content-range")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.split('/').nth(1))
        .and_then(|t| t.trim().parse::<u64>().ok())
        .filter(|&total| total == 0)
}

pub async fn probe(
    url: &str,
    headers: &HashMap<String, String>,
    proxy: Option<&str>,
    pool: &NetworkPool,
    user_agents: &[String],
) -> PdmResult<ProbeResult> {
    let client = pool
        .get_client(proxy)
        .map_err(|e| PdmError::ClientBuild(e.to_string()))?;
    let candidates = effective_user_agents(headers, user_agents);
    log::info!(
        "[ProxyDM] probe start url={} proxy={:?} uas={}",
        url,
        proxy,
        candidates.len()
    );

    // Try each candidate UA, return first success.
    let mut first_err: Option<String> = None;
    let mut fatal_http: Option<u16> = None;
    let total_candidates = candidates.len();

    for (i, ua) in candidates.iter().enumerate() {
        let is_last_ua = i + 1 >= total_candidates;
        log::info!(
            "[ProxyDM] probe attempt #{} ua_prefix={:?}...",
            i,
            &ua[..ua
                .char_indices()
                .nth(40)
                .map(|(i, _)| i)
                .unwrap_or(ua.len())]
        );
        // Try Range first to detect 206 support. Engine Range is applied last
        // so a replayed Range cannot be appended beside bytes=0-0.
        let mut range_req =
            crate::headers::prepare_request(client.get(url), headers, ua, Some("bytes=0-0"));
        range_req = range_req.timeout(std::time::Duration::from_secs(30));

        let resp = match range_req.send().await {
            Ok(r) => r,
            Err(e) => {
                if first_err.is_none() {
                    let mut msg = format!("{}", e);
                    let mut src = e.source();
                    while let Some(s) = src {
                        msg.push_str(&format!(": {}", s));
                        src = s.source();
                    }
                    first_err = Some(msg);
                }
                continue;
            }
        };

        let initial_status = resp.status();
        let (resp, supports_range, file_size) = if initial_status
            == reqwest::StatusCode::PARTIAL_CONTENT
        {
            let size = resp
                .headers()
                .get("content-range")
                .and_then(|v| v.to_str().ok())
                .and_then(|s| {
                    s.split('/')
                        .nth(1)
                        .and_then(|n| n.trim().parse::<u64>().ok())
                })
                .unwrap_or(0);
            (resp, true, size)
        } else if initial_status == reqwest::StatusCode::OK {
            let size = resp
                .headers()
                .get("content-length")
                .and_then(|v| v.to_str().ok())
                .and_then(|s| s.parse::<u64>().ok())
                .unwrap_or(0);
            (resp, false, size)
        } else if initial_status == reqwest::StatusCode::RANGE_NOT_SATISFIABLE
            && parse_empty_416_total(&resp).is_some()
        {
            // `416` with `Content-Range: bytes */0` is a legitimate 0-byte file.
            (resp, false, 0)
        } else if matches!(initial_status.as_u16(), 400 | 403 | 405 | 416 | 501) {
            // Gateways / presigned endpoints that reject `Range: bytes=0-0`: retry
            // without `Range` and promote `r2` so metadata comes from the 200 OK.
            let mut get_req = crate::headers::prepare_request(client.get(url), headers, ua, None);
            get_req = get_req.timeout(std::time::Duration::from_secs(30));
            match get_req.send().await {
                Ok(r2) if r2.status().is_success() => {
                    let size = r2
                        .headers()
                        .get("content-length")
                        .and_then(|v| v.to_str().ok())
                        .and_then(|s| s.parse::<u64>().ok())
                        .unwrap_or(0);
                    (r2, false, size)
                }
                Ok(r2) => {
                    let code = r2.status().as_u16();
                    // A non-browser default UA often gets 403 from WAFs; keep trying
                    // browser fallback UAs before declaring 403 fatal.
                    if code == 403 && !is_last_ua {
                        fatal_http.get_or_insert(code);
                        continue;
                    }
                    if crate::retry::is_fatal_client_status(code) {
                        return Err(PdmError::Http(code));
                    }
                    if first_err.is_none() {
                        first_err = Some(format!("HTTP {}", r2.status()));
                    }
                    continue;
                }
                Err(e) => {
                    if first_err.is_none() {
                        first_err = Some(e.to_string());
                    }
                    continue;
                }
            }
        } else if crate::retry::is_fatal_client_status(initial_status.as_u16()) {
            return Err(PdmError::Http(initial_status.as_u16()));
        } else {
            if !is_last_ua {
                if first_err.is_none() {
                    first_err = Some(format!("HTTP {}", initial_status));
                }
                continue;
            }
            return Err(PdmError::Probe(format!("HTTP {}", initial_status)));
        };

        let status = resp.status();
        let protocol = crate::network::protocol::protocol_label(resp.version());

        // Every field is extracted before the body is touched: draining moves
        // `resp`, and the metadata must survive that.
        let content_type = resp
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_string();
        let etag = resp
            .headers()
            .get("etag")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_string();
        let last_modified = resp
            .headers()
            .get("last-modified")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_string();
        let cd_owned = resp
            .headers()
            .get("content-disposition")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string());
        let final_url = resp.url().to_string();
        let ct_lower = content_type.to_ascii_lowercase();
        let is_hls = ct_lower.contains("mpegurl")
            || final_url.to_ascii_lowercase().contains(".m3u8")
            || url.to_ascii_lowercase().contains(".m3u8");
        let content_length = resp
            .headers()
            .get(reqwest::header::CONTENT_LENGTH)
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.parse::<u64>().ok());

        // Prefer Content-Disposition, then redirected `final_url` path, then original `url`.
        let file_name = cd_owned
            .as_deref()
            .and_then(crate::filename::from_content_disposition)
            .or_else(|| crate::filename::from_url(&final_url))
            .or_else(|| crate::filename::from_url(url))
            .unwrap_or_else(|| "download".to_string());

        log::info!(
            "[ProxyDM] probe SUCCESS ua#{} range={} size={} name={}",
            i,
            supports_range,
            file_size,
            file_name
        );
        log::debug!(
            "[net] probe protocol={} status={} range={} content_length={:?}",
            protocol,
            status.as_u16(),
            supports_range,
            content_length
        );

        // A `Range: bytes=0-0` 206 carries a tiny body (usually 1 byte).
        // Consuming it releases the socket back to reqwest's idle pool, so the
        // download that follows can reuse the warm TCP/TLS connection instead
        // of paying for a new handshake. Only a *known, small* 206 body is ever
        // read: a server that ignores Range and answers 200 with a multi-GB
        // Content-Length is dropped, never drained.
        if supports_range {
            match content_length {
                Some(n) if n <= MAX_DRAIN_BYTES => {
                    let drained = resp.bytes().await.map(|b| b.len()).unwrap_or(0);
                    log::debug!(
                        "[net] probe drained {} byte(s) for connection reuse",
                        drained
                    );
                }
                _ => {
                    log::debug!("[net] probe body not drained (unknown or large 206)");
                }
            }
        }

        return Ok(ProbeResult {
            supports_range,
            file_size,
            file_name,
            content_type,
            etag,
            last_modified,
            final_url,
            is_hls,
        });
    }

    if let Some(code) = fatal_http {
        return Err(PdmError::Http(code));
    }

    let err_msg = match first_err {
        Some(ref e) => format!("All probe attempts failed (first error: {})", e),
        None => "All probe attempts failed".to_string(),
    };
    log::error!("[ProxyDM] probe FAILED: {}", err_msg);
    Err(PdmError::Probe(err_msg))
}

/// Probe result with filename override applied.
#[derive(Default)]
pub struct ProbeOutcome {
    pub file_name: String,
    pub file_size: u64,
    pub supports_range: bool,
    pub content_type: String,
    pub etag: String,
    pub last_modified: String,
    pub final_url: String,
    pub is_hls: bool,
}

/// Probe with a 3s budget. If that misses, retry through `fallback_proxy`
/// (typically the configured default proxy, or Direct).
pub async fn probe_then_default_proxy(
    url: &str,
    headers: &HashMap<String, String>,
    primary_proxy: Option<&str>,
    fallback_proxy: Option<&str>,
    pool: &NetworkPool,
    user_agents: &[String],
) -> PdmResult<ProbeResult> {
    let first = tokio::time::timeout(
        std::time::Duration::from_secs(3),
        probe(url, headers, primary_proxy, pool, user_agents),
    )
    .await;
    match first {
        Ok(Ok(result)) => Ok(result),
        other => {
            let same = primary_proxy == fallback_proxy;
            if same {
                return match other {
                    Ok(r) => r,
                    Err(_) => probe(url, headers, primary_proxy, pool, user_agents).await,
                };
            }
            log::info!(
                "[ProxyDM] probe missed 3s budget or failed; retrying via fallback proxy={:?}",
                fallback_proxy
            );
            probe(url, headers, fallback_proxy, pool, user_agents).await
        }
    }
}

const PROBE_TTL: Duration = Duration::from_secs(20);

/// Largest probe response body that may be drained to return a connection to
/// the pool. A `bytes=0-0` 206 is 1 byte; this only bounds a misbehaving
/// server. Anything larger is dropped, never read into memory.
const MAX_DRAIN_BYTES: u64 = 64 * 1024;

struct ProbeCacheEntry {
    at: Instant,
    result: ProbeResult,
}

/// Short-lived probe results so New Download and Start do not hit the network twice.
pub struct ProbeCache {
    inner: Mutex<HashMap<String, ProbeCacheEntry>>,
}

impl ProbeCache {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(HashMap::new()),
        }
    }

    pub fn get(&self, key: &str) -> Option<ProbeResult> {
        let mut map = self.inner.lock().ok()?;
        let fresh = map
            .get(key)
            .is_some_and(|entry| entry.at.elapsed() < PROBE_TTL);
        if fresh {
            return map.get(key).map(|entry| entry.result.clone());
        }
        map.remove(key);
        None
    }

    pub fn put(&self, key: String, result: ProbeResult) {
        if let Ok(mut map) = self.inner.lock() {
            map.retain(|_, entry| entry.at.elapsed() < PROBE_TTL);
            map.insert(
                key,
                ProbeCacheEntry {
                    at: Instant::now(),
                    result,
                },
            );
        }
    }
}

fn header_value(headers: &HashMap<String, String>, names: &[&str]) -> String {
    for name in names {
        if let Some((_, value)) = headers
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
        {
            return value.clone();
        }
    }
    String::new()
}

/// Identity of a probe. A different URL, proxy, user-agent, or auth header misses.
pub fn probe_cache_key(
    url: &str,
    proxy: Option<&str>,
    headers: &HashMap<String, String>,
    user_agents: &[String],
) -> String {
    let ua = user_agents.first().map(String::as_str).unwrap_or("");
    format!(
        "{url}\n{}\n{ua}\n{}\n{}\n{}\n{}",
        proxy.unwrap_or(""),
        header_value(headers, &["cookie"]),
        header_value(headers, &["authorization"]),
        header_value(headers, &["referer", "referrer"]),
        header_value(headers, &["origin"]),
    )
}

/// Return a cached probe, or run one and store it. The key is the primary proxy,
/// so a result taken through proxy A is not reused for proxy B.
pub async fn cached_probe(
    cache: &ProbeCache,
    url: &str,
    headers: &HashMap<String, String>,
    primary_proxy: Option<&str>,
    fallback_proxy: Option<&str>,
    pool: &NetworkPool,
    user_agents: &[String],
) -> PdmResult<ProbeResult> {
    let key = probe_cache_key(url, primary_proxy, headers, user_agents);
    if let Some(hit) = cache.get(&key) {
        log::debug!("[startup] probe-cache=hit");
        return Ok(hit);
    }
    log::debug!("[startup] probe-cache=miss");
    let result = probe_then_default_proxy(
        url,
        headers,
        primary_proxy,
        fallback_proxy,
        pool,
        user_agents,
    )
    .await?;
    cache.put(key, result.clone());
    Ok(result)
}

/// Probe with fallback: on failure, derive filename from URL.
pub async fn probe_with_fallback(
    url: &str,
    headers: &HashMap<String, String>,
    proxy_url: Option<&str>,
    pool: &std::sync::Arc<NetworkPool>,
    user_agents: &[String],
    filename_override: &str,
) -> ProbeOutcome {
    let result = probe(url, headers, proxy_url, pool, user_agents).await;

    match result {
        Ok(r) => {
            let name = if filename_override.is_empty() {
                r.file_name
            } else {
                filename_override.to_string()
            };
            ProbeOutcome {
                file_name: crate::filename::sanitize(&name),
                file_size: r.file_size,
                supports_range: r.supports_range,
                content_type: r.content_type,
                etag: r.etag,
                last_modified: r.last_modified,
                final_url: r.final_url,
                is_hls: r.is_hls,
            }
        }
        Err(_e) => {
            let name = if filename_override.is_empty() {
                crate::filename::from_url(url).unwrap_or_else(|| "download".to_string())
            } else {
                filename_override.to_string()
            };
            ProbeOutcome {
                file_name: crate::filename::sanitize(&name),
                file_size: 0,
                supports_range: false,
                ..Default::default()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    /// Spawn a minimal HTTP server that responds with the given status, content-length,
    /// and optional Content-Disposition header. Returns the base URL.
    async fn spawn_mock_server(
        status_line: &str,
        content_length: u64,
        content_disposition: Option<&str>,
    ) -> String {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        // Own the strings before moving into the spawned task
        let status_line = status_line.to_string();
        let cd = content_disposition.map(|s| s.to_string());

        tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            // Read the full request (drain until we see the end-of-headers marker)
            let mut buf = vec![0u8; 4096];
            let mut total = Vec::new();
            loop {
                let n = stream.read(&mut buf).await.unwrap_or(0);
                if n == 0 {
                    break;
                }
                total.extend_from_slice(&buf[..n]);
                if total.windows(4).any(|w| w == b"\r\n\r\n") {
                    break;
                }
            }

            let mut resp = format!("{}\r\nContent-Length: {}\r\n", status_line, content_length);
            if let Some(ref header) = cd {
                resp.push_str(&format!("Content-Disposition: {}\r\n", header));
            }
            resp.push_str("\r\n");
            let _ = stream.write_all(resp.as_bytes()).await;
        });

        format!("http://{}", addr)
    }

    // ── ProbeOutcome construction ──────────────────────────────────────────

    #[test]
    fn probe_outcome_fields_are_set_correctly() {
        let outcome = ProbeOutcome {
            file_name: "report.pdf".to_string(),
            file_size: 4096,
            supports_range: true,
            ..Default::default()
        };
        assert_eq!(outcome.file_name, "report.pdf");
        assert_eq!(outcome.file_size, 4096);
        assert!(outcome.supports_range);
    }

    #[test]
    fn probe_outcome_defaults_for_failed_probe() {
        let outcome = ProbeOutcome {
            file_name: "download".to_string(),
            file_size: 0,
            supports_range: false,
            ..Default::default()
        };
        assert_eq!(outcome.file_name, "download");
        assert_eq!(outcome.file_size, 0);
        assert!(!outcome.supports_range);
    }

    // ── probe_with_fallback: filename override on success ──────────────────

    #[tokio::test]
    async fn fallback_override_used_as_is_on_success() {
        let url = spawn_mock_server(
            "HTTP/1.1 200 OK",
            1024,
            Some("attachment; filename=\"server_name.dat\""),
        )
        .await;

        let pool = Arc::new(NetworkPool::new(false));
        let headers = HashMap::new();
        let uas = vec!["TestUA/1.0".to_string()];

        let outcome =
            probe_with_fallback(&url, &headers, None, &pool, &uas, "my_override.txt").await;

        assert_eq!(outcome.file_name, "my_override.txt");
        assert_eq!(outcome.file_size, 1024);
        assert!(!outcome.supports_range);
    }

    // ── probe_with_fallback: error fallback with override ──────────────────

    #[tokio::test]
    async fn fallback_override_used_as_is_on_error() {
        // Port 1 is almost certainly not listening → connection refused → probe error
        let url = "http://127.0.0.1:1/path/file.pdf";
        let pool = Arc::new(NetworkPool::new(false));
        let headers = HashMap::new();
        let uas = vec!["TestUA/1.0".to_string()];

        let outcome =
            probe_with_fallback(url, &headers, None, &pool, &uas, "forced_name.zip").await;

        assert_eq!(outcome.file_name, "forced_name.zip");
        assert_eq!(outcome.file_size, 0);
        assert!(!outcome.supports_range);
    }

    // ── probe_with_fallback: error fallback derives name from URL ──────────

    #[tokio::test]
    async fn fallback_empty_override_on_error_uses_from_url() {
        let url = "http://127.0.0.1:1/some/deep/archive.tar.gz";
        let pool = Arc::new(NetworkPool::new(false));
        let headers = HashMap::new();
        let uas = vec!["TestUA/1.0".to_string()];

        let outcome = probe_with_fallback(url, &headers, None, &pool, &uas, "").await;

        let expected = crate::filename::from_url(url).unwrap_or_else(|| "download".to_string());
        assert_eq!(outcome.file_name, expected);
        assert_eq!(outcome.file_size, 0);
        assert!(!outcome.supports_range);
    }

    #[tokio::test]
    async fn fallback_empty_override_on_error_no_extension_uses_download() {
        // URL with no recognizable filename in path → from_url may return None → "download"
        let url = "http://127.0.0.1:1/a";
        let pool = Arc::new(NetworkPool::new(false));
        let headers = HashMap::new();
        let uas = vec!["TestUA/1.0".to_string()];

        let outcome = probe_with_fallback(url, &headers, None, &pool, &uas, "").await;

        let expected = crate::filename::from_url(url).unwrap_or_else(|| "download".to_string());
        assert_eq!(outcome.file_name, expected);
        assert_eq!(outcome.file_size, 0);
        assert!(!outcome.supports_range);
    }

    // ── probe_with_fallback: empty override on success uses probe name ─────

    #[tokio::test]
    async fn fallback_empty_override_on_success_uses_probe_filename() {
        let url = spawn_mock_server(
            "HTTP/1.1 200 OK",
            2048,
            Some("attachment; filename=\"from_header.xlsx\""),
        )
        .await;

        let pool = Arc::new(NetworkPool::new(false));
        let headers = HashMap::new();
        let uas = vec!["TestUA/1.0".to_string()];

        let outcome = probe_with_fallback(&url, &headers, None, &pool, &uas, "").await;

        assert_eq!(outcome.file_name, "from_header.xlsx");
        assert_eq!(outcome.file_size, 2048);
        assert!(!outcome.supports_range);
    }

    #[tokio::test]
    async fn fallback_empty_override_on_success_uses_url_when_no_cd() {
        // No Content-Disposition header → filename derived from URL path via extract_filename
        let url = spawn_mock_server("HTTP/1.1 200 OK", 512, None).await;

        let pool = Arc::new(NetworkPool::new(false));
        let headers = HashMap::new();
        let uas = vec!["TestUA/1.0".to_string()];

        let outcome = probe_with_fallback(&url, &headers, None, &pool, &uas, "").await;

        // No CD header → extract_filename falls back to from_url(url),
        // then sanitized (the mock URL's host:port contains a ':').
        let expected = crate::filename::sanitize(
            &crate::filename::extract_filename(&url, None)
                .unwrap_or_else(|| "download".to_string()),
        );
        assert_eq!(outcome.file_name, expected);
        assert_eq!(outcome.file_size, 512);
    }

    async fn spawn_guarded_server(need_header: &str, need_value: &str) -> String {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let need_header = need_header.to_string();
        let need_value = need_value.to_string();
        tokio::spawn(async move {
            loop {
                let (mut stream, _) = match listener.accept().await {
                    Ok(c) => c,
                    Err(_) => return,
                };
                let mut buf = vec![0u8; 4096];
                let mut total = Vec::new();
                loop {
                    let n = stream.read(&mut buf).await.unwrap_or(0);
                    if n == 0 {
                        break;
                    }
                    total.extend_from_slice(&buf[..n]);
                    if total.windows(4).any(|w| w == b"\r\n\r\n") {
                        break;
                    }
                }
                let req = String::from_utf8_lossy(&total);
                let ok = req.lines().any(|l| {
                    l.to_ascii_lowercase()
                        .starts_with(&need_header.to_ascii_lowercase())
                        && l.contains(&need_value)
                });
                let body = if ok {
                    "HTTP/1.1 206 Partial Content\r\nContent-Range: bytes 0-0/2048\r\nContent-Length: 1\r\nContent-Disposition: attachment; filename=secret.bin\r\n\r\nX"
                } else {
                    "HTTP/1.1 403 Forbidden\r\nContent-Length: 0\r\n\r\n"
                };
                let _ = stream.write_all(body.as_bytes()).await;
            }
        });
        format!("http://{}/secret.bin", addr)
    }

    #[tokio::test]
    async fn probe_cookie_protected() {
        let url = spawn_guarded_server("Cookie:", "sid=abc").await;
        let pool = Arc::new(NetworkPool::new(false));
        let mut headers = HashMap::new();
        headers.insert("Cookie".into(), "sid=abc".into());
        let r = probe(&url, &headers, None, &pool, &["ua".into()])
            .await
            .unwrap();
        assert_eq!(r.file_size, 2048);
        assert!(r.supports_range);
    }

    #[tokio::test]
    async fn probe_referer_protected() {
        let url = spawn_guarded_server("Referer:", "https://app.example/").await;
        let pool = Arc::new(NetworkPool::new(false));
        let mut headers = HashMap::new();
        headers.insert("Referer".into(), "https://app.example/".into());
        let r = probe(&url, &headers, None, &pool, &["ua".into()])
            .await
            .unwrap();
        assert_eq!(r.file_name, "secret.bin");
    }

    #[tokio::test]
    async fn probe_authorization_protected() {
        let url = spawn_guarded_server("Authorization:", "Bearer tok").await;
        let pool = Arc::new(NetworkPool::new(false));
        let mut headers = HashMap::new();
        headers.insert("Authorization".into(), "Bearer tok".into());
        let r = probe(&url, &headers, None, &pool, &["ua".into()])
            .await
            .unwrap();
        assert!(r.supports_range);
    }

    #[tokio::test]
    async fn probe_missing_auth_is_http_error() {
        let url = spawn_guarded_server("Authorization:", "Bearer tok").await;
        let pool = Arc::new(NetworkPool::new(false));
        let headers = HashMap::new();
        let err = probe(&url, &headers, None, &pool, &["ua".into()])
            .await
            .unwrap_err();
        assert!(matches!(err, PdmError::Http(403) | PdmError::Probe(_)));
    }

    #[test]
    fn probe_cache_key_changes_with_proxy_and_auth_headers() {
        let uas = vec!["ua".to_string()];
        let bare = HashMap::new();
        let direct = probe_cache_key("http://example/a.bin", None, &bare, &uas);
        let proxied = probe_cache_key(
            "http://example/a.bin",
            Some("http://127.0.0.1:9"),
            &bare,
            &uas,
        );
        assert_ne!(direct, proxied);
        for (name, value) in [
            ("Cookie", "sid=1"),
            ("Authorization", "Bearer t"),
            ("Referer", "https://from.example/"),
            ("Origin", "https://from.example"),
        ] {
            let mut headers = HashMap::new();
            headers.insert(name.to_string(), value.to_string());
            assert_ne!(
                direct,
                probe_cache_key("http://example/a.bin", None, &headers, &uas),
                "{name}"
            );
        }
    }

    #[tokio::test]
    async fn cached_probe_reuses_one_response() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let hits = Arc::new(AtomicUsize::new(0));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let count = hits.clone();
        tokio::spawn(async move {
            loop {
                let Ok((mut stream, _)) = listener.accept().await else {
                    return;
                };
                count.fetch_add(1, Ordering::Relaxed);
                tokio::spawn(async move {
                    let mut buf = [0u8; 2048];
                    let _ = stream.read(&mut buf).await;
                    let body = b"HTTP/1.1 206 Partial Content\r\nContent-Range: bytes 0-0/1000\r\nContent-Length: 1\r\nContent-Disposition: attachment; filename=a.bin\r\n\r\nX";
                    let _ = stream.write_all(body).await;
                });
            }
        });
        let url = format!("http://127.0.0.1:{port}/a.bin");
        let cache = ProbeCache::new();
        let pool = Arc::new(NetworkPool::new(false));
        let uas = vec!["ua".to_string()];
        let headers = HashMap::new();
        let first = cached_probe(&cache, &url, &headers, None, None, &pool, &uas)
            .await
            .unwrap();
        let second = cached_probe(&cache, &url, &headers, None, None, &pool, &uas)
            .await
            .unwrap();
        assert_eq!(first.file_size, 1000);
        assert_eq!(second.file_size, 1000);
        assert_eq!(
            hits.load(Ordering::Relaxed),
            1,
            "start reused a fresh probe"
        );
        let mut cookied = HashMap::new();
        cookied.insert("Cookie".into(), "sid=1".into());
        cached_probe(&cache, &url, &cookied, None, None, &pool, &uas)
            .await
            .unwrap();
        assert_eq!(
            hits.load(Ordering::Relaxed),
            2,
            "a cookied download reused a cookieless probe"
        );
    }

    #[tokio::test]
    async fn range_rejected_fallback_keeps_plain_get_metadata() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        tokio::spawn(async move {
            loop {
                let Ok((mut stream, _)) = listener.accept().await else {
                    return;
                };
                tokio::spawn(async move {
                    let mut buf = [0u8; 2048];
                    let n = stream.read(&mut buf).await.unwrap_or(0);
                    let req = String::from_utf8_lossy(&buf[..n]);
                    if req.to_ascii_lowercase().contains("range:") {
                        let _ = stream
                            .write_all(b"HTTP/1.1 405 Method Not Allowed\r\nContent-Type: text/plain\r\nContent-Length: 0\r\n\r\n")
                            .await;
                    } else {
                        let _ = stream
                            .write_all(b"HTTP/1.1 200 OK\r\nContent-Type: application/zip\r\nContent-Length: 4096\r\nContent-Disposition: attachment; filename*=UTF-8''%E8%B5%84%E6%96%99.zip\r\n\r\n")
                            .await;
                    }
                });
            }
        });
        let url = format!("http://127.0.0.1:{port}/download?id=9");
        let pool = Arc::new(NetworkPool::new(false));
        let res = probe(&url, &HashMap::new(), None, &pool, &["ua".into()])
            .await
            .unwrap();
        assert!(!res.supports_range);
        assert_eq!(res.file_size, 4096);
        assert_eq!(res.content_type, "application/zip");
        assert_eq!(res.file_name, "资料.zip");
    }

    #[tokio::test]
    async fn waf_403_on_default_ua_continues_to_browser_ua_and_explicit_ua_dedupes() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let hits = Arc::new(AtomicUsize::new(0));
        let count = hits.clone();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        tokio::spawn(async move {
            loop {
                let Ok((mut stream, _)) = listener.accept().await else {
                    return;
                };
                let count = count.clone();
                tokio::spawn(async move {
                    count.fetch_add(1, Ordering::Relaxed);
                    let mut buf = [0u8; 2048];
                    let n = stream.read(&mut buf).await.unwrap_or(0);
                    let req = String::from_utf8_lossy(&buf[..n]);
                    if req.contains("Mozilla/5.0") {
                        let _ = stream
                            .write_all(b"HTTP/1.1 206 Partial Content\r\nContent-Range: bytes 0-0/512\r\nContent-Length: 1\r\n\r\nA")
                            .await;
                    } else {
                        let _ = stream
                            .write_all(b"HTTP/1.1 403 Forbidden\r\nContent-Length: 0\r\n\r\n")
                            .await;
                    }
                });
            }
        });
        let url = format!("http://127.0.0.1:{port}/pkg.bin");
        let pool = Arc::new(NetworkPool::new(false));
        let uas = vec![
            "ProxyDM/1.0".to_string(),
            "Mozilla/5.0 Chrome/131.0".to_string(),
        ];
        let res = probe(&url, &HashMap::new(), None, &pool, &uas)
            .await
            .unwrap();
        assert_eq!(res.file_size, 512);
        assert!(res.supports_range);

        // Explicit User-Agent in headers must not loop through 5 fallback UAs.
        hits.store(0, Ordering::Relaxed);
        let mut hdrs = HashMap::new();
        hdrs.insert("User-Agent".into(), "CustomLockedUA/1.0".into());
        let err = probe(&url, &hdrs, None, &pool, &uas).await.unwrap_err();
        assert!(matches!(err, PdmError::Http(403)));
        // 1 Range attempt + 1 plain-GET fallback attempt = 2 requests total (not 10).
        assert_eq!(hits.load(Ordering::Relaxed), 2);
    }
}
