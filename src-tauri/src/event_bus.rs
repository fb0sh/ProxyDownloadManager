use serde::Serialize;
use tauri::{AppHandle, Emitter};

/// All frontend-facing events. Each variant maps to a specific Tauri event name.
#[derive(Debug, Clone, Copy)]
pub enum FrontendEvent {
    BrowserDownloadUrl,
    DownloadCreated,
    DownloadStarted,
    DownloadProgress,
    DownloadCompleted,
    DownloadError,
    DownloadPaused,
    DownloadResumed,
    DownloadCancelled,
}

impl FrontendEvent {
    pub fn name(&self) -> &'static str {
        match self {
            Self::BrowserDownloadUrl => "browser-download-url",
            Self::DownloadCreated => "download-created",
            Self::DownloadStarted => "download-started",
            Self::DownloadProgress => "download-progress",
            Self::DownloadCompleted => "download-completed",
            Self::DownloadError => "download-error",
            Self::DownloadPaused => "download-paused",
            Self::DownloadResumed => "download-resumed",
            Self::DownloadCancelled => "download-cancelled",
        }
    }
}

/// Centralized event emission. All frontend-bound events MUST go through this.
pub struct EventBus {
    app_handle: AppHandle,
}

impl EventBus {
    pub fn new(app_handle: AppHandle) -> Self {
        Self { app_handle }
    }

    pub fn emit(&self, event: FrontendEvent, payload: impl Serialize + Clone) {
        let _ = self.app_handle.emit(event.name(), payload);
    }
}
