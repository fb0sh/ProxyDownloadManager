use crate::types::{PdmError, PdmResult};
use crate::network::pool::NetworkPool;
use std::collections::HashMap;
use std::error::Error;

#[derive(Debug)]
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

pub async fn probe(
    url: &str,
    headers: &HashMap<String, String>,
    proxy: Option<&str>,
    pool: &NetworkPool,
    user_agents: &[String],
) -> PdmResult<ProbeResult> {
    let client = pool.get_client(proxy).map_err(|e| PdmError::ClientBuild(e.to_string()))?;
    log::info!("[ProxyDM] probe start url={} proxy={:?} uas={}", url, proxy, user_agents.len());

    // Try each UA, return first success
    let mut first_err: Option<String> = None;
    for (i, ua) in user_agents.iter().chain(std::iter::once(&String::new())).enumerate() {
        log::info!("[ProxyDM] probe attempt #{} ua_prefix={:?}...", i,
            &ua[..ua.char_indices().nth(40).map(|(i, _)| i).unwrap_or(ua.len())]);
        // Try Range first to detect 206 support
        let mut range_req = client.get(url);
        range_req = range_req.header("Range", "bytes=0-0");
        range_req = range_req.timeout(std::time::Duration::from_secs(30));
        range_req = crate::headers::apply_headers(range_req, headers, ua);

        let resp = range_req.send().await;
        let resp = match resp {
            Ok(r) => r,
            Err(e) => {
                if first_err.is_none() {
                    // Capture full error chain: Display + source
                    let mut msg = format!("{}", e);
                    let mut src = e.source();
                    while let Some(s) = src {
                        msg.push_str(&format!(": {}", s));
                        src = s.source();
                    }
                    first_err = Some(msg);
                }
                continue; // try next UA on network error
            }
        };
        let status = resp.status();

        let supports_range = status == reqwest::StatusCode::PARTIAL_CONTENT;

        let file_size = if supports_range {
            resp.headers().get("content-range")
                .and_then(|v| v.to_str().ok())
                .and_then(|s| {
                    s.split('/').nth(1).and_then(|n| n.trim().parse::<u64>().ok())
                })
                .unwrap_or(0)
        } else if status == reqwest::StatusCode::OK {
            resp.headers().get("content-length")
                .and_then(|v| v.to_str().ok())
                .and_then(|s| s.parse::<u64>().ok())
                .unwrap_or(0)
        } else if status == reqwest::StatusCode::FORBIDDEN || status == reqwest::StatusCode::METHOD_NOT_ALLOWED {
            let mut get_req = client.get(url);
            get_req = get_req.timeout(std::time::Duration::from_secs(30));
            get_req = crate::headers::apply_headers(get_req, headers, ua);
            match get_req.send().await {
                Ok(r2) if r2.status().is_success() => {
                    r2.headers().get("content-length")
                        .and_then(|v| v.to_str().ok())
                        .and_then(|s| s.parse::<u64>().ok())
                        .unwrap_or(0)
                }
                Ok(r2) if crate::retry::is_fatal_client_status(r2.status().as_u16()) => {
                    return Err(PdmError::Http(r2.status().as_u16()));
                }
                _ => continue,
            }
        } else if crate::retry::is_fatal_client_status(status.as_u16()) {
            return Err(PdmError::Http(status.as_u16()));
        } else {
            return Err(PdmError::Probe(format!("HTTP {}", status)));
        };

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
        let is_hls = content_type.contains("mpegurl")
            || content_type.contains("x-mpegURL")
            || url.contains(".m3u8");

        let file_name = crate::filename::extract_filename(url, cd_owned.as_deref())
            .unwrap_or_else(|| "download".to_string());

        log::info!("[ProxyDM] probe SUCCESS ua#{} range={} size={} name={}", i, supports_range, file_size, file_name);
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
            let name = if filename_override.is_empty() { r.file_name } else { filename_override.to_string() };
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
        let r = probe(&url, &headers, None, &pool, &["ua".into()]).await.unwrap();
        assert_eq!(r.file_size, 2048);
        assert!(r.supports_range);
    }

    #[tokio::test]
    async fn probe_referer_protected() {
        let url = spawn_guarded_server("Referer:", "https://app.example/").await;
        let pool = Arc::new(NetworkPool::new(false));
        let mut headers = HashMap::new();
        headers.insert("Referer".into(), "https://app.example/".into());
        let r = probe(&url, &headers, None, &pool, &["ua".into()]).await.unwrap();
        assert_eq!(r.file_name, "secret.bin");
    }

    #[tokio::test]
    async fn probe_authorization_protected() {
        let url = spawn_guarded_server("Authorization:", "Bearer tok").await;
        let pool = Arc::new(NetworkPool::new(false));
        let mut headers = HashMap::new();
        headers.insert("Authorization".into(), "Bearer tok".into());
        let r = probe(&url, &headers, None, &pool, &["ua".into()]).await.unwrap();
        assert!(r.supports_range);
    }

    #[tokio::test]
    async fn probe_missing_auth_is_http_error() {
        let url = spawn_guarded_server("Authorization:", "Bearer tok").await;
        let pool = Arc::new(NetworkPool::new(false));
        let headers = HashMap::new();
        let err = probe(&url, &headers, None, &pool, &["ua".into()]).await.unwrap_err();
        assert!(matches!(err, PdmError::Http(403) | PdmError::Probe(_)));
    }
}
