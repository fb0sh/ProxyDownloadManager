use crate::types::{Task, DownloadPart, PartStatus, PdmError, PdmResult};
use std::collections::VecDeque;
use std::sync::Mutex;

const ALIGN: u64 = 4096;

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
        let num_conns = if connections > 0 { connections.min(32) } else { 1 };
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
    let max_conns = max_connections.max(1).min(32);

    if requested > 0 {
        requested.min(32)
    } else if file_size == 0 {
        max_conns.min(2)
    } else if file_size < 100 * 1024 * 1024 {
        max_conns.min(2)
    } else if file_size < 1024 * 1024 * 1024 {
        max_conns.min(4)
    } else if file_size < 10u64 * 1024 * 1024 * 1024 {
        max_conns.min(8)
    } else {
        max_conns.min(16)
    }
}

/// Check if there's enough disk space for the download.
pub fn check_disk_space(path: &str, file_size: u64) -> PdmResult<()> {
    if file_size > 0 {
        let pdm_path = format!("{}.pdm", path);
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
}
