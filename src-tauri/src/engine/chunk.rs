use crate::types::{DownloadPart, PartStatus, PdmError, PdmResult, Task};
use std::collections::VecDeque;
use std::sync::Mutex;

const ALIGN: u64 = 4096;
pub const MAX_CONNECTIONS: u32 = 64;
const MIN_SPLIT: u64 = 2 * 1024 * 1024;
/// Minimum tail size an idle worker may steal from an active in-flight lease.
pub const MIN_INFLIGHT_SPLIT: u64 = 1024 * 1024;
/// Smallest planned range. Small enough that Auto can add workers all the way
/// to its ceiling without splitting an in-flight transfer.
const MIN_CHUNK: u64 = 2 * 1024 * 1024;

pub fn align_up(v: u64) -> u64 {
    (v + ALIGN - 1) & !(ALIGN - 1)
}

/// Compute chunk size so each worker handles ~20 chunks.
/// Formula: file_size / (connections × 20), clamped to [MIN_CHUNK, 64MB].
fn dynamic_chunk_size(file_size: u64, connections: u32) -> u64 {
    let conns = connections.max(1);
    let target = file_size / (conns as u64 * 20); // ~20 chunks/worker
    target.max(MIN_CHUNK).min(64 * 1024 * 1024) // 64MB max
}

pub fn compute_chunks(file_size: u64, num_chunks: u32, _min_chunk_size: u64) -> Vec<Task> {
    if num_chunks == 0 {
        return vec![Task {
            offset: 0,
            length: file_size,
        }];
    }
    let chunk_size =
        dynamic_chunk_size(file_size, num_chunks).max(align_up(file_size / num_chunks as u64));
    let chunk_size = align_up(chunk_size);

    let mut tasks = Vec::new();
    let mut offset = 0u64;
    while offset < file_size {
        let length = if offset + chunk_size > file_size {
            file_size - offset
        } else {
            chunk_size
        };
        tasks.push(Task { offset, length });
        offset += chunk_size;
    }
    tasks
}

// ── Chunk planning (moved from services/chunk_planner.rs) ──

pub struct ChunkPlan {
    pub connections: u32,
    pub parts: Vec<DownloadPart>,
}

pub fn plan_chunks(
    file_size: u64,
    requested_connections: u32,
    supports_range: bool,
    max_connections: u32,
) -> ChunkPlan {
    let connections = compute_connection_count(file_size, requested_connections, max_connections);
    let auto = requested_connections == 0 && max_connections == 0;

    let parts = if supports_range && file_size > 0 {
        let num_conns = if auto {
            // Auto plans one range per *potential* connection, not just the
            // conservative start, so the adaptive controller can add workers
            // without splitting a transfer that is already in flight.
            auto_plan_parts(file_size)
        } else if connections > 0 {
            connections.min(MAX_CONNECTIONS)
        } else {
            1
        };
        let min_chunk = 2u64 * 1024 * 1024;
        let tasks = compute_chunks(file_size, num_conns, min_chunk);
        tasks
            .iter()
            .enumerate()
            .map(|(i, t)| DownloadPart {
                index: i as u32,
                start: t.offset,
                end: t.offset + t.length,
                downloaded: 0,
                temp_path: String::new(),
                status: PartStatus::Pending,
                retries: 0,
            })
            .collect()
    } else {
        vec![DownloadPart {
            index: 0,
            start: 0,
            end: file_size,
            downloaded: 0,
            temp_path: String::new(),
            status: PartStatus::Pending,
            retries: 0,
        }]
    };

    ChunkPlan { connections, parts }
}

/// Number of ranges Auto plans for a file. Matches the adaptive ceiling so
/// every worker the controller can ask for has a queued range to take.
pub fn auto_plan_parts(file_size: u64) -> u32 {
    crate::engine::adaptive::ceiling_for(file_size).clamp(1, MAX_CONNECTIONS)
}

/// Resolve how many connections a download actually opens.
///
/// `default_connections` is the settings value. `0` means Auto and the count
/// comes from the file size. Any other value is the default thread count: a
/// task that did not choose its own count uses it directly and does not
/// size-detect. An explicit `requested` count is not clamped by that default.
pub fn compute_connection_count(file_size: u64, requested: u32, default_connections: u32) -> u32 {
    if requested > 0 {
        return requested.clamp(1, MAX_CONNECTIONS);
    }
    if default_connections > 0 {
        return default_connections.clamp(1, MAX_CONNECTIONS);
    }
    auto_initial_connections(file_size).clamp(1, MAX_CONNECTIONS)
}

/// Conservative Auto starting point. Not a cap: Auto raises it from measured
/// throughput via `engine::adaptive`.
///
/// < 2 MiB → 1, everything else → 4. Large files deliberately do not start at
/// 32/64; they ramp up only when the extra workers actually pay off.
pub fn auto_initial_connections(file_size: u64) -> u32 {
    const MIB: u64 = 1024 * 1024;
    if file_size == 0 {
        4
    } else if file_size < 2 * MIB {
        1
    } else {
        4
    }
}

/// Backwards-compatible alias. A proxy no longer lowers the Auto count — the
/// measured throughput decides, direct or proxied.
pub fn auto_connections(file_size: u64) -> u32 {
    auto_initial_connections(file_size)
}

pub fn auto_connections_for(file_size: u64, _via_proxy: bool) -> u32 {
    auto_initial_connections(file_size)
}

/// Check if there's enough disk space for the download.
pub fn check_disk_space(path: &str, file_size: u64) -> PdmResult<()> {
    if file_size > 0 {
        let save = std::path::Path::new(path);
        if let Some(parent) = save.parent() {
            if let Ok(available) = fs2::available_space(parent) {
                let needed = file_size + (2u64 * 1024 * 1024);
                if available < needed {
                    return Err(PdmError::Other(format!(
                        "Insufficient disk space: need {}, available {}",
                        needed, available
                    )));
                }
            }
        }
    }
    Ok(())
}

pub struct ChunkQueue {
    tasks: Mutex<VecDeque<Task>>,
}

impl ChunkQueue {
    pub fn new(tasks: Vec<Task>) -> Self {
        Self {
            tasks: Mutex::new(VecDeque::from(tasks)),
        }
    }

    pub fn pop(&self) -> Option<Task> {
        let mut tasks = self.tasks.lock().ok()?;
        tasks.pop_front()
    }

    pub fn push(&self, task: Task) {
        if let Ok(mut tasks) = self.tasks.lock() {
            tasks.push_back(task);
        }
    }

    pub fn drain(&self) -> Vec<Task> {
        let mut tasks = self.tasks.lock().unwrap();
        tasks.drain(..).collect()
    }

    pub fn is_empty(&self) -> bool {
        self.tasks.lock().map(|t| t.is_empty()).unwrap_or(true)
    }

    pub fn len(&self) -> usize {
        self.tasks.lock().map(|t| t.len()).unwrap_or(0)
    }

    pub fn remaining_bytes(&self) -> u64 {
        self.tasks
            .lock()
            .map(|t| t.iter().map(|task| task.length).sum())
            .unwrap_or(0)
    }

    /// Pop a task, splitting the largest remaining range in half when the
    /// queue would otherwise leave idle workers with one fat tail.
    pub fn pop_or_steal(&self) -> Option<Task> {
        let mut tasks = self.tasks.lock().ok()?;
        if let Some(task) = tasks.pop_front() {
            if task.length > MIN_SPLIT * 2 && tasks.is_empty() {
                let half = align_up(task.length / 2).min(task.length);
                if half >= MIN_SPLIT && task.length - half >= MIN_SPLIT {
                    tasks.push_back(Task {
                        offset: task.offset + half,
                        length: task.length - half,
                    });
                    return Some(Task {
                        offset: task.offset,
                        length: half,
                    });
                }
            }
            return Some(task);
        }
        None
    }

    /// Split the largest queued task so a newly idle worker can help.
    pub fn split_largest(&self, min_bytes: u64) -> Option<Task> {
        let mut tasks = self.tasks.lock().ok()?;
        let idx = tasks
            .iter()
            .enumerate()
            .max_by_key(|(_, t)| t.length)
            .map(|(i, t)| (i, t.length))?;
        if idx.1 < min_bytes.max(MIN_SPLIT) * 2 {
            return None;
        }
        let mut task = tasks.remove(idx.0)?;
        let half = align_up(task.length / 2).min(task.length);
        if half < MIN_SPLIT || task.length - half < MIN_SPLIT {
            tasks.insert(idx.0, task);
            return None;
        }
        let stolen = Task {
            offset: task.offset + half,
            length: task.length - half,
        };
        task.length = half;
        tasks.insert(idx.0, task);
        Some(stolen)
    }
}

/// Extent-style byte range lease held by one active worker.
///
/// Invariant: `committed_end <= inflight_end <= limit_end`.
/// - The owning worker reserves `[committed_end .. inflight_end)` before calling
///   `write_at` (which runs outside the lock) and advances `committed_end` once
///   the write completes.
/// - Idle workers may only split `[inflight_end .. limit_end)`, shrinking
///   `limit_end` atomically under the mutex. Two workers can never overlap a
///   single byte or leave an unwritten gap on pause/abort.
#[derive(Debug, Default)]
struct LeaseInner {
    committed_end: u64,
    inflight_end: u64,
    limit_end: u64,
    active: bool,
}

#[derive(Default)]
pub struct SpanLease {
    inner: Mutex<LeaseInner>,
}

impl SpanLease {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn activate(&self, start: u64, end: u64) {
        if let Ok(mut g) = self.inner.lock() {
            g.committed_end = start;
            g.inflight_end = start;
            g.limit_end = end.max(start);
            g.active = end > start;
        }
    }

    pub fn limit_end(&self) -> u64 {
        self.inner.lock().map(|g| g.limit_end).unwrap_or(0)
    }

    /// Reserve up to `want_len` bytes starting at `committed_end` for an
    /// upcoming disk write. Returns `(write_offset, allowed_bytes)`.
    pub fn begin_write(&self, want_len: usize) -> (u64, usize) {
        let Ok(mut g) = self.inner.lock() else {
            return (0, 0);
        };
        let avail = g.limit_end.saturating_sub(g.committed_end);
        let allow = (want_len as u64).min(avail) as usize;
        let at = g.committed_end;
        g.inflight_end = g.committed_end + allow as u64;
        (at, allow)
    }

    /// Commit the previously reserved write and return `(written, reached_limit)`.
    pub fn commit_write(&self) -> (u64, bool) {
        let Ok(mut g) = self.inner.lock() else {
            return (0, true);
        };
        let n = g.inflight_end.saturating_sub(g.committed_end);
        g.committed_end = g.inflight_end;
        let done = g.committed_end >= g.limit_end;
        (n, done)
    }

    pub fn rollback_write(&self) {
        if let Ok(mut g) = self.inner.lock() {
            g.inflight_end = g.committed_end;
        }
    }

    /// Deactivate the lease and return any uncommitted `[committed_end .. limit_end)`.
    pub fn finish(&self) -> Option<Task> {
        let mut g = self.inner.lock().ok()?;
        g.active = false;
        g.inflight_end = g.committed_end;
        let rem = g.limit_end.saturating_sub(g.committed_end);
        if rem > 0 {
            Some(Task {
                offset: g.committed_end,
                length: rem,
            })
        } else {
            None
        }
    }

    fn stealable(&self) -> u64 {
        self.inner
            .lock()
            .ok()
            .filter(|g| g.active)
            .map(|g| g.limit_end.saturating_sub(g.inflight_end))
            .unwrap_or(0)
    }

    fn try_steal_half(&self, min_split: u64) -> Option<Task> {
        let mut g = self.inner.lock().ok()?;
        if !g.active {
            return None;
        }
        let stealable = g.limit_end.saturating_sub(g.inflight_end);
        if stealable < min_split * 2 {
            return None;
        }
        let keep = align_up(stealable / 2);
        let split_at = g.inflight_end.saturating_add(keep);
        if split_at >= g.limit_end || g.limit_end - split_at < min_split {
            return None;
        }
        let stolen = Task {
            offset: split_at,
            length: g.limit_end - split_at,
        };
        g.limit_end = split_at;
        Some(stolen)
    }
}

/// Registry of active worker leases for in-flight tail work stealing.
#[derive(Default)]
pub struct LeasePool {
    leases: Mutex<Vec<std::sync::Arc<SpanLease>>>,
}

impl LeasePool {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&self) -> std::sync::Arc<SpanLease> {
        let lease = std::sync::Arc::new(SpanLease::new());
        if let Ok(mut list) = self.leases.lock() {
            list.push(lease.clone());
        }
        lease
    }

    /// Split the largest active in-flight range in half so an idle worker can
    /// help finish a slow tail chunk on a fresh connection.
    pub fn steal_largest(&self, min_split: u64) -> Option<Task> {
        let snapshot: Vec<std::sync::Arc<SpanLease>> =
            self.leases.lock().ok()?.iter().cloned().collect();
        let mut candidates: Vec<(u64, std::sync::Arc<SpanLease>)> = snapshot
            .into_iter()
            .map(|l| (l.stealable(), l))
            .filter(|(rem, _)| *rem >= min_split * 2)
            .collect();
        candidates.sort_by(|a, b| b.0.cmp(&a.0));
        for (_, lease) in candidates {
            if let Some(task) = lease.try_steal_half(min_split) {
                return Some(task);
            }
        }
        None
    }

    pub fn any_active(&self) -> bool {
        let Ok(list) = self.leases.lock() else {
            return false;
        };
        list.iter().any(|l| {
            l.inner
                .lock()
                .map(|g| g.active && g.limit_end > g.committed_end)
                .unwrap_or(false)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_align_up() {
        assert_eq!(align_up(0), 0);
        assert_eq!(align_up(1), 4096);
        assert_eq!(align_up(4096), 4096);
        assert_eq!(align_up(4097), 8192);
    }

    #[test]
    fn test_compute_chunks_zero_chunks() {
        let tasks = compute_chunks(100, 0, 1024);
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].offset, 0);
        assert_eq!(tasks[0].length, 100);
    }

    #[test]
    fn test_compute_chunks_small_file() {
        let tasks = compute_chunks(100, 4, 200);
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].length, 100);
    }

    #[test]
    fn test_compute_chunks_large_file() {
        let tasks = compute_chunks(10 * 1024 * 1024, 4, 0);
        assert!(tasks.len() >= 2, "got {} chunks", tasks.len());
        let total: u64 = tasks.iter().map(|t| t.length).sum();
        assert_eq!(total, 10 * 1024 * 1024);
    }

    #[test]
    fn test_compute_chunks_alignment() {
        let tasks = compute_chunks(5_000_000, 3, 1024 * 1024);
        for t in &tasks {
            assert_eq!(t.offset % ALIGN, 0, "offset {} not aligned", t.offset);
        }
    }

    #[test]
    fn test_chunk_queue_basic_ops() {
        let tasks = vec![
            Task {
                offset: 0,
                length: 100,
            },
            Task {
                offset: 100,
                length: 200,
            },
        ];
        let q = ChunkQueue::new(tasks);
        assert_eq!(q.len(), 2);
        assert!(!q.is_empty());
        assert_eq!(q.remaining_bytes(), 300);

        let t = q.pop().unwrap();
        assert_eq!(t.offset, 0);
        assert_eq!(q.len(), 1);

        q.push(Task {
            offset: 300,
            length: 50,
        });
        assert_eq!(q.len(), 2);

        let drained = q.drain();
        assert_eq!(drained.len(), 2);
        assert!(q.is_empty());
    }

    #[test]
    fn test_chunk_queue_empty_pop() {
        let q = ChunkQueue::new(vec![]);
        assert!(q.pop().is_none());
        assert!(q.is_empty());
        assert_eq!(q.remaining_bytes(), 0);
    }

    #[test]
    fn auto_plan_for_large_file_is_many_parts_not_one() {
        // The Kali ISO size. Auto plans several parts, not one full-file task.
        let size = 4_831_174_656u64;
        let plan = plan_chunks(size, 0, true, 0);
        assert_eq!(plan.connections, 4);
        assert_eq!(plan.parts.len(), 64);
        let ranges: Vec<crate::engine::part_progress::PartRange> = plan
            .parts
            .iter()
            .map(|p| crate::engine::part_progress::PartRange {
                start: p.start,
                end: p.end,
            })
            .collect();
        let tasks = crate::engine::part_progress::remaining_tasks_from_parts(
            &ranges,
            &vec![0; ranges.len()],
        );
        assert_eq!(tasks.len(), 64);
        assert_eq!(tasks.iter().map(|t| t.length).sum::<u64>(), size);
        assert_eq!(compute_chunks(size, 1, 0).len(), 1);
        assert_eq!(compute_chunks(size, 0, 0).len(), 1);
    }

    #[test]
    fn auto_connections_start_conservative_and_ignore_proxy() {
        assert_eq!(auto_connections(1024), 1);
        assert_eq!(auto_connections(3 * 1024 * 1024), 4);
        assert_eq!(auto_connections(32 * 1024 * 1024), 4);
        assert_eq!(auto_connections(200 * 1024 * 1024), 4);
        assert_eq!(auto_connections(2 * 1024 * 1024 * 1024), 4);
        // A proxy does not lower the Auto count; throughput decides.
        assert_eq!(auto_connections_for(2 * 1024 * 1024 * 1024, true), 4);
        assert_eq!(auto_connections_for(600 * 1024 * 1024, true), 4);
    }

    #[test]
    fn auto_plans_enough_parts_for_the_adaptive_ceiling() {
        // Auto's planned part count must cover every worker the controller can
        // request, otherwise scale-up has nothing to hand out.
        for (size, ceiling) in [
            (16u64 * 1024 * 1024, 8u32),
            (32 * 1024 * 1024, 16),
            (64 * 1024 * 1024, 32),
            (128 * 1024 * 1024, 64),
            (4_831_174_656, 64),
        ] {
            let plan = plan_chunks(size, 0, true, 0);
            assert_eq!(
                plan.parts.len(),
                ceiling as usize,
                "size={size} ceiling={ceiling}"
            );
            let total: u64 = plan.parts.iter().map(|p| p.end - p.start).sum();
            assert_eq!(total, size);
        }
    }

    #[test]
    fn requested_connections_honor_64() {
        assert_eq!(compute_connection_count(10 * 1024 * 1024 * 1024, 64, 0), 64);
        // A settings default is not a cap on an explicit per-download count.
        assert_eq!(
            compute_connection_count(10 * 1024 * 1024 * 1024, 64, 32),
            64
        );
        assert_eq!(compute_connection_count(1024, 0, 0), 1);
        assert_eq!(compute_connection_count(8 * 1024 * 1024, 0, 0), 4);
    }

    #[test]
    fn settings_default_skips_size_detection() {
        let iso = 4_831_174_656u64;
        assert_eq!(compute_connection_count(iso, 0, 16), 16);
        // A tiny file would be 1 under Auto. A configured default stays 16.
        assert_eq!(compute_connection_count(1024, 0, 16), 16);
        assert_eq!(compute_connection_count(iso, 4, 16), 4);
        let plan = plan_chunks(iso, 0, true, 16);
        assert_eq!(plan.connections, 16);
        assert_eq!(plan.parts.len(), 16);
    }

    #[test]
    fn steal_splits_large_tail() {
        let q = ChunkQueue::new(vec![Task {
            offset: 0,
            length: 32 * 1024 * 1024,
        }]);
        let first = q.pop_or_steal().unwrap();
        assert!(first.length < 32 * 1024 * 1024);
        assert!(!q.is_empty());
        let total = first.length + q.remaining_bytes();
        assert_eq!(total, 32 * 1024 * 1024);
    }

    #[test]
    fn span_lease_steals_without_overlapping_inflight_write() {
        let pool = LeasePool::new();
        let lease_a = pool.register();
        let total = 10 * 1024 * 1024u64;
        lease_a.activate(0, total);

        // Worker A reserves 512 KiB for an ongoing disk write (`pwrite` outside lock).
        let (off, allow) = lease_a.begin_write(512 * 1024);
        assert_eq!(off, 0);
        assert_eq!(allow, 512 * 1024);

        // Idle Worker B steals half of the remaining unreserved tail.
        let stolen = pool
            .steal_largest(MIN_INFLIGHT_SPLIT)
            .expect("should steal");
        assert!(stolen.offset > 512 * 1024, "must never cross inflight_end");
        assert_eq!(stolen.offset % ALIGN, 0);
        assert_eq!(stolen.offset + stolen.length, total);

        // Worker A commits its 512 KiB write and then pauses (`finish()`).
        let (committed, done) = lease_a.commit_write();
        assert_eq!(committed, 512 * 1024);
        assert!(!done);
        let rem_a = lease_a.finish().expect("remaining tail of A");

        // Exact disjoint partition: [0..committed) U rem_a U stolen == [0..total)
        assert_eq!(rem_a.offset, committed);
        assert_eq!(rem_a.offset + rem_a.length, stolen.offset);
        assert_eq!(committed + rem_a.length + stolen.length, total);
    }
}
