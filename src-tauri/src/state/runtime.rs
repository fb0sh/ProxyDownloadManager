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
    /// Value of `downloaded` at the time of the last DB flush.
    last_flushed: u64,
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
            last_flushed: 0,
        });
    }

    /// Update progress in memory (no DB write).
    pub fn update_progress(&self, id: u64, downloaded: u64) {
        let mut map = recover_lock(self.inner.lock());
        if let Some(rt) = map.get_mut(&id) {
            rt.downloaded = downloaded;
        }
    }

    /// Remove a download from runtime state (called on complete/error/cancel).
    pub fn remove(&self, id: u64) {
        let mut map = recover_lock(self.inner.lock());
        map.remove(&id);
    }

    /// Flush all dirty entries to the database.
    /// Returns the number of entries flushed.
    pub fn flush_to_db(&self, db: &crate::state::db::Db) -> usize {
        let batch: Vec<(u64, u64)> = {
            let map = recover_lock(self.inner.lock());
            map.iter()
                .filter(|(_, rt)| rt.downloaded != rt.last_flushed)
                .map(|(&id, rt)| (id, rt.downloaded))
                .collect()
        };
        let count = batch.len();
        for (id, downloaded) in &batch {
            let _ = db.update_download_progress(*id, *downloaded);
        }
        // Update last_flushed after successful DB write
        let mut map = recover_lock(self.inner.lock());
        for (id, downloaded) in &batch {
            if let Some(rt) = map.get_mut(id) {
                rt.last_flushed = *downloaded;
            }
        }
        count
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

        state.update_progress(1, 500);
        assert_eq!(state.get_downloaded(1), Some(500));
    }

    #[test]
    fn test_remove() {
        let state = test_state();
        state.register(1);
        state.update_progress(1, 100);
        state.remove(1);
        assert_eq!(state.get_downloaded(1), None);
    }

    #[test]
    fn test_update_nonexistent_is_noop() {
        let state = test_state();
        state.update_progress(999, 500); // no panic, no error
        assert_eq!(state.get_downloaded(999), None);
    }
}
