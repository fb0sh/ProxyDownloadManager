pub mod engine_config;
pub use engine_config::EngineConfig;

use serde::{Deserialize, Serialize};
use std::fmt;

/// Structured error type for the ProxyDownloadManager domain.
#[derive(Debug, Clone, PartialEq)]
pub enum PdmError {
    /// Download was cancelled by the user.
    Cancelled,
    /// HTTP error with status code.
    Http(u16),
    /// Failed to build an HTTP client (invalid proxy, TLS error).
    ClientBuild(String),
    /// Probe failed (all user-agents exhausted).
    Probe(String),
    /// Download not found.
    NotFound(u64),
    /// Database error.
    Db(String),
    /// Configuration error (load, save, parse).
    Config(String),
    /// I/O error (file operations).
    Io(String),
    /// WebSocket / network error.
    Network(String),
    /// Generic fallback with message.
    Other(String),
}

impl fmt::Display for PdmError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Cancelled => write!(f, "Cancelled"),
            Self::Http(code) => write!(f, "HTTP {}", code),
            Self::ClientBuild(msg) => write!(f, "Client build failed: {}", msg),
            Self::Probe(msg) => write!(f, "Probe failed: {}", msg),
            Self::NotFound(id) => write!(f, "Download {} not found", id),
            Self::Db(msg) => write!(f, "Database error: {}", msg),
            Self::Config(msg) => write!(f, "Config error: {}", msg),
            Self::Io(msg) => write!(f, "I/O error: {}", msg),
            Self::Network(msg) => write!(f, "Network error: {}", msg),
            Self::Other(msg) => write!(f, "{}", msg),
        }
    }
}

impl std::error::Error for PdmError {}

impl From<reqwest::Error> for PdmError {
    fn from(e: reqwest::Error) -> Self {
        Self::Network(e.to_string())
    }
}

impl From<rusqlite::Error> for PdmError {
    fn from(e: rusqlite::Error) -> Self {
        Self::Db(e.to_string())
    }
}

impl From<std::io::Error> for PdmError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e.to_string())
    }
}

impl From<String> for PdmError {
    fn from(s: String) -> Self {
        Self::Other(s)
    }
}

impl From<&str> for PdmError {
    fn from(s: &str) -> Self {
        Self::Other(s.to_string())
    }
}

impl<T> From<std::sync::PoisonError<T>> for PdmError {
    fn from(_: std::sync::PoisonError<T>) -> Self {
        Self::Other("Mutex poisoned".to_string())
    }
}

impl From<serde_json::Error> for PdmError {
    fn from(e: serde_json::Error) -> Self {
        Self::Other(e.to_string())
    }
}

/// Convenience alias for domain results.
pub type PdmResult<T> = Result<T, PdmError>;

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
        use serde::ser::SerializeMap;
        match self {
            DownloadStatus::Failed(msg) => {
                let mut map = s.serialize_map(Some(1))?;
                map.serialize_entry("failed", msg)?;
                map.end()
            }
            other => {
                let v = match other {
                    DownloadStatus::Downloading => "downloading",
                    DownloadStatus::Paused => "paused",
                    DownloadStatus::Completed => "completed",
                    DownloadStatus::Queued => "queued",
                    DownloadStatus::Failed(_) => unreachable!(),
                };
                s.serialize_str(v)
            }
        }
    }
}

impl<'de> Deserialize<'de> for DownloadStatus {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        use serde::de;

        struct DownloadStatusVisitor;

        impl<'de> de::Visitor<'de> for DownloadStatusVisitor {
            type Value = DownloadStatus;

            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("a string or {\"failed\":\"message\"}")
            }

            fn visit_str<E: de::Error>(self, v: &str) -> Result<Self::Value, E> {
                Ok(match v {
                    "downloading" => DownloadStatus::Downloading,
                    "paused" => DownloadStatus::Paused,
                    "completed" => DownloadStatus::Completed,
                    "queued" => DownloadStatus::Queued,
                    s if s.starts_with("failed:") => DownloadStatus::Failed(s[7..].to_string()),
                    _ => DownloadStatus::Queued,
                })
            }

            fn visit_map<M: de::MapAccess<'de>>(self, mut map: M) -> Result<Self::Value, M::Error> {
                while let Some((key, value)) = map.next_entry::<String, String>()? {
                    if key == "failed" {
                        return Ok(DownloadStatus::Failed(value));
                    }
                }
                Ok(DownloadStatus::Queued)
            }
        }

        d.deserialize_any(DownloadStatusVisitor)
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
pub struct DownloadState {
    pub url: String,
    pub id: u64,
    pub file_name: String,
    pub save_path: String,
    pub total_size: u64,
    pub downloaded: u64,
    pub tasks: Vec<Task>,
    pub proxy_name: String,
    pub workers: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
    #[serde(default = "default_global_shortcut")]
    pub global_shortcut: String,
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
            global_shortcut: default_global_shortcut(),
        }
    }
}

fn default_silent_startup() -> bool {
    true
}

fn default_global_shortcut() -> String {
    "Ctrl+Super+J".to_string()
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
    DownloadErrored,
    DownloadRemoved,
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
        assert_eq!(json, r#"{"failed":"connection refused"}"#);
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
