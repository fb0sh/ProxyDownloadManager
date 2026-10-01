//! The download lifecycle driven through `DownloadManager`, the way the Tauri
//! commands drive it: a local origin on one side, the ledger rows and the
//! events the frontend would receive on the other.

use super::*;
use crate::engine::file_io;
use crate::engine::range_http::{assert_file, spawn_server, BodyMode};
use crate::state::db::Db;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};
use tokio::sync::mpsc;

/// One event as the frontend would get it, plus the ledger status of that
/// download at the moment it was emitted.
struct Seen {
    name: &'static str,
    payload: serde_json::Value,
    status: Option<DownloadStatus>,
}

/// `DownloadStarted` carries a bare id; every other event carries `{ "id": … }`.
fn event_id(payload: &serde_json::Value) -> Option<u64> {
    payload
        .as_u64()
        .or_else(|| payload.get("id").and_then(|id| id.as_u64()))
}

struct Rig {
    dm: Arc<DownloadManager>,
    ledger: Arc<ProgressLedger>,
    seen: Arc<Mutex<Vec<Seen>>>,
    dir: PathBuf,
}

impl Rig {
    /// `max_active == 0` is unlimited, as in settings.
    fn new(max_active: u32) -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        crate::state::gob::init_test_home();
        let dir = std::env::temp_dir().join(format!("pdm_lifecycle_{}_{n}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let ledger = Arc::new(ProgressLedger::new(
            Db::from_path(&dir.join("test.db")).unwrap(),
        ));

        let seen = Arc::new(Mutex::new(Vec::new()));
        let bus = {
            let seen = seen.clone();
            let ledger = ledger.clone();
            Arc::new(EventBus::with_sink(move |name, payload| {
                let status = event_id(&payload)
                    .and_then(|id| ledger.get_item(id).ok().flatten())
                    .map(|item| item.status);
                seen.lock().unwrap().push(Seen {
                    name,
                    payload,
                    status,
                });
            }))
        };

        // Temp files are keyed by id under one shared test home, so every rig
        // takes its own id block, clear of the ranges the engine tests use.
        let first_id = (std::process::id() as u64) * 1_000_000 + n * 1_000;
        let (event_tx, mut event_rx) = mpsc::unbounded_channel();
        let pool = WorkerPool::new(max_active, event_tx, false, first_id, 0);
        let settings = Arc::new(SettingsService::with_settings(Settings {
            download_dir: dir.to_string_lossy().into_owned(),
            // One retry: enough to see it happen, short enough to wait out.
            max_retries: 1,
            ..Settings::default()
        }));
        let dm = Arc::new(DownloadManager::new(
            ledger.clone(),
            pool,
            Logger::new().unwrap(),
            settings,
            bus,
        ));

        // The same pump lib.rs runs: engine events → DownloadManager.
        let pump = dm.clone();
        tokio::spawn(async move {
            while let Some(event) = event_rx.recv().await {
                pump.handle_event(event);
            }
        });

        Self {
            dm,
            ledger,
            seen,
            dir,
        }
    }

    async fn start(&self, url: &str, file_name: &str, start_paused: bool) -> PdmResult<u64> {
        self.dm
            .start_download(
                url.to_string(),
                file_name.to_string(),
                String::new(),
                String::new(),
                4,
                HashMap::new(),
                0,
                start_paused,
            )
            .await
    }

    /// The row as `list_downloads` returns it: live progress overlaid.
    fn item(&self, id: u64) -> DownloadItem {
        self.ledger
            .list_items()
            .unwrap()
            .into_iter()
            .find(|item| item.id == id)
            .unwrap_or_else(|| panic!("download {id} has no row"))
    }

    async fn wait_for(
        &self,
        id: u64,
        what: &str,
        done: impl Fn(&DownloadItem) -> bool,
    ) -> DownloadItem {
        for _ in 0..500 {
            let item = self.item(id);
            if done(&item) {
                return item;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        let item = self.item(id);
        panic!(
            "download {id} never reached {what}: status={:?} downloaded={}",
            item.status, item.downloaded
        );
    }

    async fn wait_completed(&self, id: u64) -> DownloadItem {
        self.wait_for(id, "Completed", |item| {
            matches!(item.status, DownloadStatus::Completed)
        })
        .await
    }

    /// Names of the events emitted for one download, in order.
    fn names(&self, id: u64) -> Vec<&'static str> {
        self.seen
            .lock()
            .unwrap()
            .iter()
            .filter(|event| event_id(&event.payload) == Some(id))
            .map(|event| event.name)
            .collect()
    }

    /// Position of an event in everything the frontend was sent.
    fn position(&self, id: u64, event: FrontendEvent) -> Option<usize> {
        self.seen
            .lock()
            .unwrap()
            .iter()
            .position(|e| e.name == event.name() && event_id(&e.payload) == Some(id))
    }
}

impl Drop for Rig {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

#[tokio::test]
async fn a_started_download_completes_and_the_ledger_knows_first() {
    let rig = Rig::new(0);
    let size = 8 * 1024 * 1024;
    let (url, _srv) = spawn_server(size, BodyMode::Exact, false).await;

    let id = rig.start(&url, "plain.bin", false).await.unwrap();
    let done = rig.wait_completed(id).await;

    assert_eq!(done.downloaded, size);
    assert_eq!(done.parts.len(), 4, "four connections plan four parts");
    assert!(done.parts.iter().all(|p| p.downloaded == p.end - p.start));
    assert_file(&done.save_path, size);
    assert!(!Path::new(&file_io::temp_path(id)).exists());

    let started = rig.position(id, FrontendEvent::DownloadStarted);
    let completed = rig.position(id, FrontendEvent::DownloadCompleted);
    assert!(started.is_some() && started < completed);
    assert!(rig
        .names(id)
        .contains(&FrontendEvent::DownloadProgress.name()));

    // The frontend refetches on this event, so the row must already say so.
    let seen = rig.seen.lock().unwrap();
    let event = &seen[completed.unwrap()];
    assert!(matches!(event.status, Some(DownloadStatus::Completed)));
    assert_eq!(event.payload["file_name"], "plain.bin");
}

#[tokio::test]
async fn pause_keeps_the_bytes_and_resume_finishes_the_same_download() {
    let rig = Rig::new(0);
    let size = 256 * 1024;
    let (url, srv) = spawn_server(size, BodyMode::Exact, true).await;

    let id = rig.start(&url, "paused.bin", false).await.unwrap();
    rig.wait_for(id, "its first bytes", |item| item.downloaded > 0)
        .await;

    // The origin has gone silent mid-body and stays that way.
    let asked = Instant::now();
    rig.dm.pause_download(id).await.unwrap();
    assert!(
        asked.elapsed() < Duration::from_secs(2),
        "pause waited {:?} on a silent origin",
        asked.elapsed()
    );

    let paused = rig.item(id);
    assert!(matches!(paused.status, DownloadStatus::Paused));
    assert!(paused.downloaded > 0 && paused.downloaded < size);
    assert!(Path::new(&file_io::temp_path(id)).exists());

    srv.stall.store(false, Ordering::Relaxed);
    rig.dm.resume_download(id).await.unwrap();
    let done = rig.wait_completed(id).await;

    assert_eq!(done.save_path, paused.save_path);
    assert_file(&done.save_path, size);
    let names = rig.names(id);
    assert!(names.contains(&FrontendEvent::DownloadPaused.name()));
    assert!(names.contains(&FrontendEvent::DownloadResumed.name()));
    assert!(
        !names.contains(&FrontendEvent::DownloadError.name()),
        "the paused worker's exit reached the frontend as an error: {names:?}"
    );
}

#[tokio::test]
async fn a_download_added_paused_waits_for_resume() {
    let rig = Rig::new(0);
    let size = 512 * 1024;
    let (url, _srv) = spawn_server(size, BodyMode::Exact, false).await;

    let id = rig.start(&url, "later.bin", true).await.unwrap();
    assert!(matches!(rig.item(id).status, DownloadStatus::Paused));
    assert!(rig.position(id, FrontendEvent::DownloadStarted).is_none());

    rig.dm.resume_download(id).await.unwrap();
    let done = rig.wait_completed(id).await;
    assert_file(&done.save_path, size);
}

#[tokio::test]
async fn over_the_limit_a_download_queues_and_starts_when_the_slot_frees() {
    let rig = Rig::new(1);
    let size = 256 * 1024;
    let (slow_url, slow) = spawn_server(size, BodyMode::Exact, true).await;
    let (fast_url, _fast) = spawn_server(size, BodyMode::Exact, false).await;

    let first = rig.start(&slow_url, "first.bin", false).await.unwrap();
    let second = rig.start(&fast_url, "second.bin", false).await.unwrap();
    assert!(matches!(rig.item(second).status, DownloadStatus::Queued));

    slow.stall.store(false, Ordering::Relaxed);
    rig.wait_completed(first).await;
    let done = rig.wait_completed(second).await;
    assert_file(&done.save_path, size);

    let first_completed = rig.position(first, FrontendEvent::DownloadCompleted);
    let second_started = rig.position(second, FrontendEvent::DownloadStarted);
    assert!(first_completed.is_some() && first_completed < second_started);
}

#[tokio::test]
async fn pausing_a_queued_download_takes_it_out_of_the_line() {
    let rig = Rig::new(1);
    let size = 256 * 1024;
    let (slow_url, slow) = spawn_server(size, BodyMode::Exact, true).await;
    let (fast_url, _fast) = spawn_server(size, BodyMode::Exact, false).await;

    let first = rig.start(&slow_url, "first.bin", false).await.unwrap();
    let second = rig.start(&fast_url, "second.bin", false).await.unwrap();
    assert!(matches!(rig.item(second).status, DownloadStatus::Queued));

    rig.dm.pause_download(second).await.unwrap();
    assert!(matches!(rig.item(second).status, DownloadStatus::Paused));

    // The slot frees, and the paused download must not take it.
    slow.stall.store(false, Ordering::Relaxed);
    rig.wait_completed(first).await;
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert!(matches!(rig.item(second).status, DownloadStatus::Paused));
    assert!(rig
        .position(second, FrontendEvent::DownloadStarted)
        .is_none());
}

#[tokio::test]
async fn the_same_source_cannot_be_started_twice_while_active() {
    let rig = Rig::new(0);
    let (url, _srv) = spawn_server(256 * 1024, BodyMode::Exact, true).await;

    let first = rig.start(&url, "once.bin", false).await.unwrap();
    let again = rig.start(&url, "twice.bin", false).await;
    assert_eq!(again, Err(PdmError::DuplicateDownload(first)));

    rig.dm.delete_download(first, true).await.unwrap();
}

#[tokio::test]
async fn an_unreachable_source_retries_then_fails_in_the_ledger_before_the_frontend_hears() {
    let rig = Rig::new(0);
    let url = "http://127.0.0.1:1/gone.bin";

    let id = rig.start(url, "gone.bin", false).await.unwrap();
    rig.wait_for(id, "Failed", |item| {
        matches!(item.status, DownloadStatus::Failed(_))
    })
    .await;

    let at = rig
        .position(id, FrontendEvent::DownloadError)
        .expect("the failure was never announced");
    let seen = rig.seen.lock().unwrap();
    let retrying = seen.iter().position(|event| {
        event_id(&event.payload) == Some(id) && event.payload["status"] == "retrying"
    });
    assert!(
        retrying.is_some_and(|retrying| retrying < at),
        "the frontend never heard about the retry"
    );
    assert!(matches!(seen[at].status, Some(DownloadStatus::Failed(_))));
    assert_eq!(seen[at].payload["url"], url);
    assert!(!seen[at].payload["message"].as_str().unwrap().is_empty());
}

#[tokio::test]
async fn delete_stops_the_worker_and_leaves_nothing_behind() {
    let rig = Rig::new(0);
    let (url, _srv) = spawn_server(256 * 1024, BodyMode::Exact, true).await;

    let id = rig.start(&url, "doomed.bin", false).await.unwrap();
    let running = rig
        .wait_for(id, "its first bytes", |item| item.downloaded > 0)
        .await;

    // The origin is silent mid-body; delete must not wait for it.
    let asked = Instant::now();
    rig.dm.delete_download(id, true).await.unwrap();
    assert!(
        asked.elapsed() < Duration::from_secs(2),
        "delete waited {:?} on a silent origin",
        asked.elapsed()
    );

    assert!(rig.ledger.get_item(id).unwrap().is_none());
    assert!(!Path::new(&file_io::temp_path(id)).exists());
    assert!(!Path::new(&running.save_path).exists());
    assert!(rig.position(id, FrontendEvent::DownloadError).is_none());
}

#[tokio::test]
async fn resuming_a_fully_downloaded_file_only_finalizes_it() {
    let rig = Rig::new(0);
    let size = 64 * 1024u64;
    let id = rig.dm.worker_pool.next_id();
    let save_path = rig.dir.join("whole.bin").to_string_lossy().into_owned();

    // Every byte is in the temp file; the pause landed before the rename.
    let temp = file_io::temp_path(id);
    std::fs::create_dir_all(Path::new(&temp).parent().unwrap()).unwrap();
    let body: Vec<u8> = (0..size).map(|at| at as u8).collect();
    std::fs::write(&temp, body).unwrap();
    rig.ledger
        .insert_item(&DownloadItem {
            id,
            // Unreachable: a worker started for this row could only fail.
            url: "http://127.0.0.1:1/whole.bin".to_string(),
            file_name: "whole.bin".to_string(),
            save_path: save_path.clone(),
            total_size: size,
            downloaded: size,
            status: DownloadStatus::Paused,
            resumable: Some(true),
            created_at: now_str(),
            ..Default::default()
        })
        .unwrap();

    rig.dm.resume_download(id).await.unwrap();

    assert!(matches!(rig.item(id).status, DownloadStatus::Completed));
    assert_file(&save_path, size);
    assert!(!Path::new(&temp).exists());
    assert!(rig.position(id, FrontendEvent::DownloadStarted).is_none());
    assert!(rig.position(id, FrontendEvent::DownloadCompleted).is_some());
}
