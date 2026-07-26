use std::collections::HashMap;
use std::sync::Mutex;

/// In-memory runtime state for active downloads.
/// Updates are cheap (no DB). Flushed to DB periodically by a background task.
pub struct DownloadManagerState {
    inner: Mutex<HashMap<u64, DownloadRuntime>>,
}

#[derive(Clone, Debug)]
pub struct DownloadRuntime {
    pub downloaded: u64,
    /// Per-part downloaded bytes for Progress Map (aligned with DownloadItem.parts).
    pub part_downloaded: Vec<u64>,
    /// Value of `downloaded` at the time of the last DB flush.
    last_flushed: u64,
    /// Whether part progress needs to be written to DB.
    parts_dirty: bool,
    /// Whether the DB parts row was already restructured to a single cell
    /// (Concurrent → Single degrade happens at most once per engine run).
    single_reset: bool,
    /// Set when progress records were invalidated for a truncate-and-restart:
    /// progress events already queued before the restart are stale and must
    /// be dropped until the restart's own reset event arrives.
    restart_pending: bool,
}

/// Recover from a poisoned mutex by unwrapping the guard.
/// This is safe because our lock regions don't do I/O that could fail —
/// a panic inside a lock is a bug, and we'd rather propagate the panic
/// than silently lose all progress updates.
fn recover_lock<T>(result: Result<T, std::sync::PoisonError<T>>) -> T {
    result.unwrap_or_else(|e| e.into_inner())
}

impl DownloadManagerState {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(HashMap::new()),
        }
    }

    /// Register a new active download (called when download starts).
    pub fn register(&self, id: u64) {
        let mut map = recover_lock(self.inner.lock());
        map.insert(id, DownloadRuntime {
            downloaded: 0,
            part_downloaded: vec![],
            last_flushed: 0,
            parts_dirty: false,
            single_reset: false,
            restart_pending: false,
        });
    }

    /// Mark that this download's records were invalidated for a restart.
    pub fn set_restart_pending(&self, id: u64) {
        let mut map = recover_lock(self.inner.lock());
        if let Some(rt) = map.get_mut(&id) {
            rt.restart_pending = true;
        }
    }

    /// Gate for progress events after an invalidation: stale events queued
    /// before the restart are dropped (returns false); the restart's own
    /// reset event clears the gate and passes. FIFO event order makes this
    /// exact — anything non-reset after the gate was set is pre-restart.
    pub fn restart_gate(&self, id: u64, is_reset_event: bool) -> bool {
        let mut map = recover_lock(self.inner.lock());
        match map.get_mut(&id) {
            Some(rt) if rt.restart_pending => {
                if is_reset_event {
                    rt.restart_pending = false;
                    true
                } else {
                    false
                }
            }
            _ => true,
        }
    }

    /// Mark that this download's DB parts were reset to a single cell.
    /// Returns true only the first time per registration, so the caller does
    /// the DB restructure once instead of on every progress event.
    /// Unregistered ids return false: those are stale events queued from
    /// before a pause/complete removed the entry, and must stay inert.
    pub fn mark_single_reset(&self, id: u64) -> bool {
        let mut map = recover_lock(self.inner.lock());
        match map.get_mut(&id) {
            Some(rt) => {
                let first = !rt.single_reset;
                rt.single_reset = true;
                first
            }
            None => false,
        }
    }

    /// Update progress in memory (no DB write).
    pub fn update_progress(&self, id: u64, downloaded: u64, part_downloaded: Option<Vec<u64>>) {
        let mut map = recover_lock(self.inner.lock());
        if let Some(rt) = map.get_mut(&id) {
            rt.downloaded = downloaded;
            if let Some(parts) = part_downloaded {
                rt.part_downloaded = parts;
                rt.parts_dirty = true;
            }
        }
    }

    pub(crate) fn get_part_downloaded(&self, id: u64) -> Option<Vec<u64>> {
        let map = recover_lock(self.inner.lock());
        map.get(&id).map(|rt| rt.part_downloaded.clone())
    }

    /// Remove a download from runtime state (called on complete/error/cancel).
    pub fn remove(&self, id: u64) {
        let mut map = recover_lock(self.inner.lock());
        map.remove(&id);
    }

    /// Flush all dirty entries to the database.
    /// Returns the number of entries successfully flushed.
    /// Failed entries stay dirty and retry on the next flush cycle.
    pub fn flush_to_db(&self, db: &crate::state::db::Db) -> usize {
        let batch: Vec<(u64, u64, Vec<u64>, bool)> = {
            let map = recover_lock(self.inner.lock());
            map.iter()
                .filter(|(_, rt)| rt.downloaded != rt.last_flushed || rt.parts_dirty)
                .map(|(&id, rt)| {
                    (
                        id,
                        rt.downloaded,
                        rt.part_downloaded.clone(),
                        rt.parts_dirty,
                    )
                })
                .collect()
        };
        let mut flushed = 0usize;
        let mut map = recover_lock(self.inner.lock());
        for (id, downloaded, part_downloaded, parts_dirty) in &batch {
            let parts = if *parts_dirty && !part_downloaded.is_empty() {
                Some(part_downloaded.as_slice())
            } else {
                None
            };
            if db.flush_progress(*id, *downloaded, parts).is_ok() {
                if let Some(rt) = map.get_mut(id) {
                    rt.last_flushed = *downloaded;
                    rt.parts_dirty = false;
                }
                flushed += 1;
            }
            // Failed entries keep their last_flushed / parts_dirty, so they stay dirty
            // and will be retried on the next flush cycle.
        }
        flushed
    }

    pub(crate) fn get_downloaded(&self, id: u64) -> Option<u64> {
        let map = recover_lock(self.inner.lock());
        map.get(&id).map(|rt| rt.downloaded)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_state() -> DownloadManagerState {
        DownloadManagerState::new()
    }

    #[test]
    fn test_register_and_progress() {
        let state = test_state();
        state.register(1);
        assert_eq!(state.get_downloaded(1), Some(0));

        state.update_progress(1, 500, None);
        assert_eq!(state.get_downloaded(1), Some(500));
    }

    #[test]
    fn test_remove() {
        let state = test_state();
        state.register(1);
        state.update_progress(1, 100, Some(vec![100]));
        state.remove(1);
        assert_eq!(state.get_downloaded(1), None);
    }

    #[test]
    fn test_update_nonexistent_is_noop() {
        let state = test_state();
        state.update_progress(999, 500, None); // no panic, no error
        assert_eq!(state.get_downloaded(999), None);
    }

    #[test]
    fn test_flush_to_db() {
        let state = test_state();
        let ts = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let dir = std::env::temp_dir().join(format!("pdm_flush_test_{}", ts));
        std::fs::create_dir_all(&dir).ok();
        let db_path = dir.join("test.db");
        let db = crate::state::db::Db::from_path(&db_path).unwrap();

        // Insert a download item so flush_progress succeeds
        let item = crate::types::DownloadItem {
            id: 1,
            url: "https://example.com/file.zip".to_string(),
            file_name: "file.zip".to_string(),
            save_path: "/tmp/file.zip".to_string(),
            total_size: 1000,
            downloaded: 0,
            status: crate::types::DownloadStatus::Downloading,
            parts: vec![],
            proxy_name: String::new(),
            connections: 1,
            resumable: None,
            created_at: String::new(),
            last_try: String::new(),
        };
        db.insert_download(&item).unwrap();

        state.register(1);
        state.update_progress(1, 500, Some(vec![500]));

        // First flush: should flush 1 entry
        let flushed = state.flush_to_db(&db);
        assert_eq!(flushed, 1);

        // Second flush: nothing dirty, should flush 0
        let flushed = state.flush_to_db(&db);
        assert_eq!(flushed, 0);

        // Update again and flush
        state.update_progress(1, 800, Some(vec![800]));
        let flushed = state.flush_to_db(&db);
        assert_eq!(flushed, 1);

        // Verify DB has the final value
        let item = db.get_by_id(1).unwrap().unwrap();
        assert_eq!(item.downloaded, 800);
    }
}
