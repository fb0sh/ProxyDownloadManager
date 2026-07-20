use crate::types::{Event, EventKind, PdmResult};

/// Describes what state transition and frontend event an engine event maps to.
/// Pure transformation — no side effects, no dependencies.
#[derive(Debug, Clone, PartialEq)]
pub enum EventAction {
    /// Update runtime progress (id, downloaded_bytes).
    UpdateProgress(u64, u64),
    /// Download started: seed runtime + emit frontend event.
    DownloadStarted(u64),
    /// Download completed: finalize state + emit frontend event.
    DownloadCompleted(u64),
    /// Download errored: update state + emit frontend event.
    DownloadErrored(u64, String),
    /// No state change needed (e.g., unknown event kind).
    Noop,
}

/// Transform an engine event into a structured action.
/// Pure: no I/O, no side effects, trivially testable.
pub fn transform_event(event: &Event) -> EventAction {
    let id = event.download_id;

    match event.kind {
        EventKind::DownloadStarted => EventAction::DownloadStarted(id),
        EventKind::DownloadCompleted => EventAction::DownloadCompleted(id),
        EventKind::DownloadErrored => {
            let msg = event.data.clone().unwrap_or_default();
            EventAction::DownloadErrored(id, msg)
        }
        EventKind::DownloadProgress => {
            if let Some(ref data) = event.data {
                if let Ok(downloaded) = data.parse::<u64>() {
                    return EventAction::UpdateProgress(id, downloaded);
                }
            }
            EventAction::Noop
        }
        _ => EventAction::Noop,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::EventKind;

    fn make_event(kind: EventKind, id: u64, data: Option<String>) -> Event {
        Event {
            kind,
            download_id: id,
            data,
        }
    }

    #[test]
    fn test_started_event() {
        let action = transform_event(&make_event(EventKind::DownloadStarted, 1, None));
        assert_eq!(action, EventAction::DownloadStarted(1));
    }

    #[test]
    fn test_completed_event() {
        let action = transform_event(&make_event(EventKind::DownloadCompleted, 2, None));
        assert_eq!(action, EventAction::DownloadCompleted(2));
    }

    #[test]
    fn test_errored_event() {
        let action = transform_event(&make_event(EventKind::DownloadErrored, 3,
            Some("timeout".to_string())));
        assert_eq!(action, EventAction::DownloadErrored(3, "timeout".to_string()));
    }

    #[test]
    fn test_progress_event() {
        let action = transform_event(&make_event(EventKind::DownloadProgress, 4,
            Some("1024".to_string())));
        assert_eq!(action, EventAction::UpdateProgress(4, 1024));
    }

    #[test]
    fn test_progress_bad_data() {
        let action = transform_event(&make_event(EventKind::DownloadProgress, 5,
            Some("not-a-number".to_string())));
        assert_eq!(action, EventAction::Noop);
    }

    #[test]
    fn test_unknown_event_is_noop() {
        // EventKind doesn't have other variants, but future-proof the test
        assert_eq!(transform_event(&make_event(EventKind::DownloadStarted, 99, None)),
            EventAction::DownloadStarted(99));
    }
}
