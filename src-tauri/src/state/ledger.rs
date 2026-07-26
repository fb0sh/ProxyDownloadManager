use crate::engine::part_progress::{
    parts_downloaded_from_tasks, remaining_tasks_from_parts, PartRange,
};
use crate::state::db::Db;
use crate::state::gob;
use crate::state::runtime::DownloadManagerState;
use crate::types::*;

/// 进度账本 (Progress Ledger): the one owner of download progress state.
///
/// Progress exists in three copies — the in-memory runtime map, the SQLite row
/// (`downloaded` + `parts`, always written in one statement), and the gob
/// resume file. Every write and every reconciliation between them goes through
/// this module; callers never learn which copy a number came from.
///
/// The single reconciliation rule lives in [`reconcile`].
pub struct ProgressLedger {
    db: Db,
    runtime: DownloadManagerState,
}

/// Reconciled progress for one download, derived from a single chosen source.
/// `ranges`, `part_downloaded` and `tasks` are always mutually consistent.
struct ProgressBasis {
    ranges: Vec<PartRange>,
    part_downloaded: Vec<u64>,
    tasks: Vec<Task>,
    downloaded: u64,
}

/// THE reconciliation rule — the only place that decides which progress copy
/// wins. Candidates are the DB row's per-part bytes and the gob file's
/// remaining tasks (engine truth at cancel); whichever implies more verified
/// progress is chosen, and every other quantity (total, per-part bytes,
/// remaining tasks) is derived from that one source so they can never
/// disagree. A bare byte total is only trusted as a prefix when no multi-part
/// plan exists — concurrent writes are not a file prefix.
fn reconcile(item: &DownloadItem, saved: Option<&DownloadState>) -> ProgressBasis {
    let ranges: Vec<PartRange> = if item.parts.is_empty() {
        if item.total_size > 0 {
            vec![PartRange { start: 0, end: item.total_size }]
        } else {
            vec![]
        }
    } else {
        item.parts
            .iter()
            .map(|p| PartRange { start: p.start, end: p.end })
            .collect()
    };

    // Size unknown: parts carry no information (a single 0-length cell) and a
    // resume restarts from scratch regardless — keep the byte total so pause
    // and crash recovery don't zero the displayed progress.
    if item.total_size == 0 {
        return ProgressBasis {
            part_downloaded: vec![0; ranges.len()],
            tasks: vec![],
            downloaded: item.downloaded,
            ranges,
        };
    }

    // Candidate A: per-part progress from the DB row. When no parts were ever
    // planned the download was sequential, so a byte-total prefix is valid.
    let db_parts: Vec<u64> = if item.parts.is_empty() {
        ranges.iter().map(|r| item.downloaded.min(r.len())).collect()
    } else {
        item.parts
            .iter()
            .zip(&ranges)
            .map(|(p, r)| p.downloaded.min(r.len()))
            .collect()
    };
    let db_sum: u64 = db_parts.iter().sum();

    // Candidate B: remaining tasks from the gob file. Only comparable when it
    // describes the same file (total size matches) and actually has tasks.
    // Consistency guard: the progress implied by the task list must not exceed
    // the byte counter saved alongside it — a task list that under-covers the
    // remaining work (older versions could drop in-flight tasks on cancel)
    // would otherwise inflate progress and resume past bytes never fetched.
    let gob = saved.filter(|s| s.total_size == item.total_size && !s.tasks.is_empty());
    let gob_parts = gob.map(|s| parts_downloaded_from_tasks(&ranges, &s.tasks));

    match (gob, gob_parts) {
        (Some(s), Some(parts))
            if parts.iter().sum::<u64>() >= db_sum
                && parts.iter().sum::<u64>() <= s.downloaded =>
        {
            ProgressBasis {
                downloaded: parts.iter().sum(),
                part_downloaded: parts,
                tasks: s.tasks.clone(),
                ranges,
            }
        }
        _ => ProgressBasis {
            downloaded: db_sum,
            tasks: remaining_tasks_from_parts(&ranges, &db_parts),
            part_downloaded: db_parts,
            ranges,
        },
    }
}

/// Write a basis back onto the item: total, per-part bytes and part statuses.
/// Creates the single part row when none were planned, so the DB row is
/// self-describing afterwards.
fn apply_basis(item: &mut DownloadItem, basis: &ProgressBasis) {
    item.downloaded = basis.downloaded;
    if item.parts.is_empty() && !basis.ranges.is_empty() {
        item.parts = basis
            .ranges
            .iter()
            .enumerate()
            .map(|(i, r)| DownloadPart {
                index: i as u32,
                start: r.start,
                end: r.end,
                downloaded: 0,
                temp_path: String::new(),
                status: PartStatus::Pending,
                retries: 0,
            })
            .collect();
    }
    for (i, d) in basis.part_downloaded.iter().enumerate() {
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

impl ProgressLedger {
    pub fn new(db: Db) -> Self {
        Self {
            db,
            runtime: DownloadManagerState::new(),
        }
    }

    // ── Reads (the ledger is the only owner of the Db) ──

    pub fn get_item(&self, id: u64) -> PdmResult<Option<DownloadItem>> {
        self.db.get_by_id(id)
    }

    /// List rows with live runtime progress overlaid, so the Progress Map and
    /// the table stay current between flushes.
    pub fn list_items(&self) -> PdmResult<Vec<DownloadItem>> {
        let mut items = self.db.list_downloads()?;
        for item in items.iter_mut() {
            if let Some(dl) = self.runtime.get_downloaded(item.id) {
                item.downloaded = dl;
            }
            if let Some(parts) = self.runtime.get_part_downloaded(item.id) {
                if !parts.is_empty() {
                    if item.parts.is_empty() {
                        item.parts = vec![DownloadPart {
                            index: 0,
                            start: 0,
                            end: item.total_size,
                            downloaded: parts.first().copied().unwrap_or(0),
                            temp_path: String::new(),
                            status: PartStatus::Downloading,
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

    // ── Lifecycle transitions ──

    /// Initialize runtime for a started download, seeded from the reconciled
    /// basis so the Progress Map doesn't flash 0 after pause→resume before the
    /// first engine event.
    pub fn on_started(&self, id: u64) {
        self.runtime.register(id);
        if let Ok(Some(item)) = self.db.get_by_id(id) {
            let saved = gob::load_state(id).ok().flatten();
            let basis = reconcile(&item, saved.as_ref());
            if basis.downloaded > 0 {
                self.runtime
                    .update_progress(id, basis.downloaded, Some(basis.part_downloaded));
            }
        }
    }

    /// Record real-time progress (called on every DownloadProgress event).
    /// Memory first; the 1s flush loop writes `downloaded` + `parts` into
    /// SQLite in one statement. Degrading to Single restructures the DB parts
    /// once, then flows through the same memory-first path.
    pub fn record_progress(
        &self,
        id: u64,
        downloaded: u64,
        part_downloaded: Option<Vec<u64>>,
        reset_to_single: bool,
    ) {
        // After invalidate_for_restart, events queued before the restart are
        // stale and must not resurrect pre-truncate progress.
        if !self.runtime.restart_gate(id, reset_to_single) {
            return;
        }
        if reset_to_single {
            if self.runtime.mark_single_reset(id) {
                let _ = self.db.reset_parts_to_single(id, downloaded);
                // The Single engine truncated the file and restarted — any
                // prior gob describes a dead layout; a resume that trusted it
                // would write old task offsets into the truncated file.
                let _ = gob::delete_state(id);
            }
            let parts = part_downloaded.unwrap_or_else(|| vec![downloaded]);
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

    /// Mark download as paused. The caller has already cancelled the engine,
    /// so runtime is final: flush it, reconcile with the gob the engine wrote
    /// on cancel, and persist the winning basis to both DB and gob.
    pub fn on_paused(&self, id: u64) -> PdmResult<()> {
        self.flush();
        self.runtime.remove(id);
        if let Ok(Some(mut item)) = self.db.get_by_id(id) {
            if matches!(item.status, DownloadStatus::Downloading) {
                let saved = gob::load_state(id).ok().flatten();
                let basis = reconcile(&item, saved.as_ref());
                apply_basis(&mut item, &basis);
                item.status = DownloadStatus::Paused;
                self.db.update_download(&item)?;
                self.save_gob(&item, &basis);
            }
        }
        Ok(())
    }

    /// Validate, transition to Downloading, and return the complete resume
    /// plan. The plan is the only path to a resume `EngineConfig` — callers
    /// never patch fields afterwards.
    pub fn begin_resume(&self, id: u64) -> PdmResult<ResumePlan> {
        let mut item = self.db.get_by_id(id)?.ok_or(PdmError::NotFound(id))?;
        if matches!(
            item.status,
            DownloadStatus::Downloading | DownloadStatus::Completed
        ) {
            return Err(PdmError::Other(format!(
                "Download {} is {:?} — nothing to resume",
                id, item.status
            )));
        }
        let saved = gob::load_state(id).ok().flatten();
        let basis = reconcile(&item, saved.as_ref());
        item.status = DownloadStatus::Downloading;
        item.last_try = now_str();
        // Never zero out progress on resume — only status/last_try change.
        self.db.update_download(&item)?;
        Ok(ResumePlan {
            downloaded: basis.downloaded,
            part_ranges: basis.ranges.iter().map(|r| (r.start, r.end)).collect(),
            part_downloaded: basis.part_downloaded,
            tasks: basis.tasks,
            item,
        })
    }

    /// App start: rows left as Downloading after crash/kill → Paused, with
    /// resume metadata rebuilt through [`reconcile`]. Never fabricates
    /// progress: if only a byte total survived for a multi-part download, the
    /// parts (all zero) win and the total is corrected down.
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
            let saved = gob::load_state(item.id).ok().flatten();
            let basis = reconcile(&item, saved.as_ref());
            apply_basis(&mut item, &basis);
            item.status = DownloadStatus::Paused;
            if self.db.update_download(&item).is_ok() {
                self.save_gob(&item, &basis);
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

    /// Persist engine resume state (engine cancel callback).
    pub fn save_resume_state(&self, id: u64, state: &DownloadState) {
        let _ = gob::save_state(id, state);
    }

    /// Engine hook, called right before a degrade truncates the file: reset
    /// every progress record FIRST, so a crash between the two can never
    /// leave a copy claiming progress the truncated file no longer has.
    /// Also arms the restart gate: progress events already queued from before
    /// the restart get dropped instead of resurrecting stale numbers.
    pub fn invalidate_for_restart(&self, id: u64) {
        let _ = self.db.reset_parts_to_single(id, 0);
        let _ = gob::delete_state(id);
        self.runtime.mark_single_reset(id);
        self.runtime.update_progress(id, 0, Some(vec![0]));
        self.runtime.set_restart_pending(id);
    }

    /// Flush all runtime progress to DB. Returns entries flushed.
    pub fn flush(&self) -> usize {
        self.runtime.flush_to_db(&self.db)
    }

    fn save_gob(&self, item: &DownloadItem, basis: &ProgressBasis) {
        let saved = DownloadState {
            url: item.url.clone(),
            id: item.id,
            file_name: item.file_name.clone(),
            save_path: item.save_path.clone(),
            total_size: item.total_size,
            downloaded: basis.downloaded,
            tasks: basis.tasks.clone(),
            proxy_name: item.proxy_name.clone(),
            workers: item.connections.max(1),
        };
        let _ = gob::save_state(item.id, &saved);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn test_ledger(suffix: &str) -> (ProgressLedger, PathBuf) {
        gob::init_test_home();
        let dir = std::env::temp_dir().join(format!("pdm_ledger_{}_{}", suffix, std::process::id()));
        std::fs::create_dir_all(&dir).ok();
        let path = dir.join("test.db");
        let db = Db::from_path(&path).unwrap();
        (ProgressLedger::new(db), dir)
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

    fn two_parts(dl0: u64, dl1: u64) -> Vec<DownloadPart> {
        vec![
            DownloadPart {
                index: 0,
                start: 0,
                end: 500,
                downloaded: dl0,
                temp_path: String::new(),
                status: PartStatus::Downloading,
                retries: 0,
            },
            DownloadPart {
                index: 1,
                start: 500,
                end: 1000,
                downloaded: dl1,
                temp_path: String::new(),
                status: PartStatus::Downloading,
                retries: 0,
            },
        ]
    }

    fn gob_state(id: u64, downloaded: u64, tasks: Vec<Task>) -> DownloadState {
        DownloadState {
            url: format!("https://example.com/file{}.zip", id),
            id,
            file_name: format!("file{}.zip", id),
            save_path: format!("/tmp/file{}.zip", id),
            total_size: 1000,
            downloaded,
            tasks,
            proxy_name: "".to_string(),
            workers: 4,
        }
    }

    // ── reconcile: the one rule ──

    #[test]
    fn reconcile_prefers_gob_when_it_has_more_progress() {
        let mut item = sample_item(1);
        item.parts = two_parts(100, 100); // DB says 200
        // gob says only 300 bytes remain → 700 done
        let gob = gob_state(1, 700, vec![Task { offset: 400, length: 100 }, Task { offset: 800, length: 200 }]);
        let basis = reconcile(&item, Some(&gob));
        assert_eq!(basis.downloaded, 700);
        assert_eq!(basis.part_downloaded, vec![400, 300]);
        assert_eq!(basis.tasks, gob.tasks);
    }

    #[test]
    fn reconcile_prefers_db_parts_over_stale_gob() {
        let mut item = sample_item(1);
        item.parts = two_parts(400, 300); // DB says 700
        // stale gob from an earlier pause: says 700 bytes remain → 300 done
        let gob = gob_state(1, 300, vec![Task { offset: 300, length: 200 }, Task { offset: 500, length: 500 }]);
        let basis = reconcile(&item, Some(&gob));
        assert_eq!(basis.downloaded, 700);
        assert_eq!(basis.part_downloaded, vec![400, 300]);
        // tasks rebuilt from parts, not taken from the stale gob
        assert_eq!(
            basis.tasks,
            vec![Task { offset: 400, length: 100 }, Task { offset: 800, length: 200 }]
        );
    }

    #[test]
    fn reconcile_ignores_gob_with_mismatched_total_size() {
        let mut item = sample_item(1);
        item.parts = two_parts(100, 0);
        let mut gob = gob_state(1, 900, vec![Task { offset: 900, length: 100 }]);
        gob.total_size = 2000; // different file
        let basis = reconcile(&item, Some(&gob));
        assert_eq!(basis.downloaded, 100);
    }

    #[test]
    fn reconcile_rejects_undercovering_gob() {
        // Legacy gob whose task list dropped an in-flight task on cancel:
        // tasks imply 800 done but the byte counter only saw 300. Trusting the
        // tasks would resume past bytes never fetched — must fall back to DB.
        let mut item = sample_item(1);
        item.parts = two_parts(100, 100);
        let gob = gob_state(1, 300, vec![Task { offset: 800, length: 200 }]); // implies 800 done
        let basis = reconcile(&item, Some(&gob));
        assert_eq!(basis.downloaded, 200, "under-covering gob must be rejected");
        assert_eq!(basis.part_downloaded, vec![100, 100]);
    }

    #[test]
    fn reconcile_never_fabricates_prefix_for_multi_part() {
        // The F1 corruption case: only the byte total was flushed, parts are
        // all zero, no gob. The old code painted a contiguous prefix; for a
        // concurrent download those bytes were never a prefix.
        let mut item = sample_item(1);
        item.downloaded = 400;
        item.parts = two_parts(0, 0);
        let basis = reconcile(&item, None);
        assert_eq!(basis.downloaded, 0, "byte total must not be trusted");
        assert_eq!(basis.part_downloaded, vec![0, 0]);
        // remaining work = everything
        assert_eq!(
            basis.tasks,
            vec![Task { offset: 0, length: 500 }, Task { offset: 500, length: 500 }]
        );
    }

    #[test]
    fn reconcile_preserves_total_for_unknown_size() {
        // Blind download (size unknown): the planner stores one 0-length part,
        // which must not clamp the displayed byte total to zero on pause/crash.
        let mut item = sample_item(1);
        item.total_size = 0;
        item.downloaded = 12345;
        item.parts = vec![DownloadPart {
            index: 0,
            start: 0,
            end: 0,
            downloaded: 0,
            temp_path: String::new(),
            status: PartStatus::Downloading,
            retries: 0,
        }];
        let basis = reconcile(&item, None);
        assert_eq!(basis.downloaded, 12345);
        assert!(basis.tasks.is_empty());
    }

    #[test]
    fn reconcile_trusts_prefix_when_no_parts_planned() {
        // Sequential download (no parts row): byte total is a valid prefix.
        let mut item = sample_item(1);
        item.downloaded = 300;
        let basis = reconcile(&item, None);
        assert_eq!(basis.downloaded, 300);
        assert_eq!(basis.part_downloaded, vec![300]);
        assert_eq!(basis.tasks, vec![Task { offset: 300, length: 700 }]);
    }

    // ── lifecycle ──

    #[test]
    fn test_ledger_flush_empty() {
        let (ledger, dir) = test_ledger("flush");
        assert_eq!(ledger.flush(), 0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_ledger_crud() {
        let (ledger, dir) = test_ledger("crud");

        // id 61: on_deleted removes this id's gob file in the GLOBAL state
        // dir, and gob.rs tests use id 1 concurrently.
        let item = sample_item(61);
        ledger.insert_item(&item).unwrap();

        let items = ledger.list_items().unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].id, 61);

        let got = ledger.get_item(61).unwrap().unwrap();
        assert_eq!(got.file_name, "file61.zip");

        ledger.on_deleted(61).unwrap();
        assert!(ledger.list_items().unwrap().is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_on_completed_updates_status() {
        let (ledger, dir) = test_ledger("completed");
        let mut item = sample_item(7);
        item.status = DownloadStatus::Downloading;
        ledger.insert_item(&item).unwrap();
        ledger.runtime.register(7);
        ledger.runtime.update_progress(7, 500, None);

        ledger.on_completed(7);

        let got = ledger.get_item(7).unwrap().unwrap();
        assert!(matches!(got.status, DownloadStatus::Completed));
        // on_completed also snaps downloaded to total_size
        assert_eq!(got.downloaded, 1000);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_on_error_sets_failed() {
        let (ledger, dir) = test_ledger("error");
        let mut item = sample_item(3);
        item.status = DownloadStatus::Downloading;
        ledger.insert_item(&item).unwrap();
        ledger.runtime.register(3);

        ledger.on_error(3, "timeout".to_string());

        let got = ledger.get_item(3).unwrap().unwrap();
        assert!(matches!(got.status, DownloadStatus::Failed(ref msg) if msg == "timeout"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_on_error_skips_paused() {
        let (ledger, dir) = test_ledger("error_paused");
        let mut item = sample_item(5);
        item.status = DownloadStatus::Paused;
        ledger.insert_item(&item).unwrap();
        ledger.runtime.register(5);

        ledger.on_error(5, "cancelled".to_string());

        let got = ledger.get_item(5).unwrap().unwrap();
        assert!(matches!(got.status, DownloadStatus::Paused)); // unchanged
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_on_paused_saves_gob_for_non_resumable() {
        let (ledger, dir) = test_ledger("paused");
        let mut item = sample_item(9);
        item.downloaded = 300;
        item.total_size = 1000;
        item.resumable = Some(false); // non-resumable
        item.status = DownloadStatus::Downloading;
        ledger.insert_item(&item).unwrap();

        ledger.on_paused(9).unwrap();

        // Should have saved gob for resume
        let loaded = gob::load_state(9).unwrap().unwrap();
        assert_eq!(loaded.downloaded, 300);
        assert_eq!(loaded.tasks.len(), 1);
        assert_eq!(loaded.tasks[0].offset, 300);
        assert_eq!(loaded.tasks[0].length, 700);

        // DB status should be Paused
        let got = ledger.get_item(9).unwrap().unwrap();
        assert!(matches!(got.status, DownloadStatus::Paused));

        ledger.on_deleted(9).unwrap();
        assert!(gob::load_state(9).unwrap().is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_recover_stale_downloads_pauses_and_keeps_progress() {
        let (ledger, dir) = test_ledger("recover");
        let mut item = sample_item(42);
        item.status = DownloadStatus::Downloading;
        item.downloaded = 400;
        item.total_size = 1000;
        item.parts = two_parts(200, 200);
        ledger.insert_item(&item).unwrap();

        let n = ledger.recover_stale_downloads();
        assert_eq!(n, 1);

        let got = ledger.get_item(42).unwrap().unwrap();
        assert!(matches!(got.status, DownloadStatus::Paused));
        assert_eq!(got.downloaded, 400);
        assert_eq!(got.parts[0].downloaded, 200);
        assert_eq!(got.parts[1].downloaded, 200);

        let gob = gob::load_state(42).unwrap().unwrap();
        assert_eq!(gob.downloaded, 400);
        assert!(!gob.tasks.is_empty());

        ledger.on_deleted(42).unwrap();
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_recover_does_not_fabricate_prefix() {
        // Crash left downloaded=400 but parts all zero (the old fabrication
        // case). Recovery must trust the parts and restart cleanly instead of
        // inventing a contiguous prefix that corrupts the file on resume.
        let (ledger, dir) = test_ledger("recover_nofab");
        let mut item = sample_item(43);
        item.status = DownloadStatus::Downloading;
        item.downloaded = 400;
        item.parts = two_parts(0, 0);
        ledger.insert_item(&item).unwrap();

        let n = ledger.recover_stale_downloads();
        assert_eq!(n, 1);

        let got = ledger.get_item(43).unwrap().unwrap();
        assert!(matches!(got.status, DownloadStatus::Paused));
        assert_eq!(got.downloaded, 0, "must not keep an unverifiable total");
        assert_eq!(got.parts[0].downloaded, 0);
        assert_eq!(got.parts[1].downloaded, 0);

        // gob covers the whole file again
        let gob = gob::load_state(43).unwrap().unwrap();
        assert_eq!(
            gob.tasks,
            vec![Task { offset: 0, length: 500 }, Task { offset: 500, length: 500 }]
        );

        ledger.on_deleted(43).unwrap();
        let _ = std::fs::remove_dir_all(&dir);
    }

    // ── begin_resume ──

    #[test]
    fn test_begin_resume_returns_complete_plan() {
        let (ledger, dir) = test_ledger("resume_plan");
        let mut item = sample_item(50);
        item.status = DownloadStatus::Paused;
        item.parts = two_parts(500, 100); // part 0 complete
        item.downloaded = 600;
        ledger.insert_item(&item).unwrap();

        let plan = ledger.begin_resume(50).unwrap();
        assert_eq!(plan.downloaded, 600);
        assert_eq!(plan.part_ranges, vec![(0, 500), (500, 1000)]);
        assert_eq!(plan.part_downloaded, vec![500, 100]);
        assert_eq!(plan.tasks, vec![Task { offset: 600, length: 400 }]);
        assert_eq!(plan.item.id, 50);

        // status flipped to Downloading, progress untouched
        let got = ledger.get_item(50).unwrap().unwrap();
        assert!(matches!(got.status, DownloadStatus::Downloading));
        assert_eq!(got.downloaded, 600);
        assert!(!got.last_try.is_empty());

        ledger.on_deleted(50).unwrap();
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_begin_resume_prefers_gob_tasks() {
        let (ledger, dir) = test_ledger("resume_gob");
        let mut item = sample_item(51);
        item.status = DownloadStatus::Paused;
        item.parts = two_parts(100, 100);
        ledger.insert_item(&item).unwrap();
        // engine-written gob: precise mid-chunk offsets, more progress than DB
        let saved = gob_state(51, 700, vec![Task { offset: 450, length: 50 }, Task { offset: 700, length: 300 }]);
        ledger.save_resume_state(51, &saved);

        let plan = ledger.begin_resume(51).unwrap();
        assert_eq!(plan.tasks, saved.tasks);
        assert_eq!(plan.downloaded, 650); // derived from tasks: 1000 - 350
        assert_eq!(plan.part_downloaded, vec![450, 200]);

        ledger.on_deleted(51).unwrap();
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_begin_resume_rejects_wrong_status() {
        let (ledger, dir) = test_ledger("resume_guard");
        let mut item = sample_item(52);
        item.status = DownloadStatus::Completed;
        ledger.insert_item(&item).unwrap();
        assert!(ledger.begin_resume(52).is_err());

        let mut item = sample_item(53);
        item.status = DownloadStatus::Downloading;
        ledger.insert_item(&item).unwrap();
        assert!(ledger.begin_resume(53).is_err());

        assert!(ledger.begin_resume(999).is_err()); // not found

        ledger.on_deleted(52).unwrap();
        ledger.on_deleted(53).unwrap();
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_invalidate_gates_stale_events_until_reset() {
        let (ledger, dir) = test_ledger("restart_gate");
        let mut item = sample_item(70);
        item.status = DownloadStatus::Downloading;
        item.parts = two_parts(0, 0);
        ledger.insert_item(&item).unwrap();

        ledger.on_started(70);
        ledger.record_progress(70, 600, Some(vec![300, 300]), false);
        assert_eq!(ledger.runtime.get_downloaded(70), Some(600));

        // Degrade: records invalidated before the truncate.
        ledger.invalidate_for_restart(70);
        assert_eq!(ledger.runtime.get_downloaded(70), Some(0));

        // A stale pre-restart event must be dropped, not resurrect 600.
        ledger.record_progress(70, 600, Some(vec![300, 300]), false);
        assert_eq!(ledger.runtime.get_downloaded(70), Some(0));

        // The restart's own reset event clears the gate…
        ledger.record_progress(70, 0, Some(vec![0]), true);
        assert_eq!(ledger.runtime.get_downloaded(70), Some(0));

        // …and the Single engine's real progress flows again.
        ledger.record_progress(70, 100, Some(vec![100]), true);
        assert_eq!(ledger.runtime.get_downloaded(70), Some(100));

        ledger.on_deleted(70).unwrap();
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_on_started_seeds_runtime_from_gob() {
        let (ledger, dir) = test_ledger("started");
        let mut item = sample_item(10);
        item.status = DownloadStatus::Downloading;
        ledger.insert_item(&item).unwrap();

        // First start — no gob, no progress: runtime registers at 0.
        ledger.on_started(10);
        assert_eq!(ledger.runtime.get_downloaded(10), Some(0));

        // Engine saved state: 500 bytes remain → 500 done.
        let state = gob_state(10, 500, vec![Task { offset: 500, length: 500 }]);
        ledger.save_resume_state(10, &state);

        // Second start (resume) — runtime seeded via reconcile (500).
        ledger.on_started(10);
        assert_eq!(ledger.runtime.get_downloaded(10), Some(500));

        ledger.on_deleted(10).unwrap();
        let _ = std::fs::remove_dir_all(&dir);
    }
}
