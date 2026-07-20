use crate::types::*;
use crate::state::db::Db;
use crate::state::gob;
use crate::state::runtime::DownloadManagerState;

/// Facade that coordinates the three download state layers:
/// - DB (SQLite): persistent records, flushed every 5 seconds
/// - gob (JSON files on disk): resume state for engine
/// - Runtime (in-memory HashMap): real-time progress
///
/// All state transitions go through this facade, making the
/// three-layer persistence an implementation detail.
pub struct DownloadStateFacade {
    pub(crate) db: Db,
    pub(crate) runtime: DownloadManagerState,
}

impl DownloadStateFacade {
    pub fn new(db: Db, runtime: DownloadManagerState) -> Self {
        Self { db, runtime }
    }

    // ── DB accessors (encapsulate direct db calls) ──

    pub fn get_item(&self, id: u64) -> PdmResult<Option<DownloadItem>> {
        self.db.get_by_id(id)
    }

    pub fn list_items(&self) -> PdmResult<Vec<DownloadItem>> {
        self.db.list_downloads()
    }

    pub fn insert_item(&self, item: &DownloadItem) -> PdmResult<()> {
        self.db.insert_download(item)
    }

    pub fn update_item(&self, item: &DownloadItem) -> PdmResult<()> {
        self.db.update_download(item)
    }

    pub fn delete_item(&self, id: u64) -> PdmResult<()> {
        self.db.delete_download(id)
    }

    // ── Lifecycle events ──

    /// Initialize runtime for a newly started download.
    /// If a gob file exists (resume case), seed runtime with saved progress
    /// so that flush_to_db doesn't reset the DB value to 0.
    pub fn on_started(&self, id: u64) {
        let saved = gob::load_state(id).ok().flatten();
        self.runtime.register(id);
        if let Some(s) = saved {
            if s.downloaded > 0 {
                self.runtime.update_progress(id, s.downloaded);
            }
        }
    }

    /// Update real-time progress (called on every DownloadProgress event).
    pub fn update_progress(&self, id: u64, downloaded: u64) {
        self.runtime.update_progress(id, downloaded);
    }

    /// Mark download as completed: clean up runtime, update DB status.
    pub fn on_completed(&self, id: u64) {
        self.runtime.remove(id);
        if let Ok(Some(mut item)) = self.db.get_by_id(id) {
            item.status = DownloadStatus::Completed;
            item.downloaded = item.total_size;
            for part in item.parts.iter_mut() {
                if !matches!(part.status, PartStatus::Completed) {
                    part.status = PartStatus::Completed;
                }
            }
            let _ = self.db.update_download(&item);
        }
    }

    /// Mark download as failed: clean up runtime, update DB status.
    /// Skips if the download is paused (paused downloads emit errors
    /// during cancel which should not overwrite the paused status).
    pub fn on_error(&self, id: u64, error_msg: String) {
        self.runtime.remove(id);
        if let Ok(Some(mut item)) = self.db.get_by_id(id) {
            if matches!(item.status, DownloadStatus::Paused) {
                return;
            }
            item.status = DownloadStatus::Failed(error_msg);
            for part in item.parts.iter_mut() {
                if matches!(part.status, PartStatus::Pending | PartStatus::Downloading) {
                    part.status = PartStatus::Failed("download failed".to_string());
                }
            }
            let _ = self.db.update_download(&item);
        }
    }

    /// Mark download as paused: flush progress, save resume state if needed,
    /// update DB status to Paused. The caller has already cancelled the engine.
    pub fn on_paused(&self, id: u64) -> PdmResult<()> {
        self.flush();
        if let Ok(Some(mut item)) = self.db.get_by_id(id) {
            if matches!(item.status, DownloadStatus::Downloading) {
                if item.resumable != Some(true) {
                    self.save_gob_for_non_resumable(&item);
                }
                item.status = DownloadStatus::Paused;
                self.db.update_download(&item)?;
            }
        }
        Ok(())
    }

    /// Delete a download: remove from DB and delete resume state.
    pub fn on_deleted(&self, id: u64) -> PdmResult<()> {
        self.db.delete_download(id)?;
        let _ = gob::delete_state(id);
        Ok(())
    }

    // ── gob accessors (public: needed by engine cancel callbacks and resume logic) ──

    /// Save engine resume state (called by engine cancel callback).
    pub fn save_resume_state(&self, id: u64, state: &DownloadState) {
        let _ = gob::save_state(id, state);
    }

    /// Load engine resume state for reconstructing engine config on resume.
    pub fn load_resume_state(&self, id: u64) -> Option<DownloadState> {
        gob::load_state(id).ok().flatten()
    }

    /// Save gob state for non-resumable downloads (used internally by on_paused).
    fn save_gob_for_non_resumable(&self, item: &DownloadItem) {
        let remaining = item.total_size.saturating_sub(item.downloaded);
        if remaining > 0 {
            let saved = DownloadState {
                url: item.url.clone(),
                id: item.id,
                file_name: item.file_name.clone(),
                save_path: item.save_path.clone(),
                total_size: item.total_size,
                downloaded: item.downloaded,
                tasks: vec![Task { offset: item.downloaded, length: remaining }],
                proxy_name: item.proxy_name.clone(),
                workers: 1,
            };
            let _ = gob::save_state(item.id, &saved);
        }
    }

    // ── Flush ──

    /// Flush all runtime progress to DB.
    pub fn flush(&self) -> usize {
        self.runtime.flush_to_db(&self.db)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn test_facade(suffix: &str) -> (DownloadStateFacade, PathBuf) {
        let dir = std::env::temp_dir().join(format!("pdm_facade_{}_{}", suffix, std::process::id()));
        std::fs::create_dir_all(&dir).ok();
        let path = dir.join("test.db");
        let db = Db::from_path(&path).unwrap();
        let runtime = DownloadManagerState::new();
        (DownloadStateFacade::new(db, runtime), dir)
    }

    fn sample_item(id: u64) -> DownloadItem {
        DownloadItem {
            id,
            url: format!("https://example.com/file{}.zip", id),
            file_name: format!("file{}.zip", id),
            save_path: format!("/tmp/file{}.zip", id),
            total_size: 1000,
            downloaded: 0,
            status: DownloadStatus::Queued,
            parts: vec![],
            proxy_name: "".to_string(),
            connections: 4,
            resumable: Some(true),
            created_at: "1234567890".to_string(),
            last_try: "".to_string(),
        }
    }

    #[test]
    fn test_facade_flush_empty() {
        let (facade, dir) = test_facade("flush");
        assert_eq!(facade.flush(), 0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_facade_crud() {
        let (facade, dir) = test_facade("crud");

        let item = sample_item(1);
        facade.insert_item(&item).unwrap();

        let items = facade.list_items().unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].id, 1);

        let got = facade.get_item(1).unwrap().unwrap();
        assert_eq!(got.file_name, "file1.zip");

        let mut updated = got;
        updated.downloaded = 500;
        facade.update_item(&updated).unwrap();
        let got2 = facade.get_item(1).unwrap().unwrap();
        assert_eq!(got2.downloaded, 500);

        facade.delete_item(1).unwrap();
        assert!(facade.list_items().unwrap().is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_on_completed_updates_status() {
        let (facade, dir) = test_facade("completed");
        let mut item = sample_item(7);
        item.status = DownloadStatus::Downloading;
        facade.insert_item(&item).unwrap();
        facade.runtime.register(7);
        facade.runtime.update_progress(7, 500);

        facade.on_completed(7);

        let got = facade.get_item(7).unwrap().unwrap();
        assert!(matches!(got.status, DownloadStatus::Completed));
        // downloaded stays at 0 in DB (on_completed only changes status)
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_on_error_sets_failed() {
        let (facade, dir) = test_facade("error");
        let mut item = sample_item(3);
        item.status = DownloadStatus::Downloading;
        facade.insert_item(&item).unwrap();
        facade.runtime.register(3);

        facade.on_error(3, "timeout".to_string());

        let got = facade.get_item(3).unwrap().unwrap();
        assert!(matches!(got.status, DownloadStatus::Failed(ref msg) if msg == "timeout"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_on_error_skips_paused() {
        let (facade, dir) = test_facade("error_paused");
        let mut item = sample_item(5);
        item.status = DownloadStatus::Paused;
        facade.insert_item(&item).unwrap();
        facade.runtime.register(5);

        facade.on_error(5, "cancelled".to_string());

        let got = facade.get_item(5).unwrap().unwrap();
        assert!(matches!(got.status, DownloadStatus::Paused)); // unchanged
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_on_paused_saves_gob_for_non_resumable() {
        let (facade, dir) = test_facade("paused");
        let mut item = sample_item(9);
        item.downloaded = 300;
        item.total_size = 1000;
        item.resumable = Some(false); // non-resumable
        item.status = DownloadStatus::Downloading;
        facade.insert_item(&item).unwrap();

        facade.on_paused(9).unwrap();

        // Should have saved gob for resume
        let loaded = facade.load_resume_state(9).unwrap();
        assert_eq!(loaded.downloaded, 300);
        assert_eq!(loaded.tasks.len(), 1);
        assert_eq!(loaded.tasks[0].offset, 300);
        assert_eq!(loaded.tasks[0].length, 700);

        // DB status should be Paused
        let got = facade.get_item(9).unwrap().unwrap();
        assert!(matches!(got.status, DownloadStatus::Paused));

        facade.on_deleted(9).unwrap();
        assert!(facade.load_resume_state(9).is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_on_started_seeds_runtime() {
        let (facade, dir) = test_facade("started");
        // First start — no gob, runtime register sets downloaded to current time
        facade.on_started(10);
        let initial = facade.runtime.get_downloaded(10);
        assert!(initial.is_some());

        // Simulate engine saved state at 500 bytes
        let state = DownloadState {
            url: "https://example.com/file.zip".to_string(),
            id: 10,
            file_name: "file.zip".to_string(),
            save_path: "/tmp/file.zip".to_string(),
            total_size: 1000,
            downloaded: 500,
            tasks: vec![],
            proxy_name: "".to_string(),
            workers: 4,
        };
        facade.save_resume_state(10, &state);

        // Second start (resume) — runtime should be seeded from gob (500)
        facade.on_started(10);
        assert_eq!(facade.runtime.get_downloaded(10), Some(500));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
