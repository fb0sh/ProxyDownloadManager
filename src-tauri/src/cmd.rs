use crate::types::*;
use crate::download_manager::DownloadManager;
use crate::event_bus::{EventBus, FrontendEvent};
use crate::icons::{IconCache, IconData};
use std::process::Command as StdCommand;
use std::sync::Arc;
use tauri::{Emitter, Manager, State};

pub struct AppState {
    pub dm: Arc<DownloadManager>,
    pub app_handle: tauri::AppHandle,
    pub bus: Arc<EventBus>,
}

// ── Tauri commands: thin adapters ──

#[tauri::command]
pub fn list_downloads(state: State<'_, Arc<AppState>>) -> Result<Vec<DownloadItem>, String> {
    state.dm.list_items().map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn start_download(
    state: State<'_, Arc<AppState>>,
    url: String,
    filename: String,
    save_path: String,
    proxy_name: String,
    connections: u32,
) -> Result<u64, String> {
    state.dm.start_download(url, filename, save_path, proxy_name, connections).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn redownload_download(
    state: State<'_, Arc<AppState>>,
    id: u64,
) -> Result<u64, String> {
    state.dm.redownload_download(id).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn pause_download(state: State<'_, Arc<AppState>>, id: u64) -> Result<(), String> {
    state.dm.pause_download(id).await.map_err(|e| e.to_string())?;
    state.bus.emit(FrontendEvent::DownloadPaused, serde_json::json!({ "id": id }));
    Ok(())
}

#[tauri::command]
pub async fn resume_download(state: State<'_, Arc<AppState>>, id: u64) -> Result<(), String> {
    state.dm.resume_download(id).await.map_err(|e| e.to_string())?;
    state.bus.emit(FrontendEvent::DownloadResumed, serde_json::json!({ "id": id }));
    Ok(())
}

#[tauri::command]
pub async fn cancel_download(state: State<'_, Arc<AppState>>, id: u64) -> Result<(), String> {
    state.dm.cancel_download(id).await;
    state.bus.emit(FrontendEvent::DownloadCancelled, serde_json::json!({ "id": id }));
    Ok(())
}

#[tauri::command]
pub async fn delete_download(
    state: State<'_, Arc<AppState>>,
    id: u64,
    delete_file: bool,
) -> Result<(), String> {
    state.dm.delete_download(id, delete_file).await.map_err(|e| e.to_string())
}

// ── Settings ──

#[tauri::command]
pub fn get_settings(state: State<'_, Arc<AppState>>) -> Result<Settings, String> {
    Ok(state.dm.get_settings())
}

#[tauri::command]
pub fn save_settings(state: State<'_, Arc<AppState>>, settings: Settings) -> Result<(), String> {
    eprintln!("[ProxyDM] save_settings lang={} dl_dir={} max_conns={} tls_invalid={}",
        settings.language, settings.download_dir, settings.max_connections, settings.danger_accept_invalid_certs);
    state.dm.log_info(&format!("Settings saved: language={} download_dir={}", settings.language, settings.download_dir));

    let flags = state.dm.save_settings(&settings).map_err(|e| e.to_string())?;

    if let Err(e) = crate::platform::sync_autostart(&state.app_handle, flags.launch_at_startup, flags.silent_startup) {
        eprintln!("[ProxyDM] Failed to sync autostart: {}", e);
    }

    #[cfg(desktop)]
    if flags.shortcut_changed {
        use tauri_plugin_global_shortcut::GlobalShortcutExt;
        let app = &state.app_handle;
        if !flags.old_shortcut.is_empty() {
            let _ = app.global_shortcut().unregister(flags.old_shortcut.as_str());
        }
        if !flags.new_shortcut.is_empty() {
            if let Err(e) = app.global_shortcut().register(flags.new_shortcut.as_str()) {
                eprintln!("[ProxyDM] Failed to update global shortcut: {}", e);
            }
        }
    }
    Ok(())
}

// ── Utility commands ──

#[tauri::command]
pub fn exit_app(app: tauri::AppHandle) {
    eprintln!("[ProxyDM] exit_app called");
    app.exit(0);
}

#[tauri::command]
pub fn read_logs(max_lines: Option<usize>) -> Result<Vec<String>, String> {
    crate::log::read_logs(max_lines.unwrap_or(30))
}

#[tauri::command]
pub fn file_exists(path: String) -> bool {
    std::path::Path::new(&path).exists()
}

#[tauri::command]
pub fn get_file_icon(
    icon_cache: State<'_, IconCache>,
    file_name: String,
) -> IconData {
    icon_cache.get(&file_name)
}

#[tauri::command]
pub fn open_file(path: String) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    let status = StdCommand::new("open").arg(&path).status();
    #[cfg(target_os = "windows")]
    let status = StdCommand::new("cmd").args(["/c", "start", "", &path]).status();
    #[cfg(target_os = "linux")]
    let status = StdCommand::new("xdg-open").arg(&path).status();

    match status {
        Ok(s) if s.success() => Ok(()),
        Ok(s) => Err(format!("exit code: {}", s)),
        Err(e) => Err(e.to_string()),
    }
}

#[tauri::command]
pub fn open_extensions_folder(app: tauri::AppHandle) -> Result<(), String> {
    let ext_dir = crate::platform::resolve_extensions_dir(&app)?;

    #[cfg(target_os = "macos")]
    let status = StdCommand::new("open").arg(&ext_dir).status();
    #[cfg(target_os = "windows")]
    let status = StdCommand::new("explorer").arg(&ext_dir).status();
    #[cfg(target_os = "linux")]
    let status = StdCommand::new("xdg-open").arg(&ext_dir).status();

    match status {
        Ok(s) if s.success() => Ok(()),
        Ok(s) => Err(format!("exit code: {}", s)),
        Err(e) => Err(e.to_string()),
    }
}

#[tauri::command]
pub fn get_extensions_dir(app: tauri::AppHandle) -> Result<String, String> {
    let result = crate::platform::resolve_extensions_dir(&app);
    match &result {
        Ok(p) => eprintln!("[ProxyDM] get_extensions_dir -> {}", p),
        Err(e) => eprintln!("[ProxyDM] get_extensions_dir ERROR: {}", e),
    }
    result
}

#[tauri::command]
pub async fn test_proxy(
    state: State<'_, Arc<AppState>>,
    proxy_name: String,
) -> Result<serde_json::Value, String> {
    state.dm.test_proxy(&proxy_name).await.map_err(|e| e.to_string())
}
