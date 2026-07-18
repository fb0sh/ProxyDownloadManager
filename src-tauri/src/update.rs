use crate::cmd::AppState;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tauri::State;

#[derive(Serialize)]
pub struct AssetInfo {
    pub name: String,
    pub url: String,
    pub recommended: bool,
}

#[derive(Serialize)]
pub struct UpdateInfo {
    pub latest_version: String,
    pub current_version: String,
    pub has_update: bool,
    pub release_url: String,
    pub release_notes: String,
    pub assets: Vec<AssetInfo>,
}

#[derive(Deserialize)]
pub(crate) struct GithubRelease {
    pub tag_name: String,
    pub html_url: String,
    pub body: Option<String>,
    pub assets: Vec<GithubAsset>,
}

#[derive(Deserialize)]
pub(crate) struct GithubAsset {
    pub name: String,
    pub browser_download_url: String,
}

pub fn compare_versions(a: &str, b: &str) -> i32 {
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

pub fn current_platform_suffix() -> &'static str {
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
