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
}

/// What an engine is doing when the byte count alone does not say.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Downloading,
    Retrying,
    Merging,
}

impl Phase {
    /// The status word the frontend and the downloads table use.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Downloading => "downloading",
            Self::Retrying => "retrying",
            Self::Merging => "merging",
        }
    }
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
