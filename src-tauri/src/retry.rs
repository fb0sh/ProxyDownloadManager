use crate::types::PdmError;
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// Errors that should be retried with backoff.
pub fn is_retryable(err: &PdmError) -> bool {
    match err {
        PdmError::Cancelled | PdmError::RangeLost => false,
        PdmError::Http(code) => is_retryable_status(*code),
        PdmError::Network(msg) | PdmError::Io(msg) | PdmError::Other(msg) | PdmError::RetriesExhausted(msg) => {
            is_retryable_message(msg)
        }
        PdmError::Incomplete(_) => true,
        PdmError::Probe(msg) => is_retryable_message(msg),
        _ => false,
    }
}

pub fn is_retryable_status(code: u16) -> bool {
    matches!(code, 408 | 425 | 429 | 500 | 502 | 503 | 504)
}

pub fn is_fatal_client_status(code: u16) -> bool {
    matches!(code, 400 | 401 | 403 | 404 | 405 | 410 | 416)
}

pub fn is_retryable_message(msg: &str) -> bool {
    let m = msg.to_ascii_lowercase();
    m.contains("timeout")
        || m.contains("timed out")
        || m.contains("connection reset")
        || m.contains("connection refused")
        || m.contains("broken pipe")
        || m.contains("temporarily")
        || m.contains("try again")
        || m.contains("dns error")
        || m.contains("name or service not known")
        || m.contains("failed to lookup")
        || m.contains("reset")
        || m.contains("error 408")
        || m.contains("error 429")
        || m.contains("error 500")
        || m.contains("error 502")
        || m.contains("error 503")
        || m.contains("error 504")
        || m.contains("http 408")
        || m.contains("http 429")
        || m.contains("http 500")
        || m.contains("http 502")
        || m.contains("http 503")
        || m.contains("http 504")
}

#[allow(dead_code)]
pub fn auth_hint(status: u16) -> Option<&'static str> {
    match status {
        401 => Some("Cookie, Authorization, or a refreshed download URL may be required"),
        403 => Some("Cookie, Referer, Authorization, or a refreshed download URL may be required"),
        _ => None,
    }
}

/// Exponential backoff with full jitter: 1, 2, 4, 8, 16, then 30s cap.
pub fn backoff_delay(attempt: u32) -> Duration {
    let base = 1u64 << attempt.min(4); // 1,2,4,8,16
    let capped = base.min(30);
    let jitter_ms = jitter_ms(capped * 1000);
    Duration::from_millis(jitter_ms.max(200))
}

fn jitter_ms(max_ms: u64) -> u64 {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let seed = now.subsec_nanos() as u64 ^ now.as_secs();
    if max_ms == 0 {
        0
    } else {
        seed % max_ms
    }
}

#[allow(dead_code)]
#[derive(Debug, Clone, Default)]
pub struct ProxyStats {
    pub success: u64,
    pub failure: u64,
    pub last_latency_ms: u64,
    pub last_failure: Option<Instant>,
}

#[allow(dead_code)]
pub struct ProxyScoreboard {
    inner: Mutex<HashMap<String, ProxyStats>>,
}

impl ProxyScoreboard {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(HashMap::new()),
        }
    }

    pub fn record_success(&self, name: &str, latency_ms: u64) {
        if name.is_empty() {
            return;
        }
        if let Ok(mut map) = self.inner.lock() {
            let e = map.entry(name.to_string()).or_default();
            e.success += 1;
            e.last_latency_ms = latency_ms;
        }
    }

    pub fn record_failure(&self, name: &str) {
        if name.is_empty() {
            return;
        }
        if let Ok(mut map) = self.inner.lock() {
            let e = map.entry(name.to_string()).or_default();
            e.failure += 1;
            e.last_failure = Some(Instant::now());
        }
    }

    pub fn snapshot(&self, name: &str) -> ProxyStats {
        self.inner
            .lock()
            .ok()
            .and_then(|m| m.get(name).cloned())
            .unwrap_or_default()
    }

    /// Rank names: prefer fewer recent failures, then more successes.
    pub fn rank<'a>(&self, names: &'a [String]) -> Vec<&'a str> {
        let map = self.inner.lock().ok();
        let mut indexed: Vec<(usize, &'a str, u64, u64)> = names
            .iter()
            .enumerate()
            .map(|(i, n)| {
                let stats = map
                    .as_ref()
                    .and_then(|m| m.get(n))
                    .cloned()
                    .unwrap_or_default();
                (i, n.as_str(), stats.failure, stats.success)
            })
            .collect();
        indexed.sort_by(|a, b| a.2.cmp(&b.2).then(b.3.cmp(&a.3)).then(a.0.cmp(&b.0)));
        indexed.into_iter().map(|(_, n, _, _)| n).collect()
    }
}

impl Default for ProxyScoreboard {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_http_codes() {
        assert!(is_retryable_status(429));
        assert!(is_retryable_status(503));
        assert!(is_retryable_status(408));
        assert!(!is_retryable_status(403));
        assert!(!is_retryable_status(404));
        assert!(is_fatal_client_status(401));
        assert!(is_fatal_client_status(403));
    }

    #[test]
    fn classifies_network_messages() {
        assert!(is_retryable_message("connection reset by peer"));
        assert!(is_retryable_message("operation timed out"));
        assert!(is_retryable_message("HTTP 502"));
        assert!(!is_retryable_message("HTTP 403"));
    }

    #[test]
    fn backoff_is_capped() {
        for i in 0..12 {
            let d = backoff_delay(i);
            assert!(d <= Duration::from_secs(30));
            assert!(d >= Duration::from_millis(200));
        }
    }

    #[test]
    fn scoreboard_ranks_failures_last() {
        let board = ProxyScoreboard::new();
        board.record_failure("bad");
        board.record_failure("bad");
        board.record_success("good", 20);
        let names = vec!["bad".into(), "good".into(), "fresh".into()];
        let ranked = board.rank(&names);
        assert_eq!(ranked[0], "good");
        assert_eq!(ranked[ranked.len() - 1], "bad");
    }
}
