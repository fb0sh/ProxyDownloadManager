// =============================================================================
// ProxyDM — A download manager built with egui (Rust)
// Features: multi-threaded downloads, pause/resume, proxy support,
//           persistent state, tree-filtered table view, system file icons.
// =============================================================================

use eframe::egui::{self, Align, Color32, CornerRadius, Frame, Layout, Margin, Vec2, RichText, ScrollArea};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

// ─── Constants ────────────────────────────────────────────────────────────────

const APP_NAME: &str = "ProxyDM";
const SETTINGS_FILE: &str = "proxydm_settings.json";
const DOWNLOADS_FILE: &str = "proxydm_downloads.json";

// ─── Data Types ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
enum DownloadStatus {
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

#[derive(Debug, Clone, Serialize, Deserialize)]
struct DownloadPart {
    index: u32,
    start: u64,
    end: u64,
    downloaded: u64,
    temp_path: String,
    status: PartStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
enum PartStatus {
    #[serde(rename = "pending")]
    Pending,
    #[serde(rename = "downloading")]
    Downloading,
    #[serde(rename = "completed")]
    Completed,
    #[serde(rename = "failed")]
    Failed(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct DownloadItem {
    id: u64,
    url: String,
    file_name: String,
    save_path: String,
    total_size: u64,
    downloaded: u64,
    status: DownloadStatus,
    last_try: String,
    created_at: String,
    #[serde(default = "default_parts")]
    parts: Vec<DownloadPart>,
    #[serde(default = "default_connections")]
    connections: u32,
    #[serde(default)]
    proxy_name: String,
}

fn default_parts() -> Vec<DownloadPart> { Vec::new() }
fn default_connections() -> u32 { 4 }

#[derive(Debug, Clone, PartialEq)]
enum TreeFilter {
    All,
    Completed,
    Incompleted,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
enum ProxyProtocol {
    #[serde(rename = "http")]
    Http,
    #[serde(rename = "socks5")]
    Socks5,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ProxyEntry {
    name: String,
    protocol: ProxyProtocol,
    host: String,
    port: u16,
    username: String,
    password: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AppSettings {
    download_dir: String,
    #[serde(default = "default_proxies")]
    proxies: Vec<ProxyEntry>,
    #[serde(default)]
    default_proxy: String,
}

fn default_proxies() -> Vec<ProxyEntry> { Vec::new() }

impl Default for AppSettings {
    fn default() -> Self {
        let default_dir = std::env::current_dir()
            .unwrap_or_default()
            .join("Downloads")
            .to_string_lossy()
            .to_string();
        Self {
            download_dir: default_dir,
            proxies: Vec::new(),
            default_proxy: String::new(),
        }
    }
}

struct ActiveDownload {
    cancels: Vec<Arc<AtomicBool>>,
    completed_parts: Arc<AtomicU32>,
}

// ─── System File Icon Cache ───────────────────────────────────────────────────

#[cfg(target_os = "macos")]
mod system_icon {
    use eframe::egui::{self, ColorImage, TextureHandle};
    use std::collections::HashMap;

    pub struct IconCache {
        cache: HashMap<String, TextureHandle>,
        temp_dir: std::path::PathBuf,
    }

    impl IconCache {
        pub fn new(_ctx: &egui::Context) -> Self {
            let temp_dir = std::env::temp_dir().join("proxydm_icons");
            let _ = std::fs::create_dir_all(&temp_dir);
            Self {
                cache: HashMap::new(),
                temp_dir,
            }
        }

        pub fn get_icon(&mut self, file_name: &str, ctx: &egui::Context) -> TextureHandle {
            let ext = file_name
                .rsplit('.')
                .next()
                .unwrap_or("")
                .to_lowercase();
            let ext_key = if ext.is_empty() {
                "generic".to_string()
            } else {
                ext
            };

            if !self.cache.contains_key(&ext_key) {
                let temp_file = self.temp_dir.join(format!("icon.{}", ext_key));
                if !temp_file.exists() {
                    let _ = std::fs::write(&temp_file, b"");
                }

                let icon = file_icon_provider::get_file_icon(&temp_file, 32);
                if let Ok(fp_icon) = icon {
                    let color_image = ColorImage::from_rgba_unmultiplied(
                        [fp_icon.width as usize, fp_icon.height as usize],
                        &fp_icon.pixels,
                    );
                    let texture = ctx.load_texture(
                        format!("file_icon_{}", ext_key),
                        color_image,
                        egui::TextureOptions::NEAREST,
                    );
                    self.cache.insert(ext_key.clone(), texture);
                } else {
                    let fallback = create_fallback_texture(ctx, &ext_key);
                    self.cache.insert(ext_key.clone(), fallback);
                }

                let _ = std::fs::remove_file(&temp_file);
            }

            self.cache
                .get(&ext_key)
                .cloned()
                .unwrap_or_else(|| create_fallback_texture(ctx, &ext_key))
        }
    }

    fn create_fallback_texture(ctx: &egui::Context, _label: &str) -> TextureHandle {
        let size = 32;
        let mut pixels = vec![200u8; size as usize * size as usize * 4];
        for y in 0..size {
            for x in 0..size {
                let idx = (y * size + x) as usize * 4;
                let is_border = x < 2 || y < 2 || x >= size - 2 || y >= size - 2;
                let is_corner = x >= size - 8 && y < 8 && (x - (size - 8)) < (8 - y) + 3;
                if is_border || is_corner {
                    pixels[idx] = 180;
                    pixels[idx + 1] = 180;
                    pixels[idx + 2] = 180;
                    pixels[idx + 3] = 255;
                } else {
                    pixels[idx + 3] = 0;
                }
            }
        }
        let color_image = ColorImage::from_rgba_unmultiplied([size as usize, size as usize], &pixels);
        ctx.load_texture(format!("icon_fallback_{}", _label), color_image, egui::TextureOptions::NEAREST)
    }
}

#[cfg(not(target_os = "macos"))]
mod system_icon {
    use eframe::egui::{self, ColorImage, TextureHandle};
    use std::collections::HashMap;

    pub struct IconCache {
        cache: HashMap<String, TextureHandle>,
    }

    impl IconCache {
        pub fn new(_ctx: &egui::Context) -> Self {
            Self {
                cache: HashMap::new(),
            }
        }

        pub fn get_icon(&mut self, file_name: &str, ctx: &egui::Context) -> TextureHandle {
            let ext = file_name
                .rsplit('.')
                .next()
                .unwrap_or("")
                .to_lowercase();
            let key = if ext.is_empty() {
                "generic".to_string()
            } else {
                ext
            };

            if !self.cache.contains_key(&key) {
                let fallback = create_fallback_texture(ctx, &key);
                self.cache.insert(key.clone(), fallback);
            }

            self.cache
                .get(&key)
                .cloned()
                .unwrap_or_else(|| create_fallback_texture(ctx, &key))
        }
    }

    fn create_fallback_texture(ctx: &egui::Context, _label: &str) -> TextureHandle {
        let size = 32;
        let mut pixels = vec![200u8; size as usize * size as usize * 4];
        for y in 0..size {
            for x in 0..size {
                let idx = (y * size + x) as usize * 4;
                let is_border = x < 2 || y < 2 || x >= size - 2 || y >= size - 2;
                let folded = x >= size - 8 && y < 8 && (x - (size - 8)) < (8 - y) + 3;
                if is_border || folded {
                    pixels[idx] = 180;
                    pixels[idx + 1] = 180;
                    pixels[idx + 2] = 180;
                    pixels[idx + 3] = 255;
                } else {
                    pixels[idx + 3] = 0;
                }
            }
        }
        let color_image = ColorImage::from_rgba_unmultiplied([size as usize, size as usize], &pixels);
        ctx.load_texture("icon_fallback", color_image, egui::TextureOptions::NEAREST)
    }
}

use system_icon::IconCache;

// ─── Timestamp helper ─────────────────────────────────────────────────────────

fn now_str() -> String {
    chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string()
}

// ─── Simple URL decode ────────────────────────────────────────────────────────

fn url_decode(s: &str) -> String {
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

// ─── Size formatting ──────────────────────────────────────────────────────────

fn format_size(bytes: u64) -> String {
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

// ─── Status display ───────────────────────────────────────────────────────────

fn status_icon_and_text(status: &DownloadStatus) -> (&'static str, Color32) {
    match status {
        DownloadStatus::Downloading => ("⬇ Downloading...", Color32::from_rgb(0, 120, 215)),
        DownloadStatus::Paused => ("⏸ Paused", Color32::from_rgb(255, 170, 0)),
        DownloadStatus::Completed => ("✅ Completed", Color32::from_rgb(0, 180, 0)),
        DownloadStatus::Failed(_) => ("❌ Failed", Color32::RED),
        DownloadStatus::Queued => ("⏳ Queued", Color32::GRAY),
    }
}

// ─── Persistence ──────────────────────────────────────────────────────────────

fn load_json<T: serde::de::DeserializeOwned>(path: &str) -> Option<T> {
    fs::read_to_string(path).ok().and_then(|s| serde_json::from_str(&s).ok())
}

fn save_json<T: serde::Serialize + ?Sized>(path: &str, value: &T) {
    if let Ok(json) = serde_json::to_string_pretty(value) {
        let _ = fs::write(path, &json);
    }
}

// ─── Build reqwest client with optional proxy ──────────────────────────────────

fn build_client(proxy_entry: Option<&ProxyEntry>) -> anyhow::Result<reqwest::blocking::Client> {
    let mut builder = reqwest::blocking::Client::builder()
        .user_agent("ProxyDM/0.1")
        .timeout(std::time::Duration::from_secs(30))
        .redirect(reqwest::redirect::Policy::limited(10));

    if let Some(entry) = proxy_entry {
        let proxy_url = if entry.port == 0 {
            format!("{}://{}", entry.protocol.scheme(), entry.host)
        } else {
            format!("{}://{}:{}", entry.protocol.scheme(), entry.host, entry.port)
        };

        // reqwest accepts socks5:// and http:// schemes in Proxy::all
        if let Ok(p) = reqwest::Proxy::all(&proxy_url) {
            builder = builder.proxy(p);
        }
    }

    let client = builder.build()?;
    Ok(client)
}

impl ProxyProtocol {
    fn scheme(&self) -> &'static str {
        match self {
            ProxyProtocol::Http => "http",
            ProxyProtocol::Socks5 => "socks5",
        }
    }
}

// ─── Multi-Thread Download ────────────────────────────────────────────────────

/// Temporary file suffix for a download part
fn part_temp_path(save_path: &str, part_index: u32) -> String {
    format!("{}.part{}", save_path, part_index)
}

/// Merge completed part files into the final file
fn merge_parts(item: &DownloadItem) -> Result<(), String> {
    let output_path = Path::new(&item.save_path);
    if let Some(parent) = output_path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("mkdir: {}", e))?;
    }

    let mut output = fs::File::create(output_path)
        .map_err(|e| format!("create output: {}", e))?;

    for part in &item.parts {
        let part_path = Path::new(&part.temp_path);
        if part_path.exists() {
            let mut input = fs::File::open(part_path)
                .map_err(|e| format!("open part {}: {}", part.index, e))?;
            std::io::copy(&mut input, &mut output)
                .map_err(|e| format!("copy part {}: {}", part.index, e))?;
            drop(input);
            let _ = fs::remove_file(part_path);
        }
    }
    output.flush().map_err(|e| format!("flush: {}", e))?;
    Ok(())
}

/// Single part download thread — downloads Range: bytes=start-end
#[allow(clippy::too_many_arguments)]
fn spawn_part_thread(
    item_id: u64,
    url: String,
    part: DownloadPart,
    settings: AppSettings,
    proxy_entry: Option<ProxyEntry>,
    cancel: Arc<AtomicBool>,
    state: Arc<Mutex<Vec<DownloadItem>>>,
    completed_counter: Arc<AtomicU32>,
) {
    std::thread::spawn(move || {
        let client = match build_client(proxy_entry.as_ref()) {
            Ok(c) => c,
            Err(e) => {
                let mut items = state.lock().unwrap();
                if let Some(item) = items.iter_mut().find(|d| d.id == item_id) {
                    item.status = DownloadStatus::Failed(format!("Client: {}", e));
                    item.last_try = now_str();
                }
                return;
            }
        };

        if let Some(parent) = Path::new(&part.temp_path).parent() {
            let _ = fs::create_dir_all(parent);
        }

        // Calculate resume offset for this part
        let part_offset = part.downloaded;
        let part_remaining = if part.end > 0 {
            part.end.saturating_sub(part.start + part_offset)
        } else {
            u64::MAX
        };

        // If part is already fully downloaded, skip
        if part_offset > 0 && part_remaining == 0 && part.end > 0 {
            completed_counter.fetch_add(1, Ordering::Relaxed);
            return;
        }

        // Build Range header for this part
        let range_start = part.start + part_offset;
        let range_end = if part.end > 0 { part.end } else { 0 };

        let mut req = client.get(&url);
        if range_end > 0 && range_start <= range_end {
            req = req.header("Range", format!("bytes={}-{}", range_start, range_end));
        } else if range_start > 0 {
            req = req.header("Range", format!("bytes={}-", range_start));
        }

        let response = match req.timeout(std::time::Duration::from_secs(120)).send() {
            Ok(r) => r,
            Err(e) => {
                let mut items = state.lock().unwrap();
                if let Some(item) = items.iter_mut().find(|d| d.id == item_id) {
                    if let Some(p) = item.parts.iter_mut().find(|p| p.index == part.index) {
                        p.status = PartStatus::Failed(format!("Req: {}", e));
                    }
                    item.status = DownloadStatus::Failed(format!("Part {}: {}", part.index, e));
                    item.last_try = now_str();
                }
                return;
            }
        };

        let status = response.status();
        if !status.is_success() && status != reqwest::StatusCode::PARTIAL_CONTENT {
            let mut items = state.lock().unwrap();
            if let Some(item) = items.iter_mut().find(|d| d.id == item_id) {
                if let Some(p) = item.parts.iter_mut().find(|p| p.index == part.index) {
                    p.status = PartStatus::Failed(format!("HTTP {}", status));
                }
                item.last_try = now_str();
            }
            return;
        }

        // Update part status to Downloading
        {
            let mut items = state.lock().unwrap();
            if let Some(item) = items.iter_mut().find(|d| d.id == item_id) {
                if let Some(p) = item.parts.iter_mut().find(|p| p.index == part.index) {
                    p.status = PartStatus::Downloading;
                }
            }
        }

        // Open part temp file
        let mut file = match fs::OpenOptions::new()
            .create(true)
            .append(part_offset > 0)
            .write(true)
            .open(&part.temp_path)
        {
            Ok(f) => f,
            Err(e) => {
                let mut items = state.lock().unwrap();
                if let Some(item) = items.iter_mut().find(|d| d.id == item_id) {
                    if let Some(p) = item.parts.iter_mut().find(|p| p.index == part.index) {
                        p.status = PartStatus::Failed(format!("File: {}", e));
                    }
                }
                return;
            }
        };

        if part_offset == 0 {
            let _ = file.set_len(0);
        }

        let mut response_reader = response;
        let mut local_downloaded: u64 = part_offset;
        let mut buffer = [0u8; 64 * 1024];
        let update_interval = 256 * 1024;
        let mut bytes_since_update: u64 = 0;

        loop {
            if cancel.load(Ordering::Relaxed) {
                let mut items = state.lock().unwrap();
                if let Some(item) = items.iter_mut().find(|d| d.id == item_id) {
                    if let Some(p) = item.parts.iter_mut().find(|p| p.index == part.index) {
                        p.downloaded = local_downloaded;
                    }
                    item.downloaded = item.parts.iter().map(|p| p.downloaded).sum();
                }
                let _ = file.flush();
                return;
            }

            match response_reader.read(&mut buffer) {
                Ok(0) => {
                    let mut items = state.lock().unwrap();
                    if let Some(item) = items.iter_mut().find(|d| d.id == item_id) {
                        if let Some(p) = item.parts.iter_mut().find(|p| p.index == part.index) {
                            p.downloaded = local_downloaded;
                            p.status = PartStatus::Completed;
                        }
                        item.downloaded = item.parts.iter().map(|p| p.downloaded).sum();
                        item.last_try = now_str();
                    }
                    let _ = file.flush();
                    completed_counter.fetch_add(1, Ordering::Relaxed);
                    return;
                }
                Ok(n) => {
                    if let Err(e) = file.write_all(&buffer[..n]) {
                        let mut items = state.lock().unwrap();
                        if let Some(item) = items.iter_mut().find(|d| d.id == item_id) {
                            if let Some(p) = item.parts.iter_mut().find(|p| p.index == part.index) {
                                p.status = PartStatus::Failed(format!("Write: {}", e));
                                p.downloaded = local_downloaded;
                            }
                            item.downloaded = item.parts.iter().map(|p| p.downloaded).sum();
                        }
                        return;
                    }
                    local_downloaded += n as u64;
                    bytes_since_update += n as u64;

                    if bytes_since_update >= update_interval {
                        bytes_since_update = 0;
                        let mut items = state.lock().unwrap();
                        if let Some(item) = items.iter_mut().find(|d| d.id == item_id) {
                            if let Some(p) = item.parts.iter_mut().find(|p| p.index == part.index) {
                                p.downloaded = local_downloaded;
                            }
                            item.downloaded = item.parts.iter().map(|p| p.downloaded).sum();
                        }
                    }
                }
                Err(e) => {
                    let mut items = state.lock().unwrap();
                    if let Some(item) = items.iter_mut().find(|d| d.id == item_id) {
                        if let Some(p) = item.parts.iter_mut().find(|p| p.index == part.index) {
                            p.downloaded = local_downloaded;
                            if matches!(
                                e.kind(),
                                std::io::ErrorKind::TimedOut
                                    | std::io::ErrorKind::ConnectionReset
                                    | std::io::ErrorKind::ConnectionAborted
                                    | std::io::ErrorKind::BrokenPipe
                            ) {
                                p.status = PartStatus::Pending;
                            } else {
                                p.status = PartStatus::Failed(format!("Read: {}", e));
                            }
                        }
                        item.downloaded = item.parts.iter().map(|p| p.downloaded).sum();
                        item.last_try = now_str();
                    }
                    let _ = file.flush();
                    return;
                }
            }
        }
    });
}

/// Probe server for Range support + file size, then start multi-part downloads
fn start_multi_part_download(
    item_id: u64,
    url: String,
    save_path: String,
    settings: AppSettings,
    cancels: Vec<Arc<AtomicBool>>,
    completed_counter: Arc<AtomicU32>,
    state: Arc<Mutex<Vec<DownloadItem>>>,
) {
    let connections = cancels.len() as u32;

    std::thread::spawn(move || {
        if let Some(parent) = Path::new(&save_path).parent() {
            let _ = fs::create_dir_all(parent);
        }

        // Resolve which proxy to use for this download
        let resolved_proxy: Option<ProxyEntry> = {
            let items = state.lock().unwrap();
            let item = items.iter().find(|d| d.id == item_id);
            match item {
                Some(itm) if !itm.proxy_name.is_empty() => {
                    // Find named proxy in settings
                    settings.proxies.iter().find(|p| p.name == itm.proxy_name).cloned()
                }
                _ => {
                    // No proxy or no name — check default
                    if !settings.default_proxy.is_empty() {
                        settings.proxies.iter().find(|p| p.name == settings.default_proxy).cloned()
                    } else {
                        None
                    }
                }
            }
        };

        let client = match build_client(resolved_proxy.as_ref()) {
            Ok(c) => c,
            Err(e) => {
                let mut items = state.lock().unwrap();
                if let Some(item) = items.iter_mut().find(|d| d.id == item_id) {
                    item.status = DownloadStatus::Failed(format!("Client: {}", e));
                    item.last_try = now_str();
                }
                return;
            }
        };

        // ── Probe server with HEAD request ──
        let head_req = client.head(&url).timeout(std::time::Duration::from_secs(30));
        let head_resp = head_req.send();

        let (supports_range, total_size) = match head_resp {
            Ok(resp) => {
                let status = resp.status();
                if !status.is_success() && status != reqwest::StatusCode::PARTIAL_CONTENT {
                    let mut items = state.lock().unwrap();
                    if let Some(item) = items.iter_mut().find(|d| d.id == item_id) {
                        item.status = DownloadStatus::Failed(format!("Server: HTTP {}", status));
                        item.last_try = now_str();
                    }
                    return;
                }
                let range_ok = resp.headers().get("accept-ranges")
                    .and_then(|v| v.to_str().ok())
                    .map(|v| v.contains("bytes"))
                    .unwrap_or(false);
                let size = resp.content_length().unwrap_or(0);
                (range_ok, size)
            }
            Err(_) => {
                // HEAD failed — probe with a Range: bytes=0-0 GET
                let get_resp = client.get(&url)
                    .header("Range", "bytes=0-0")
                    .timeout(std::time::Duration::from_secs(30))
                    .send();
                match get_resp {
                    Ok(resp) => {
                        let range_ok = resp.status() == reqwest::StatusCode::PARTIAL_CONTENT;
                        let size = resp.headers().get("content-range")
                            .and_then(|v| v.to_str().ok())
                            .and_then(|cr| cr.rsplit('/').next())
                            .and_then(|t| t.parse::<u64>().ok())
                            .unwrap_or(resp.content_length().unwrap_or(0));
                        (range_ok, size)
                    }
                    Err(e) => {
                        let mut items = state.lock().unwrap();
                        if let Some(item) = items.iter_mut().find(|d| d.id == item_id) {
                            item.status = DownloadStatus::Failed(format!("Probe: {}", e));
                            item.last_try = now_str();
                        }
                        return;
                    }
                }
            }
        };

        // Calculate parts
        let mut parts: Vec<DownloadPart> = Vec::new();
        let num_parts = if supports_range && total_size > 1024 * 1024 {
            // At least 1MB per part, capped at connections count
            ((total_size / (1024 * 1024)).max(1).min(connections as u64)) as u32
        } else {
            1
        };

        if num_parts > 1 && total_size > 0 {
            let part_size = total_size / num_parts as u64;
            for i in 0..num_parts {
                let start = i as u64 * part_size;
                let end = if i == num_parts - 1 {
                    total_size - 1
                } else {
                    (i as u64 + 1) * part_size - 1
                };
                parts.push(DownloadPart {
                    index: i,
                    start,
                    end,
                    downloaded: 0,
                    temp_path: part_temp_path(&save_path, i),
                    status: PartStatus::Pending,
                });
            }
        } else {
            parts.push(DownloadPart {
                index: 0,
                start: 0,
                end: if total_size > 0 { total_size - 1 } else { 0 },
                downloaded: 0,
                temp_path: part_temp_path(&save_path, 0),
                status: PartStatus::Pending,
            });
        }

        // Store part info in shared state
        {
            let mut items = state.lock().unwrap();
            if let Some(item) = items.iter_mut().find(|d| d.id == item_id) {
                item.total_size = total_size;
                item.status = DownloadStatus::Downloading;
                item.parts = parts.clone();
                item.connections = num_parts;
                item.last_try = now_str();
            }
        }

        // ── Spawn a thread for each part ──
        for i in 0..num_parts as usize {
            let cancel = cancels[i].clone();
            let comp = completed_counter.clone();
            let st = state.clone();
            let stg = settings.clone();
            let u = url.clone();
            let proxy = resolved_proxy.clone();

            spawn_part_thread(
                item_id,
                u,
                parts[i].clone(),
                stg,
                proxy,
                cancel,
                st,
                comp,
            );
        }

        // ── Monitor completion ──
        let total_parts = num_parts;
        loop {
            std::thread::sleep(std::time::Duration::from_millis(500));

            let completed = completed_counter.load(Ordering::Relaxed);
            if completed >= total_parts {
                // All parts done! Merge.
                let mut items = state.lock().unwrap();
                if let Some(item) = items.iter_mut().find(|d| d.id == item_id) {
                    let all_ok = item.parts.iter().all(|p| p.status == PartStatus::Completed);
                    if all_ok {
                        item.status = DownloadStatus::Downloading; // transitioning
                        drop(items);

                        let item_snapshot = {
                            let items2 = state.lock().unwrap();
                            items2.iter().find(|d| d.id == item_id).cloned()
                        };

                        match item_snapshot {
                            Some(ref snap) => match merge_parts(snap) {
                                Ok(()) => {
                                    let mut items3 = state.lock().unwrap();
                                    if let Some(item3) = items3.iter_mut().find(|d| d.id == item_id) {
                                        item3.status = DownloadStatus::Completed;
                                        item3.downloaded = item3.total_size;
                                        item3.last_try = now_str();
                                        item3.parts.clear();
                                    }
                                }
                                Err(e) => {
                                    let mut items3 = state.lock().unwrap();
                                    if let Some(item3) = items3.iter_mut().find(|d| d.id == item_id) {
                                        item3.status = DownloadStatus::Failed(format!("Merge: {}", e));
                                        item3.last_try = now_str();
                                    }
                                }
                            },
                            None => {}
                        }
                    } else {
                        let failed: Vec<String> = item.parts.iter()
                            .filter_map(|p| {
                                if let PartStatus::Failed(msg) = &p.status {
                                    Some(format!("Part {}: {}", p.index, msg))
                                } else {
                                    None
                                }
                            })
                            .collect();
                        if !failed.is_empty() {
                            item.status = DownloadStatus::Failed(failed.join("; "));
                            item.last_try = now_str();
                        }
                    }
                }
                return;
            }

            // Check if all cancel flags are set (pause/delete)
            let all_cancelled = cancels.iter().all(|c| c.load(Ordering::Relaxed));
            if all_cancelled {
                let mut items = state.lock().unwrap();
                if let Some(item) = items.iter_mut().find(|d| d.id == item_id) {
                    item.downloaded = item.parts.iter().map(|p| p.downloaded).sum();
                }
                return;
            }

            // Sync total downloaded from all parts
            {
                let mut items = state.lock().unwrap();
                if let Some(item) = items.iter_mut().find(|d| d.id == item_id) {
                    item.downloaded = item.parts.iter().map(|p| p.downloaded).sum();
                }
            }
        }
    });
}

struct ProxyDownloadManager {
    downloads: Vec<DownloadItem>,
    shared: Arc<Mutex<Vec<DownloadItem>>>,
    filter: TreeFilter,
    selected_id: Option<u64>,
    settings: AppSettings,
    active_downloads: HashMap<u64, ActiveDownload>,
    icon_cache: Option<IconCache>,

    // UI dialogs
    show_new_dialog: bool,
    show_settings: bool,
    show_about: bool,
    new_url: String,
    new_filename: String,
    new_proxy_name: String,
    // Proxy editor
    show_proxy_editor: bool,
    edit_proxy: Option<ProxyEntry>,
    edit_proxy_index: Option<usize>,

    next_id: u64,
    status_message: Option<String>,
    status_message_timer: f32,
    save_counter: u32,
}

impl Default for ProxyDownloadManager {
    fn default() -> Self {
        let downloads: Vec<DownloadItem> = load_json(DOWNLOADS_FILE).unwrap_or_default();
        let settings: AppSettings = load_json(SETTINGS_FILE).unwrap_or_default();
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

        let shared = Arc::new(Mutex::new(downloads.clone()));

        Self {
            downloads,
            shared,
            filter: TreeFilter::All,
            selected_id: None,
            settings,
            active_downloads: HashMap::new(),
            icon_cache: None,
            show_new_dialog: false,
            show_settings: false,
            show_about: false,
            new_url: String::new(),
            new_filename: String::new(),
            new_proxy_name: String::new(),
            show_proxy_editor: false,
            edit_proxy: None,
            edit_proxy_index: None,
            next_id,
            status_message: None,
            status_message_timer: 0.0,
            save_counter: 0,
        }
    }
}

impl ProxyDownloadManager {
    fn file_name_from_url(url: &str) -> String {
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

    fn filtered_downloads(&self) -> Vec<&DownloadItem> {
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

    fn selected_item(&self) -> Option<&DownloadItem> {
        self.selected_id
            .and_then(|id| self.downloads.iter().find(|d| d.id == id))
    }

    fn set_status(&mut self, msg: String) {
        self.status_message = Some(msg);
        self.status_message_timer = 3.0;
    }

    fn start_download(&mut self, id: u64) {
        let item = match self.downloads.iter().find(|d| d.id == id) {
            Some(i) => i.clone(),
            None => return,
        };

        let num_connections: usize = 4;
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
            id, url, save_path, settings,
            cancels.clone(),
            completed_counter.clone(),
            shared,
        );

        self.active_downloads.insert(id, ActiveDownload {
            cancels,
            completed_parts: completed_counter,
        });
    }

    fn pause_download(&mut self, id: u64) {
        if let Some(active) = self.active_downloads.remove(&id) {
            for c in &active.cancels {
                c.store(true, Ordering::Relaxed);
            }
        }
        let mut items = self.shared.lock().unwrap();
        if let Some(item) = items.iter_mut().find(|d| d.id == id) {
            item.status = DownloadStatus::Paused;
            // Save per-part progress
            item.downloaded = item.parts.iter().map(|p| p.downloaded).sum();
        }
    }

    fn resume_download(&mut self, id: u64) {
        if self.active_downloads.contains_key(&id) {
            return;
        }

        let item = match self.downloads.iter().find(|d| d.id == id) {
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

        start_multi_part_download(id, url, save_path, settings, cancels.clone(), completed_counter.clone(), shared);

        self.active_downloads.insert(id, ActiveDownload {
            cancels,
            completed_parts: completed_counter,
        });
    }

    fn stop_download(&mut self, id: u64) {
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
    }

    fn delete_download(&mut self, id: u64) {
        if let Some(active) = self.active_downloads.remove(&id) {
            for c in &active.cancels {
                c.store(true, Ordering::Relaxed);
            }
        }

        let mut items = self.shared.lock().unwrap();
        if let Some(item) = items.iter().find(|d| d.id == id) {
            let _ = fs::remove_file(&item.save_path);
            // Remove any part temp files
            for part in &item.parts {
                let _ = fs::remove_file(&part.temp_path);
            }
        }
        items.retain(|d| d.id != id);

        if self.selected_id == Some(id) {
            self.selected_id = None;
        }
    }

    fn add_new_download(&mut self, url: &str, custom_name: Option<&str>) {
        let file_name = custom_name
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .unwrap_or_else(|| Self::file_name_from_url(url));

        let save_dir = PathBuf::from(&self.settings.download_dir);
        let save_path = save_dir.join(&file_name);

        let proxy_name = std::mem::take(&mut self.new_proxy_name);

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
                connections: 4,
                proxy_name,
                last_try: String::new(),
                created_at: now_str(),
            });
        }

        self.start_download(item_id);

        self.set_status(format!("Added download: {}", file_name));
    }
}

// ─── eframe App implementation ────────────────────────────────────────────────

impl eframe::App for ProxyDownloadManager {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        // ── Initialize icon cache (once) ──────────────────────────────────────
        if self.icon_cache.is_none() {
            self.icon_cache = Some(IconCache::new(ui.ctx()));
        }

        // ── Sync from shared state ────────────────────────────────────────────
        if let Ok(shared) = self.shared.lock() {
            self.downloads = shared.clone();
        }

        // ── Time-based status message fading ──────────────────────────────────
        if self.status_message.is_some() {
            self.status_message_timer -= ui.input(|i| i.unstable_dt);
            if self.status_message_timer <= 0.0 {
                self.status_message = None;
            }
        }

        // ── Pre-compute immutable values ──────────────────────────────────────
        let all_count = self.downloads.len();
        let completed_count = self.downloads.iter().filter(|d| d.status == DownloadStatus::Completed).count();
        let incompleted_count = all_count - completed_count;
        let can_resume = self.selected_item().map_or(false, |item| {
            matches!(item.status, DownloadStatus::Paused | DownloadStatus::Failed(_))
        });
        let can_stop = self.selected_item().map_or(false, |item| {
            matches!(item.status, DownloadStatus::Downloading)
        });
        let has_active = !self.active_downloads.is_empty();
        let status_msg = self.status_message.clone();
        let filter = self.filter.clone();

        // ── Local action signals for closures ─────────────────────────────────
        let mut tb_new = false;
        let mut tb_resume = false;
        let mut tb_stop = false;
        let mut tb_delete = false;
        let mut tb_quit = false;
        let mut tb_settings = false;
        let mut tb_about = false;
        let mut sb_filter: Option<TreeFilter> = None;

        // ── Top Toolbar ───────────────────────────────────────────────────────
        egui::Panel::top("toolbar").show_inside(ui, |ui| {
            ui.horizontal(|ui| {
                let btn_height = 28.0;

                if ui.add_sized(
                    Vec2::new(130.0, btn_height),
                    egui::Button::new(RichText::new("📥 New Download").size(14.0)),
                )
                .clicked()
                {
                    tb_new = true;
                }

                let resume_btn = ui.add_sized(
                    Vec2::new(100.0, btn_height),
                    egui::Button::new(RichText::new("▶ Resume").size(14.0)),
                );
                if resume_btn.clicked() && can_resume {
                    tb_resume = true;
                }

                let stop_btn = ui.add_sized(
                    Vec2::new(90.0, btn_height),
                    egui::Button::new(RichText::new("⏹ Stop").size(14.0)),
                );
                if stop_btn.clicked() && can_stop {
                    tb_stop = true;
                }

                let delete_btn = ui.add_sized(
                    Vec2::new(90.0, btn_height),
                    egui::Button::new(RichText::new("🗑 Delete").size(14.0)),
                );
                if delete_btn.clicked() {
                    tb_delete = true;
                }

                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui.add_sized(
                        Vec2::new(80.0, btn_height),
                        egui::Button::new(RichText::new("❌ Quit").size(14.0)),
                    )
                    .clicked()
                    {
                        tb_quit = true;
                    }

                    if ui.add_sized(
                        Vec2::new(90.0, btn_height),
                        egui::Button::new(RichText::new("ℹ About").size(14.0)),
                    )
                    .clicked()
                    {
                        tb_about = true;
                    }

                    if ui.add_sized(
                        Vec2::new(100.0, btn_height),
                        egui::Button::new(RichText::new("⚙ Settings").size(14.0)),
                    )
                    .clicked()
                    {
                        tb_settings = true;
                    }
                });
            });
        });

        // ── Handle toolbar actions ────────────────────────────────────────────
        if tb_new {
            self.show_new_dialog = true;
            self.new_url.clear();
            self.new_filename.clear();
            self.new_proxy_name = self.settings.default_proxy.clone();
        }
        if tb_resume {
            if let Some(id) = self.selected_id {
                self.resume_download(id);
            }
        }
        if tb_stop {
            if let Some(id) = self.selected_id {
                self.stop_download(id);
            }
        }
        if tb_delete {
            if let Some(id) = self.selected_id {
                self.delete_download(id);
            }
        }
        if tb_quit {
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
        }
        if tb_settings {
            self.show_settings = true;
        }
        if tb_about {
            self.show_about = true;
        }

        // ── Sidebar (Tree View) ──────────────────────────────────────────────
        egui::Panel::left("sidebar")
            .resizable(false)
            .default_size(180.0)
            .show_inside(ui, |ui| {
                ui.add_space(8.0);
                ui.heading("📂 Downloads");
                ui.separator();
                ui.add_space(4.0);

                if ui
                    .add(egui::Button::new(format!("📁 All ({})", all_count))
                        .selected(filter == TreeFilter::All)
                        .min_size(Vec2::new(160.0, 28.0)))
                    .clicked()
                {
                    sb_filter = Some(TreeFilter::All);
                }

                if ui
                    .add(egui::Button::new(format!("✅ Completed ({})", completed_count))
                        .selected(filter == TreeFilter::Completed)
                        .min_size(Vec2::new(160.0, 28.0)))
                    .clicked()
                {
                    sb_filter = Some(TreeFilter::Completed);
                }

                if ui
                    .add(egui::Button::new(format!("⏳ Incomplete ({})", incompleted_count))
                        .selected(filter == TreeFilter::Incompleted)
                        .min_size(Vec2::new(160.0, 28.0)))
                    .clicked()
                {
                    sb_filter = Some(TreeFilter::Incompleted);
                }

                ui.add_space(16.0);
                ui.separator();
                ui.add_space(4.0);

                ui.label(
                    RichText::new(format!("Total: {}", all_count))
                        .size(12.0)
                        .color(Color32::GRAY),
                );

                if let Some(msg) = &status_msg {
                    ui.add_space(8.0);
                    ui.label(RichText::new(msg.as_str()).size(12.0).color(Color32::from_rgb(0, 180, 0)));
                }
            });

        // ── Handle sidebar filter changes ─────────────────────────────────────
        if let Some(f) = sb_filter {
            self.filter = f;
        }

        // ── Central Table View ───────────────────────────────────────────────
        let cloned_items: Vec<DownloadItem> = self.downloads.clone();
        let icon_cache = &mut self.icon_cache;
        let mut selected_id = self.selected_id;

        egui::CentralPanel::default().show_inside(ui, |ui| {
            let filtered_items: Vec<&DownloadItem> = match self.filter {
                TreeFilter::All => cloned_items.iter().collect(),
                TreeFilter::Completed => cloned_items.iter().filter(|d| d.status == DownloadStatus::Completed).collect(),
                TreeFilter::Incompleted => cloned_items.iter().filter(|d| d.status != DownloadStatus::Completed).collect(),
            };

            ui.horizontal(|ui| {
                let label = match self.filter {
                    TreeFilter::All => "All Downloads",
                    TreeFilter::Completed => "Completed Downloads",
                    TreeFilter::Incompleted => "Incomplete Downloads",
                };
                ui.label(RichText::new(label).size(14.0).strong());
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.label(
                        RichText::new(format!("{} items", filtered_items.len()))
                            .size(12.0)
                            .color(Color32::GRAY),
                    );
                });
            });
            ui.separator();

            // Table header
            let header_color = Color32::from_rgb(45, 45, 48);
            let header_height = 26.0;
            Frame::NONE
                .fill(header_color)
                .corner_radius(CornerRadius::same(4))
                .inner_margin(Margin::symmetric(8, 2))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        let col_fixed: f32 = 100.0 + 180.0 + 100.0 + 120.0;
                        let avail = ui.available_width() - col_fixed;
                        ui.add_sized(Vec2::new(avail.max(180.0), header_height),
                            egui::Label::new(RichText::new("File Name").strong().size(13.0)));
                        ui.add_sized(Vec2::new(100.0, header_height),
                            egui::Label::new(RichText::new("Size").strong().size(13.0)));
                        ui.add_sized(Vec2::new(180.0, header_height),
                            egui::Label::new(RichText::new("Status").strong().size(13.0)));
                        ui.add_sized(Vec2::new(100.0, header_height),
                            egui::Label::new(RichText::new("Proxy").strong().size(13.0)));
                        ui.add_sized(Vec2::new(120.0, header_height),
                            egui::Label::new(RichText::new("Last Try").strong().size(13.0)));
                    });
                });

            ui.add_space(2.0);

            // Table rows
            ScrollArea::vertical()
                .id_salt("download_table")
                .auto_shrink([false; 2])
                .show(ui, |ui| {
                    let row_height = 32.0;
                    let row_color_normal = Color32::from_rgb(30, 30, 30);
                    let row_color_alt = Color32::from_rgb(37, 37, 37);
                    let row_color_selected = Color32::from_rgb(50, 70, 100);

                    for (idx, item) in filtered_items.iter().enumerate() {
                        let is_selected = selected_id == Some(item.id);
                        let bg = if is_selected { row_color_selected }
                            else if idx % 2 == 0 { row_color_normal }
                            else { row_color_alt };

                        let icon_texture = icon_cache
                            .as_mut()
                            .map(|cache| cache.get_icon(&item.file_name, ui.ctx()));

                        let response = Frame::NONE
                            .fill(bg)
                            .corner_radius(CornerRadius::same(2))
                            .inner_margin(Margin::symmetric(8, 2))
                            .show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    let col_fixed: f32 = 100.0 + 180.0 + 100.0 + 120.0;
                                    let avail = ui.available_width() - col_fixed;
                                    let name_width = avail.max(180.0);

                                    // File Name column (with icon)
                                    ui.add_sized(Vec2::new(name_width, row_height), |ui: &mut egui::Ui| {
                                        ui.horizontal(|ui| {
                                            if let Some(tex) = &icon_texture {
                                                ui.add(egui::Image::new(tex).fit_to_exact_size(Vec2::new(20.0, 20.0)));
                                            }
                                            ui.label(RichText::new(&item.file_name).size(13.0).color(Color32::WHITE));
                                        }).response
                                    });

                                    // Size column
                                    ui.add_sized(Vec2::new(100.0, row_height),
                                        egui::Label::new(RichText::new(format_size(item.total_size)).size(13.0).color(Color32::LIGHT_GRAY)));

                                    // Status column
                                    let (status_text, status_color) = status_icon_and_text(&item.status);
                                    let status_display = match &item.status {
                                        DownloadStatus::Failed(msg) if !msg.is_empty() => format!("{}: {}", status_text, msg),
                                        _ => status_text.to_string(),
                                    };
                                    ui.add_sized(Vec2::new(180.0, row_height), |ui: &mut egui::Ui| {
                                        ui.horizontal(|ui| {
                                            ui.label(RichText::new(&status_display).size(13.0).color(status_color));
                                            if matches!(item.status, DownloadStatus::Downloading) && item.total_size > 0 {
                                                let p = (item.downloaded as f64 / item.total_size as f64).clamp(0.0, 1.0) as f32;
                                                ui.add(egui::ProgressBar::new(p).desired_width(80.0).show_percentage());
                                            }
                                        }).response
                                    });

                                    // Proxy column
                                    let proxy_display = if item.proxy_name.is_empty() {
                                        "-".to_string()
                                    } else {
                                        format!("🔌 {}", item.proxy_name)
                                    };
                                    ui.add_sized(Vec2::new(100.0, row_height),
                                        egui::Label::new(RichText::new(proxy_display).size(12.0).color(Color32::from_rgb(180, 160, 100))));

                                    // Last Try column
                                    let last_try_display = if item.last_try.is_empty() { "-".to_string() } else { item.last_try.clone() };
                                    ui.add_sized(Vec2::new(120.0, row_height),
                                        egui::Label::new(RichText::new(last_try_display).size(13.0).color(Color32::GRAY)));
                                }).response
                            });

                        if response.response.clicked() {
                            selected_id = Some(item.id);
                        }
                    }

                    if filtered_items.is_empty() {
                        ui.add_space(40.0);
                        ui.vertical_centered(|ui| {
                            ui.label(RichText::new("📭 No downloads yet").size(18.0).color(Color32::GRAY));
                            ui.add_space(8.0);
                            ui.label(RichText::new("Click 'New Download' to get started").size(14.0).color(Color32::DARK_GRAY));
                        });
                    }
                });
        });

        self.selected_id = selected_id;

        // ── New Download Dialog ──────────────────────────────────────────────
        if self.show_new_dialog {
            egui::Window::new("New Download")
                .id(egui::Id::new("new_download_window"))
                .collapsible(false)
                .resizable(false)
                .default_size(Vec2::new(500.0, 200.0))
                .show(ui.ctx(), |ui| {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("URL:").strong());
                        let resp = ui.add_sized(Vec2::new(380.0, 24.0),
                            egui::TextEdit::singleline(&mut self.new_url).hint_text("https://example.com/file.zip"));
                        resp.request_focus();
                    });
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("Name:").strong());
                        ui.add_sized(Vec2::new(380.0, 24.0),
                            egui::TextEdit::singleline(&mut self.new_filename).hint_text("Optional — auto-detected from URL"));
                    });
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("Proxy:").strong());
                        let names: Vec<String> = std::iter::once(String::new())
                            .chain(self.settings.proxies.iter().map(|p| p.name.clone()))
                            .collect();
                        let current_idx = names.iter().position(|n| *n == self.new_proxy_name).unwrap_or(0);
                        let mut sel = current_idx;
                        egui::ComboBox::from_id_salt("download_proxy")
                            .selected_text(if self.new_proxy_name.is_empty() {
                                "No Proxy".to_string()
                            } else {
                                self.new_proxy_name.clone()
                            })
                            .show_ui(ui, |ui| {
                                for (i, name) in names.iter().enumerate() {
                                    let display = if name.is_empty() { "No Proxy".to_string() } else { name.clone() };
                                    if ui.selectable_label(sel == i, &display).clicked() {
                                        sel = i;
                                    }
                                }
                            });
                        if sel != current_idx {
                            self.new_proxy_name = if sel == 0 { String::new() } else { names[sel].clone() };
                        }
                    });
                    ui.add_space(12.0);
                    ui.horizontal(|ui| {
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            let url_ok = !self.new_url.trim().is_empty();
                            let dl_btn = ui.add_enabled(url_ok,
                                egui::Button::new(RichText::new("📥 Download").size(14.0)));
                            if dl_btn.clicked() {
                                let url = self.new_url.trim().to_string();
                                let filename = if self.new_filename.trim().is_empty() { None }
                                    else { Some(self.new_filename.trim().to_string()) };
                                self.new_proxy_name = self.new_proxy_name.trim().to_string();
                                self.add_new_download(&url, filename.as_deref());
                                self.show_new_dialog = false;
                                if let Ok(items) = self.shared.lock() {
                                    save_json(DOWNLOADS_FILE, &*items);
                                }
                            }
                            ui.add_space(8.0);
                            if ui.add_sized(Vec2::new(80.0, 28.0), egui::Button::new("Cancel")).clicked() {
                                self.show_new_dialog = false;
                            }
                        });
                    });
                });
        }

        // ── Settings Window ──────────────────────────────────────────────────
        if self.show_settings {
            egui::Window::new("⚙ Settings")
                .id(egui::Id::new("settings_window"))
                .collapsible(false)
                .resizable(true)
                .default_size(Vec2::new(520.0, 420.0))
                .show(ui.ctx(), |ui| {
                    ui.label(RichText::new("Download Settings").strong().size(15.0));
                    ui.separator();
                    ui.horizontal(|ui| {
                        ui.label("Download Directory:");
                        ui.add_sized(Vec2::new(360.0, 24.0),
                            egui::TextEdit::singleline(&mut self.settings.download_dir));
                    });

                    ui.add_space(12.0);
                    ui.label(RichText::new("Proxy Lists").strong().size(15.0));
                    ui.separator();

                    // Default proxy selector
                    ui.horizontal(|ui| {
                        ui.label("Default Proxy:");
                        let names: Vec<String> = std::iter::once(String::new())
                            .chain(self.settings.proxies.iter().map(|p| p.name.clone()))
                            .collect();
                        let current_idx = names.iter().position(|n| *n == self.settings.default_proxy).unwrap_or(0);
                        let mut sel = current_idx;
                        egui::ComboBox::from_id_salt("default_proxy")
                            .selected_text(if self.settings.default_proxy.is_empty() {
                                "None".to_string()
                            } else {
                                self.settings.default_proxy.clone()
                            })
                            .show_ui(ui, |ui| {
                                for (i, name) in names.iter().enumerate() {
                                    let display = if name.is_empty() { "None".to_string() } else { name.clone() };
                                    if ui.selectable_label(sel == i, &display).clicked() {
                                        sel = i;
                                    }
                                }
                            });
                        if sel != current_idx {
                            self.settings.default_proxy = if sel == 0 {
                                String::new()
                            } else {
                                names[sel].clone()
                            };
                        }
                    });

                    ui.add_space(4.0);

                    // Proxy list header
                    Frame::NONE
                        .fill(Color32::from_rgb(40, 40, 43))
                        .corner_radius(CornerRadius::same(3))
                        .inner_margin(Margin::symmetric(6, 2))
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.add_sized(Vec2::new(120.0, 20.0),
                                    egui::Label::new(RichText::new("Name").strong().size(12.0)));
                                ui.add_sized(Vec2::new(60.0, 20.0),
                                    egui::Label::new(RichText::new("Type").strong().size(12.0)));
                                ui.add_sized(Vec2::new(130.0, 20.0),
                                    egui::Label::new(RichText::new("Host:Port").strong().size(12.0)));
                                ui.add_sized(Vec2::new(80.0, 20.0),
                                    egui::Label::new(RichText::new("").strong().size(12.0)));
                            });
                        });

                    // Proxy list (scrollable)
                    let mut to_delete: Option<usize> = None;
                    let mut to_edit: Option<usize> = None;
                    ScrollArea::vertical()
                        .id_salt("proxy_list")
                        .max_height(140.0)
                        .show(ui, |ui| {
                            for (i, proxy) in self.settings.proxies.iter().enumerate() {
                                let bg = if i % 2 == 0 { Color32::from_rgb(30, 30, 30) } else { Color32::from_rgb(35, 35, 38) };
                                Frame::NONE.fill(bg).inner_margin(Margin::symmetric(6, 2)).show(ui, |ui| {
                                    ui.horizontal(|ui| {
                                        ui.add_sized(Vec2::new(120.0, 20.0),
                                            egui::Label::new(RichText::new(&proxy.name).size(12.0)));
                                        let proto = match proxy.protocol { ProxyProtocol::Http => "HTTP", ProxyProtocol::Socks5 => "SOCKS5" };
                                        ui.add_sized(Vec2::new(60.0, 20.0),
                                            egui::Label::new(RichText::new(proto).size(12.0).color(Color32::LIGHT_GRAY)));
                                        ui.add_sized(Vec2::new(130.0, 20.0),
                                            egui::Label::new(RichText::new(format!("{}:{}", proxy.host, proxy.port)).size(12.0).color(Color32::LIGHT_GRAY)));
                                        if ui.add_sized(Vec2::new(40.0, 20.0), egui::Button::new("✏️")).clicked() {
                                            to_edit = Some(i);
                                        }
                                        if ui.add_sized(Vec2::new(40.0, 20.0), egui::Button::new("🗑")).clicked() {
                                            to_delete = Some(i);
                                        }
                                    });
                                });
                            }
                            if self.settings.proxies.is_empty() {
                                ui.add_space(8.0);
                                ui.label(RichText::new("No proxies configured. Click 'Add Proxy' to create one.").size(12.0).color(Color32::GRAY));
                            }
                        });

                    ui.horizontal(|ui| {
                        if ui.button("➕ Add Proxy").clicked() {
                            self.edit_proxy = Some(ProxyEntry {
                                name: String::new(),
                                protocol: ProxyProtocol::Http,
                                host: String::new(),
                                port: 8080,
                                username: String::new(),
                                password: String::new(),
                            });
                            self.edit_proxy_index = None;
                            self.show_proxy_editor = true;
                        }
                    });

                    // Handle delete
                    if let Some(idx) = to_delete {
                        let name = self.settings.proxies[idx].name.clone();
                        if self.settings.default_proxy == name {
                            self.settings.default_proxy = String::new();
                        }
                        self.settings.proxies.remove(idx);
                    }

                    // Handle edit
                    if let Some(idx) = to_edit {
                        self.edit_proxy = Some(self.settings.proxies[idx].clone());
                        self.edit_proxy_index = Some(idx);
                        self.show_proxy_editor = true;
                    }

                    // Proxy editor dialog
                    if self.show_proxy_editor {
                        if let Some(ref mut proxy) = self.edit_proxy {
                            egui::Window::new(if self.edit_proxy_index.is_some() { "Edit Proxy" } else { "Add Proxy" })
                                .id(egui::Id::new("proxy_editor"))
                                .collapsible(false)
                                .resizable(false)
                                .default_size(Vec2::new(400.0, 260.0))
                                .show(ui.ctx(), |ui| {
                                    ui.horizontal(|ui| {
                                        ui.label("Name:");
                                        ui.add_sized(Vec2::new(200.0, 24.0),
                                            egui::TextEdit::singleline(&mut proxy.name).hint_text("my-proxy"));
                                    });
                                    ui.horizontal(|ui| {
                                        ui.label("Protocol:");
                                        let protos = ["HTTP", "SOCKS5"];
                                        let mut sel = if proxy.protocol == ProxyProtocol::Socks5 { 1 } else { 0 };
                                        egui::ComboBox::from_id_salt("proxy_proto")
                                            .selected_text(if sel == 0 { "HTTP" } else { "SOCKS5" })
                                            .show_ui(ui, |ui| {
                                                for (i, p) in protos.iter().enumerate() {
                                                    if ui.selectable_label(sel == i, *p).clicked() { sel = i; }
                                                }
                                            });
                                        proxy.protocol = if sel == 0 { ProxyProtocol::Http } else { ProxyProtocol::Socks5 };
                                    });
                                    ui.horizontal(|ui| {
                                        ui.label("Host:");
                                        ui.add_sized(Vec2::new(200.0, 24.0),
                                            egui::TextEdit::singleline(&mut proxy.host).hint_text("127.0.0.1"));
                                        ui.label("Port:");
                                        let mut port_str = proxy.port.to_string();
                                        if ui.add_sized(Vec2::new(60.0, 24.0),
                                            egui::TextEdit::singleline(&mut port_str).hint_text("8080")).changed() {
                                            proxy.port = port_str.parse().unwrap_or(8080);
                                        }
                                    });
                                    ui.horizontal(|ui| {
                                        ui.label("Username:");
                                        ui.add_sized(Vec2::new(150.0, 24.0),
                                            egui::TextEdit::singleline(&mut proxy.username));
                                        ui.label("Password:");
                                        ui.add_sized(Vec2::new(150.0, 24.0),
                                            egui::TextEdit::singleline(&mut proxy.password).password(true));
                                    });
                                    ui.add_space(12.0);
                                    ui.horizontal(|ui| {
                                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                            if ui.button("Save").clicked() && !proxy.name.is_empty() {
                                                if let Some(idx) = self.edit_proxy_index {
                                                    self.settings.proxies[idx] = proxy.clone();
                                                } else {
                                                    self.settings.proxies.push(proxy.clone());
                                                }
                                                self.show_proxy_editor = false;
                                            }
                                            ui.add_space(8.0);
                                            if ui.button("Cancel").clicked() {
                                                self.show_proxy_editor = false;
                                            }
                                        });
                                    });
                                });
                        }
                    }

                    ui.add_space(12.0);
                    ui.separator();
                    ui.horizontal(|ui| {
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if ui.add_sized(Vec2::new(120.0, 28.0), egui::Button::new("Save & Close")).clicked() {
                                save_json(SETTINGS_FILE, &self.settings);
                                self.set_status("Settings saved".to_string());
                                self.show_settings = false;
                            }
                            ui.add_space(8.0);
                            if ui.add_sized(Vec2::new(80.0, 28.0), egui::Button::new("Cancel")).clicked() {
                                if let Some(s) = load_json(SETTINGS_FILE) { self.settings = s; }
                                self.show_settings = false;
                            }
                        });
                    });
                });
        }

        // ── About Window ─────────────────────────────────────────────────────
        if self.show_about {
            egui::Window::new("ℹ About ProxyDM")
                .id(egui::Id::new("about_window"))
                .collapsible(false)
                .resizable(false)
                .default_size(Vec2::new(350.0, 220.0))
                .show(ui.ctx(), |ui| {
                    ui.vertical_centered(|ui| {
                        ui.add_space(16.0);
                        ui.label(RichText::new("ProxyDM").size(24.0).strong());
                        ui.label(RichText::new("Version 0.1.0").size(14.0).color(Color32::GRAY));
                        ui.add_space(12.0);
                        ui.label("A download manager built with Rust and egui");
                        ui.label("Supports HTTP/HTTPS downloads with pause/resume");
                        ui.label("and proxy configuration.");
                        ui.add_space(8.0);
                        ui.label(RichText::new("🔧 Proxy Download Manager").size(12.0).color(Color32::DARK_GRAY));
                        ui.add_space(16.0);
                        if ui.add_sized(Vec2::new(100.0, 28.0), egui::Button::new("Close")).clicked() {
                            self.show_about = false;
                        }
                    });
                });
        }

        // ── Auto-save ────────────────────────────────────────────────────────
        self.save_counter += 1;
        if self.save_counter >= 60 {
            self.save_counter = 0;
            if let Ok(items) = self.shared.lock() {
                save_json(DOWNLOADS_FILE, &*items);
            }
        }

        // ── Request repaint while downloading ────────────────────────────────
        if has_active {
            ui.ctx().request_repaint();
        }
    }
}

// ─── Entry Point ──────────────────────────────────────────────────────────────

fn main() -> Result<(), eframe::Error> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([960.0, 600.0])
            .with_min_inner_size([640.0, 400.0])
            .with_title("Proxy Download Manager"),
        ..Default::default()
    };

    eframe::run_native(
        APP_NAME,
        options,
        Box::new(|_cc| Ok(Box::new(ProxyDownloadManager::default()))),
    )
}
