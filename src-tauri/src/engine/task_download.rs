use crate::network::limiter::MultiLimiter;
use crate::types::{Task, PdmError};
use crate::engine::file_io::write_at;
use crate::engine::part_progress::PartProgressTracker;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

/// Outcome of a single chunk download attempt.
#[derive(Debug, PartialEq)]
pub enum TaskResult {
    /// Chunk fully downloaded.
    Complete,
    /// Partial progress: remaining bytes should be re-queued.
    Partial { remaining: Task },
    /// User cancelled.
    Cancelled,
    /// Unrecoverable error — don't retry this chunk.
    Fatal(String),
}

fn note_write(
    bytes_written: &AtomicU64,
    parts: Option<&PartProgressTracker>,
    file_offset: u64,
    len: u64,
) {
    if len == 0 {
        return;
    }
    bytes_written.fetch_add(len, Ordering::Relaxed);
    if let Some(p) = parts {
        p.record_write(file_offset, len);
    }
}

pub async fn download_task(
    url: &str,
    client: &reqwest::Client,
    file: &std::fs::File,
    task: &Task,
    cancel: &AtomicBool,
    limiter: &MultiLimiter,
    user_agent: &str,
    bytes_written: &AtomicU64,
    parts: Option<Arc<PartProgressTracker>>,
) -> TaskResult {
    let mut written: u64 = 0;
    let range_end = if task.length == 0 {
        String::new()
    } else {
        format!("{}", task.offset + task.length - 1)
    };
    let range_header = format!("bytes={}-{}", task.offset, range_end);
    let mut req = client
        .get(url)
        .header("Range", &range_header);
    if !user_agent.is_empty() {
        req = req.header("User-Agent", user_agent);
    }
    log::info!("[ProxyDM] concurrent_task offset={} range_end={}", task.offset, range_end);
    let resp = match req.send().await {
        Ok(r) => r,
        Err(e) => {
            let mut msg = format!("Request failed: {}", e);
            let mut src = std::error::Error::source(&e);
            while let Some(s) = src {
                msg.push_str(&format!(": {}", s));
                src = s.source();
            }
            log::error!("[ProxyDM] concurrent_task REQUEST ERROR offset={}: {}", task.offset, msg);
            return TaskResult::Fatal(msg);
        }
    };

    if cancel.load(Ordering::Relaxed) {
        return TaskResult::Cancelled;
    }

    let status = resp.status();
    log::info!("[ProxyDM] concurrent_task offset={} HTTP {} (expected 206 or 200)", task.offset, status);

    // For offset > 0: 200 means server ignored Range — fatal
    if status == reqwest::StatusCode::OK && task.offset > 0 {
        return TaskResult::Fatal(format!("Server ignored Range header (HTTP 200), offset={}", task.offset));
    }
    if status != reqwest::StatusCode::OK && status != reqwest::StatusCode::PARTIAL_CONTENT {
        return TaskResult::Fatal(format!("HTTP {}", status));
    }

    let stream = resp.bytes_stream();
    use futures_util::StreamExt;
    let mut stream = std::pin::pin!(stream);
    let base_offset = task.offset;
    let chunk_size = task.length;

    const BUF_SIZE: usize = 1024 * 1024; // 1MB
    let mut buf = Vec::with_capacity(BUF_SIZE);

    // Slow chunk detection: if >30s elapsed and <10% done, abort
    let start_time = std::time::Instant::now();

    loop {
        // Check cancel (responsive Stop even during streaming)
        if cancel.load(Ordering::Relaxed) {
            // Flush buffered data before returning to avoid data loss
            if !buf.is_empty() {
                if let Err(e) = write_at(file, &buf, base_offset + written) {
                    return TaskResult::Fatal(format!("write_at error on cancel: {}", e));
                }
                let n = buf.len() as u64;
                note_write(bytes_written, parts.as_deref(), base_offset + written, n);
                written += n;
                buf.clear();
            }
            let remaining = chunk_size.saturating_sub(written);
            if remaining > 0 {
                return TaskResult::Partial {
                    remaining: Task { offset: base_offset + written, length: remaining },
                };
            }
            return TaskResult::Cancelled;
        }

        // Abort slow chunks so other workers can steal remaining work
        let elapsed = start_time.elapsed();
        if elapsed > std::time::Duration::from_secs(30)
            && chunk_size > 0
            && written < chunk_size / 10
        {
            log::info!("[ProxyDM] slow chunk offset={} written={}/{} after {}s, re-queuing",
                base_offset, written, chunk_size, elapsed.as_secs());
            let remaining = chunk_size.saturating_sub(written);
            return TaskResult::Partial {
                remaining: Task { offset: base_offset + written, length: remaining },
            };
        }

        let chunk_result = tokio::time::timeout(
            std::time::Duration::from_secs(10), stream.next()
        ).await;
        let chunk = match chunk_result {
            Ok(Some(Ok(c))) => c,
            Ok(Some(Err(e))) => {
                let remaining = chunk_size.saturating_sub(written);
                if remaining > 0 && written > 0 {
                    return TaskResult::Partial {
                        remaining: Task { offset: base_offset + written, length: remaining },
                    };
                }
                return TaskResult::Fatal(format!("Stream error: {}", e));
            }
            Ok(None) => {
                if !buf.is_empty() {
                    if let Err(e) = write_at(file, &buf, base_offset + written) {
                        return TaskResult::Fatal(format!("write_at error: {}", e));
                    }
                    let n = buf.len() as u64;
                    note_write(bytes_written, parts.as_deref(), base_offset + written, n);
                    written += n;
                }
                break;
            }
            Err(_elapsed) => {
                if cancel.load(Ordering::Relaxed) {
                    // Flush buffered data before returning
                    if !buf.is_empty() {
                        if let Err(e) = write_at(file, &buf, base_offset + written) {
                            return TaskResult::Fatal(format!("write_at error on cancel: {}", e));
                        }
                        let n = buf.len() as u64;
                        note_write(bytes_written, parts.as_deref(), base_offset + written, n);
                        written += n;
                        buf.clear();
                    }
                    let remaining = chunk_size.saturating_sub(written);
                    if remaining > 0 {
                        return TaskResult::Partial {
                            remaining: Task { offset: base_offset + written, length: remaining },
                        };
                    }
                    return TaskResult::Cancelled;
                }
                continue;
            }
        };
        limiter.wait_n(chunk.len() as u64).await;

        buf.extend_from_slice(&chunk);

        if buf.len() >= BUF_SIZE {
            if let Err(e) = write_at(file, &buf, base_offset + written) {
                return TaskResult::Fatal(format!("write_at error: {}", e));
            }
            let n = buf.len() as u64;
            note_write(bytes_written, parts.as_deref(), base_offset + written, n);
            written += n;
            buf.clear();
        }
    }

    TaskResult::Complete
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn task_result_complete_is_not_partial() {
        assert_eq!(TaskResult::Complete, TaskResult::Complete);
        assert_ne!(TaskResult::Complete, TaskResult::Cancelled);
    }

    #[test]
    fn task_result_partial_has_remaining() {
        let remaining = Task { offset: 3000, length: 2000 };
        let r = TaskResult::Partial { remaining: remaining.clone() };
        if let TaskResult::Partial { remaining } = r {
            assert_eq!(remaining.offset, 3000);
            assert_eq!(remaining.length, 2000);
        } else {
            panic!("expected Partial");
        }
    }

    #[test]
    fn task_result_fatal_contains_message() {
        let r = TaskResult::Fatal("HTTP 403".to_string());
        if let TaskResult::Fatal(msg) = r {
            assert_eq!(msg, "HTTP 403");
        } else {
            panic!("expected Fatal");
        }
    }
}
