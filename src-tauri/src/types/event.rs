use crate::types::PdmError;

/// One report from a download engine.
#[derive(Debug, Clone, PartialEq)]
pub struct Event {
    pub kind: EventKind,
    pub download_id: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub enum EventKind {
    DownloadStarted,
    /// Bytes on disk: the total, and each planned Part's share of it.
    /// `reset_to_single` collapses the Progress Map to one cell. The Single
    /// engine always reports this way, and a degrade announces itself with it.
    DownloadProgress {
        downloaded: u64,
        parts: Vec<u64>,
        reset_to_single: bool,
    },
    /// HLS counts segments, and only learns how many there are from the
    /// playlist.
    SegmentProgress {
        done: u64,
        total: u64,
        phase: Phase,
    },
    /// What the engine is doing while the byte count stands still.
    PhaseChanged(Phase),
    DownloadCompleted,
    DownloadErrored(PdmError),
}

impl EventKind {
    /// The variant without its payload, for the log.
    pub fn name(&self) -> &'static str {
        match self {
            Self::DownloadStarted => "DownloadStarted",
            Self::DownloadProgress { .. } => "DownloadProgress",
            Self::SegmentProgress { .. } => "SegmentProgress",
            Self::PhaseChanged(_) => "PhaseChanged",
            Self::DownloadCompleted => "DownloadCompleted",
            Self::DownloadErrored(_) => "DownloadErrored",
        }
    }
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
