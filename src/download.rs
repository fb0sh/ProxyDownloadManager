// =============================================================================
// download.rs — Multi-threaded download engine
//
// Architecture:
//   start_multi_part_download() → HEAD probe → split Range → N part threads
//   Each part thread: spawn_part_thread() → Range GET → 64KB streaming
//   All parts complete → merge_parts() → cleanup .partN files
// =============================================================================

use crate::types::*;
use crate::log_info;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::Path;
use std::fs;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

// ─── HTTP client builder ──────────────────────────────────────────────────────

pub fn build_client(proxy_entry: Option<&ProxyEntry>, user_agent: &str) -> anyhow::Result<reqwest::blocking::Client> {
    let mut builder = reqwest::blocking::Client::builder()
        .user_agent(user_agent)
        .timeout(std::time::Duration::from_secs(30))
        .redirect(reqwest::redirect::Policy::limited(10));

    if let Some(entry) = proxy_entry {
        let proxy_url = if entry.port == 0 {
            format!("{}://{}", entry.protocol.scheme(), entry.host)
        } else {
            format!("{}://{}:{}", entry.protocol.scheme(), entry.host, entry.port)
        };

        if let Ok(p) = reqwest::Proxy::all(&proxy_url) {
            builder = builder.proxy(p);
        }
    }

    let client = builder.build()?;
    Ok(client)
}

// ─── Part file management ─────────────────────────────────────────────────────

/// Temporary file path for a download part (~/.pdm/parts/{name}.part{N})
pub fn part_temp_path(save_path: &str, part_index: u32) -> String {
    let fname = Path::new(save_path)
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "download".to_string());
    let parts_dir = pdm_dir().join("parts");
    let _ = fs::create_dir_all(&parts_dir);
    parts_dir
        .join(format!("{}.part{}", fname, part_index))
        .to_string_lossy()
        .to_string()
}

/// Merge completed part files into the final output file, reporting progress to shared state
pub fn merge_parts(item: &DownloadItem, state: &Arc<Mutex<Vec<DownloadItem>>>, item_id: u64) -> Result<(), String> {
    let output_path = Path::new(&item.save_path);
    if let Some(parent) = output_path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("mkdir: {}", e))?;
    }

    let mut output = fs::File::create(output_path)
        .map_err(|e| format!("create output: {}", e))?;

    let total_parts = item.parts.len();
    for (i, part) in item.parts.iter().enumerate() {
        let part_path = Path::new(&part.temp_path);
        if part_path.exists() {
            let mut input = fs::File::open(part_path)
                .map_err(|e| format!("open part {}: {}", part.index, e))?;
            std::io::copy(&mut input, &mut output)
                .map_err(|e| format!("copy part {}: {}", part.index, e))?;
            drop(input);
            let _ = fs::remove_file(part_path);
        }
        // Report merge progress after each part
        {
            let mut items = state.lock().unwrap();
            if let Some(item_state) = items.iter_mut().find(|d| d.id == item_id) {
                item_state.merge_progress = (i + 1) as f32 / total_parts.max(1) as f32;
            }
        }
    }
    output.flush().map_err(|e| format!("flush: {}", e))?;
    Ok(())
}

// ─── Part download thread ─────────────────────────────────────────────────────

/// Inner download logic for one part. Returns Ok(()) on clean completion,
/// Err(true) if cancelled by user, Err(false) on failure.
/// NOTE: The outer spawn wrapper always increments completed_counter on exit
/// so the coordinator never stalls waiting for a thread that exited early.
fn part_thread_inner(
    item_id: u64,
    url: &str,
    part: &DownloadPart,
    proxy_entry: Option<&ProxyEntry>,
    cancel: &AtomicBool,
    state: &Arc<Mutex<Vec<DownloadItem>>>,
    max_retries: u32,
    user_agent: &str,
) -> Result<(), bool> {
    let part_offset = part.downloaded;

    for attempt in 0..=max_retries {
        if attempt > 0 {
            log_info!("Part #{} retry {}/{}", part.index, attempt, max_retries);
            // Update retry count in shared state
            let mut items = state.lock().unwrap();
            if let Some(item) = items.iter_mut().find(|d| d.id == item_id) {
                if let Some(p) = item.parts.iter_mut().find(|p| p.index == part.index) {
                    p.retries = attempt;
                }
            }
            drop(items);
            std::thread::sleep(std::time::Duration::from_secs(1));
        }

        let client = match build_client(proxy_entry, user_agent) {
            Ok(c) => c,
            Err(e) => {
                if attempt >= max_retries {
                    let mut items = state.lock().unwrap();
                    if let Some(item) = items.iter_mut().find(|d| d.id == item_id) {
                        item.status = DownloadStatus::Failed(format!("Client: {}", e));
                        item.last_try = now_str();
                    }
                    return Err(false);
                }
                continue;
            }
        };

        if let Some(parent) = Path::new(&part.temp_path).parent() {
            let _ = fs::create_dir_all(parent);
        }

        let part_remaining = if part.end > 0 {
            part.end.saturating_sub(part.start + part_offset)
        } else {
            u64::MAX
        };

        // Already fully downloaded (safety net — should rarely trigger now since
        // start_multi_part_download pre-marks completed parts and skips threads)
        if part_offset > 0 && part_remaining == 0 && part.end > 0 {
            log_info!("Part #{} already fully downloaded ({} bytes)", part.index, part_offset);
            // Must update status in shared state so coordinator sees it
            let mut items = state.lock().unwrap();
            if let Some(item) = items.iter_mut().find(|d| d.id == item_id) {
                if let Some(p) = item.parts.iter_mut().find(|p| p.index == part.index) {
                    p.status = PartStatus::Completed;
                }
            }
            return Ok(());
        }

        let range_start = part.start + part_offset;
        let range_end = if part.end > 0 { part.end } else { 0 };

        let mut req = client.get(url);
        if range_end > 0 && range_start <= range_end {
            req = req.header("Range", format!("bytes={}-{}", range_start, range_end));
        } else if range_start > 0 {
            req = req.header("Range", format!("bytes={}-", range_start));
        }
    let response = match req.timeout(std::time::Duration::from_secs(120)).send() {
        Ok(r) => r,
        Err(e) => {
            log_info!("Part #{} request error (attempt {}): {}", part.index, attempt + 1, e);
            if attempt >= max_retries {
                let mut items = state.lock().unwrap();
                if let Some(item) = items.iter_mut().find(|d| d.id == item_id) {
                    item.status = DownloadStatus::Failed(format!("Part {}: {}", part.index, e));
                    item.last_try = now_str();
                }
                return Err(false);
            }
            continue;
        }
    };

    let status = response.status();
    if !status.is_success() && status != reqwest::StatusCode::PARTIAL_CONTENT {
        log_info!("Part #{} HTTP {} (attempt {})", part.index, status, attempt + 1);
        if attempt >= max_retries {
            let mut items = state.lock().unwrap();
            if let Some(item) = items.iter_mut().find(|d| d.id == item_id) {
                if let Some(p) = item.parts.iter_mut().find(|p| p.index == part.index) {
                    p.status = PartStatus::Failed(format!("HTTP {}", status));
                }
                item.last_try = now_str();
            }
            return Err(false);
        }
        continue;
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

    let mut file = fs::OpenOptions::new()
        .create(true)
        .write(true)
        .open(&part.temp_path)
        .map_err(|e| {
            let mut items = state.lock().unwrap();
            if let Some(item) = items.iter_mut().find(|d| d.id == item_id) {
                if let Some(p) = item.parts.iter_mut().find(|p| p.index == part.index) {
                    p.status = PartStatus::Failed(format!("File: {}", e));
                }
            }
            false
        })?;

    // Truncate to exactly part_offset so we never append atop stale data
    // from a previous cancelled/interrupted run.
    let truncate_err = |e: std::io::Error, msg: &str| -> bool {
        let mut items = state.lock().unwrap();
        if let Some(item) = items.iter_mut().find(|d| d.id == item_id) {
            if let Some(p) = item.parts.iter_mut().find(|p| p.index == part.index) {
                p.status = PartStatus::Failed(format!("{}: {}", msg, e));
            }
        }
        false
    };
    if part_offset == 0 {
        file.set_len(0).map_err(|e| truncate_err(e, "set_len"))?;
    } else {
        let actual_len = file.metadata()
            .map_err(|e| truncate_err(e, "metadata"))?
            .len();
        if actual_len > part_offset {
            file.set_len(part_offset)
                .map_err(|e| truncate_err(e, "set_len"))?;
            log_info!("Part #{} truncated from {} to {} bytes",
                part.index, actual_len, part_offset);
        }
    }
    // Position cursor at end for appending
    file.seek(SeekFrom::End(0))
        .map_err(|e| truncate_err(e, "seek"))?;

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
            return Err(true); // cancelled
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
                log_info!("Part #{} completed ({} bytes)", part.index, local_downloaded);
                return Ok(());
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
                    return Err(false);
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
                        p.status = PartStatus::Pending;
                    }
                    item.downloaded = item.parts.iter().map(|p| p.downloaded).sum();
                    item.last_try = now_str();
                }
                let _ = file.flush();
                log_info!("Part #{} read error (attempt {}): {}", part.index, attempt + 1, e);
                if attempt >= max_retries {
                    return Err(false);
                }
                break; // break inner download loop to retry outer loop
            }
        }
    }
    }
    // All retries exhausted
    log_info!("Part #{} failed after {} retries", part.index, max_retries);
    Err(false)
}

/// Spawn a single part download thread.
/// ALWAYS increments completed_counter on exit, so the coordinator never stalls.
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
        // Guard against panics: catch_unwind ensures counter always increments
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            part_thread_inner(
                item_id, &url, &part,
                proxy_entry.as_ref(),
                &cancel,
                &state,
                settings.max_retries,
                &settings.user_agent,
            )
        })).ok().and_then(|r| r.ok());
        // Always increment so the coordinator never waits forever
        completed_counter.fetch_add(1, Ordering::Relaxed);
        match result {
            Some(()) => { /* success */ }
            None => { /* panic or failure — already marked in shared state */ }
        }
    });
}

// ─── Multi-part download coordinator ──────────────────────────────────────────

/// Probe server, split into parts, spawn part threads, monitor completion
pub fn start_multi_part_download(
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

        // Resolve which proxy to use and capture item info for logging
        let (item_file_name, item_proxy_name, proxy_for_log, resolved_proxy) = {
            let items = state.lock().unwrap();
            let it = items.iter().find(|d| d.id == item_id);
            let fname = it.map(|d| d.file_name.clone()).unwrap_or_default();
            let pname = it.map(|d| d.proxy_name.clone()).unwrap_or_default();
            let resolved = match it {
                Some(itm) if !itm.proxy_name.is_empty() => {
                    settings.proxies.iter().find(|p| p.name == itm.proxy_name).cloned()
                }
                _ => {
                    if !settings.default_proxy.is_empty() {
                        settings.proxies.iter().find(|p| p.name == settings.default_proxy).cloned()
                    } else {
                        None
                    }
                }
            };
            let plog = match &resolved {
                Some(p) => format!("{}://{}:{}", p.protocol.scheme(), p.host, p.port),
                None => "none".to_string(),
            };
            (fname, pname, plog, resolved)
        };

        log_info!("Item#{} file=\"{}\" proxy={} (name={}) max_conn={}",
            item_id, item_file_name, proxy_for_log, item_proxy_name, connections);

        let client = match build_client(resolved_proxy.as_ref(), &settings.user_agent) {
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

        // Check if item has existing parts (for resume)
        let existing_parts: Vec<DownloadPart> = {
            let items = state.lock().unwrap();
            items.iter()
                .find(|d| d.id == item_id)
                .map(|d| d.parts.clone())
                .unwrap_or_default()
        };

        log_info!("Item#{} probe result: size={}, range={}, parts={} (max_conn={})",
            item_id, total_size, supports_range,
            if supports_range && total_size > 1024*1024 {
                (total_size / (1024*1024)).max(1).min(connections as u64)
            } else { 1 },
            connections);

        // Calculate parts
        let mut parts: Vec<DownloadPart> = Vec::new();
        let num_parts = if supports_range && total_size > 1024 * 1024 {
            ((total_size / (1024 * 1024)).max(1).min(connections as u64)) as u32
        } else {
            1
        };

        let mut pre_completed: u32 = 0;
        if num_parts > 1 && total_size > 0 {
            let part_size = total_size / num_parts as u64;
            for i in 0..num_parts {
                let start = i as u64 * part_size;
                let end = if i == num_parts - 1 {
                    total_size - 1
                } else {
                    (i as u64 + 1) * part_size - 1
                };
                // Resume: carry over downloaded from existing part if match
                let old_downloaded = existing_parts.iter()
                    .find(|p| p.index == i)
                    .map(|p| p.downloaded)
                    .unwrap_or(0);
                let part_size_bytes = end - start + 1;
                let is_already_done = old_downloaded >= part_size_bytes;
                if is_already_done {
                    pre_completed += 1;
                }
                parts.push(DownloadPart {
                    index: i,
                    start,
                    end,
                    downloaded: old_downloaded,
                    temp_path: part_temp_path(&save_path, i),
                    status: if is_already_done { PartStatus::Completed } else { PartStatus::Pending },
                    retries: 0,
                });
            }
        } else {
            let downloaded = existing_parts.first()
                .map(|p| p.downloaded)
                .unwrap_or(0);
            let saved_total = existing_parts.first()
                .map(|p| if p.end > 0 { p.end + 1 } else { 0 })
                .unwrap_or(0);
            let real_total = if total_size > 0 { total_size } else { saved_total };
            let is_already_done = downloaded >= real_total && real_total > 0;
            if is_already_done {
                pre_completed += 1;
            }
            parts.push(DownloadPart {
                index: 0,
                start: 0,
                end: if real_total > 0 { real_total - 1 } else { 0 },
                downloaded,
                temp_path: part_temp_path(&save_path, 0),
                status: if is_already_done { PartStatus::Completed } else { PartStatus::Pending },
                retries: 0,
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
                item.resumable = Some(supports_range);
                item.last_try = now_str();
            }
        }

        // ── Seed the counter with pre-completed parts ──
        if pre_completed > 0 {
            completed_counter.store(pre_completed, Ordering::Relaxed);
        }

        // ── Spawn a thread for each part that is NOT already completed ──
        for i in 0..num_parts as usize {
            if parts[i].status == PartStatus::Completed {
                // Already fully downloaded from previous session — skip thread
                log_info!("Part #{} already fully completed, skipping thread", i);
                continue;
            }
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
        let mut last_progress = std::time::Instant::now();
        let mut last_dl = 0u64;
        loop {
            std::thread::sleep(std::time::Duration::from_millis(500));

            let _completed = completed_counter.load(Ordering::Relaxed);

            // Defensive: also check if all parts are in terminal state
            let (mut should_proceed, _any_failed, current_dl, current_total) = {
                let items = state.lock().unwrap();
                match items.iter().find(|d| d.id == item_id) {
                    Some(it) => {
                        // Only proceed to merge when ALL parts reached a terminal state
                        // (Completed or Failed). The `completed >= total_parts` counter
                        // check was removed because on resume the counter can be
                        // inflated by pre-seeded values, triggering a premature exit.
                        // Stall detection below handles any edge cases.
                        let terminal = it.parts.iter().all(|p| matches!(p.status, PartStatus::Completed | PartStatus::Failed(_)));
                        let any_failed = it.parts.iter().any(|p| matches!(p.status, PartStatus::Failed(_)));
                        (terminal, any_failed, it.downloaded, it.total_size)
                    }
                    None => (false, false, 0, 0),
                }
            };

            // Track progress: reset stall timer when downloaded bytes increase
            if current_dl != last_dl {
                last_progress = std::time::Instant::now();
                last_dl = current_dl;
            }

            if !should_proceed {
                let stalled_secs = last_progress.elapsed().as_secs();

                // Stalled for 5s+ with data complete → force merge
                if stalled_secs >= 5 && current_total > 0 && current_dl >= current_total {
                    log_info!("Item#{} file=\"{}\" stalled {}s, dl={} total={} — forcing merge",
                        item_id, item_file_name, stalled_secs, current_dl, current_total);
                    should_proceed = true;
                }
                // Stalled for 5s+ with unknown size but got data → force merge
                else if stalled_secs >= 5 && current_total == 0 && current_dl > 0 {
                    log_info!("Item#{} file=\"{}\" stalled {}s, dl={} (unknown size) — forcing merge",
                        item_id, item_file_name, stalled_secs, current_dl);
                    should_proceed = true;
                }
                // Stalled for 15s+ with no data or incomplete → fail
                else if stalled_secs >= 15 {
                    log_info!("Item#{} file=\"{}\" TIMEOUT stalled {}s — Failed (dl={} total={})",
                        item_id, item_file_name, stalled_secs, current_dl, current_total);
                    let mut items = state.lock().unwrap();
                    if let Some(item) = items.iter_mut().find(|d| d.id == item_id) {
                        item.status = DownloadStatus::Failed("Download timed out".to_string());
                        item.last_try = now_str();
                    }
                    return;
                }
            }

            if should_proceed {
                let mut items = state.lock().unwrap();
                if let Some(item) = items.iter_mut().find(|d| d.id == item_id) {
                    let all_ok = item.parts.iter().all(|p| p.status == PartStatus::Completed);
                    if all_ok {
                        // ── All parts Completed → merge ──
                        item.status = DownloadStatus::Downloading; // transitioning
                        item.merge_progress = 0.01; // signal merging started
                        drop(items);

                        log_info!("Item#{} file=\"{}\" all {} parts done, starting merge...",
                            item_id, item_file_name, total_parts);

                        let item_snapshot = {
                            let items2 = state.lock().unwrap();
                            items2.iter().find(|d| d.id == item_id).cloned()
                        };

                        match item_snapshot {
                            Some(ref snap) => match merge_parts(snap, &state, item_id) {
                                Ok(()) => {
                                    let mut items3 = state.lock().unwrap();
                                    if let Some(item3) = items3.iter_mut().find(|d| d.id == item_id) {
                                        item3.merge_progress = 0.0;
                                        item3.status = DownloadStatus::Completed;
                                        item3.downloaded = item3.total_size;
                                        item3.last_try = now_str();
                                        item3.parts.clear();
                                    }
                                    log_info!("Item#{} file=\"{}\" merge OK → Completed",
                                        item_id, item_file_name);
                                }
                                Err(e) => {
                                    let mut items3 = state.lock().unwrap();
                                    if let Some(item3) = items3.iter_mut().find(|d| d.id == item_id) {
                                        item3.merge_progress = 0.0;
                                        item3.status = DownloadStatus::Failed(format!("Merge: {}", e));
                                        item3.last_try = now_str();
                                    }
                                    log_info!("Item#{} file=\"{}\" merge FAILED: {}",
                                        item_id, item_file_name, e);
                                }
                            },
                            None => {}
                        }
                        return; // coordinator done, exit thread
                    } else {
                        // Some parts are not Completed yet.
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
                            drop(items);
                            log_info!("Item#{} file=\"{}\" failed ({} failed parts)",
                                item_id, item_file_name, failed.len());
                            return;
                        }
                        // Parts still downloading or pending — keep monitoring
                        log_info!("Item#{} file=\"{}\" should_proceed but {} parts not Completed — staying in loop",
                            item_id, item_file_name,
                            total_parts - item.parts.iter().filter(|p| p.status == PartStatus::Completed).count() as u32);
                    }
                } else {
                    // Item not found in state (deleted)
                    return;
                }
                // Don't return — continue monitoring loop
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
