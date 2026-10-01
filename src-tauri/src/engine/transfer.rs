//! One HTTP fetch into a file: the request, the status rules, the body loop
//! (throttle, stall detection, buffered writes, stop) and the retry budget.
//! Engines decide what to fetch and where it goes; no engine reads a body.

use crate::engine::file_io::write_at;
use crate::headers::prepare_request;
use crate::network::limiter::MultiLimiter;
use crate::network::protocol::PerfStats;
use crate::retry::{backoff_delay, is_fatal_client_status};
use crate::types::{EngineConfig, PdmError, Phase, Task};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// How long a body may sit with no bytes before it is stalled.
/// Limiter waits happen outside this timer.
pub const BODY_IDLE: Duration = Duration::from_secs(30);
const WRITE_BUFFER: usize = 256 * 1024;
const PROGRESS_FLUSH: Duration = Duration::from_millis(250);
/// Longest a blocked read or backoff goes without looking at the stop flag.
const STOP_POLL: Duration = Duration::from_millis(100);

/// What to ask the server for, and where the answer goes in the file.
#[derive(Debug, Clone, PartialEq)]
pub enum Want {
    /// These bytes, written at their own offset. `length == 0` asks for
    /// everything from `offset` on; a server that ignores that Range sends
    /// the whole object, which then replaces the file from byte 0.
    Range(Task),
    /// The whole object, no Range header, replacing the file from byte 0.
    Whole,
}

/// Outcome of one request.
#[derive(Debug, PartialEq)]
pub enum Attempt {
    /// Everything asked for is in the file.
    Complete,
    /// Some of it is missing: the transfer was stopped, or the body broke
    /// off after making progress. `remaining` is what to ask for next.
    Partial { remaining: Task },
    /// The server did not answer a bounded Range with that range. Nothing
    /// was written; ranged transfer cannot proceed.
    RangeLost,
    /// A client error that must not be retried (401/403/404…).
    Refused(u16),
    /// Nothing new was written. Worth another try.
    Failed(PdmError),
}

/// Outcome of a fetch that retries on its own.
#[derive(Debug, PartialEq)]
pub enum Fetched {
    Complete,
    /// The stop flag was set. Bytes already written stay in the file.
    Stopped,
    RangeLost,
    Failed(PdmError),
}

/// How many more times one fetch may be retried, and how long to wait.
pub struct RetryBudget {
    max: u32,
    left: u32,
    backoff: fn(u32) -> Duration,
}

impl RetryBudget {
    /// Progress was made: the next failure starts from a full budget.
    pub fn reset(&mut self) {
        self.left = self.max;
    }

    /// Spend one retry. The delay to wait first, or `None` when none is left.
    pub fn take(&mut self) -> Option<Duration> {
        if self.left == 0 {
            return None;
        }
        self.left -= 1;
        Some((self.backoff)(self.max - self.left))
    }
}

/// What every fetch of one download shares.
pub struct Transfer {
    pub client: reqwest::Client,
    pub headers: Arc<HashMap<String, String>>,
    pub user_agent: String,
    pub limiter: Arc<MultiLimiter>,
    /// Set to stop every fetch: a user pause, or an abort the engine decided.
    pub stop: Arc<AtomicBool>,
    pub max_retries: u32,
    pub perf: Arc<PerfStats>,
    pub idle: Duration,
    pub backoff: fn(u32) -> Duration,
}

impl Transfer {
    pub fn new(
        client: reqwest::Client,
        cfg: &EngineConfig,
        limiter: Arc<MultiLimiter>,
        stop: Arc<AtomicBool>,
        perf: Arc<PerfStats>,
    ) -> Self {
        Self {
            client,
            headers: Arc::new(cfg.headers.clone()),
            user_agent: cfg.user_agent.clone(),
            limiter,
            stop,
            max_retries: cfg.max_retries,
            perf,
            idle: BODY_IDLE,
            backoff: backoff_delay,
        }
    }

    pub fn retry_budget(&self) -> RetryBudget {
        RetryBudget {
            max: self.max_retries,
            left: self.max_retries,
            backoff: self.backoff,
        }
    }

    fn stopped(&self) -> bool {
        self.stop.load(Ordering::Relaxed)
    }

    /// Wait out a backoff. False when the stop flag was set first.
    pub async fn wait(&self, delay: Duration) -> bool {
        let deadline = Instant::now() + delay;
        loop {
            if self.stopped() {
                return false;
            }
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return true;
            }
            tokio::time::sleep(left.min(STOP_POLL)).await;
        }
    }

    /// One request. `on_write(offset, len)` is called for every span that
    /// reached the file. Bytes are never written outside what `want` covers.
    pub async fn attempt(
        &self,
        url: &str,
        file: &std::fs::File,
        want: &Want,
        expected_total: u64,
        on_write: &(dyn Fn(u64, u64) + Sync),
    ) -> Attempt {
        let (task, range) = match want {
            Want::Whole => (
                Task {
                    offset: 0,
                    length: 0,
                },
                None,
            ),
            Want::Range(task) => {
                let end = if task.length == 0 {
                    String::new()
                } else {
                    (task.offset + task.length - 1).to_string()
                };
                (task.clone(), Some(format!("bytes={}-{}", task.offset, end)))
            }
        };
        let req = prepare_request(
            self.client.get(url),
            &self.headers,
            &self.user_agent,
            range.as_deref(),
        );
        log::debug!(
            "[ProxyDM] transfer offset={} range={:?}",
            task.offset,
            range
        );
        let resp = match send_headers(req, Some(&self.stop)).await {
            Ok(resp) => resp,
            Err(HeaderWait::Cancelled) => return Attempt::Partial { remaining: task },
            Err(HeaderWait::TimedOut) => {
                return Attempt::Failed(PdmError::Timeout(HeaderWait::TimedOut.to_string()));
            }
            Err(HeaderWait::Failed(msg)) => {
                log::error!(
                    "[ProxyDM] transfer REQUEST ERROR offset={}: {}",
                    task.offset,
                    msg
                );
                return Attempt::Failed(PdmError::Network(msg));
            }
        };

        self.perf.note_header(resp.version());
        let status = resp.status();
        if task.offset == 0 {
            log::info!(
                "[ProxyDM] transfer offset=0 HTTP {} range={:?}",
                status,
                range
            );
            log::debug!(
                "[net] offset=0 protocol={} status={}",
                crate::network::protocol::protocol_label(resp.version()),
                status.as_u16()
            );
        } else {
            log::debug!("[ProxyDM] transfer offset={} HTTP {}", task.offset, status);
        }

        if self.stopped() {
            // Nothing written yet: hand the whole request back so a resume
            // snapshot taken now still covers it.
            return Attempt::Partial { remaining: task };
        }

        // `at`: where this body starts in the file. `cap`: how many of its
        // bytes belong there; 0 reads until the body ends.
        let mut at = task.offset;
        let mut cap = task.length;
        let accepted = if range.is_some() {
            status == reqwest::StatusCode::OK || status == reqwest::StatusCode::PARTIAL_CONTENT
        } else {
            status.is_success()
        };
        if !accepted {
            let code = status.as_u16();
            log::info!(
                "[ProxyDM] transfer offset={} HTTP {} range={:?}",
                task.offset,
                code,
                range
            );
            return if is_fatal_client_status(code) {
                Attempt::Refused(code)
            } else {
                Attempt::Failed(PdmError::Http(code))
            };
        }
        if range.is_some() && status == reqwest::StatusCode::OK {
            // The server ignored the range. An open-ended request takes the
            // whole object from byte 0 instead. A bounded one may only keep a
            // full object that is no longer than the bytes it asked for;
            // anything else must not be written at this offset.
            if task.length == 0 {
                at = 0;
            } else {
                let content_len = resp
                    .headers()
                    .get(reqwest::header::CONTENT_LENGTH)
                    .and_then(|v| v.to_str().ok())
                    .and_then(|s| s.parse::<u64>().ok());
                if task.offset > 0 || matches!(content_len, Some(n) if n > task.length) {
                    log::warn!(
                        "[ProxyDM] server ignored Range header (HTTP 200), offset={}",
                        task.offset
                    );
                    return Attempt::RangeLost;
                }
            }
        }
        if range.is_some() && status == reqwest::StatusCode::PARTIAL_CONTENT {
            let header = resp
                .headers()
                .get(reqwest::header::CONTENT_RANGE)
                .and_then(|v| v.to_str().ok())
                .map(str::to_string);
            let Some((start, end, total)) = header.as_deref().and_then(parse_content_range) else {
                log::warn!(
                    "[ProxyDM] 206 with missing or bad Content-Range {:?} offset={}",
                    header,
                    task.offset
                );
                return Attempt::RangeLost;
            };
            let requested_end = if task.length == 0 {
                u64::MAX
            } else {
                task.offset + task.length - 1
            };
            if !validate_content_range(
                task.offset,
                requested_end,
                start,
                end,
                total,
                expected_total,
            ) {
                log::warn!(
                    "[ProxyDM] Content-Range {start}-{end}/{total:?} != requested {}-{} total={expected_total}",
                    task.offset, requested_end
                );
                return Attempt::RangeLost;
            }
            // A short Content-Range caps the body early; the unread tail of a
            // bounded request is handed back as the remainder.
            let sent = end - start + 1;
            cap = if task.length == 0 {
                sent
            } else {
                task.length.min(sent)
            };
        }
        if at == 0 && task.length == 0 {
            if let Err(e) = file.set_len(0) {
                return Attempt::Failed(PdmError::Io(e.to_string()));
            }
        }

        let stream = resp.bytes_stream();
        use futures_util::StreamExt;
        let mut stream = std::pin::pin!(stream);
        let mut written = 0u64;
        let mut buf = Vec::with_capacity(WRITE_BUFFER);
        let mut last_flush = Instant::now();
        let mut last_byte = Instant::now();

        let end = loop {
            if self.stopped() {
                break BodyEnd::Stopped;
            }
            let idle_left = self.idle.saturating_sub(last_byte.elapsed());
            if idle_left.is_zero() {
                log::debug!(
                    "[ProxyDM] body idle offset={} written={} for {}s",
                    at,
                    written,
                    self.idle.as_secs()
                );
                break BodyEnd::Idle;
            }
            // The read never blocks longer than STOP_POLL, so a stop is seen
            // while the server is silent instead of after the idle window.
            let read = tokio::time::timeout(idle_left.min(STOP_POLL), stream.next());
            // Flush while the next read is still blocked, so a small chunk is
            // visible within PROGRESS_FLUSH instead of sitting until the next packet.
            let next = if buf.is_empty() {
                read.await
            } else {
                tokio::select! {
                    biased;
                    _ = tokio::time::sleep(PROGRESS_FLUSH.saturating_sub(last_flush.elapsed())) => {
                        match flush(file, &mut buf, at + written, on_write) {
                            Ok(n) => written += n,
                            Err(e) => break BodyEnd::WriteFailed(e),
                        }
                        last_flush = Instant::now();
                        continue;
                    }
                    next = read => next,
                }
            };
            let chunk = match next {
                Ok(Some(Ok(chunk))) => chunk,
                Ok(Some(Err(e))) => break BodyEnd::Broken(e.to_string()),
                Ok(None) => break BodyEnd::Ended,
                Err(_) => continue,
            };
            if chunk.is_empty() {
                continue;
            }
            self.perf.note_body();
            // A stop ends the throttle wait: the bytes are already here, and
            // a tight limit must not hold a pause.
            tokio::select! {
                biased;
                _ = self.limiter.wait_n(chunk.len() as u64) => {}
                _ = until_cancelled(Some(&self.stop)) => {}
            }
            // Idle starts after throttling. A user limit must not look like a stall.
            last_byte = Instant::now();

            buf.extend_from_slice(&chunk);

            // Bound the write to what was asked for: a server that ignores
            // Range on the offset-0 task streams the WHOLE file, and
            // everything past the cap belongs to other tasks.
            if cap > 0 && written + buf.len() as u64 >= cap {
                buf.truncate((cap - written) as usize);
                break BodyEnd::Ended;
            }
            if buf.len() >= WRITE_BUFFER {
                match flush(file, &mut buf, at + written, on_write) {
                    Ok(n) => written += n,
                    Err(e) => break BodyEnd::WriteFailed(e),
                }
                last_flush = Instant::now();
            }
        };

        // Whatever is buffered goes to disk before the rest is handed back.
        let end = match flush(file, &mut buf, at + written, on_write) {
            Ok(n) => {
                written += n;
                end
            }
            Err(e) => BodyEnd::WriteFailed(e),
        };

        let remaining = if task.length == 0 {
            // An open-ended body that ended on its own is all there is.
            if matches!(end, BodyEnd::Ended) {
                return Attempt::Complete;
            }
            Task {
                offset: at + written,
                length: 0,
            }
        } else if written < task.length {
            Task {
                offset: at + written,
                length: task.length - written,
            }
        } else {
            return Attempt::Complete;
        };
        // Bytes that reached the file are never fetched twice: the rest is
        // handed back even when this request ended in an error.
        if written > 0 || self.stopped() {
            return Attempt::Partial { remaining };
        }
        Attempt::Failed(match end {
            BodyEnd::WriteFailed(e) => PdmError::Io(format!("write_at error: {e}")),
            BodyEnd::Idle => PdmError::Timeout(format!("no data for {}s", self.idle.as_secs())),
            BodyEnd::Broken(e) => PdmError::Network(format!("Stream error: {e}")),
            BodyEnd::Ended | BodyEnd::Stopped => {
                PdmError::Network("the body ended before any data".into())
            }
        })
    }

    /// Fetch `want` to the end: retries with backoff, continues after a body
    /// that broke off, and returns as soon as the stop flag is set.
    /// `on_phase` hears `Retrying` before a backoff and `Downloading` after it.
    pub async fn fetch(
        &self,
        url: &str,
        file: &std::fs::File,
        want: Want,
        expected_total: u64,
        on_write: &(dyn Fn(u64, u64) + Sync),
        on_phase: &(dyn Fn(Phase) + Sync),
    ) -> Fetched {
        // A server asked for the whole object has not shown it honors Range,
        // so a broken body starts over instead of asking for the tail.
        let whole = want == Want::Whole;
        let mut want = want;
        let mut budget = self.retry_budget();
        // Furthest byte any attempt reached: only passing it is progress, so
        // a body that keeps restarting from 0 cannot retry forever.
        let mut reached = match &want {
            Want::Range(task) => task.offset,
            Want::Whole => 0,
        };
        loop {
            let failure = match self
                .attempt(url, file, &want, expected_total, on_write)
                .await
            {
                Attempt::Complete => return Fetched::Complete,
                Attempt::RangeLost => return Fetched::RangeLost,
                Attempt::Refused(code) => return Fetched::Failed(PdmError::Http(code)),
                Attempt::Failed(error) => error,
                Attempt::Partial { remaining } => {
                    if self.stopped() {
                        return Fetched::Stopped;
                    }
                    self.perf.note_stall();
                    let offset = remaining.offset;
                    if !whole {
                        want = Want::Range(remaining);
                    }
                    if offset > reached {
                        reached = offset;
                        budget.reset();
                        continue;
                    }
                    PdmError::Network("the body broke off without new data".into())
                }
            };
            if self.stopped() {
                return Fetched::Stopped;
            }
            let Some(delay) = budget.take() else {
                return Fetched::Failed(PdmError::RetriesExhausted(Box::new(failure)));
            };
            log::warn!("[ProxyDM] transfer failed, retrying in {delay:?}: {failure}");
            self.perf.note_retry();
            on_phase(Phase::Retrying);
            if !self.wait(delay).await {
                return Fetched::Stopped;
            }
            on_phase(Phase::Downloading);
        }
    }
}

/// How a response body stopped being read.
enum BodyEnd {
    /// The server finished it, or it reached the cap.
    Ended,
    Stopped,
    /// No bytes for the whole idle window.
    Idle,
    Broken(String),
    WriteFailed(std::io::Error),
}

fn flush(
    file: &std::fs::File,
    buf: &mut Vec<u8>,
    at: u64,
    on_write: &(dyn Fn(u64, u64) + Sync),
) -> std::io::Result<u64> {
    if buf.is_empty() {
        return Ok(0);
    }
    write_at(file, buf, at)?;
    let n = buf.len() as u64;
    on_write(at, n);
    buf.clear();
    Ok(n)
}

/// `Content-Range: bytes start-end/total` (`total` may be `*`).
fn parse_content_range(header: &str) -> Option<(u64, u64, Option<u64>)> {
    let rest = header.trim().strip_prefix("bytes ")?;
    let (range, total) = rest.split_once('/')?;
    let (start, end) = range.split_once('-')?;
    let start: u64 = start.trim().parse().ok()?;
    let end: u64 = end.trim().parse().ok()?;
    let total = match total.trim() {
        "*" => None,
        s => Some(s.parse().ok()?),
    };
    Some((start, end, total))
}

/// A 206 is usable only when the returned interval sits inside the request
/// and, when we already know the object size, the total matches.
fn validate_content_range(
    requested_start: u64,
    requested_end: u64,
    actual_start: u64,
    actual_end: u64,
    actual_total: Option<u64>,
    expected_total: u64,
) -> bool {
    if actual_start != requested_start || actual_end < actual_start || actual_end > requested_end {
        return false;
    }
    if expected_total > 0 {
        if let Some(total) = actual_total {
            if total != expected_total {
                return false;
            }
        }
    }
    true
}

/// Why waiting for response headers failed. The body is not covered:
/// reqwest's request timeout would abort a multi-gigabyte transfer.
#[derive(Debug)]
pub enum HeaderWait {
    Cancelled,
    TimedOut,
    Failed(String),
}

impl std::fmt::Display for HeaderWait {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Cancelled => f.write_str("cancelled"),
            Self::TimedOut => f.write_str("timed out waiting for response headers"),
            Self::Failed(msg) => f.write_str(msg),
        }
    }
}

/// Send a request and wait only for the response headers.
/// `cancel` is polled while the headers are outstanding so pause does not
/// sit behind a 30s header timeout.
pub async fn send_headers(
    req: reqwest::RequestBuilder,
    cancel: Option<&AtomicBool>,
) -> Result<reqwest::Response, HeaderWait> {
    let fut = req.send();
    tokio::pin!(fut);
    let timeout = tokio::time::sleep(std::time::Duration::from_secs(30));
    tokio::pin!(timeout);
    tokio::select! {
        biased;
        _ = until_cancelled(cancel) => Err(HeaderWait::Cancelled),
        result = &mut fut => match result {
            Ok(resp) => Ok(resp),
            Err(e) => {
                let mut msg = e.to_string();
                let mut src = std::error::Error::source(&e);
                while let Some(s) = src {
                    msg.push_str(&format!(": {s}"));
                    src = s.source();
                }
                Err(HeaderWait::Failed(msg))
            }
        },
        _ = &mut timeout => Err(HeaderWait::TimedOut),
    }
}

async fn until_cancelled(cancel: Option<&AtomicBool>) {
    let Some(flag) = cancel else {
        std::future::pending::<()>().await;
        return;
    };
    loop {
        if flag.load(Ordering::Relaxed) {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, AtomicUsize};
    use std::sync::Mutex;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    #[test]
    fn content_range_accepts_exact_match() {
        assert!(validate_content_range(0, 99, 0, 99, Some(100), 100));
        assert!(validate_content_range(50, 99, 50, 80, Some(100), 100));
    }

    #[test]
    fn content_range_rejects_bad_start_end_and_total() {
        assert!(!validate_content_range(10, 20, 0, 20, Some(100), 100));
        assert!(!validate_content_range(0, 10, 0, 11, Some(100), 100));
        assert!(!validate_content_range(0, 10, 5, 4, Some(100), 100));
        assert!(!validate_content_range(0, 10, 0, 10, Some(99), 100));
    }

    #[test]
    fn content_range_missing_total_ok_when_size_unknown() {
        assert!(validate_content_range(0, 10, 0, 10, None, 0));
        assert!(validate_content_range(0, 10, 0, 10, None, 100));
    }

    #[test]
    fn parse_content_range_star_total() {
        assert_eq!(parse_content_range("bytes 0-9/*"), Some((0, 9, None)));
        assert_eq!(
            parse_content_range("bytes 8-15/100"),
            Some((8, 15, Some(100)))
        );
        assert_eq!(parse_content_range("not-a-range"), None);
    }

    #[test]
    fn body_idle_is_thirty_seconds() {
        assert_eq!(BODY_IDLE, Duration::from_secs(30));
    }

    #[test]
    fn a_retry_budget_runs_out_and_refills_on_progress() {
        let mut budget = RetryBudget {
            max: 2,
            left: 2,
            backoff: |attempt| Duration::from_secs(attempt as u64),
        };
        assert_eq!(budget.take(), Some(Duration::from_secs(1)));
        assert_eq!(budget.take(), Some(Duration::from_secs(2)));
        assert_eq!(budget.take(), None);
        budget.reset();
        assert_eq!(budget.take(), Some(Duration::from_secs(1)));
    }

    /// What the origin does with one connection. Connections take the steps
    /// in order; once they run out the origin stops accepting.
    #[derive(Clone)]
    enum Step {
        /// Close without answering.
        Drop,
        /// Answer with this status and no body.
        Status(u16),
        /// 200 with the whole object, whatever was asked.
        Whole,
        /// 200 with the whole object's length, cut off after this many bytes.
        Cut(usize),
        /// 206 with the bytes asked for.
        Range,
        /// 206, one byte at a time, faster than the idle window.
        Trickle,
        /// 206, a few bytes, then silence while the socket stays open.
        Stall,
        /// 206, a first slice, a pause, then the rest. The pause is long
        /// enough for the client to read the slice and enter the limiter.
        Split { first: usize, gap: Duration },
    }

    /// Byte `i` of the object every step serves.
    fn object(len: u64) -> Vec<u8> {
        (0..len).map(|i| (i % 251) as u8).collect()
    }

    /// `(start, end)` of the Range header, `end` inclusive when given.
    fn requested_range(req: &str) -> Option<(u64, Option<u64>)> {
        for line in req.lines() {
            let line = line.trim();
            if line.len() < 6 || !line[..6].eq_ignore_ascii_case("range:") {
                continue;
            }
            let spec = line[6..].trim().strip_prefix("bytes=")?;
            let (start, end) = spec.split_once('-')?;
            let end = end.trim();
            return Some((
                start.trim().parse().ok()?,
                if end.is_empty() {
                    None
                } else {
                    Some(end.parse().ok()?)
                },
            ));
        }
        None
    }

    async fn read_http(stream: &mut tokio::net::TcpStream) -> Option<String> {
        let mut total = Vec::new();
        let mut buf = [0u8; 2048];
        loop {
            let n = stream.read(&mut buf).await.unwrap_or(0);
            if n == 0 {
                return None;
            }
            total.extend_from_slice(&buf[..n]);
            if total.windows(4).any(|w| w == b"\r\n\r\n") || total.len() > 8192 {
                break;
            }
        }
        Some(String::from_utf8_lossy(&total).into_owned())
    }

    /// A local origin for an object of `total` bytes. Returns its URL and
    /// the number of requests it has answered.
    async fn origin(total: u64, steps: Vec<Step>) -> (String, Arc<AtomicUsize>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let hits = Arc::new(AtomicUsize::new(0));
        let seen = hits.clone();
        tokio::spawn(async move {
            for step in steps {
                let Ok((stream, _)) = listener.accept().await else {
                    return;
                };
                tokio::spawn(answer(stream, step, total, seen.clone()));
            }
        });
        (format!("http://127.0.0.1:{port}/file.bin"), hits)
    }

    async fn answer(
        mut stream: tokio::net::TcpStream,
        step: Step,
        total: u64,
        hits: Arc<AtomicUsize>,
    ) {
        let Some(req) = read_http(&mut stream).await else {
            return;
        };
        hits.fetch_add(1, Ordering::Relaxed);
        let body = object(total);
        let (start, end) = match requested_range(&req) {
            Some((start, end)) => (start, end.unwrap_or(total - 1)),
            None => (0, total - 1),
        };
        let slice = &body[start as usize..=end as usize];
        let partial = format!(
            "HTTP/1.1 206 Partial Content\r\nContent-Range: bytes {start}-{end}/{total}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            slice.len()
        );
        let whole =
            format!("HTTP/1.1 200 OK\r\nContent-Length: {total}\r\nConnection: close\r\n\r\n");
        match step {
            Step::Drop => return,
            Step::Status(code) => {
                let head = format!(
                    "HTTP/1.1 {code} Status\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                );
                let _ = stream.write_all(head.as_bytes()).await;
            }
            Step::Whole => {
                let _ = stream.write_all(whole.as_bytes()).await;
                let _ = stream.write_all(&body).await;
            }
            Step::Cut(sent) => {
                let _ = stream.write_all(whole.as_bytes()).await;
                let _ = stream.write_all(&body[..sent]).await;
                let _ = stream.flush().await;
                return;
            }
            Step::Range => {
                let _ = stream.write_all(partial.as_bytes()).await;
                let _ = stream.write_all(slice).await;
            }
            Step::Trickle => {
                let _ = stream.write_all(partial.as_bytes()).await;
                for byte in slice {
                    if stream.write_all(&[*byte]).await.is_err() {
                        return;
                    }
                    let _ = stream.flush().await;
                    tokio::time::sleep(Duration::from_millis(40)).await;
                }
            }
            Step::Stall => {
                let _ = stream.write_all(partial.as_bytes()).await;
                let _ = stream.write_all(&slice[..128.min(slice.len())]).await;
                let _ = stream.flush().await;
                tokio::time::sleep(Duration::from_secs(3)).await;
                return;
            }
            Step::Split { first, gap } => {
                let first = first.min(slice.len());
                let _ = stream.write_all(partial.as_bytes()).await;
                let _ = stream.write_all(&slice[..first]).await;
                let _ = stream.flush().await;
                tokio::time::sleep(gap).await;
                let _ = stream.write_all(&slice[first..]).await;
            }
        }
        let _ = stream.shutdown().await;
    }

    /// A transfer writing into its own temp file, with every write recorded.
    struct Bench {
        transfer: Transfer,
        file: std::fs::File,
        path: std::path::PathBuf,
        /// Bytes written, counting a restart's rewrites again.
        bytes: AtomicU64,
        /// End of the last span written.
        end: AtomicU64,
        phases: Mutex<Vec<Phase>>,
    }

    impl Bench {
        fn new(idle: Duration, bps: u64, max_retries: u32) -> Self {
            static NEXT: AtomicU64 = AtomicU64::new(1);
            let path = std::env::temp_dir().join(format!(
                "pdm_transfer_{}_{}.bin",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            let file = std::fs::OpenOptions::new()
                .create(true)
                .write(true)
                .read(true)
                .truncate(true)
                .open(&path)
                .unwrap();
            Self {
                transfer: Transfer {
                    client: reqwest::Client::builder().build().unwrap(),
                    headers: Arc::new(HashMap::new()),
                    user_agent: "pdm-test".into(),
                    limiter: Arc::new(MultiLimiter::new(bps, 0)),
                    stop: Arc::new(AtomicBool::new(false)),
                    max_retries,
                    perf: Arc::new(PerfStats::new(0)),
                    idle,
                    backoff: |_| Duration::from_millis(20),
                },
                file,
                path,
                bytes: AtomicU64::new(0),
                end: AtomicU64::new(0),
                phases: Mutex::new(Vec::new()),
            }
        }

        fn on_write(&self) -> impl Fn(u64, u64) + Sync + '_ {
            |offset, len| {
                self.bytes.fetch_add(len, Ordering::Relaxed);
                self.end.store(offset + len, Ordering::Relaxed);
            }
        }

        async fn attempt(&self, url: &str, want: Want, total: u64) -> Attempt {
            self.transfer
                .attempt(url, &self.file, &want, total, &self.on_write())
                .await
        }

        async fn fetch(&self, url: &str, want: Want, total: u64) -> Fetched {
            self.transfer
                .fetch(url, &self.file, want, total, &self.on_write(), &|phase| {
                    self.phases.lock().unwrap().push(phase)
                })
                .await
        }

        fn on_disk(&self) -> Vec<u8> {
            std::fs::read(&self.path).unwrap()
        }
    }

    impl Drop for Bench {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.path);
        }
    }

    fn first(len: u64) -> Want {
        Want::Range(Task {
            offset: 0,
            length: len,
        })
    }

    #[tokio::test]
    async fn slow_but_continuous_body_is_not_a_stall() {
        let total = 25u64;
        let (url, _) = origin(total, vec![Step::Trickle]).await;
        let bench = Bench::new(Duration::from_millis(300), 0, 0);
        let result = bench.attempt(&url, first(total), total).await;
        assert_eq!(result, Attempt::Complete);
        assert_eq!(bench.on_disk(), object(total));
    }

    #[tokio::test]
    async fn silent_body_is_a_stall() {
        let total = 4096u64;
        let (url, _) = origin(total, vec![Step::Stall]).await;
        let bench = Bench::new(Duration::from_millis(400), 0, 0);
        let started = Instant::now();
        let result = bench.attempt(&url, first(total), total).await;
        let elapsed = started.elapsed();
        assert_eq!(
            result,
            Attempt::Partial {
                remaining: Task {
                    offset: 128,
                    length: total - 128,
                }
            },
            "after {elapsed:?}"
        );
        assert!(
            elapsed < Duration::from_secs(2),
            "idle waited {elapsed:?} instead of ~400ms"
        );
    }

    #[tokio::test]
    async fn a_stop_during_a_silent_body_returns_at_once() {
        let total = 4096u64;
        let (url, _) = origin(total, vec![Step::Stall]).await;
        // The real idle window: only the stop flag can end this read early.
        let bench = Bench::new(BODY_IDLE, 0, 0);
        let stop = bench.transfer.stop.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(300)).await;
            stop.store(true, Ordering::Relaxed);
        });
        let started = Instant::now();
        let result = bench.attempt(&url, first(total), total).await;
        let elapsed = started.elapsed();
        assert_eq!(
            result,
            Attempt::Partial {
                remaining: Task {
                    offset: 128,
                    length: total - 128,
                }
            }
        );
        assert!(
            elapsed < Duration::from_secs(1),
            "stop waited {elapsed:?} on a silent body"
        );
    }

    #[tokio::test]
    async fn limiter_wait_is_not_a_body_stall() {
        let total = 16 * 1024u64;
        let split = Step::Split {
            first: 8 * 1024,
            gap: Duration::from_millis(400),
        };
        let (url, _) = origin(total, vec![split]).await;
        // 8 KiB at 8 KiB/s waits about a second, longer than the 250ms idle.
        let bench = Bench::new(Duration::from_millis(250), 8 * 1024, 0);
        let started = Instant::now();
        let result = bench.attempt(&url, first(total), total).await;
        let elapsed = started.elapsed();
        assert_eq!(
            result,
            Attempt::Complete,
            "throttled read looked like a stall after {elapsed:?}"
        );
        assert_eq!(bench.on_disk(), object(total));
        assert!(
            elapsed > Duration::from_millis(600),
            "limiter did not wait: {elapsed:?}"
        );
    }

    #[tokio::test]
    async fn progress_is_visible_before_a_one_megabyte_buffer() {
        let total = 64 * 1024u64;
        let split = Step::Split {
            first: 4096,
            gap: Duration::from_millis(800),
        };
        let (url, _) = origin(total, vec![split]).await;
        let bench = Arc::new(Bench::new(Duration::from_secs(5), 0, 0));
        let running = bench.clone();
        let handle = tokio::spawn(async move { running.attempt(&url, first(total), total).await });
        let started = Instant::now();
        let mut seen = 0u64;
        while started.elapsed() < Duration::from_millis(600) {
            seen = bench.bytes.load(Ordering::Relaxed);
            if seen > 0 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        assert!(seen > 0, "no bytes flushed within 600ms");
        assert!(
            seen < total,
            "full file was buffered before any progress: {seen}"
        );
        assert_eq!(handle.await.unwrap(), Attempt::Complete);
    }

    #[tokio::test]
    async fn fetch_retries_a_dropped_connection_until_it_gets_through() {
        let total = 4000u64;
        let (url, hits) = origin(total, vec![Step::Drop, Step::Status(503), Step::Whole]).await;
        let bench = Bench::new(BODY_IDLE, 0, 3);
        assert_eq!(
            bench.fetch(&url, Want::Whole, total).await,
            Fetched::Complete
        );
        assert_eq!(bench.on_disk(), object(total));
        assert_eq!(hits.load(Ordering::Relaxed), 3);
        assert_eq!(
            *bench.phases.lock().unwrap(),
            vec![
                Phase::Retrying,
                Phase::Downloading,
                Phase::Retrying,
                Phase::Downloading
            ]
        );
    }

    #[tokio::test]
    async fn fetch_fails_with_the_last_cause_once_the_budget_is_spent() {
        let total = 4000u64;
        let steps = vec![Step::Status(503), Step::Status(503), Step::Status(503)];
        let (url, hits) = origin(total, steps).await;
        let bench = Bench::new(BODY_IDLE, 0, 2);
        assert_eq!(
            bench.fetch(&url, Want::Whole, total).await,
            Fetched::Failed(PdmError::RetriesExhausted(Box::new(PdmError::Http(503))))
        );
        assert_eq!(hits.load(Ordering::Relaxed), 3);
    }

    #[tokio::test]
    async fn fetch_does_not_retry_a_refusal() {
        let total = 4000u64;
        let (url, hits) = origin(total, vec![Step::Status(404), Step::Whole]).await;
        let bench = Bench::new(BODY_IDLE, 0, 5);
        assert_eq!(
            bench.fetch(&url, Want::Whole, total).await,
            Fetched::Failed(PdmError::Http(404))
        );
        assert_eq!(hits.load(Ordering::Relaxed), 1);
    }

    #[tokio::test]
    async fn a_whole_body_that_breaks_off_starts_over() {
        let total = 4000u64;
        let (url, hits) = origin(total, vec![Step::Cut(1000), Step::Whole]).await;
        let bench = Bench::new(BODY_IDLE, 0, 0);
        assert_eq!(
            bench.fetch(&url, Want::Whole, total).await,
            Fetched::Complete
        );
        assert_eq!(bench.on_disk(), object(total));
        assert_eq!(bench.end.load(Ordering::Relaxed), total);
        assert_eq!(hits.load(Ordering::Relaxed), 2);
    }

    #[tokio::test]
    async fn a_whole_body_that_never_gets_further_gives_up() {
        let total = 4000u64;
        let steps = vec![Step::Cut(1000), Step::Cut(1000), Step::Cut(900)];
        let (url, hits) = origin(total, steps).await;
        let bench = Bench::new(BODY_IDLE, 0, 1);
        assert!(matches!(
            bench.fetch(&url, Want::Whole, total).await,
            Fetched::Failed(PdmError::RetriesExhausted(_))
        ));
        assert_eq!(hits.load(Ordering::Relaxed), 3);
    }

    #[tokio::test]
    async fn a_ranged_body_that_breaks_off_asks_for_the_rest() {
        let total = 4096u64;
        let (url, hits) = origin(total, vec![Step::Stall, Step::Range]).await;
        let bench = Bench::new(Duration::from_millis(300), 0, 0);
        assert_eq!(
            bench.fetch(&url, first(total), total).await,
            Fetched::Complete
        );
        assert_eq!(bench.on_disk(), object(total));
        // The second request only carried what the first one left.
        assert_eq!(bench.bytes.load(Ordering::Relaxed), total);
        assert_eq!(hits.load(Ordering::Relaxed), 2);
    }

    fn tail(offset: u64) -> Want {
        Want::Range(Task { offset, length: 0 })
    }

    #[tokio::test]
    async fn an_honored_tail_is_appended_where_it_belongs() {
        let total = 4000u64;
        let (url, _) = origin(total, vec![Step::Range]).await;
        let bench = Bench::new(BODY_IDLE, 0, 0);
        write_at(&bench.file, &object(total)[..1000], 0).unwrap();
        assert_eq!(
            bench.fetch(&url, tail(1000), total).await,
            Fetched::Complete
        );
        assert_eq!(bench.on_disk(), object(total));
        assert_eq!(bench.bytes.load(Ordering::Relaxed), 3000);
    }

    #[tokio::test]
    async fn an_ignored_tail_replaces_the_file_from_the_start() {
        let total = 4000u64;
        let (url, _) = origin(total, vec![Step::Whole]).await;
        let bench = Bench::new(BODY_IDLE, 0, 0);
        // Stale bytes past the object's end must not survive the restart.
        write_at(&bench.file, &[0xEE; 6000], 0).unwrap();
        assert_eq!(
            bench.fetch(&url, tail(1000), total).await,
            Fetched::Complete
        );
        assert_eq!(bench.on_disk(), object(total));
        assert_eq!(bench.end.load(Ordering::Relaxed), total);
    }

    #[tokio::test]
    async fn a_bounded_range_answered_with_the_whole_object_is_range_lost() {
        let total = 4000u64;
        let (url, _) = origin(total, vec![Step::Whole]).await;
        let bench = Bench::new(BODY_IDLE, 0, 3);
        let want = Want::Range(Task {
            offset: 1000,
            length: 1000,
        });
        assert_eq!(bench.fetch(&url, want, total).await, Fetched::RangeLost);
        assert_eq!(bench.bytes.load(Ordering::Relaxed), 0);
    }

    #[tokio::test]
    async fn a_stop_during_backoff_ends_the_fetch() {
        let total = 4000u64;
        let (url, hits) = origin(total, vec![Step::Status(503), Step::Whole]).await;
        let mut bench = Bench::new(BODY_IDLE, 0, 3);
        bench.transfer.backoff = |_| Duration::from_secs(30);
        let stop = bench.transfer.stop.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(300)).await;
            stop.store(true, Ordering::Relaxed);
        });
        let started = Instant::now();
        assert_eq!(
            bench.fetch(&url, Want::Whole, total).await,
            Fetched::Stopped
        );
        assert!(started.elapsed() < Duration::from_secs(2));
        assert_eq!(hits.load(Ordering::Relaxed), 1);
    }
}
