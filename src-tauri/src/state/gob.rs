use crate::types::{DownloadState, PendingDownloadRequest, Task};
use std::path::PathBuf;

fn state_dir() -> PathBuf {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    home.join(".ProxyDM/state")
}

fn detail_path(id: u64) -> PathBuf {
    state_dir().join(format!("detail-{}.json", id))
}

fn pending_path() -> PathBuf {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    home.join(".ProxyDM/pending_new_download.json")
}

pub fn save_state(id: u64, state: &DownloadState) -> Result<(), String> {
    let path = detail_path(id);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let json = serde_json::to_string_pretty(state).map_err(|e| e.to_string())?;
    std::fs::write(&path, json).map_err(|e| e.to_string())?;
    Ok(())
}

pub fn load_state(id: u64) -> Result<Option<DownloadState>, String> {
    let path = detail_path(id);
    if !path.exists() {
        return Ok(None);
    }
    let json = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
    let state: DownloadState = serde_json::from_str(&json).map_err(|e| e.to_string())?;
    Ok(Some(state))
}

pub fn delete_state(id: u64) -> Result<(), String> {
    let path = detail_path(id);
    if path.exists() {
        std::fs::remove_file(&path).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Write pending download request for IPC between main and new-download-window processes.
/// Destructive read: reads then deletes the file.
pub fn write_pending_request(req: &PendingDownloadRequest) -> Result<(), String> {
    let path = pending_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let json = serde_json::to_string_pretty(req).map_err(|e| e.to_string())?;
    std::fs::write(&path, json).map_err(|e| e.to_string())?;
    Ok(())
}

pub fn take_pending_request() -> Result<Option<PendingDownloadRequest>, String> {
    let path = pending_path();
    if !path.exists() {
        return Ok(None);
    }
    let json = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
    std::fs::remove_file(&path).ok();
    let req: PendingDownloadRequest = serde_json::from_str(&json).map_err(|e| e.to_string())?;
    Ok(Some(req))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_dir() -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("pdm_gob_{}", std::process::id()));
        std::fs::create_dir_all(&dir).ok();
        dir
    }

    fn sample_state(id: u64) -> DownloadState {
        DownloadState {
            url: format!("https://example.com/file{}.zip", id),
            id,
            file_name: format!("file{}.zip", id),
            save_path: format!("/tmp/file{}.zip", id),
            total_size: 1000,
            downloaded: 500,
            tasks: vec![Task { offset: 500, length: 500 }],
            proxy_name: "".to_string(),
            workers: 4,
        }
    }

    #[test]
    fn test_save_and_load_state() {
        let state = sample_state(1);
        save_state(1, &state).unwrap();
        let loaded = load_state(1).unwrap().unwrap();
        assert_eq!(loaded.id, 1);
        assert_eq!(loaded.total_size, 1000);
        assert_eq!(loaded.downloaded, 500);
        delete_state(1).unwrap();
    }

    #[test]
    fn test_load_nonexistent() {
        let r = load_state(9999).unwrap();
        assert!(r.is_none());
    }

    #[test]
    fn test_delete_nonexistent() {
        delete_state(9999).unwrap();
    }

    #[test]
    fn test_pending_request_roundtrip() {
        let req = PendingDownloadRequest {
            url: "https://example.com/file.zip".to_string(),
            filename: "file.zip".to_string(),
            proxy_name: "my-proxy".to_string(),
            connections: 8,
        };
        write_pending_request(&req).unwrap();
        let taken = take_pending_request().unwrap().unwrap();
        assert_eq!(taken.url, "https://example.com/file.zip");
        assert_eq!(taken.connections, 8);
        // Second read should return None
        let again = take_pending_request().unwrap();
        assert!(again.is_none());
    }
}
