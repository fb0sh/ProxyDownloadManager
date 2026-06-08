// =============================================================================
// types.rs — Data types, enums, constants, and pure helper functions
// =============================================================================

use serde::{Deserialize, Serialize};
use std::time::Instant;

// ─── Constants ────────────────────────────────────────────────────────────────

pub const APP_NAME: &str = "ProxyDM";

/// $HOME/Downloads/.pdm/
pub fn pdm_dir() -> std::path::PathBuf {
    dirs::download_dir()
        .unwrap_or_else(|| {
            dirs::home_dir()
                .map(|d| d.join("Downloads"))
                .unwrap_or_else(|| std::env::current_dir().unwrap_or_default().join("Downloads"))
        })
        .join(".pdm")
}

pub fn settings_path() -> std::path::PathBuf {
    pdm_dir().join("pdm.toml")
}

pub fn downloads_path() -> std::path::PathBuf {
    pdm_dir().join("downloads.db")
}

pub fn default_download_dir() -> String {
    dirs::download_dir()
        .or_else(|| dirs::home_dir().map(|d| d.join("Downloads")))
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_default().join("Downloads"))
        .to_string_lossy()
        .to_string()
}

// ─── Enums ────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum DownloadStatus {
    #[serde(rename = "downloading")]
    Downloading,
    #[serde(rename = "paused")]
    Paused,
    #[serde(rename = "completed")]
    Completed,
    #[serde(rename = "failed")]
    Failed(String),
    #[serde(rename = "queued")]
    Queued,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum PartStatus {
    #[serde(rename = "pending")]
    Pending,
    #[serde(rename = "downloading")]
    Downloading,
    #[serde(rename = "completed")]
    Completed,
    #[serde(rename = "failed")]
    Failed(String),
}

#[derive(Debug, Clone, PartialEq)]
pub enum TreeFilter {
    All,
    Completed,
    Incompleted,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ProxyProtocol {
    #[serde(rename = "http")]
    Http,
    #[serde(rename = "socks5")]
    Socks5,
}

impl ProxyProtocol {
    pub fn scheme(&self) -> &'static str {
        match self {
            ProxyProtocol::Http => "http",
            ProxyProtocol::Socks5 => "socks5",
        }
    }
}

// ─── Structs ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadPart {
    pub index: u32,
    pub start: u64,
    pub end: u64,
    pub downloaded: u64,
    pub temp_path: String,
    pub status: PartStatus,
    #[serde(default)]
    pub retries: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadItem {
    pub id: u64,
    pub url: String,
    pub file_name: String,
    pub save_path: String,
    pub total_size: u64,
    pub downloaded: u64,
    pub status: DownloadStatus,
    pub last_try: String,
    pub created_at: String,
    #[serde(default = "default_parts")]
    pub parts: Vec<DownloadPart>,
    #[serde(default = "default_connections")]
    pub connections: u32,
    #[serde(default)]
    pub proxy_name: String,
    #[serde(default)]
    pub resumable: Option<bool>,
    #[serde(default)]
    pub merge_progress: f32, // 0.0 = not merging, 0.01-1.0 = merging %
}

fn default_parts() -> Vec<DownloadPart> { Vec::new() }
fn default_connections() -> u32 { 4 }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProxyEntry {
    pub name: String,
    pub protocol: ProxyProtocol,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub password: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppSettings {
    pub download_dir: String,
    #[serde(default = "default_proxies")]
    pub proxies: Vec<ProxyEntry>,
    #[serde(default)]
    pub default_proxy: String,
    #[serde(default = "default_connections")]
    pub max_connections: u32,
    #[serde(default = "default_retries")]
    pub max_retries: u32,
    #[serde(default = "default_user_agent")]
    pub user_agent: String,
}

fn default_retries() -> u32 { 10 }
fn default_user_agent() -> String { "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/148.0.0.0 Safari/537.36 Edg/148.0.0.0".to_string() }

fn default_proxies() -> Vec<ProxyEntry> { Vec::new() }

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            download_dir: default_download_dir(),
            proxies: Vec::new(),
            default_proxy: String::new(),
            max_connections: 8,
            max_retries: 10,
            user_agent: "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/148.0.0.0 Safari/537.36 Edg/148.0.0.0".to_string(),
        }
    }
}

// ─── Speed Tracker (EWMA) ─────────────────────────────────────────────────────

pub struct SpeedSample {
    pub time: Instant,
    pub bytes: u64,
}

pub struct SpeedTracker {
    samples: Vec<SpeedSample>,
    smooth_speed: f64,  // EWMA bytes/sec
    alpha: f64,          // smoothing factor (0.0-1.0)
}

impl SpeedTracker {
    pub fn new() -> Self {
        Self { samples: Vec::new(), smooth_speed: 0.0, alpha: 0.15 }
    }

    pub fn update(&mut self, bytes: u64) -> f64 {
        let now = Instant::now();
        self.samples.push(SpeedSample { time: now, bytes });

        // Keep samples from last ~3 seconds
        let cutoff = std::time::Duration::from_secs(3);
        while self.samples.len() > 2 && (now - self.samples[0].time) > cutoff {
            self.samples.remove(0);
        }

        // Compute instant speed from oldest to newest sample
        if self.samples.len() >= 2 {
            let first = &self.samples[0];
            let last = &self.samples[self.samples.len() - 1];
            let elapsed = (last.time - first.time).as_secs_f64();
            if elapsed > 0.2 {
                let instant_speed = (last.bytes.saturating_sub(first.bytes)) as f64 / elapsed;
                // EWMA: smooth_speed = alpha * instant + (1-alpha) * smooth_speed
                if self.smooth_speed <= 0.0 {
                    self.smooth_speed = instant_speed;
                } else {
                    self.smooth_speed = self.alpha * instant_speed + (1.0 - self.alpha) * self.smooth_speed;
                }
            }
        }
        self.smooth_speed
    }

    pub fn speed(&self) -> f64 { self.smooth_speed }

    pub fn eta(&self, remaining: u64) -> String {
        let s = self.smooth_speed;
        if s <= 0.0 { return "-".to_string(); }
        let secs = remaining as f64 / s;
        if secs.is_infinite() || secs.is_nan() { return "-".to_string(); }
        if secs < 60.0 { format!("{:.0}s", secs) }
        else if secs < 3600.0 { format!("{:.0}m {:.0}s", secs / 60.0, secs % 60.0) }
        else { format!("{:.0}h {:.0}m", secs / 3600.0, (secs % 3600.0) / 60.0) }
    }
}

// ─── Thread coordination ──────────────────────────────────────────────────────

pub struct ActiveDownload {
    pub cancels: Vec<std::sync::Arc<std::sync::atomic::AtomicBool>>,
    pub completed_parts: std::sync::Arc<std::sync::atomic::AtomicU32>,
}

// ─── Pure helper functions ────────────────────────────────────────────────────

pub fn now_str() -> String {
    chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string()
}

/// Simple percent-decoding for URL components
pub fn url_decode(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut bytes = s.bytes();
    while let Some(b) = bytes.next() {
        if b == b'%' {
            let hi = bytes.next().and_then(hex_val);
            let lo = bytes.next().and_then(hex_val);
            match (hi, lo) {
                (Some(h), Some(l)) => result.push((h << 4 | l) as char),
                _ => result.push('%'),
            }
        } else if b == b'+' {
            result.push(' ');
        } else {
            result.push(b as char);
        }
    }
    result
}

fn hex_val(c: u8) -> Option<u8> {
    match c {
        b'0'..=b'9' => Some(c - b'0'),
        b'a'..=b'f' => Some(c - b'a' + 10),
        b'A'..=b'F' => Some(c - b'A' + 10),
        _ => None,
    }
}

/// Format bytes to human-readable string (B/KB/MB/GB)
pub fn format_size(bytes: u64) -> String {
    const UNITS: &[&str] = &["B", "KB", "MB", "GB", "TB"];
    let mut size = bytes as f64;
    let mut unit_idx = 0;
    while size >= 1024.0 && unit_idx < UNITS.len() - 1 {
        size /= 1024.0;
        unit_idx += 1;
    }
    if unit_idx == 0 {
        format!("{} B", bytes)
    } else {
        format!("{:.2} {}", size, UNITS[unit_idx])
    }
}

/// Format speed (bytes/sec) to human-readable string
pub fn format_speed(bytes_per_sec: f64) -> String {
    if bytes_per_sec <= 0.0 { return "-".to_string(); }
    if bytes_per_sec >= 1_048_576.0 { format!("{:.1} MB/s", bytes_per_sec / 1_048_576.0) }
    else if bytes_per_sec >= 1024.0 { format!("{:.1} KB/s", bytes_per_sec / 1024.0) }
    else { format!("{:.0} B/s", bytes_per_sec) }
}

/// Status display icon + color for the UI
pub fn status_icon_and_text(status: &DownloadStatus) -> (&'static str, eframe::egui::Color32) {
    use eframe::egui::Color32;
    match status {
        DownloadStatus::Downloading => ("⬇ Downloading...", Color32::from_rgb(0, 120, 215)),
        DownloadStatus::Paused => ("⏸ Paused", Color32::from_rgb(255, 170, 0)),
        DownloadStatus::Completed => ("✅ Completed", Color32::from_rgb(0, 180, 0)),
        DownloadStatus::Failed(_) => ("❌ Failed", Color32::RED),
        DownloadStatus::Queued => ("⏳ Queued", Color32::GRAY),
    }
}
