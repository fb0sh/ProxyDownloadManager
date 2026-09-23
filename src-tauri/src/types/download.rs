use serde::{Deserialize, Serialize};
use std::collections::HashMap;

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
    #[serde(default)]
    pub headers: HashMap<String, String>,
    #[serde(default)]
    pub final_url: String,
    #[serde(default)]
    pub content_type: String,
    #[serde(default)]
    pub etag: String,
    #[serde(default)]
    pub last_modified: String,
    #[serde(default)]
    pub rate_limit_bps: u64,
    #[serde(default)]
    pub error_code: String,
    #[serde(default)]
    pub error_message: String,
    #[serde(default)]
    pub http_status: Option<u16>,
    #[serde(default)]
    pub retry_count: u32,
    #[serde(default)]
    pub last_error_at: String,
}

impl Default for DownloadItem {
    fn default() -> Self {
        Self {
            id: 0,
            url: String::new(),
            file_name: String::new(),
            save_path: String::new(),
            total_size: 0,
            downloaded: 0,
            status: DownloadStatus::Queued,
            parts: vec![],
            proxy_name: String::new(),
            connections: 0,
            resumable: None,
            created_at: String::new(),
            last_try: String::new(),
            headers: HashMap::new(),
            final_url: String::new(),
            content_type: String::new(),
            etag: String::new(),
            last_modified: String::new(),
            rate_limit_bps: 0,
            error_code: String::new(),
            error_message: String::new(),
            http_status: None,
            retry_count: 0,
            last_error_at: String::new(),
        }
    }
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
    Queued,
    Connecting,
    Downloading,
    Paused,
    Retrying,
    Merging,
    Completed,
    Failed(String),
}

impl DownloadStatus {
    pub fn is_live(&self) -> bool {
        matches!(
            self,
            Self::Downloading | Self::Connecting | Self::Retrying | Self::Merging
        )
    }
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
                    DownloadStatus::Connecting => "connecting",
                    DownloadStatus::Retrying => "retrying",
                    DownloadStatus::Merging => "merging",
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
                    "connecting" => DownloadStatus::Connecting,
                    "retrying" => DownloadStatus::Retrying,
                    "merging" => DownloadStatus::Merging,
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

/// Unix-seconds timestamp string used for `created_at` / `last_try`.
pub fn now_str() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let dur = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    format!("{}", dur.as_secs())
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PendingDownloadRequest {
    #[serde(default)]
    pub protocol_version: u32,
    #[serde(default)]
    pub request_id: String,
    #[serde(default)]
    pub action: String,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub final_url: String,
    #[serde(default)]
    pub filename: String,
    #[serde(default)]
    pub method: String,
    #[serde(default)]
    pub referrer: String,
    #[serde(default)]
    pub user_agent: String,
    #[serde(default)]
    pub cookies: String,
    #[serde(default)]
    pub headers: HashMap<String, String>,
    #[serde(default)]
    pub tab_url: String,
    #[serde(default)]
    pub content_type: String,
    #[serde(default)]
    pub content_length: u64,
    #[serde(default)]
    pub proxy_name: String,
    #[serde(default)]
    pub connections: u32,
}

impl PendingDownloadRequest {
    pub fn effective_url(&self) -> &str {
        if !self.final_url.is_empty() {
            &self.final_url
        } else {
            &self.url
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadAck {
    pub protocol_version: u32,
    pub request_id: String,
    pub accepted: bool,
    pub reason: String,
}

impl DownloadAck {
    pub fn ok(request_id: &str) -> Self {
        Self {
            protocol_version: 1,
            request_id: request_id.to_string(),
            accepted: true,
            reason: String::new(),
        }
    }

    pub fn fail(request_id: &str, reason: &str) -> Self {
        Self {
            protocol_version: 1,
            request_id: request_id.to_string(),
            accepted: false,
            reason: reason.to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProbeInfo {
    pub url: String,
    pub final_url: String,
    pub file_name: String,
    pub file_size: u64,
    pub content_type: String,
    pub supports_range: bool,
    pub etag: String,
    pub last_modified: String,
    pub suggested_connections: u32,
    pub is_hls: bool,
    pub hls_variants: Vec<HlsVariantInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HlsVariantInfo {
    pub uri: String,
    pub bandwidth: u64,
    pub resolution: String,
    pub codecs: String,
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
    fn test_pending_request() {
        let req = PendingDownloadRequest {
            url: "https://example.com/file.zip".to_string(),
            filename: "file.zip".to_string(),
            proxy_name: "".to_string(),
            connections: 4,
            ..Default::default()
        };
        assert_eq!(req.url, "https://example.com/file.zip");
        assert_eq!(req.connections, 4);
    }

    #[test]
    fn test_download_status_json_roundtrip() {
        let cases = vec![
            (DownloadStatus::Downloading, "\"downloading\""),
            (DownloadStatus::Paused, "\"paused\""),
            (DownloadStatus::Completed, "\"completed\""),
            (DownloadStatus::Queued, "\"queued\""),
            (DownloadStatus::Connecting, "\"connecting\""),
            (DownloadStatus::Retrying, "\"retrying\""),
            (DownloadStatus::Merging, "\"merging\""),
        ];
        for (status, expected_json) in &cases {
            let json = serde_json::to_string(status).unwrap();
            assert_eq!(json, *expected_json, "Serialization mismatch for {:?}", status);
            let back: DownloadStatus = serde_json::from_str(&json).unwrap();
            assert!(format!("{:?}", back) == format!("{:?}", status), "Deserialization mismatch for {:?}", status);
        }
        let failed = DownloadStatus::Failed("timeout".to_string());
        let json = serde_json::to_string(&failed).unwrap();
        assert_eq!(json, r#"{"failed":"timeout"}"#);
        let back: DownloadStatus = serde_json::from_str(&json).unwrap();
        assert!(matches!(back, DownloadStatus::Failed(msg) if msg == "timeout"));
    }

    #[test]
    fn live_statuses() {
        assert!(DownloadStatus::Downloading.is_live());
        assert!(DownloadStatus::Connecting.is_live());
        assert!(DownloadStatus::Retrying.is_live());
        assert!(DownloadStatus::Merging.is_live());
        assert!(!DownloadStatus::Paused.is_live());
        assert!(!DownloadStatus::Queued.is_live());
    }

    #[test]
    fn pending_request_old_json_still_parses() {
        let json = r#"{"url":"https://example.com/a.bin","filename":"a.bin","proxy_name":"","connections":4}"#;
        let req: PendingDownloadRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.url, "https://example.com/a.bin");
        assert_eq!(req.connections, 4);
        assert!(req.headers.is_empty());
    }
}
