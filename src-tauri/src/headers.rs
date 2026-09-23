use crate::types::PendingDownloadRequest;
use std::collections::HashMap;

/// Headers that are safe and useful to replay on a download request.
const ALLOWED: &[&str] = &[
    "cookie",
    "referer",
    "origin",
    "user-agent",
    "authorization",
    "accept",
    "accept-language",
    "accept-encoding",
    "range",
    "if-range",
    "if-match",
    "if-none-match",
    "if-modified-since",
];

/// Hop-by-hop / browser-internal headers that must never be forwarded.
fn is_blocked(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    matches!(
        n.as_str(),
        "host"
            | "connection"
            | "keep-alive"
            | "proxy-connection"
            | "proxy-authenticate"
            | "te"
            | "trailer"
            | "transfer-encoding"
            | "upgrade"
            | "content-length"
            | "content-encoding"
            | "expect"
    ) || n.starts_with("sec-")
        || n.starts_with(":")
}

fn is_allowed(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    ALLOWED.iter().any(|a| *a == n)
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
        if is_allowed(k) {
            out.insert(canonical_name(k), v.clone());
        }
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
        other => other.to_string(),
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
    for name in ["Authorization", "Cookie", "Proxy-Authorization", "Set-Cookie"] {
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
    hay.to_ascii_lowercase()
        .find(&needle.to_ascii_lowercase())
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
    fn filters_hop_by_hop_and_sec() {
        let mut input = HashMap::new();
        input.insert("Host".into(), "cdn.example".into());
        input.insert("Connection".into(), "keep-alive".into());
        input.insert("Sec-Fetch-Mode".into(), "navigate".into());
        input.insert("Cookie".into(), "sid=1".into());
        input.insert("Referer".into(), "https://example.com/".into());
        input.insert("Content-Length".into(), "12".into());
        let out = filter_headers(&input);
        assert_eq!(out.get("Cookie").unwrap(), "sid=1");
        assert_eq!(out.get("Referer").unwrap(), "https://example.com/");
        assert!(!out.keys().any(|k| k.eq_ignore_ascii_case("host")));
        assert!(!out.keys().any(|k| k.to_ascii_lowercase().starts_with("sec-")));
        assert!(!out.keys().any(|k| k.eq_ignore_ascii_case("content-length")));
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
