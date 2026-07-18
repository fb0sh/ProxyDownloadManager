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
pub struct PendingDownloadRequest {
    pub url: String,
    pub filename: String,
    pub proxy_name: String,
    pub connections: u32,
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
}
