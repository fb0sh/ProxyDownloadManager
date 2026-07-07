use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadItem {
    pub id: u64,
    pub url: String,
    pub file_name: String,
    pub save_path: String,
    pub total_size: u64,
    pub downloaded: u64,
    pub status: DownloadStatus,
    pub parts: Vec<DownloadPart>,
    pub proxy_name: String,
    pub connections: u32,
    pub resumable: Option<bool>,
    pub merge_progress: f64,
    pub created_at: String,
    pub last_try: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadPart {
    pub index: u32,
    pub start: u64,
    pub end: u64,
    pub downloaded: u64,
    pub temp_path: String,
    pub status: PartStatus,
    pub retries: u32,
}

#[derive(Debug, Clone)]
pub enum DownloadStatus {
    Downloading,
    Paused,
    Completed,
    Failed(String),
    Queued,
}

impl Serialize for DownloadStatus {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let v = match self {
            DownloadStatus::Downloading => "downloading".to_string(),
            DownloadStatus::Paused => "paused".to_string(),
            DownloadStatus::Completed => "completed".to_string(),
            DownloadStatus::Failed(msg) => format!("failed:{}", msg),
            DownloadStatus::Queued => "queued".to_string(),
        };
        s.serialize_str(&v)
    }
}

impl<'de> Deserialize<'de> for DownloadStatus {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let v = String::deserialize(d)?;
        Ok(match v.as_str() {
            "downloading" => DownloadStatus::Downloading,
            "paused" => DownloadStatus::Paused,
            "completed" => DownloadStatus::Completed,
            "queued" => DownloadStatus::Queued,
            s if s.starts_with("failed:") => DownloadStatus::Failed(s[7..].to_string()),
            _ => DownloadStatus::Queued,
        })
    }
}

#[derive(Debug, Clone)]
pub enum PartStatus {
    Pending,
    Downloading,
    Completed,
    Failed(String),
}

impl Serialize for PartStatus {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let v = match self {
            PartStatus::Pending => "pending".to_string(),
            PartStatus::Downloading => "downloading".to_string(),
            PartStatus::Completed => "completed".to_string(),
            PartStatus::Failed(msg) => format!("failed:{}", msg),
        };
        s.serialize_str(&v)
    }
}

impl<'de> Deserialize<'de> for PartStatus {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let v = String::deserialize(d)?;
        Ok(match v.as_str() {
            "pending" => PartStatus::Pending,
            "downloading" => PartStatus::Downloading,
            "completed" => PartStatus::Completed,
            s if s.starts_with("failed:") => PartStatus::Failed(s[7..].to_string()),
            _ => PartStatus::Pending,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ProxyProtocol {
    #[serde(rename = "http")]
    Http,
    #[serde(rename = "socks5")]
    Socks5,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProxyConfig {
    pub protocol: ProxyProtocol,
    pub host: String,
    pub port: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadConfig {
    pub url: String,
    pub output_path: String,
    pub save_path: String,
    pub id: u64,
    pub file_name: String,
    pub is_resume: bool,
    pub headers: std::collections::HashMap<String, String>,
    pub proxy_name: String,
    pub total_size: u64,
    pub supports_range: bool,
    pub rate_limit_bps: u64,
    pub connections: u32,
    pub max_retries: u32,
    pub user_agent: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadState {
    pub url: String,
    pub id: u64,
    pub file_name: String,
    pub save_path: String,
    pub total_size: u64,
    pub downloaded: u64,
    pub tasks: Vec<Task>,
    pub elapsed_secs: u64,
    pub chunk_bitmap: Vec<bool>,
    pub actual_chunk_size: u64,
    pub proxy_name: String,
    pub workers: u32,
    pub min_chunk_size: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub offset: u64,
    pub length: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    pub download_dir: String,
    pub max_connections: u32,
    pub max_retries: u32,
    pub user_agent: String,
    pub launch_at_startup: bool,
    #[serde(default = "default_silent_startup")]
    pub silent_startup: bool,
    pub proxies: std::collections::HashMap<String, ProxyConfig>,
    pub global_rate_limit: u64,
    pub default_proxy: String,
    pub home_dir: String,
    pub language: String,
    #[serde(default)]
    pub danger_accept_invalid_certs: bool,
}

impl Default for Settings {
    fn default() -> Self {
        let home = dirs::home_dir().unwrap_or_else(|| std::path::PathBuf::from("."));
        Self {
            download_dir: dirs::download_dir()
                .unwrap_or_else(|| home.clone())
                .to_string_lossy()
                .to_string(),
            max_connections: 0, // 0 = auto
            max_retries: 10,
            user_agent: "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/150.0.0.0 Safari/537.36 Edg/150.0.0.0".to_string(),
            launch_at_startup: false,
            silent_startup: default_silent_startup(),
            proxies: std::collections::HashMap::new(),
            global_rate_limit: 0,
            default_proxy: String::new(),
            home_dir: home.join(".ProxyDM").to_string_lossy().to_string(),
            language: String::from("en"),
            danger_accept_invalid_certs: true,
        }
    }
}

fn default_silent_startup() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingDownloadRequest {
    pub url: String,
    pub filename: String,
    pub proxy_name: String,
    pub connections: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    pub kind: EventKind,
    pub download_id: u64,
    pub data: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EventKind {
    DownloadStarted,
    DownloadProgress,
    DownloadCompleted,
    DownloadPaused,
    DownloadResumed,
    DownloadErrored,
    DownloadRemoved,
    DownloadQueued,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_download_status_serde_downloading() {
        let json = serde_json::to_string(&DownloadStatus::Downloading).unwrap();
        assert_eq!(json, "\"downloading\"");
        let back: DownloadStatus = serde_json::from_str(&json).unwrap();
        assert!(matches!(back, DownloadStatus::Downloading));
    }

    #[test]
    fn test_download_status_serde_failed() {
        let s = DownloadStatus::Failed("connection refused".to_string());
        let json = serde_json::to_string(&s).unwrap();
        assert_eq!(json, "\"failed:connection refused\"");
        let back: DownloadStatus = serde_json::from_str(&json).unwrap();
        assert!(matches!(back, DownloadStatus::Failed(msg) if msg == "connection refused"));
    }

    #[test]
    fn test_download_status_serde_completed() {
        let json = serde_json::to_string(&DownloadStatus::Completed).unwrap();
        assert_eq!(json, "\"completed\"");
        let back: DownloadStatus = serde_json::from_str(&json).unwrap();
        assert!(matches!(back, DownloadStatus::Completed));
    }

    #[test]
    fn test_part_status_serde_failed() {
        let s = PartStatus::Failed("timeout".to_string());
        let json = serde_json::to_string(&s).unwrap();
        assert_eq!(json, "\"failed:timeout\"");
        let back: PartStatus = serde_json::from_str(&json).unwrap();
        assert!(matches!(back, PartStatus::Failed(msg) if msg == "timeout"));
    }

    #[test]
    fn test_settings_default() {
        let s = Settings::default();
        assert_eq!(s.max_connections, 0); // default is auto
        assert!(s.max_retries > 0);
        assert!(!s.download_dir.is_empty());
    }

    #[test]
    fn test_settings_serde_roundtrip() {
        let s = Settings::default();
        let json = serde_json::to_string(&s).unwrap();
        let back: Settings = serde_json::from_str(&json).unwrap();
        assert_eq!(s.max_connections, back.max_connections);
        assert_eq!(s.max_retries, back.max_retries);
    }

    #[test]
    fn test_event_serialize() {
        let e = Event {
            kind: EventKind::DownloadCompleted,
            download_id: 42,
            data: None,
        };
        let json = serde_json::to_string(&e).unwrap();
        assert!(json.contains("DownloadCompleted"));
        assert!(json.contains("42"));
    }

    #[test]
    fn test_pending_request() {
        let req = PendingDownloadRequest {
            url: "https://example.com/file.zip".to_string(),
            filename: "file.zip".to_string(),
            proxy_name: "".to_string(),
            connections: 4,
        };
        assert_eq!(req.url, "https://example.com/file.zip");
        assert_eq!(req.connections, 4);
    }
}
