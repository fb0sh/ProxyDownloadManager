use crate::network::pool::NetworkPool;
use std::collections::HashMap;
use std::error::Error;

pub struct ProbeResult {
    pub supports_range: bool,
    pub file_size: u64,
    pub file_name: String,
    pub accept_ranges: bool,
}

pub async fn probe(
    url: &str,
    headers: &HashMap<String, String>,
    proxy: Option<&str>,
    pool: &NetworkPool,
    user_agents: &[String],
) -> Result<ProbeResult, String> {
    let client = pool.get_client(proxy);
    eprintln!("[ProxyDM] probe start url={} proxy={:?} uas={}", url, proxy, user_agents.len());

    // Try each UA, return first success
    let mut first_err: Option<String> = None;
    for (i, ua) in user_agents.iter().chain(std::iter::once(&String::new())).enumerate() {
        eprintln!("[ProxyDM] probe attempt #{} ua_prefix={:?}...", i,
            &ua[..ua.len().min(40)]);
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

        let (file_size, accept_ranges) = if supports_range {
            let cr = resp.headers().get("content-range")
                .and_then(|v| v.to_str().ok())
                .and_then(|s| {
                    s.split('/').nth(1).and_then(|n| n.trim().parse::<u64>().ok())
                })
                .unwrap_or(0);
            let ar = resp.headers().get("accept-ranges")
                .and_then(|v| v.to_str().ok())
                .map(|v| v.contains("bytes"))
                .unwrap_or(false);
            (cr, ar)
        } else if status == reqwest::StatusCode::OK {
            let size = resp.headers().get("content-length")
                .and_then(|v| v.to_str().ok())
                .and_then(|s| s.parse::<u64>().ok())
                .unwrap_or(0);
            (size, false)
        } else if status == reqwest::StatusCode::FORBIDDEN || status == reqwest::StatusCode::METHOD_NOT_ALLOWED {
            // Try fallback GET without Range (no UA switch yet, skip to next UA on retry)
            let mut get_req = client.get(url);
            get_req = get_req.timeout(std::time::Duration::from_secs(30));
            if !ua.is_empty() {
                get_req = get_req.header("User-Agent", ua.as_str());
            }
            match get_req.send().await {
                Ok(r2) if r2.status().is_success() => {
                    let size = r2.headers().get("content-length")
                        .and_then(|v| v.to_str().ok())
                        .and_then(|s| s.parse::<u64>().ok())
                        .unwrap_or(0);
                    (size, false)
                }
                _ => continue, // try next UA
            }
        } else {
            return Err(format!("Probe failed with status: {}", status));
        };

        // Detect filename from Content-Disposition or URL
        let file_name = resp.headers().get("content-disposition")
            .and_then(|v| v.to_str().ok())
            .and_then(|s| {
                s.split(';').find_map(|part| {
                    let p = part.trim();
                    p.strip_prefix("filename=").or_else(|| p.strip_prefix("filename*=UTF-8''"))
                })
            })
            .map(|s| s.trim_matches('"').to_string())
            .unwrap_or_else(|| {
                // Strategy 2: search ALL query param values for filename=xxx
                if let Ok(parsed) = url::Url::parse(url) {
                    for (_, val) in parsed.query_pairs() {
                        let lower = val.to_lowercase();
                        if let Some(pos) = lower.find("filename=") {
                            let after = &val[pos + 9..];
                            let trimmed = after
                                .trim_start_matches('*')
                                .trim_start_matches("UTF-8''")
                                .trim_matches('"')
                                .trim();
                            let name = trimmed.split(|c: char| c == ';' || c.is_whitespace())
                                .next().unwrap_or(trimmed);
                            if !name.is_empty() && name.contains('.') {
                                return name.to_string();
                            }
                        }
                    }
                }
                // Strategy 3: scan the full URL for the last name.ext pattern
                // by splitting on delimiters, checking the last dot for 2-5 alpha ext
                {
                    let mut last = String::new();
                    for token in url.split(|c: char| c == '/' || c == '?' || c == '#' || c == '&' || c == '=' || c.is_whitespace()) {
                        if token.len() < 5 || !token.contains('.') || token.ends_with('.') { continue; }
                        if let Some(dot) = token.rfind('.') {
                            if dot < 2 { continue; }
                            let ext = &token[dot + 1..];
                            if ext.len() >= 2 && ext.len() <= 5 && ext.bytes().all(|b| b.is_ascii_alphabetic()) {
                                last = token.to_string();
                            }
                        }
                    }
                    if !last.is_empty() { return last; }
                }
                std::path::Path::new(url)
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| "download".to_string())
            });

        eprintln!("[ProxyDM] probe SUCCESS ua#{} range={} size={} name={}", i, supports_range, file_size, file_name);
        return Ok(ProbeResult {
            supports_range,
            file_size,
            file_name,
            accept_ranges,
        });
    }

    let err_msg = match first_err {
        Some(ref e) => format!("All probe attempts failed (first error: {})", e),
        None => "All probe attempts failed".to_string(),
    };
    eprintln!("[ProxyDM] probe FAILED: {}", err_msg);
    Err(err_msg)
}
