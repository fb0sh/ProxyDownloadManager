use crate::types::PendingDownloadRequest;
use std::collections::HashMap;

/// Headers that must never be replayed, rather than a list of the ones that
/// may be: a media CDN can key on anything the browser sent (`x-*` playback
/// session ids especially), and dropping a header the origin required is
/// exactly how a request that worked in the tab fails here.
///
/// Hop-by-hop and framing headers would corrupt the request, and Range /
/// If-Range belong to the engine — a stored browser value appended beside a
/// chunk's Range is two of them.

/// Hop-by-hop, framing, pseudo and engine-owned headers.
fn is_blocked(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    matches!(
        n.as_str(),
        "host"
            | "connection"
            | "keep-alive"
            | "proxy-connection"
            | "proxy-authenticate"
            | "proxy-authorization"
            | "te"
            | "trailer"
            | "transfer-encoding"
            | "upgrade"
            | "content-length"
            | "content-encoding"
            | "expect"
            | "range"
            | "if-range"
    ) || n.starts_with(':')
}

/// Keep only headers a download engine should replay.
pub fn filter_headers(input: &HashMap<String, String>) -> HashMap<String, String> {
    let mut out = HashMap::new();
    for (k, v) in input {
        if v.is_empty() {
            continue;
        }
        if is_blocked(k) {
            continue;
        }
        out.insert(canonical_name(k), v.clone());
    }
    out
}

fn canonical_name(name: &str) -> String {
    match name.to_ascii_lowercase().as_str() {
        "cookie" => "Cookie".into(),
        "referer" => "Referer".into(),
        "origin" => "Origin".into(),
        "user-agent" => "User-Agent".into(),
        "authorization" => "Authorization".into(),
        "accept" => "Accept".into(),
        "accept-language" => "Accept-Language".into(),
        "accept-encoding" => "Accept-Encoding".into(),
        // Anything else keeps the caller's spelling: header names are
        // case-insensitive on the wire, and the settings UI shows them back.
        _ => name.to_string(),
    }
}

/// Merge structured request fields into a single header map.
pub fn merge_request_headers(req: &PendingDownloadRequest) -> HashMap<String, String> {
    let mut headers = req.headers.clone();
    if !req.cookies.is_empty() {
        headers
            .entry("Cookie".into())
            .or_insert_with(|| req.cookies.clone());
    }
    if !req.referrer.is_empty() {
        headers
            .entry("Referer".into())
            .or_insert_with(|| req.referrer.clone());
    }
    if !req.user_agent.is_empty() {
        headers
            .entry("User-Agent".into())
            .or_insert_with(|| req.user_agent.clone());
    }
    filter_headers(&headers)
}

/// Apply filtered headers onto a reqwest builder. `user_agent` is used only
/// when the map does not already contain User-Agent.
pub fn apply_headers(
    mut req: reqwest::RequestBuilder,
    headers: &HashMap<String, String>,
    user_agent: &str,
) -> reqwest::RequestBuilder {
    let mut has_ua = false;
    for (k, v) in headers {
        // Old rows may still store these. reqwest appends headers, so they
        // have to be dropped here — setting Range afterwards would not replace them.
        if k.eq_ignore_ascii_case("range")
            || k.eq_ignore_ascii_case("if-range")
            || k.eq_ignore_ascii_case("accept-encoding")
        {
            continue;
        }
        if k.eq_ignore_ascii_case("user-agent") {
            has_ua = true;
        }
        req = req.header(k.as_str(), v.as_str());
    }
    if !has_ua && !user_agent.is_empty() {
        req = req.header("User-Agent", user_agent);
    }
    req
}

/// Replay caller headers, then pin the fields the engine owns.
/// `range` is set last so the request carries exactly one Range.
/// Accept-Encoding is always `identity`: a gzip body does not match byte ranges.
pub fn prepare_request(
    req: reqwest::RequestBuilder,
    headers: &HashMap<String, String>,
    user_agent: &str,
    range: Option<&str>,
) -> reqwest::RequestBuilder {
    let mut req = apply_headers(req, headers, user_agent);
    req = req.header("Accept-Encoding", "identity");
    if let Some(range) = range {
        if !range.is_empty() {
            req = req.header("Range", range);
        }
    }
    req
}

const SENSITIVE: &[&str] = &[
    "authorization",
    "proxy-authorization",
    "cookie",
    "set-cookie",
];

/// Replace sensitive header values with `<redacted>`.
pub fn redact_headers(headers: &HashMap<String, String>) -> HashMap<String, String> {
    headers
        .iter()
        .map(|(k, v)| {
            if SENSITIVE.iter().any(|s| k.eq_ignore_ascii_case(s)) {
                (k.clone(), "<redacted>".into())
            } else {
                (k.clone(), v.clone())
            }
        })
        .collect()
}

/// Redact sensitive tokens inside a free-form log line.
pub fn redact_log(line: &str) -> String {
    let mut out = line.to_string();
    for name in [
        "Authorization",
        "Cookie",
        "Proxy-Authorization",
        "Set-Cookie",
    ] {
        // JSON: "Cookie": "..."
        let json_pat = format!("\"{}\"", name);
        if let Some(idx) = find_ci(&out, &json_pat) {
            if let Some(colon) = out[idx + json_pat.len()..].find(':') {
                let start = idx + json_pat.len() + colon + 1;
                out = redact_from(&out, start);
            }
        }
        // Header-style: Cookie: value
        let hdr_pat = format!("{}:", name);
        if let Some(idx) = find_ci(&out, &hdr_pat) {
            let start = idx + hdr_pat.len();
            out = redact_from(&out, start);
        }
    }
    out
}

fn find_ci(hay: &str, needle: &str) -> Option<usize> {
    hay.to_ascii_lowercase().find(&needle.to_ascii_lowercase())
}

fn redact_from(s: &str, start: usize) -> String {
    let rest = &s[start..];
    let trimmed = rest.trim_start();
    let pad = rest.len() - trimmed.len();
    let at = start + pad;
    if trimmed.starts_with('"') {
        if let Some(end) = trimmed[1..].find('"') {
            let mut out = String::new();
            out.push_str(&s[..at]);
            out.push_str("\"<redacted>\"");
            out.push_str(&trimmed[end + 2..]);
            return out;
        }
    }
    let end_rel = trimmed
        .find(|c: char| c == ',' || c == ';' || c == '\n' || c == '}')
        .unwrap_or(trimmed.len());
    let mut out = String::new();
    out.push_str(&s[..at]);
    out.push_str("<redacted>");
    out.push_str(&trimmed[end_rel..]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_what_the_browser_sent_and_drops_the_unsafe() {
        let mut input = HashMap::new();
        input.insert("Host".into(), "cdn.example".into());
        input.insert("Connection".into(), "keep-alive".into());
        input.insert("Sec-Fetch-Mode".into(), "navigate".into());
        input.insert("Cookie".into(), "sid=1".into());
        input.insert("Referer".into(), "https://example.com/".into());
        input.insert("Content-Length".into(), "12".into());
        input.insert("Range".into(), "bytes=0-0".into());
        input.insert("If-Range".into(), "\"etag\"".into());
        input.insert("X-Playback-Session-Id".into(), "deadbeef".into());
        input.insert(":authority".into(), "cdn.example".into());
        let out = filter_headers(&input);
        assert_eq!(out.get("Cookie").unwrap(), "sid=1");
        assert_eq!(out.get("Referer").unwrap(), "https://example.com/");
        // A header the origin demanded but the old allow-list dropped.
        assert_eq!(out.get("X-Playback-Session-Id").unwrap(), "deadbeef");
        // A browser-managed header is harmless to replay and helps some CDNs.
        assert_eq!(out.get("Sec-Fetch-Mode").unwrap(), "navigate");
        for blocked in ["host", "range", "if-range", "content-length", "connection"] {
            assert!(
                !out.keys().any(|k| k.eq_ignore_ascii_case(blocked)),
                "{blocked} must not be replayed"
            );
        }
        assert!(!out.keys().any(|k| k.starts_with(':')));
    }

    #[test]
    fn merge_prefers_explicit_cookie_field() {
        let req = PendingDownloadRequest {
            cookies: "a=b".into(),
            referrer: "https://ref.example/".into(),
            user_agent: "UA/1".into(),
            url: "https://x".into(),
            ..Default::default()
        };
        let h = merge_request_headers(&req);
        assert_eq!(h.get("Cookie").unwrap(), "a=b");
        assert_eq!(h.get("Referer").unwrap(), "https://ref.example/");
        assert_eq!(h.get("User-Agent").unwrap(), "UA/1");
    }

    #[test]
    fn prepare_request_sends_one_engine_range_and_identity() {
        let client = reqwest::Client::new();
        let mut headers = HashMap::new();
        headers.insert("Range".into(), "bytes=0-0".into());
        headers.insert("If-Range".into(), "\"abc\"".into());
        headers.insert("Accept-Encoding".into(), "gzip".into());
        headers.insert("Cookie".into(), "a=b".into());
        let req = prepare_request(
            client.get("http://127.0.0.1/file"),
            &headers,
            "ProxyDM",
            Some("bytes=10-19"),
        )
        .build()
        .unwrap();
        let ranges: Vec<_> = req
            .headers()
            .get_all("range")
            .iter()
            .map(|v| v.to_str().unwrap().to_string())
            .collect();
        assert_eq!(ranges, vec!["bytes=10-19".to_string()]);
        let encodings: Vec<_> = req
            .headers()
            .get_all("accept-encoding")
            .iter()
            .map(|v| v.to_str().unwrap().to_string())
            .collect();
        assert_eq!(encodings, vec!["identity".to_string()]);
        assert!(req.headers().get("if-range").is_none());
        assert_eq!(req.headers().get("cookie").unwrap(), "a=b");
    }

    #[test]
    fn redact_replaces_sensitive_values() {
        let mut h = HashMap::new();
        h.insert("Authorization".into(), "Bearer secret".into());
        h.insert("Cookie".into(), "sid=abc".into());
        h.insert("Referer".into(), "https://ok".into());
        let r = redact_headers(&h);
        assert_eq!(r.get("Authorization").unwrap(), "<redacted>");
        assert_eq!(r.get("Cookie").unwrap(), "<redacted>");
        assert_eq!(r.get("Referer").unwrap(), "https://ok");
    }

    #[test]
    fn redact_log_line() {
        let line = r#"{"Cookie":"sid=abc","url":"https://x"}"#;
        let out = redact_log(line);
        assert!(out.contains("<redacted>"));
        assert!(!out.contains("sid=abc"));
        assert!(out.contains("https://x"));
    }
}
