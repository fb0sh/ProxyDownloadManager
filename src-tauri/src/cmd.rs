use crate::types::*;
use crate::download_manager::DownloadManager;
use crate::icons::{IconCache, IconData};
use auto_launch::AutoLaunchBuilder;
use std::process::Command as StdCommand;
use std::sync::Arc;
use tauri::{Emitter, Manager, State};

pub struct AppState {
    pub dm: Arc<DownloadManager>,
    pub app_handle: tauri::AppHandle,
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
    let _ = state.app_handle.emit("download-paused", serde_json::json!({ "id": id }));
    Ok(())
}

#[tauri::command]
pub async fn resume_download(state: State<'_, Arc<AppState>>, id: u64) -> Result<(), String> {
    state.dm.resume_download(id).await.map_err(|e| e.to_string())?;
    let _ = state.app_handle.emit("download-resumed", serde_json::json!({ "id": id }));
    Ok(())
}

#[tauri::command]
pub async fn cancel_download(state: State<'_, Arc<AppState>>, id: u64) -> Result<(), String> {
    state.dm.cancel_download(id).await;
    let _ = state.app_handle.emit("download-cancelled", serde_json::json!({ "id": id }));
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

    if let Err(e) = sync_autostart(&state.app_handle, flags.launch_at_startup, flags.silent_startup) {
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
    let ext_dir = resolve_extensions_dir(&app)?;

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
    let result = resolve_extensions_dir(&app);
    match &result {
        Ok(p) => eprintln!("[ProxyDM] get_extensions_dir -> {}", p),
        Err(e) => eprintln!("[ProxyDM] get_extensions_dir ERROR: {}", e),
    }
    result
}

// ── Update check ──

#[derive(serde::Serialize)]
pub struct AssetInfo {
    pub name: String,
    pub url: String,
    pub recommended: bool,
}

#[derive(serde::Serialize)]
pub struct UpdateInfo {
    pub latest_version: String,
    pub current_version: String,
    pub has_update: bool,
    pub release_url: String,
    pub release_notes: String,
    pub assets: Vec<AssetInfo>,
}

#[derive(serde::Deserialize)]
struct GithubRelease {
    tag_name: String,
    html_url: String,
    body: Option<String>,
    assets: Vec<GithubAsset>,
}

#[derive(serde::Deserialize)]
struct GithubAsset {
    name: String,
    browser_download_url: String,
}

#[tauri::command]
pub async fn check_update(
    state: State<'_, Arc<AppState>>,
    proxy_name: String,
) -> Result<UpdateInfo, String> {
    let release_value: serde_json::Value = state.dm.check_update(&proxy_name).await.map_err(|e| e.to_string())?;
    let release: GithubRelease = serde_json::from_value(release_value)
        .map_err(|e| format!("Failed to parse release info: {}", e))?;

    let current_version = format!("v{}", env!("CARGO_PKG_VERSION"));
    let latest_version = release.tag_name.clone();
    let has_update = compare_versions(&latest_version, &current_version) > 0;

    let platform_suffix = current_platform_suffix();
    let assets: Vec<AssetInfo> = release.assets.into_iter().map(|a| {
        let recommended = a.name.ends_with(platform_suffix);
        AssetInfo {
            name: a.name,
            url: a.browser_download_url,
            recommended,
        }
    }).collect();

    Ok(UpdateInfo {
        latest_version,
        current_version,
        has_update,
        release_url: release.html_url,
        release_notes: release.body.unwrap_or_default(),
        assets,
    })
}

#[tauri::command]
pub async fn test_proxy(
    state: State<'_, Arc<AppState>>,
    proxy_name: String,
) -> Result<serde_json::Value, String> {
    state.dm.test_proxy(&proxy_name).await.map_err(|e| e.to_string())
}

// ── Internal helpers ──

fn sync_autostart(
    app: &tauri::AppHandle,
    launch_at_startup: bool,
    silent_startup: bool,
) -> Result<(), String> {
    let mut builder = AutoLaunchBuilder::new();
    builder.set_app_name(&app.package_info().name);

    let args = if silent_startup {
        vec![crate::SILENT_START_ARG]
    } else {
        Vec::new()
    };
    builder.set_args(&args);

    let current_exe = std::env::current_exe().map_err(|e| e.to_string())?;

    #[cfg(target_os = "linux")]
    {
        if let Some(appimage) = app.env().appimage.and_then(|p| p.to_str().map(|s| s.to_string())) {
            builder.set_app_path(&appimage);
        } else {
            builder.set_app_path(&current_exe.display().to_string());
        }
    }

    #[cfg(target_os = "macos")]
    {
        builder.set_use_launch_agent(true);
        let exe_path = current_exe
            .canonicalize()
            .map_err(|e| e.to_string())?
            .display()
            .to_string();
        builder.set_app_path(&exe_path);
    }

    #[cfg(target_os = "windows")]
    builder.set_app_path(&current_exe.display().to_string());

    let autostart = builder.build().map_err(|e| e.to_string())?;
    if launch_at_startup {
        autostart.enable().map_err(|e| e.to_string())
    } else {
        autostart.disable().map_err(|e| e.to_string())
    }
}

fn current_platform_suffix() -> &'static str {
    #[cfg(target_os = "macos")]
    { return ".dmg"; }
    #[cfg(target_os = "windows")]
    { return ".exe"; }
    #[cfg(target_os = "linux")]
    {
        if std::path::Path::new("/usr/bin/apt").exists()
            || std::path::Path::new("/usr/bin/apt-get").exists()
        {
            return ".deb";
        }
        if std::path::Path::new("/usr/bin/rpm").exists() {
            return ".rpm";
        }
        ".AppImage"
    }
}

fn compare_versions(a: &str, b: &str) -> i32 {
    let a = a.strip_prefix('v').unwrap_or(a);
    let b = b.strip_prefix('v').unwrap_or(b);
    let a_parts: Vec<u32> = a.split('.').filter_map(|s| s.parse().ok()).collect();
    let b_parts: Vec<u32> = b.split('.').filter_map(|s| s.parse().ok()).collect();
    let max_len = a_parts.len().max(b_parts.len());
    for i in 0..max_len {
        let a_val = a_parts.get(i).copied().unwrap_or(0);
        let b_val = b_parts.get(i).copied().unwrap_or(0);
        if a_val > b_val { return 1; }
        if a_val < b_val { return -1; }
    }
    0
}

pub(crate) fn deploy_extensions(app: &tauri::AppHandle) -> Result<String, String> {
    let src_dir = if let Ok(resource_dir) = app.path().resource_dir() {
        let ext_dir = resource_dir.join("extensions");
        if ext_dir.exists() {
            ext_dir
        } else {
            let dev = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .parent()
                .map(|p| p.join("browsers-extension"));
            match dev {
                Some(p) if p.exists() => p,
                _ => return Err("Extensions source directory not found".to_string()),
            }
        }
    } else {
        return Err("Cannot resolve resource directory".to_string());
    };

    #[cfg(target_os = "macos")]
    {
        let app_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
        let target_dir = app_dir.join("extensions");

        if !target_dir.exists() {
            std::fs::create_dir_all(&target_dir).map_err(|e| e.to_string())?;

            for entry in std::fs::read_dir(&src_dir).map_err(|e| e.to_string())? {
                let entry = entry.map_err(|e| e.to_string())?;
                let name = entry.file_name();
                let src = entry.path();
                let dst = target_dir.join(&name);

                if src.is_dir() {
                    let _ = std::fs::remove_dir_all(&dst);
                    copy_dir_recursive(&src, &dst)?;
                } else {
                    let _ = std::fs::copy(&src, &dst);
                }
            }
        }

        return Ok(target_dir.to_string_lossy().to_string());
    }

    #[cfg(not(target_os = "macos"))]
    {
        Ok(src_dir.to_string_lossy().to_string())
    }
}

fn copy_dir_recursive(src: &std::path::Path, dst: &std::path::Path) -> Result<(), String> {
    std::fs::create_dir_all(dst).map_err(|e| e.to_string())?;
    for entry in std::fs::read_dir(src).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let ty = entry.file_type().map_err(|e| e.to_string())?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());
        if ty.is_dir() {
            copy_dir_recursive(&src_path, &dst_path)?;
        } else {
            let _ = std::fs::copy(&src_path, &dst_path);
        }
    }
    Ok(())
}

fn resolve_extensions_dir(app: &tauri::AppHandle) -> Result<String, String> {
    #[cfg(target_os = "macos")]
    {
        if let Ok(app_dir) = app.path().app_data_dir() {
            let ext_dir = app_dir.join("extensions");
            if ext_dir.exists() {
                return Ok(ext_dir.to_string_lossy().to_string());
            }
        }
        return deploy_extensions(app);
    }

    #[cfg(not(target_os = "macos"))]
    {
        if let Ok(resource_dir) = app.path().resource_dir() {
            let ext_dir = resource_dir.join("extensions");
            if ext_dir.exists() {
                return Ok(ext_dir.to_string_lossy().to_string());
            }
        }

        let dev_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .map(|p| p.join("browsers-extension"));

        if let Some(path) = dev_path {
            if path.exists() {
                return Ok(path.to_string_lossy().to_string());
            }
        }

        Err("Extensions directory not found. The browser extensions may not have been bundled. Try reinstalling the application.".to_string())
    }
}
