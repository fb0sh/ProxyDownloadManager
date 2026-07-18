use serde::{Deserialize, Serialize};

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
}
