use crate::types::{PdmError, PdmResult};
use crate::network::pool::NetworkPool;
use std::collections::HashMap;
use std::error::Error;

pub struct ProbeResult {
    pub supports_range: bool,
    pub file_size: u64,
    pub file_name: String,
}

pub async fn probe(
    url: &str,
    headers: &HashMap<String, String>,
    proxy: Option<&str>,
    pool: &NetworkPool,
    user_agents: &[String],
) -> PdmResult<ProbeResult> {
    let client = pool.get_client(proxy).map_err(|e| PdmError::ClientBuild(e.to_string()))?;
    eprintln!("[ProxyDM] probe start url={} proxy={:?} uas={}", url, proxy, user_agents.len());

    // Try each UA, return first success
    let mut first_err: Option<String> = None;
    for (i, ua) in user_agents.iter().chain(std::iter::once(&String::new())).enumerate() {
        eprintln!("[ProxyDM] probe attempt #{} ua_prefix={:?}...", i,
            &ua[..ua.char_indices().nth(40).map(|(i, _)| i).unwrap_or(ua.len())]);
        // Try Range first to detect 206 support
        let mut range_req = client.get(url);
        range_req = range_req.header("Range", "bytes=0-0");
        range_req = range_req.timeout(std::time::Duration::from_secs(30));
        for (k, v) in headers {
            range_req = range_req.header(k.as_str(), v.as_str());
        }
        if !ua.is_empty() {
            range_req = range_req.header("User-Agent", ua.as_str());
        }

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
            // Try fallback GET without Range (no UA switch yet, skip to next UA on retry)
            let mut get_req = client.get(url);
            get_req = get_req.timeout(std::time::Duration::from_secs(30));
            if !ua.is_empty() {
                get_req = get_req.header("User-Agent", ua.as_str());
            }
            match get_req.send().await {
                Ok(r2) if r2.status().is_success() => {
                    r2.headers().get("content-length")
                        .and_then(|v| v.to_str().ok())
                        .and_then(|s| s.parse::<u64>().ok())
                        .unwrap_or(0)
                }
                _ => continue, // try next UA
            }
        } else {
            return Err(PdmError::Probe(format!("HTTP {}", status)));
        };

        // Detect filename from Content-Disposition or URL
        let cd_header = resp.headers().get("content-disposition")
            .and_then(|v| v.to_str().ok());
        let file_name = crate::filename::extract_filename(url, cd_header)
            .unwrap_or_else(|| "download".to_string());

        eprintln!("[ProxyDM] probe SUCCESS ua#{} range={} size={} name={}", i, supports_range, file_size, file_name);
        return Ok(ProbeResult {
            supports_range,
            file_size,
            file_name,
        });
    }

    let err_msg = match first_err {
        Some(ref e) => format!("All probe attempts failed (first error: {})", e),
        None => "All probe attempts failed".to_string(),
    };
    eprintln!("[ProxyDM] probe FAILED: {}", err_msg);
    Err(PdmError::Probe(err_msg))
}

/// Probe result with filename override applied.
pub struct ProbeOutcome {
    pub file_name: String,
    pub file_size: u64,
    pub supports_range: bool,
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
                file_name: name,
                file_size: r.file_size,
                supports_range: r.supports_range,
            }
        }
        Err(e) => {
            let name = if filename_override.is_empty() {
                crate::filename::from_url(url).unwrap_or_else(|| "download".to_string())
            } else {
                filename_override.to_string()
            };
            ProbeOutcome {
                file_name: name,
                file_size: 0,
                supports_range: false,
            }
        }
    }
}
