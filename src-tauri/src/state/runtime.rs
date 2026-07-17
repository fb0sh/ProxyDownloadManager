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

impl DownloadManagerState {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(HashMap::new()),
        }
    }

    /// Register a new active download (called when download starts).
    pub fn register(&self, id: u64) {
        if let Ok(mut map) = self.inner.lock() {
            map.insert(id, DownloadRuntime {
                downloaded: 0,
                last_flushed: 0,
            });
        }
    }

    /// Update progress in memory (no DB write).
    pub fn update_progress(&self, id: u64, downloaded: u64) {
        if let Ok(mut map) = self.inner.lock() {
            if let Some(rt) = map.get_mut(&id) {
                rt.downloaded = downloaded;
            }
        }
    }

    /// Remove a download from runtime state (called on complete/error/cancel).
    pub fn remove(&self, id: u64) {
        if let Ok(mut map) = self.inner.lock() {
            map.remove(&id);
        }
    }

    /// Flush all dirty entries to the database.
    /// Returns the number of entries flushed.
    pub fn flush_to_db(&self, db: &crate::state::db::Db) -> usize {
        let mut batch = Vec::new();
        {
            let map = match self.inner.lock() {
                Ok(m) => m,
                Err(_) => return 0,
            };
            for (&id, rt) in map.iter() {
                if rt.downloaded != rt.last_flushed {
                    batch.push((id, rt.downloaded));
                }
            }
        }
        let count = batch.len();
        for (id, downloaded) in &batch {
            let _ = db.update_download_progress(*id, *downloaded);
        }
        // Update last_flushed after successful DB write
        if let Ok(mut map) = self.inner.lock() {
            for (id, downloaded) in &batch {
                if let Some(rt) = map.get_mut(id) {
                    rt.last_flushed = *downloaded;
                }
            }
        }
        count
    }

    pub(crate) fn get_downloaded(&self, id: u64) -> Option<u64> {
        self.inner.lock().ok().and_then(|map| {
            map.get(&id).map(|rt| rt.downloaded)
        })
    }
}
