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

/// Result of applying one progress event.
pub struct ApplyOutcome {
    /// False when the event was dropped (unregistered id or stale
    /// pre-restart event).
    pub applied: bool,
    /// True exactly once per engine run: the first reset-to-single event,
    /// which is when the DB parts row must be restructured.
    pub first_single_reset: bool,
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

    /// Invalidate for a truncate-and-restart, atomically: zero the entry and
    /// arm the restart gate under ONE lock. Must run BEFORE the caller's DB
    /// reset — the flush loop checks the gate under the same lock it writes
    /// under, so no flush can land stale numbers after the DB reset.
    pub fn arm_restart(&self, id: u64) {
        let mut map = recover_lock(self.inner.lock());
        if let Some(rt) = map.get_mut(&id) {
            rt.downloaded = 0;
            rt.part_downloaded = vec![0];
            rt.parts_dirty = true;
            rt.single_reset = true;
            rt.restart_pending = true;
        }
    }

    /// Apply one progress event atomically: the restart-gate check,
    /// single-reset bookkeeping and the value write happen under one lock, so
    /// an event can never land stale values after an invalidation armed the
    /// gate. Unregistered ids are inert (stale events after pause/complete).
    pub fn apply_progress(
        &self,
        id: u64,
        downloaded: u64,
        part_downloaded: Option<Vec<u64>>,
        is_reset_event: bool,
    ) -> ApplyOutcome {
        let mut map = recover_lock(self.inner.lock());
        let Some(rt) = map.get_mut(&id) else {
            return ApplyOutcome { applied: false, first_single_reset: false };
        };
        if rt.restart_pending {
            if !is_reset_event {
                // Stale pre-restart event — drop it.
                return ApplyOutcome { applied: false, first_single_reset: false };
            }
            rt.restart_pending = false;
        }
        let first_single_reset = is_reset_event && !rt.single_reset;
        if is_reset_event {
            rt.single_reset = true;
        }
        rt.downloaded = downloaded;
        if let Some(parts) = part_downloaded {
            rt.part_downloaded = parts;
            rt.parts_dirty = true;
        }
        ApplyOutcome { applied: true, first_single_reset }
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
    /// Each entry's check and DB write happen while holding the map lock, so
    /// a flush can never write numbers that predate an `arm_restart` (which
    /// mutates the entry under the same lock BEFORE its own DB reset).
    /// Returns the number of entries successfully flushed.
    /// Failed entries stay dirty and retry on the next flush cycle.
    pub fn flush_to_db(&self, db: &crate::state::db::Db) -> usize {
        let ids: Vec<u64> = {
            let map = recover_lock(self.inner.lock());
            map.keys().copied().collect()
        };
        let mut flushed = 0usize;
        for id in ids {
            let mut map = recover_lock(self.inner.lock());
            let Some(rt) = map.get_mut(&id) else { continue };
            if rt.restart_pending {
                // Mid-invalidation: the reset event will re-dirty the entry.
                continue;
            }
            if rt.downloaded == rt.last_flushed && !rt.parts_dirty {
                continue;
            }
            let downloaded = rt.downloaded;
            let parts = if rt.parts_dirty && !rt.part_downloaded.is_empty() {
                Some(rt.part_downloaded.clone())
            } else {
                None
            };
            if db.flush_progress(id, downloaded, parts.as_deref()).is_ok() {
                rt.last_flushed = downloaded;
                rt.parts_dirty = false;
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
    fn test_flush_skips_armed_restart() {
        let state = test_state();
        let ts = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let dir = std::env::temp_dir().join(format!("pdm_flush_arm_test_{}", ts));
        std::fs::create_dir_all(&dir).ok();
        let db = crate::state::db::Db::from_path(&dir.join("test.db")).unwrap();
        let item = crate::types::DownloadItem {
            id: 1,
            url: "https://example.com/file.zip".to_string(),
            file_name: "file.zip".to_string(),
            save_path: "/tmp/file.zip".to_string(),
            total_size: 1000,
            downloaded: 0,
            status: crate::types::DownloadStatus::Downloading,
            connections: 1,
            ..Default::default()
        };
        db.insert_download(&item).unwrap();

        state.register(1);
        state.update_progress(1, 500, Some(vec![500]));
        state.arm_restart(1);

        // Armed: the flush must not write pre-restart numbers over the reset.
        assert_eq!(state.flush_to_db(&db), 0);
        assert_eq!(db.get_by_id(1).unwrap().unwrap().downloaded, 0);

        // Stale pre-restart event is dropped…
        let stale = state.apply_progress(1, 600, Some(vec![600]), false);
        assert!(!stale.applied);
        // …the restart's own reset event clears the gate…
        let reset = state.apply_progress(1, 0, Some(vec![0]), true);
        assert!(reset.applied);
        assert!(!reset.first_single_reset, "arm_restart already did the DB reset");
        // …and normal progress flushes again.
        state.apply_progress(1, 100, Some(vec![100]), true);
        assert_eq!(state.flush_to_db(&db), 1);
        assert_eq!(db.get_by_id(1).unwrap().unwrap().downloaded, 100);

        let _ = std::fs::remove_dir_all(&dir);
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
            connections: 1,
            ..Default::default()
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
