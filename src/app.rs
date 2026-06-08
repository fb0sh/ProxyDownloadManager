// =============================================================================
// app.rs — ProxyDownloadManager struct, state, and management methods
// =============================================================================

use crate::download::start_multi_part_download;
use crate::icons::IconCache;
use crate::log_info;
use crate::persist::{load_downloads, load_toml, save_toml};
use crate::types::*;

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

// ─── Main Application ─────────────────────────────────────────────────────────

pub struct ProxyDownloadManager {
    pub downloads: Vec<DownloadItem>,
    pub shared: Arc<Mutex<Vec<DownloadItem>>>,
    pub filter: TreeFilter,
    pub selected_ids: HashSet<u64>,
    pub settings: AppSettings,
    pub active_downloads: HashMap<u64, ActiveDownload>,
    pub speed_trackers: HashMap<u64, SpeedTracker>,
    pub icon_cache: Option<IconCache>,

    // UI dialogs
    pub show_new_dialog: bool,
    pub show_settings: bool,
    pub show_about: bool,
    pub new_url: String,
    pub new_filename: String,
    pub new_proxy_name: String,
    pub new_connections: u32,
    pub clipboard_checked: bool,
    pub prev_url_for_name: String,
    // Proxy editor
    pub show_proxy_editor: bool,
    pub edit_proxy: Option<ProxyEntry>,
    pub edit_proxy_index: Option<usize>,
    pub edit_item_id: Option<u64>,
    pub edit_url: String,
    pub edit_filename: String,
    pub edit_proxy_name: String,
    pub edit_connections: u32,
    pub cached_cache_size: Option<u64>,
    pub show_properties: Option<u64>,
    pub show_progress: bool,
    pub closed_detail_windows: HashSet<u64>,
    pub pending_delete_ids: Vec<u64>,
    pub manual_detail_ids: HashSet<u64>,
    pub detail_actions: Arc<Mutex<Vec<(u64, &'static str)>>>,

    pub next_id: u64,
    pub last_clipboard_text: String,
    pub clipboard_poll_counter: u32,
    pub status_message: Option<String>,
    pub status_message_timer: f32,
    pub save_counter: u32,
    pub ws_focus: Arc<AtomicBool>,
    pub ws_url: Arc<Mutex<String>>,
}

impl ProxyDownloadManager {
    pub fn new() -> Self {
        let ws_focus = Arc::new(AtomicBool::new(false));
        let ws_url = Arc::new(Mutex::new(String::new()));
        let shared = Arc::new(Mutex::new(Vec::new()));
        Self::new_with_state(shared, ws_focus, ws_url)
    }

    /// Create app with externally-managed shared state (for WebSocket integration).
    pub fn new_with_state(
        shared: Arc<Mutex<Vec<DownloadItem>>>,
        ws_focus: Arc<AtomicBool>,
        ws_url: Arc<Mutex<String>>,
    ) -> Self {
        let _ = fs::create_dir_all(pdm_dir());
        let set_path = settings_path();
        let mut settings: AppSettings = if set_path.exists() {
            load_toml(&set_path.to_string_lossy().to_string()).unwrap_or_default()
        } else {
            let s = AppSettings::default();
            save_toml(&set_path.to_string_lossy().to_string(), &s);
            s
        };
        settings.launch_at_startup = crate::startup::is_enabled();

        // Load persisted downloads into the shared state
        let dl_path = downloads_path().to_string_lossy().to_string();
        let downloads = load_downloads(&dl_path);
        let next_id = downloads.iter().map(|d| d.id).max().unwrap_or(0) + 1;

        let downloads: Vec<DownloadItem> = downloads
            .into_iter()
            .map(|mut d| {
                if d.status == DownloadStatus::Downloading || d.status == DownloadStatus::Queued {
                    d.status = DownloadStatus::Paused;
                }
                d
            })
            .collect();

        {
            let mut shared_lock = shared.lock().unwrap();
            *shared_lock = downloads.clone();
        }

        Self::new_inner(downloads, shared, settings, next_id, ws_focus, ws_url)
    }

    fn new_inner(
        downloads: Vec<DownloadItem>,
        shared: Arc<Mutex<Vec<DownloadItem>>>,
        settings: AppSettings,
        next_id: u64,
        ws_focus: Arc<AtomicBool>,
        ws_url: Arc<Mutex<String>>,
    ) -> Self {

        Self {
            downloads,
            shared,
            filter: TreeFilter::All,
            selected_ids: HashSet::new(),
            settings,
            active_downloads: HashMap::new(),
            speed_trackers: HashMap::new(),
            icon_cache: None,
            show_new_dialog: false,
            show_settings: false,
            show_about: false,
            new_url: String::new(),
            new_filename: String::new(),
            new_proxy_name: String::new(),
            new_connections: 0,
            clipboard_checked: false,
            prev_url_for_name: String::new(),
            show_proxy_editor: false,
            edit_proxy: None,
            edit_proxy_index: None,
            edit_item_id: None,
            edit_url: String::new(),
            edit_filename: String::new(),
            edit_proxy_name: String::new(),
            edit_connections: 0,
            cached_cache_size: None,
            show_properties: None,
            show_progress: false,
            closed_detail_windows: HashSet::new(),
            pending_delete_ids: Vec::new(),
            manual_detail_ids: HashSet::new(),
            detail_actions: Arc::new(Mutex::new(Vec::new())),
            next_id,
            last_clipboard_text: String::new(),
            clipboard_poll_counter: 0,
            status_message: None,
            status_message_timer: 0.0,
            save_counter: 0,
            ws_focus,
            ws_url,
        }
    }

    /// Shared state for WebSocket server and other threads.
    pub fn shared_state() -> Arc<Mutex<Vec<DownloadItem>>> {
        Arc::new(Mutex::new(Vec::new()))
    }

    /// Load settings for non-GUI contexts (e.g. WebSocket server).
    pub fn load_settings() -> AppSettings {
        let _ = fs::create_dir_all(pdm_dir());
        let set_path = settings_path();
        if set_path.exists() {
            load_toml(&set_path.to_string_lossy().to_string()).unwrap_or_default()
        } else {
            let s = AppSettings::default();
            save_toml(&set_path.to_string_lossy().to_string(), &s);
            s
        }
    }
}

impl Default for ProxyDownloadManager {
    fn default() -> Self {
        Self::new()
    }
}

impl ProxyDownloadManager {
    /// Extract a sensible file name from a download URL
    pub fn file_name_from_url(url: &str) -> String {
        let url_path = url.split('?').next().unwrap_or(url);
        let name = url_path
            .rsplit('/')
            .next()
            .filter(|s| !s.is_empty())
            .unwrap_or("download");
        let decoded = url_decode(name);
        if decoded.is_empty() {
            "download".to_string()
        } else {
            decoded
        }
    }

    /// Get downloads filtered by current tree filter
    pub fn filtered_downloads(&self) -> Vec<&DownloadItem> {
        match self.filter {
            TreeFilter::All => self.downloads.iter().collect(),
            TreeFilter::Completed => {
                self.downloads
                    .iter()
                    .filter(|d| d.status == DownloadStatus::Completed)
                    .collect()
            }
            TreeFilter::Incompleted => {
                self.downloads
                    .iter()
                    .filter(|d| d.status != DownloadStatus::Completed)
                    .collect()
            }
        }
    }

    /// Get currently selected items
    pub fn selected_items(&self) -> Vec<&DownloadItem> {
        self.downloads.iter().filter(|d| self.selected_ids.contains(&d.id)).collect()
    }

    /// Set a temporary status message (shown in sidebar for 3 seconds)
    pub fn set_status(&mut self, msg: String) {
        self.status_message = Some(msg);
        self.status_message_timer = 3.0;
    }

    // ── Download lifecycle ──────────────────────────────────────────────────

    /// Start (or resume) a download by spawning part threads
    pub fn start_download(&mut self, id: u64) {
        // Keep detail window open after download completes
        self.manual_detail_ids.insert(id);

        let item = match self.shared.lock().unwrap().iter().find(|d| d.id == id) {
            Some(i) => i.clone(),
            None => return,
        };

        let proxy_display = if item.proxy_name.is_empty() { "none" } else { &item.proxy_name };
        let thread_count = if item.connections > 0 { item.connections } else { self.settings.max_connections };
        log_info!("Item#{} START file=\"{}\" url={} proxy={} threads={}",
            id, item.file_name, item.url, proxy_display, thread_count);

        let use_connections = if item.connections > 0 {
            item.connections
        } else {
            self.settings.max_connections
        };
        let num_connections: usize = use_connections as usize;
        let mut cancels: Vec<Arc<AtomicBool>> = Vec::with_capacity(num_connections);
        for _ in 0..num_connections {
            cancels.push(Arc::new(AtomicBool::new(false)));
        }
        let completed_counter = Arc::new(AtomicU32::new(0));
        let shared = self.shared.clone();
        let settings = self.settings.clone();
        let url = item.url.clone();
        let save_path = item.save_path.clone();

        {
            let mut items = shared.lock().unwrap();
            if let Some(item) = items.iter_mut().find(|d| d.id == id) {
                item.status = DownloadStatus::Downloading;
                item.last_try = now_str();
            }
        }

        start_multi_part_download(
            id,
            url,
            save_path,
            settings,
            cancels.clone(),
            completed_counter.clone(),
            shared,
        );

        self.active_downloads.insert(
            id,
            ActiveDownload {
                cancels,
                completed_parts: completed_counter,
            },
        );
    }

    /// Pause a download (sets cancel flags, saves progress, marks as Paused)
    pub fn pause_download(&mut self, id: u64) {
        if let Some(active) = self.active_downloads.remove(&id) {
            for c in &active.cancels {
                c.store(true, Ordering::Relaxed);
            }
        }
        let mut items = self.shared.lock().unwrap();
        if let Some(item) = items.iter_mut().find(|d| d.id == id) {
            item.status = DownloadStatus::Paused;
            item.downloaded = item.parts.iter().map(|p| p.downloaded).sum();
        }
        log_info!("Item#{} PAUSED", id);
    }

    /// Resume a paused/failed download
    pub fn resume_download(&mut self, id: u64) {
        // Keep detail window open after download completes
        self.manual_detail_ids.insert(id);

        if self.active_downloads.contains_key(&id) {
            log_info!("Item#{} RESUME skipped — already active", id);
            return;
        }
        log_info!("Item#{} RESUME", id);

        let item = match self.shared.lock().unwrap().iter().find(|d| d.id == id) {
            Some(i) => i.clone(),
            None => return,
        };

        let num_parts = if item.parts.is_empty() {
            4usize
        } else {
            item.parts.len().max(1)
        };

        let mut cancels: Vec<Arc<AtomicBool>> = Vec::with_capacity(num_parts);
        for _ in 0..num_parts {
            cancels.push(Arc::new(AtomicBool::new(false)));
        }
        let completed_counter = Arc::new(AtomicU32::new(0));

        // Count already-completed parts
        if !item.parts.is_empty() {
            let done = item.parts.iter().filter(|p| p.status == PartStatus::Completed).count() as u32;
            completed_counter.store(done, Ordering::Relaxed);
        }

        let shared = self.shared.clone();
        let settings = self.settings.clone();
        let url = item.url.clone();
        let save_path = item.save_path.clone();

        {
            let mut items = shared.lock().unwrap();
            if let Some(item) = items.iter_mut().find(|d| d.id == id) {
                // Reset non-completed parts to Pending
                for p in &mut item.parts {
                    if p.status != PartStatus::Completed {
                        p.status = PartStatus::Pending;
                    }
                }
                item.status = DownloadStatus::Downloading;
                item.last_try = now_str();
            }
        }

        start_multi_part_download(
            id,
            url,
            save_path,
            settings,
            cancels.clone(),
            completed_counter.clone(),
            shared,
        );

        self.active_downloads.insert(
            id,
            ActiveDownload {
                cancels,
                completed_parts: completed_counter,
            },
        );
    }

    /// Stop a download (alias for pause)
    pub fn stop_download(&mut self, id: u64) {
        if let Some(active) = self.active_downloads.remove(&id) {
            for c in &active.cancels {
                c.store(true, Ordering::Relaxed);
            }
        }
        let mut items = self.shared.lock().unwrap();
        if let Some(item) = items.iter_mut().find(|d| d.id == id) {
            item.status = DownloadStatus::Paused;
            item.downloaded = item.parts.iter().map(|p| p.downloaded).sum();
        }
        log_info!("Item#{} STOPPED (paused)", id);
    }

    /// Delete a download (cancel threads, remove files, clean up state)
    pub fn delete_download(&mut self, id: u64) {
        if let Some(active) = self.active_downloads.remove(&id) {
            for c in &active.cancels {
                c.store(true, Ordering::Relaxed);
            }
        }

        let mut items = self.shared.lock().unwrap();
        if let Some(item) = items.iter().find(|d| d.id == id) {
            let _ = fs::remove_file(&item.save_path);
            for part in &item.parts {
                let _ = fs::remove_file(&part.temp_path);
            }
        }
        items.retain(|d| d.id != id);

        self.selected_ids.remove(&id);
        log_info!("Item#{} DELETED", id);
    }

    /// Add a new download from a URL and start it immediately
    pub fn add_new_download(&mut self, url: &str, custom_name: Option<&str>) {
        let file_name = custom_name
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .unwrap_or_else(|| Self::file_name_from_url(url));

        let save_dir = PathBuf::from(&self.settings.download_dir);
        let save_path = save_dir.join(&file_name);

        let proxy_name = std::mem::take(&mut self.new_proxy_name);
        let use_connections = std::mem::replace(&mut self.new_connections, 0);

        let item_id = self.next_id;
        self.next_id += 1;

        {
            let mut items = self.shared.lock().unwrap();
            items.push(DownloadItem {
                id: item_id,
                url: url.to_string(),
                file_name: file_name.clone(),
                save_path: save_path.to_string_lossy().to_string(),
                total_size: 0,
                downloaded: 0,
                status: DownloadStatus::Queued,
                parts: Vec::new(),
                connections: use_connections,
                proxy_name,
                resumable: None,
                merge_progress: 0.0,
                last_try: String::new(),
                created_at: now_str(),
            });
        }

        self.start_download(item_id);

        self.set_status(format!("Added download: {}", file_name));
    }
}
