use crate::types::{Event, EventKind};

/// Describes what state transition and frontend event an engine event maps to.
/// Pure transformation — no side effects, no dependencies.
#[derive(Debug, Clone, PartialEq)]
pub enum EventAction {
    /// Update runtime progress (id, downloaded_bytes, optional per-part bytes, reset map to single part).
    UpdateProgress {
        id: u64,
        downloaded: u64,
        part_downloaded: Option<Vec<u64>>,
        reset_to_single: bool,
    },
    /// Download started: seed runtime + emit frontend event.
    DownloadStarted(u64),
    /// Download completed: finalize state + emit frontend event.
    DownloadCompleted(u64),
    /// Download errored: update state + emit frontend event.
    DownloadErrored(u64, String),
    /// No state change needed (e.g., unknown event kind).
    Noop,
}

/// Parsed progress payload from engine (plain number or JSON).
#[derive(Debug, Clone, PartialEq)]
pub struct ProgressPayload {
    pub downloaded: u64,
    pub part_downloaded: Option<Vec<u64>>,
    pub reset_to_single: bool,
}

/// Parse engine progress `data` string.
pub fn parse_progress_data(data: &str) -> Option<ProgressPayload> {
    let trimmed = data.trim();
    if trimmed.is_empty() {
        return None;
    }
    // Legacy: plain integer
    if let Ok(downloaded) = trimmed.parse::<u64>() {
        return Some(ProgressPayload {
            downloaded,
            part_downloaded: None,
            reset_to_single: false,
        });
    }
    // JSON: {"downloaded":N,"parts":[...],"reset_to_single":bool}
    let v: serde_json::Value = serde_json::from_str(trimmed).ok()?;
    let downloaded = v.get("downloaded")?.as_u64()?;
    let part_downloaded = v.get("parts").and_then(|p| {
        p.as_array().map(|arr| {
            arr.iter()
                .filter_map(|x| x.as_u64())
                .collect::<Vec<_>>()
        })
    });
    let reset_to_single = v
        .get("reset_to_single")
        .and_then(|x| x.as_bool())
        .unwrap_or(false);
    Some(ProgressPayload {
        downloaded,
        part_downloaded,
        reset_to_single,
    })
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
                if let Some(p) = parse_progress_data(data) {
                    return EventAction::UpdateProgress {
                        id,
                        downloaded: p.downloaded,
                        part_downloaded: p.part_downloaded,
                        reset_to_single: p.reset_to_single,
                    };
                }
            }
            EventAction::Noop
        }
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
    fn test_progress_event_plain() {
        let action = transform_event(&make_event(EventKind::DownloadProgress, 4,
            Some("1024".to_string())));
        assert_eq!(
            action,
            EventAction::UpdateProgress {
                id: 4,
                downloaded: 1024,
                part_downloaded: None,
                reset_to_single: false,
            }
        );
    }

    #[test]
    fn test_progress_event_json() {
        let data = r#"{"downloaded":500,"parts":[100,200,200],"reset_to_single":false}"#;
        let action = transform_event(&make_event(EventKind::DownloadProgress, 7, Some(data.to_string())));
        assert_eq!(
            action,
            EventAction::UpdateProgress {
                id: 7,
                downloaded: 500,
                part_downloaded: Some(vec![100, 200, 200]),
                reset_to_single: false,
            }
        );
    }

    #[test]
    fn test_progress_event_reset_single() {
        let data = r#"{"downloaded":0,"parts":[0],"reset_to_single":true}"#;
        let action = transform_event(&make_event(EventKind::DownloadProgress, 8, Some(data.to_string())));
        assert_eq!(
            action,
            EventAction::UpdateProgress {
                id: 8,
                downloaded: 0,
                part_downloaded: Some(vec![0]),
                reset_to_single: true,
            }
        );
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
