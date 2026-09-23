use crate::types::{Task, DownloadPart, PartStatus, PdmError, PdmResult};
use std::collections::VecDeque;
use std::sync::Mutex;

const ALIGN: u64 = 4096;
pub const MAX_CONNECTIONS: u32 = 64;
const MIN_SPLIT: u64 = 2 * 1024 * 1024;

pub fn align_up(v: u64) -> u64 {
    (v + ALIGN - 1) & !(ALIGN - 1)
}

/// Compute chunk size so each worker handles ~20 chunks.
/// Formula: file_size / (connections × 20), clamped to [4MB, 64MB].
fn dynamic_chunk_size(file_size: u64, connections: u32) -> u64 {
    let conns = connections.max(1);
    let target = file_size / (conns as u64 * 20); // ~20 chunks/worker
    target
        .max(4 * 1024 * 1024)    // min 4MB
        .min(64 * 1024 * 1024)   // max 64MB
}

pub fn compute_chunks(file_size: u64, num_chunks: u32, _min_chunk_size: u64) -> Vec<Task> {
    if num_chunks == 0 {
        return vec![Task { offset: 0, length: file_size }];
    }
    let chunk_size = dynamic_chunk_size(file_size, num_chunks)
        .max(align_up(file_size / num_chunks as u64));
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

    let parts = if supports_range && file_size > 0 {
        let num_conns = if connections > 0 { connections.min(MAX_CONNECTIONS) } else { 1 };
        let min_chunk = 2u64 * 1024 * 1024;
        let tasks = compute_chunks(file_size, num_conns, min_chunk);
        tasks.iter().enumerate().map(|(i, t)| DownloadPart {
            index: i as u32,
            start: t.offset,
            end: t.offset + t.length,
            downloaded: 0,
            temp_path: String::new(),
            status: PartStatus::Pending,
            retries: 0,
        }).collect()
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

pub fn compute_connection_count(file_size: u64, requested: u32, max_connections: u32) -> u32 {
    let hard_max = if max_connections == 0 {
        MAX_CONNECTIONS
    } else {
        max_connections.min(MAX_CONNECTIONS).max(1)
    };
    if requested > 0 {
        return requested.clamp(1, hard_max);
    }
    auto_connections(file_size).clamp(1, hard_max)
}

/// Initial Auto strategy. Dynamic segmentation may raise concurrency later.
pub fn auto_connections(file_size: u64) -> u32 {
    const MIB: u64 = 1024 * 1024;
    if file_size == 0 {
        2
    } else if file_size < 2 * MIB {
        1
    } else if file_size < 16 * MIB {
        4
    } else if file_size < 128 * MIB {
        8
    } else if file_size < 1024 * MIB {
        16
    } else {
        32
    }
}

/// Check if there's enough disk space for the download.
pub fn check_disk_space(path: &str, file_size: u64) -> PdmResult<()> {
    if file_size > 0 {
        let pdm_path = crate::engine::file_io::pdm_path(path);
        if let Some(parent) = std::path::Path::new(&pdm_path).parent() {
            if let Ok(available) = fs2::available_space(parent) {
                let needed = file_size + (2u64 * 1024 * 1024);
                if available < needed {
                    return Err(PdmError::Other(format!(
                        "Insufficient disk space: need {}, available {}", needed, available
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
        self.tasks.lock()
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
            Task { offset: 0, length: 100 },
            Task { offset: 100, length: 200 },
        ];
        let q = ChunkQueue::new(tasks);
        assert_eq!(q.len(), 2);
        assert!(!q.is_empty());
        assert_eq!(q.remaining_bytes(), 300);

        let t = q.pop().unwrap();
        assert_eq!(t.offset, 0);
        assert_eq!(q.len(), 1);

        q.push(Task { offset: 300, length: 50 });
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
    fn auto_connections_follows_size_buckets() {
        assert_eq!(auto_connections(1024), 1);
        assert_eq!(auto_connections(3 * 1024 * 1024), 4);
        assert_eq!(auto_connections(32 * 1024 * 1024), 8);
        assert_eq!(auto_connections(200 * 1024 * 1024), 16);
        assert_eq!(auto_connections(2 * 1024 * 1024 * 1024), 32);
    }

    #[test]
    fn requested_connections_honor_64() {
        assert_eq!(compute_connection_count(10 * 1024 * 1024 * 1024, 64, 0), 64);
        assert_eq!(compute_connection_count(10 * 1024 * 1024 * 1024, 64, 32), 32);
        assert_eq!(compute_connection_count(1024, 0, 0), 1);
        assert_eq!(compute_connection_count(8 * 1024 * 1024, 0, 0), 4);
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
}
