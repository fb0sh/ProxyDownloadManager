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
        let mut items = self.db.list_downloads()?;
        // Overlay in-memory progress (total + parts) so Progress Map stays live between flushes.
        for item in items.iter_mut() {
            if let Some(dl) = self.runtime.get_downloaded(item.id) {
                item.downloaded = dl;
            }
            if let Some(parts) = self.runtime.get_part_downloaded(item.id) {
                if !parts.is_empty() {
                    if item.parts.is_empty() {
                        item.parts = vec![crate::types::DownloadPart {
                            index: 0,
                            start: 0,
                            end: item.total_size,
                            downloaded: parts.first().copied().unwrap_or(0),
                            temp_path: String::new(),
                            status: crate::types::PartStatus::Downloading,
                            retries: 0,
                        }];
                    } else {
                        for (i, d) in parts.iter().enumerate() {
                            if let Some(part) = item.parts.get_mut(i) {
                                part.downloaded = *d;
                            }
                        }
                    }
                }
            }
        }
        Ok(items)
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
    /// Seed total + per-part progress from DB (and gob) so Progress Map / list
    /// don't flash 0 after pause→resume before the first engine event.
    pub fn on_started(&self, id: u64) {
        let saved = gob::load_state(id).ok().flatten();
        self.runtime.register(id);
        if let Ok(Some(item)) = self.db.get_by_id(id) {
            let part_dl: Vec<u64> = item.parts.iter().map(|p| p.downloaded).collect();
            let downloaded = saved
                .as_ref()
                .map(|s| s.downloaded)
                .unwrap_or(item.downloaded)
                .max(item.downloaded);
            if downloaded > 0 || !part_dl.is_empty() {
                self.runtime.update_progress(
                    id,
                    downloaded,
                    if part_dl.is_empty() { None } else { Some(part_dl) },
                );
            }
        } else if let Some(s) = saved {
            if s.downloaded > 0 {
                self.runtime.update_progress(id, s.downloaded, None);
            }
        }
    }

    /// Update real-time progress (called on every DownloadProgress event).
    /// Memory first; 1s flush loop writes `downloaded` + `parts` into SQLite.
    pub fn update_progress(
        &self,
        id: u64,
        downloaded: u64,
        part_downloaded: Option<Vec<u64>>,
        reset_to_single: bool,
    ) {
        if reset_to_single {
            let _ = self.db.reset_parts_to_single(id, downloaded);
            let parts = part_downloaded.clone().unwrap_or_else(|| vec![downloaded]);
            self.runtime.update_progress(id, downloaded, Some(parts));
            return;
        }
        self.runtime.update_progress(id, downloaded, part_downloaded);
    }

    /// Mark download as completed: clean up runtime, update DB status.
    pub fn on_completed(&self, id: u64) {
        self.runtime.remove(id);
        if let Ok(Some(mut item)) = self.db.get_by_id(id) {
            item.status = DownloadStatus::Completed;
            item.downloaded = item.total_size;
            for part in item.parts.iter_mut() {
                let len = part.end.saturating_sub(part.start);
                part.downloaded = len;
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
        // Merge runtime totals into the item row so part.downloaded is durable for resume.
        if let Ok(Some(mut item)) = self.db.get_by_id(id) {
            if matches!(item.status, DownloadStatus::Downloading) {
                if let Some(dl) = self.runtime.get_downloaded(id) {
                    item.downloaded = dl;
                }
                if let Some(parts) = self.runtime.get_part_downloaded(id) {
                    for (i, d) in parts.iter().enumerate() {
                        if let Some(part) = item.parts.get_mut(i) {
                            part.downloaded = *d;
                            let len = part.end.saturating_sub(part.start);
                            if *d >= len && len > 0 {
                                part.status = PartStatus::Completed;
                            } else if *d > 0 {
                                part.status = PartStatus::Downloading;
                            }
                        }
                    }
                }
                // Always persist gob for resume (engine cancel may have already written
                // remaining tasks; rebuild from parts as a durable fallback).
                self.save_resume_from_item(&item);
                item.status = DownloadStatus::Paused;
                self.db.update_download(&item)?;
            }
        }
        // Active engine is stopped — drop runtime so list_items reads DB parts.
        self.runtime.remove(id);
        Ok(())
    }

    /// Build DownloadState from fixed parts so resume never falls back to "from zero".
    /// Source of truth for resume is **DB parts** (+ optional gob task offsets).
    fn save_resume_from_item(&self, item: &DownloadItem) {
        use crate::engine::part_progress::{remaining_tasks_from_parts, PartRange};
        let ranges: Vec<PartRange> = if item.parts.is_empty() {
            if item.total_size > 0 {
                vec![PartRange {
                    start: 0,
                    end: item.total_size,
                }]
            } else {
                vec![]
            }
        } else {
            item.parts
                .iter()
                .map(|p| PartRange {
                    start: p.start,
                    end: p.end,
                })
                .collect()
        };
        let part_dl: Vec<u64> = if item.parts.is_empty() {
            vec![item.downloaded]
        } else {
            item.parts.iter().map(|p| p.downloaded).collect()
        };
        // Prefer existing gob tasks (more precise mid-chunk offsets) if present.
        let existing = gob::load_state(item.id).ok().flatten();
        let tasks = if let Some(ref s) = existing {
            if !s.tasks.is_empty() {
                s.tasks.clone()
            } else {
                remaining_tasks_from_parts(&ranges, &part_dl)
            }
        } else {
            remaining_tasks_from_parts(&ranges, &part_dl)
        };
        let downloaded = existing
            .as_ref()
            .map(|s| s.downloaded.max(item.downloaded))
            .unwrap_or(item.downloaded);
        let saved = DownloadState {
            url: item.url.clone(),
            id: item.id,
            file_name: item.file_name.clone(),
            save_path: item.save_path.clone(),
            total_size: item.total_size,
            downloaded,
            tasks,
            proxy_name: item.proxy_name.clone(),
            workers: item.connections.max(1),
        };
        let _ = gob::save_state(item.id, &saved);
    }

    /// If only total `downloaded` was flushed (parts still 0), paint a contiguous
    /// prefix into parts so resume/crash recovery can rebuild remaining work.
    fn backfill_parts_from_total(item: &mut DownloadItem) {
        if item.parts.is_empty() {
            if item.total_size > 0 {
                item.parts = vec![DownloadPart {
                    index: 0,
                    start: 0,
                    end: item.total_size,
                    downloaded: item.downloaded.min(item.total_size),
                    temp_path: String::new(),
                    status: if item.downloaded >= item.total_size {
                        PartStatus::Completed
                    } else if item.downloaded > 0 {
                        PartStatus::Downloading
                    } else {
                        PartStatus::Pending
                    },
                    retries: 0,
                }];
            }
            return;
        }
        let any = item.parts.iter().any(|p| p.downloaded > 0);
        if any || item.downloaded == 0 {
            return;
        }
        let mut remaining = item.downloaded;
        for part in item.parts.iter_mut() {
            let len = part.end.saturating_sub(part.start);
            let take = remaining.min(len);
            part.downloaded = take;
            remaining = remaining.saturating_sub(take);
            if take >= len && len > 0 {
                part.status = PartStatus::Completed;
            } else if take > 0 {
                part.status = PartStatus::Downloading;
            }
        }
    }

    /// App start: rows left as Downloading after crash/kill → Paused, with
    /// resume metadata rebuilt from **database** parts (and gob if present).
    /// Returns how many items were recovered.
    pub fn recover_stale_downloads(&self) -> usize {
        let Ok(items) = self.db.list_downloads() else {
            return 0;
        };
        let mut n = 0usize;
        for mut item in items {
            if !matches!(item.status, DownloadStatus::Downloading) {
                continue;
            }
            Self::backfill_parts_from_total(&mut item);
            item.status = DownloadStatus::Paused;
            if self.db.update_download(&item).is_ok() {
                self.save_resume_from_item(&item);
                n += 1;
                log::info!(
                    "[ProxyDM] crash recovery id={} paused downloaded={}/{} parts={}",
                    item.id,
                    item.downloaded,
                    item.total_size,
                    item.parts.len()
                );
            }
        }
        n
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
        facade.runtime.update_progress(7, 500, None);

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
    fn test_recover_stale_downloads_pauses_and_keeps_progress() {
        let (facade, dir) = test_facade("recover");
        let mut item = sample_item(42);
        item.status = DownloadStatus::Downloading;
        item.downloaded = 400;
        item.total_size = 1000;
        item.parts = vec![
            DownloadPart {
                index: 0,
                start: 0,
                end: 500,
                downloaded: 200,
                temp_path: String::new(),
                status: PartStatus::Downloading,
                retries: 0,
            },
            DownloadPart {
                index: 1,
                start: 500,
                end: 1000,
                downloaded: 200,
                temp_path: String::new(),
                status: PartStatus::Downloading,
                retries: 0,
            },
        ];
        facade.insert_item(&item).unwrap();

        let n = facade.recover_stale_downloads();
        assert_eq!(n, 1);

        let got = facade.get_item(42).unwrap().unwrap();
        assert!(matches!(got.status, DownloadStatus::Paused));
        assert_eq!(got.downloaded, 400);
        assert_eq!(got.parts[0].downloaded, 200);
        assert_eq!(got.parts[1].downloaded, 200);

        let gob = facade.load_resume_state(42).unwrap();
        assert_eq!(gob.downloaded, 400);
        assert!(!gob.tasks.is_empty());

        facade.on_deleted(42).unwrap();
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
