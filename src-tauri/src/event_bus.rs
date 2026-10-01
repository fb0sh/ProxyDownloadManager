use serde::Serialize;
use tauri::{AppHandle, Emitter};

// Include auto-generated event constants from events.json
include!(concat!(env!("OUT_DIR"), "/generated_events.rs"));

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
            Self::BrowserDownloadUrl => BROWSER_DOWNLOAD_URL,
            Self::DownloadCreated => DOWNLOAD_CREATED,
            Self::DownloadStarted => DOWNLOAD_STARTED,
            Self::DownloadProgress => DOWNLOAD_PROGRESS,
            Self::DownloadCompleted => DOWNLOAD_COMPLETED,
            Self::DownloadError => DOWNLOAD_ERROR,
            Self::DownloadPaused => DOWNLOAD_PAUSED,
            Self::DownloadResumed => DOWNLOAD_RESUMED,
            Self::DownloadCancelled => DOWNLOAD_CANCELLED,
        }
    }
}

/// Centralized event emission. All frontend-bound events MUST go through this.
/// The bus only knows a sink: Tauri's emitter in the app, a recorder in tests.
pub struct EventBus {
    sink: Box<dyn Fn(&'static str, serde_json::Value) + Send + Sync>,
}

impl EventBus {
    pub fn new(app_handle: AppHandle) -> Self {
        Self::with_sink(move |name, payload| {
            let _ = app_handle.emit(name, payload);
        })
    }

    pub fn with_sink(
        sink: impl Fn(&'static str, serde_json::Value) + Send + Sync + 'static,
    ) -> Self {
        Self {
            sink: Box::new(sink),
        }
    }

    pub fn emit(&self, event: FrontendEvent, payload: impl Serialize) {
        if let Ok(payload) = serde_json::to_value(payload) {
            (self.sink)(event.name(), payload);
        }
    }
}
