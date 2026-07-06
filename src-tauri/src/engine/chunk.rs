use crate::types::Task;
use std::collections::VecDeque;
use std::sync::Mutex;

const ALIGN: u64 = 4096;

pub fn align_down(v: u64) -> u64 {
    v & !(ALIGN - 1)
}

pub fn align_up(v: u64) -> u64 {
    (v + ALIGN - 1) & !(ALIGN - 1)
}

pub fn compute_chunks(file_size: u64, num_chunks: u32, min_chunk_size: u64) -> Vec<Task> {
    if num_chunks == 0 {
        return vec![Task { offset: 0, length: file_size }];
    }
    let chunk_size = (file_size / num_chunks as u64).max(min_chunk_size);
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

pub fn split_task(task: &Task, split_point: u64) -> (Task, Task) {
    let split = align_down(split_point.max(task.offset + ALIGN).min(task.offset + task.length - ALIGN));
    let left_len = split - task.offset;
    let right_len = task.length - left_len;
    (
        Task { offset: task.offset, length: left_len },
        Task { offset: split, length: right_len },
    )
}

pub struct ChunkQueue {
    tasks: Mutex<VecDeque<Task>>,
    closed: Mutex<bool>,
}

impl ChunkQueue {
    pub fn new(tasks: Vec<Task>) -> Self {
        Self {
            tasks: Mutex::new(VecDeque::from(tasks)),
            closed: Mutex::new(false),
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

    pub fn close(&self) {
        *self.closed.lock().unwrap() = true;
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
    fn test_align_down() {
        assert_eq!(align_down(0), 0);
        assert_eq!(align_down(1), 0);
        assert_eq!(align_down(4096), 4096);
        assert_eq!(align_down(5000), 4096);
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
        let tasks = compute_chunks(10 * 1024 * 1024, 4, 2 * 1024 * 1024);
        assert!(tasks.len() >= 4, "got {} chunks", tasks.len());
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
    fn test_split_task() {
        let task = Task { offset: 0, length: 10_000_000 };
        let (left, right) = split_task(&task, 5_000_000);
        assert_eq!(left.offset, 0);
        assert_eq!(right.offset, left.length);
        assert_eq!(left.length + right.length, task.length);
        assert_eq!(right.offset % ALIGN, 0);
    }

    #[test]
    fn test_split_task_boundary() {
        let task = Task { offset: 0, length: 4096 };
        let (left, right) = split_task(&task, 2048);
        // For ALIGN-sized tasks, split returns (empty, original) since
        // split_point + ALIGN > offset + length - ALIGN
        assert_eq!(left.length, 0);
        assert_eq!(right.length, 4096);
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
