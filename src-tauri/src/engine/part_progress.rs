//! Fixed-part progress tracking for the Progress Map.
//! Parts are planned once; writes are attributed by file offset into those ranges.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

/// One fixed byte range on the file (half-open: `[start, end)`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PartRange {
    pub start: u64,
    pub end: u64,
}

impl PartRange {
    pub fn len(&self) -> u64 {
        self.end.saturating_sub(self.start)
    }
}

/// Thread-safe tracker: each write is attributed to fixed part ranges by offset.
pub struct PartProgressTracker {
    ranges: Vec<PartRange>,
    downloaded: Vec<AtomicU64>,
}

impl PartProgressTracker {
    pub fn new(ranges: Vec<PartRange>) -> Arc<Self> {
        let downloaded = ranges.iter().map(|_| AtomicU64::new(0)).collect();
        Arc::new(Self { ranges, downloaded })
    }

    /// Seed each part's downloaded bytes (e.g. from DB after pause). Caps at part length.
    pub fn seed_from_parts(self: &Arc<Self>, part_downloaded: &[u64]) {
        for (i, range) in self.ranges.iter().enumerate() {
            let done = part_downloaded.get(i).copied().unwrap_or(0).min(range.len());
            self.downloaded[i].store(done, Ordering::Relaxed);
        }
    }

    /// Attribute `len` bytes written at absolute file `offset` into overlapping parts.
    pub fn record_write(&self, offset: u64, len: u64) {
        if len == 0 {
            return;
        }
        let write_end = offset.saturating_add(len);
        for (i, range) in self.ranges.iter().enumerate() {
            let overlap_start = offset.max(range.start);
            let overlap_end = write_end.min(range.end);
            if overlap_end > overlap_start {
                let add = overlap_end - overlap_start;
                let cap = range.len();
                loop {
                    let cur = self.downloaded[i].load(Ordering::Relaxed);
                    let next = (cur + add).min(cap);
                    if self.downloaded[i]
                        .compare_exchange_weak(cur, next, Ordering::Relaxed, Ordering::Relaxed)
                        .is_ok()
                    {
                        break;
                    }
                }
            }
        }
    }

    pub fn snapshot(&self) -> Vec<u64> {
        self.downloaded
            .iter()
            .map(|a| a.load(Ordering::Relaxed))
            .collect()
    }

    pub fn ranges(&self) -> &[PartRange] {
        &self.ranges
    }
}

/// Pure helper: apply a write into a mutable downloaded slice (for tests / offline).
pub fn apply_write_to_parts(ranges: &[PartRange], downloaded: &mut [u64], offset: u64, len: u64) {
    assert_eq!(ranges.len(), downloaded.len());
    if len == 0 {
        return;
    }
    let write_end = offset.saturating_add(len);
    for (i, range) in ranges.iter().enumerate() {
        let overlap_start = offset.max(range.start);
        let overlap_end = write_end.min(range.end);
        if overlap_end > overlap_start {
            let add = overlap_end - overlap_start;
            downloaded[i] = (downloaded[i] + add).min(range.len());
        }
    }
}

/// Part fill percent 0–100 for Progress Map cells.
pub fn part_percent(downloaded: u64, start: u64, end: u64) -> u32 {
    let len = end.saturating_sub(start);
    if len == 0 {
        return 0;
    }
    let pct = (downloaded.min(len) as u128 * 100) / len as u128;
    pct.min(100) as u32
}

/// Encode progress event payload (total + per-part downloaded).
/// `reset_to_single`: degrade to Single — one part covering the whole file.
pub fn encode_progress_data(downloaded: u64, parts: &[u64], reset_to_single: bool) -> String {
    serde_json::json!({
        "downloaded": downloaded,
        "parts": parts,
        "reset_to_single": reset_to_single,
    })
    .to_string()
}

/// Rebuild remaining Range tasks from fixed parts (for resume when gob tasks missing).
/// Each incomplete part becomes `Task { offset: start + done, length: remaining }`.
pub fn remaining_tasks_from_parts(
    ranges: &[PartRange],
    part_downloaded: &[u64],
) -> Vec<crate::types::Task> {
    ranges
        .iter()
        .enumerate()
        .filter_map(|(i, range)| {
            let done = part_downloaded
                .get(i)
                .copied()
                .unwrap_or(0)
                .min(range.len());
            let left = range.len().saturating_sub(done);
            if left == 0 {
                None
            } else {
                Some(crate::types::Task {
                    offset: range.start + done,
                    length: left,
                })
            }
        })
        .collect()
}

/// Derive per-part downloaded bytes from remaining tasks: everything a task
/// doesn't cover is done. Inverse of [`remaining_tasks_from_parts`], used to
/// reconcile gob task lists with fixed part ranges.
pub fn parts_downloaded_from_tasks(
    ranges: &[PartRange],
    tasks: &[crate::types::Task],
) -> Vec<u64> {
    ranges
        .iter()
        .map(|range| {
            let len = range.len();
            let remaining: u64 = tasks
                .iter()
                .map(|t| {
                    let t_end = t.offset.saturating_add(t.length);
                    let s = t.offset.max(range.start);
                    let e = t_end.min(range.end);
                    e.saturating_sub(s)
                })
                .sum();
            len.saturating_sub(remaining.min(len))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn apply_write_splits_across_parts() {
        let ranges = vec![
            PartRange { start: 0, end: 100 },
            PartRange { start: 100, end: 200 },
        ];
        let mut dl = vec![0u64; 2];
        apply_write_to_parts(&ranges, &mut dl, 80, 40); // 80..120
        assert_eq!(dl, vec![20, 20]);
    }

    #[test]
    fn apply_write_caps_at_part_len() {
        let ranges = vec![PartRange { start: 0, end: 50 }];
        let mut dl = vec![40u64];
        apply_write_to_parts(&ranges, &mut dl, 0, 100);
        assert_eq!(dl[0], 50);
    }

    #[test]
    fn apply_write_outside_parts_is_noop() {
        let ranges = vec![PartRange { start: 0, end: 100 }];
        let mut dl = vec![10u64];
        apply_write_to_parts(&ranges, &mut dl, 500, 10);
        assert_eq!(dl[0], 10);
    }

    #[test]
    fn tracker_record_write_and_snapshot() {
        let t = PartProgressTracker::new(vec![
            PartRange { start: 0, end: 100 },
            PartRange { start: 100, end: 300 },
        ]);
        t.record_write(50, 100); // 50..150 → 50 in p0, 50 in p1
        assert_eq!(t.snapshot(), vec![50, 50]);
    }

    #[test]
    fn parts_from_tasks_inverse_of_remaining() {
        let ranges = vec![
            PartRange { start: 0, end: 500 },
            PartRange { start: 500, end: 1000 },
        ];
        // 50 bytes remain mid-part-0, 300 remain at the tail of part 1
        let tasks = vec![
            crate::types::Task { offset: 450, length: 50 },
            crate::types::Task { offset: 700, length: 300 },
        ];
        assert_eq!(parts_downloaded_from_tasks(&ranges, &tasks), vec![450, 200]);
        // no tasks → everything done
        assert_eq!(parts_downloaded_from_tasks(&ranges, &[]), vec![500, 500]);
        // tasks covering everything → nothing done
        let full = remaining_tasks_from_parts(&ranges, &[0, 0]);
        assert_eq!(parts_downloaded_from_tasks(&ranges, &full), vec![0, 0]);
    }

    #[test]
    fn seed_from_parts_respects_saved_progress() {
        let t = PartProgressTracker::new(vec![
            PartRange { start: 0, end: 100 },
            PartRange { start: 100, end: 200 },
        ]);
        t.seed_from_parts(&[100, 30]);
        assert_eq!(t.snapshot(), vec![100, 30]);
    }

    #[test]
    fn remaining_tasks_skips_complete_parts() {
        let ranges = vec![
            PartRange { start: 0, end: 100 },
            PartRange { start: 100, end: 300 },
            PartRange { start: 300, end: 400 },
        ];
        let tasks = remaining_tasks_from_parts(&ranges, &[100, 50, 0]);
        assert_eq!(tasks.len(), 2);
        assert_eq!(tasks[0].offset, 150);
        assert_eq!(tasks[0].length, 150);
        assert_eq!(tasks[1].offset, 300);
        assert_eq!(tasks[1].length, 100);
    }

    #[test]
    fn part_percent_basic() {
        assert_eq!(part_percent(0, 0, 100), 0);
        assert_eq!(part_percent(50, 0, 100), 50);
        assert_eq!(part_percent(100, 0, 100), 100);
        assert_eq!(part_percent(200, 0, 100), 100);
        assert_eq!(part_percent(0, 0, 0), 0);
    }
}
