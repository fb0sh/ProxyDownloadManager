/// Unified filename extraction from URLs and Content-Disposition headers.

/// Make a server-supplied filename safe on every platform (Windows is the
/// strictest): replaces path separators, reserved characters and control
/// chars, strips trailing dots/spaces, escapes reserved device names
/// (CON, NUL, COM1…), and clamps the length while keeping the extension.
pub fn sanitize(name: &str) -> String {
    const INVALID: &[char] = &['/', '\\', ':', '*', '?', '"', '<', '>', '|'];
    let mut s: String = name
        .chars()
        .map(|c| {
            if c.is_control() || INVALID.contains(&c) {
                '_'
            } else {
                c
            }
        })
        .collect();
    s = s.trim().trim_end_matches(['.', ' ']).to_string();

    let stem_upper = s
        .split('.')
        .next()
        .unwrap_or("")
        .trim()
        .to_ascii_uppercase();
    let reserved = matches!(
        stem_upper.as_str(),
        "CON"
            | "PRN"
            | "AUX"
            | "NUL"
            | "COM1"
            | "COM2"
            | "COM3"
            | "COM4"
            | "COM5"
            | "COM6"
            | "COM7"
            | "COM8"
            | "COM9"
            | "LPT1"
            | "LPT2"
            | "LPT3"
            | "LPT4"
            | "LPT5"
            | "LPT6"
            | "LPT7"
            | "LPT8"
            | "LPT9"
    );
    if reserved {
        s = format!("_{}", s);
    }
    if s.is_empty() {
        return "download".to_string();
    }

    const MAX_LEN: usize = 150;
    if s.chars().count() > MAX_LEN {
        let (stem, ext) = match s.rfind('.') {
            Some(dot) if s.len() - dot <= 10 => (s[..dot].to_string(), s[dot..].to_string()),
            _ => (s.clone(), String::new()),
        };
        let keep = MAX_LEN.saturating_sub(ext.chars().count());
        s = format!("{}{}", stem.chars().take(keep).collect::<String>(), ext);
    }
    s
}

/// Decode percent-encoded UTF-8 sequences (`%XX`). Invalid UTF-8 falls back
/// to lossy replacement so malformed server headers never panic or drop names.
pub fn percent_decode(input: &str) -> String {
    if !input.contains('%') {
        return input.to_string();
    }
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let (Some(hi), Some(lo)) = (hex_val(bytes[i + 1]), hex_val(bytes[i + 2])) {
                out.push((hi << 4) | lo);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8(out)
        .unwrap_or_else(|e| String::from_utf8_lossy(e.as_bytes()).into_owned())
}

fn hex_val(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

fn strip_rfc5987_prefix(raw: &str) -> &str {
    // RFC 5987 / 8187: `charset'lang'encoded-value` (e.g. `UTF-8''...` or `utf-8'en'...`).
    if let Some((charset, rest)) = raw.split_once('\'') {
        if let Some((_lang, value)) = rest.split_once('\'') {
            let cs = charset.trim().to_ascii_lowercase();
            if cs.is_empty() || cs == "utf-8" || cs == "utf8" || cs == "iso-8859-1" {
                return value;
            }
        }
    }
    raw
}

/// Extract filename from a Content-Disposition header value.
/// Follows RFC 6266 §4.3: `filename*` takes precedence over `filename`.
pub fn from_content_disposition(header: &str) -> Option<String> {
    let mut ext_name: Option<String> = None;
    let mut plain_name: Option<String> = None;

    for part in header.split(';') {
        let p = part.trim();
        let Some((raw_key, raw_val)) = p.split_once('=') else {
            continue;
        };
        let key = raw_key.trim();
        let val = raw_val.trim().trim_matches('"').trim_matches('\'').trim();
        if val.is_empty() {
            continue;
        }
        if key.eq_ignore_ascii_case("filename*") {
            let stripped = strip_rfc5987_prefix(val);
            let decoded = percent_decode(stripped);
            if !decoded.is_empty() {
                ext_name = Some(decoded);
            }
        } else if key.eq_ignore_ascii_case("filename") && plain_name.is_none() {
            let decoded = percent_decode(val);
            if !decoded.is_empty() {
                plain_name = Some(decoded);
            }
        }
    }

    ext_name.or(plain_name)
}

/// Extract filename from a URL using three strategies:
/// 1. Parsed URL path (last segment with an extension, ignoring query/fragment)
/// 2. Query parameter (`filename`, `file`, `name`, `response-content-disposition`)
/// 3. URL token scan (last `name.ext` pattern with 2-6 alphanumeric extension)
pub fn from_url(url: &str) -> Option<String> {
    if let Ok(parsed) = url::Url::parse(url) {
        // Strategy 1a: S3 / OSS `response-content-disposition` or explicit filename query param
        for (key, val) in parsed.query_pairs() {
            let k = key.to_ascii_lowercase();
            if k == "response-content-disposition" || k == "rscd" {
                if let Some(cd_name) = from_content_disposition(&val) {
                    return Some(cd_name);
                }
            }
            let lower = val.to_lowercase();
            if let Some(pos) = lower.find("filename=") {
                let after = &val[pos + 9..];
                let trimmed = strip_rfc5987_prefix(
                    after
                        .trim_start_matches('*')
                        .trim_start_matches('=')
                        .trim_matches('"')
                        .trim(),
                );
                let name = trimmed
                    .split(|c: char| c == ';' || c.is_whitespace())
                    .next()
                    .unwrap_or(trimmed);
                let decoded = percent_decode(name);
                if !decoded.is_empty() && decoded.contains('.') {
                    return Some(decoded);
                }
            }
            if matches!(k.as_str(), "filename" | "file" | "attname" | "download") {
                let decoded = percent_decode(val.trim());
                if !decoded.is_empty()
                    && decoded.contains('.')
                    && !decoded.contains('/')
                    && !decoded.contains('\\')
                {
                    return Some(decoded);
                }
            }
        }

        // Strategy 1b: Clean URL path segment (unaffected by `?token=...` query strings)
        if let Some(seg) = parsed
            .path_segments()
            .and_then(|mut s| s.rfind(|seg| !seg.is_empty()))
        {
            let decoded = percent_decode(seg);
            if has_file_extension(&decoded) {
                return Some(decoded);
            }
        }
    } else if let Some(name) = std::path::Path::new(url)
        .file_name()
        .map(|n| percent_decode(&n.to_string_lossy()))
        .filter(|n| has_file_extension(n) && !n.contains('?') && !n.contains('&'))
    {
        return Some(name);
    }

    // Strategy 3: scan the full URL for the last name.ext pattern
    last_name_from_str(url)
}

fn has_file_extension(name: &str) -> bool {
    let Some(dot) = name.rfind('.') else {
        return false;
    };
    if dot == 0 {
        return false;
    }
    let ext = &name[dot + 1..];
    (1..=8).contains(&ext.len())
        && ext.bytes().all(|b| b.is_ascii_alphanumeric())
        && ext.bytes().any(|b| b.is_ascii_alphabetic())
}

/// Top-level extraction: tries Content-Disposition first, then URL.
pub fn extract_filename(url: &str, content_disposition: Option<&str>) -> Option<String> {
    content_disposition
        .and_then(from_content_disposition)
        .or_else(|| from_url(url))
}

fn last_name_from_str(s: &str) -> Option<String> {
    let mut last: Option<String> = None;
    for token in s.split(|c: char| {
        c == '/' || c == '?' || c == '#' || c == '&' || c == '=' || c.is_whitespace()
    }) {
        if token.len() < 4 || !token.contains('.') || token.ends_with('.') {
            continue;
        }
        let Some(dot) = token.rfind('.') else {
            continue;
        };
        if dot < 1 {
            continue;
        }
        let ext = &token[dot + 1..];
        if ext.len() < 2 || ext.len() > 5 {
            continue;
        }
        if !ext.bytes().all(|b| b.is_ascii_alphanumeric())
            || !ext.bytes().any(|b| b.is_ascii_alphabetic())
        {
            continue;
        }
        last = Some(percent_decode(token));
    }
    last
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_content_disposition_filename() {
        let header = "attachment; filename=\"report.pdf\"";
        assert_eq!(from_content_disposition(header).unwrap(), "report.pdf");
    }

    #[test]
    fn test_content_disposition_utf8() {
        let header = "attachment; filename*=UTF-8''%E6%8A%A5%E5%91%8A.pdf";
        let name = from_content_disposition(header).unwrap();
        assert_eq!(name, "报告.pdf");
    }

    #[test]
    fn test_content_disposition_rfc6266_prefers_filename_star_and_case_insensitive() {
        let header = "attachment; filename=\"fallback.pdf\"; filename*=utf-8'zh-CN'%E5%B9%B4%E5%BA%A6%E6%8A%A5%E5%91%8A%202026.pdf";
        assert_eq!(
            from_content_disposition(header).unwrap(),
            "年度报告 2026.pdf"
        );
    }

    #[test]
    fn test_from_url_alphanumeric_ext_with_query_and_domain_param() {
        // Previously `.mp4?referer=https://www.bilibili.com` either missed `.mp4` (digit in ext)
        // or returned `www.bilibili.com` from query token scan.
        assert_eq!(
            from_url("https://cdn.example.com/videos/%E6%BC%94%E7%A4%BA.mp4?token=abc&origin=https://www.bilibili.com").unwrap(),
            "演示.mp4"
        );
        assert_eq!(
            from_url("https://s3.example.com/obj?response-content-disposition=attachment%3B%20filename%2A%3DUTF-8%27%27%E8%B5%84%E6%96%99.7z&sig=1").unwrap(),
            "资料.7z"
        );
    }

    #[test]
    fn test_from_url_path() {
        assert_eq!(
            from_url("https://example.com/files/doc.pdf").unwrap(),
            "doc.pdf"
        );
    }

    #[test]
    fn test_from_url_query_param() {
        assert_eq!(
            from_url("https://example.com/dl?file=photo.jpg&size=large").unwrap(),
            "photo.jpg"
        );
    }

    #[test]
    fn test_from_url_token_scan() {
        assert_eq!(
            from_url("https://example.com/dl?token=abc&file=report.xlsx").unwrap(),
            "report.xlsx"
        );
    }

    #[test]
    fn test_extract_filename_cd_over_url() {
        let url = "https://example.com/dl?id=123";
        let cd = "attachment; filename=\"data.csv\"";
        assert_eq!(extract_filename(url, Some(cd)).unwrap(), "data.csv");
    }

    #[test]
    fn test_extract_filename_url_fallback() {
        assert_eq!(
            extract_filename("https://example.com/files/app.zip", None).unwrap(),
            "app.zip"
        );
    }

    #[test]
    fn test_last_name_from_str() {
        assert_eq!(
            last_name_from_str("https://example.com/dl?file=report.pdf").unwrap(),
            "report.pdf"
        );
    }

    #[test]
    fn test_sanitize_windows_invalid_chars() {
        assert_eq!(sanitize("a:b?c*.zip"), "a_b_c_.zip");
        assert_eq!(sanitize("re\"po<rt>.pdf"), "re_po_rt_.pdf");
        assert_eq!(sanitize("path/to\\file.txt"), "path_to_file.txt");
    }

    #[test]
    fn test_sanitize_trailing_dots_and_spaces() {
        assert_eq!(sanitize("report.pdf. "), "report.pdf");
        assert_eq!(sanitize("  name.txt..."), "name.txt");
    }

    #[test]
    fn test_sanitize_reserved_device_names() {
        assert_eq!(sanitize("CON"), "_CON");
        assert_eq!(sanitize("nul.txt"), "_nul.txt");
        assert_eq!(sanitize("COM1.log"), "_COM1.log");
        assert_eq!(sanitize("console.txt"), "console.txt"); // not reserved
    }

    #[test]
    fn test_sanitize_empty_and_length() {
        assert_eq!(sanitize(""), "download");
        assert_eq!(sanitize("???"), "___");
        let long = format!("{}.zip", "x".repeat(300));
        let out = sanitize(&long);
        assert!(out.chars().count() <= 150);
        assert!(out.ends_with(".zip"));
    }

    #[test]
    fn test_sanitize_keeps_normal_names() {
        assert_eq!(sanitize("报告 2026.pdf"), "报告 2026.pdf");
        assert_eq!(sanitize("app-1.2.3.dmg"), "app-1.2.3.dmg");
    }
}
