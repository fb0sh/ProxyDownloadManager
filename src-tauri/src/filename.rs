/// Unified filename extraction from URLs and Content-Disposition headers.

/// Make a server-supplied filename safe on every platform (Windows is the
/// strictest): replaces path separators, reserved characters and control
/// chars, strips trailing dots/spaces, escapes reserved device names
/// (CON, NUL, COM1…), and clamps the length while keeping the extension.
pub fn sanitize(name: &str) -> String {
    const INVALID: &[char] = &['/', '\\', ':', '*', '?', '"', '<', '>', '|'];
    let mut s: String = name
        .chars()
        .map(|c| if c.is_control() || INVALID.contains(&c) { '_' } else { c })
        .collect();
    s = s.trim().trim_end_matches(['.', ' ']).to_string();

    let stem_upper = s.split('.').next().unwrap_or("").trim().to_ascii_uppercase();
    let reserved = matches!(
        stem_upper.as_str(),
        "CON" | "PRN" | "AUX" | "NUL"
            | "COM1" | "COM2" | "COM3" | "COM4" | "COM5" | "COM6" | "COM7" | "COM8" | "COM9"
            | "LPT1" | "LPT2" | "LPT3" | "LPT4" | "LPT5" | "LPT6" | "LPT7" | "LPT8" | "LPT9"
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

/// Extract filename from a Content-Disposition header value.
pub fn from_content_disposition(header: &str) -> Option<String> {
    header
        .split(';')
        .find_map(|part| {
            let p = part.trim();
            p.strip_prefix("filename=")
                .or_else(|| p.strip_prefix("filename*=UTF-8''"))
        })
        .map(|s| s.trim_matches('"').to_string())
        .filter(|n| !n.is_empty())
}

/// Extract filename from a URL using three strategies:
/// 1. Path extraction (last segment with an extension)
/// 2. Query parameter `filename=xxx` scan
/// 3. URL token scan (last `name.ext` pattern with 2-5 alpha extension)
pub fn from_url(url: &str) -> Option<String> {
    // Strategy 1: path extraction (only if the path segment is clean)
    if let Some(name) = std::path::Path::new(url)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .filter(|n| n.contains('.') && !n.contains('?') && !n.contains('&'))
    {
        return Some(name);
    }

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
                let name = trimmed
                    .split(|c: char| c == ';' || c.is_whitespace())
                    .next()
                    .unwrap_or(trimmed);
                if !name.is_empty() && name.contains('.') {
                    return Some(name.to_string());
                }
            }
        }
    }

    // Strategy 3: scan the full URL for the last name.ext pattern
    last_name_from_str(url)
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
        if token.len() < 5 || !token.contains('.') || token.ends_with('.') {
            continue;
        }
        let dot = token.rfind('.')?;
        if dot < 2 {
            continue;
        }
        let ext = &token[dot + 1..];
        if ext.len() < 2 || ext.len() > 5 {
            continue;
        }
        if !ext.bytes().all(|b| b.is_ascii_alphabetic()) {
            continue;
        }
        last = Some(token.to_string());
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
        // The function strips the filename*= prefix, leaving the percent-encoded value
        assert!(name.ends_with(".pdf"));
        assert!(!name.is_empty());
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
