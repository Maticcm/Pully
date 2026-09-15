//! Optional newline-delimited progress events supported by some SpotiFLAC
//! wrappers. The public positional CLI does not require or emit these; the
//! downloader also discovers completed audio files directly after exit.

use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum SpotiFlacEvent {
    Progress {
        percent: f64,
        #[serde(rename = "downloadedBytes")]
        downloaded_bytes: Option<u64>,
        #[serde(rename = "totalBytes")]
        total_bytes: Option<u64>,
        speed: Option<String>,
        eta: Option<String>,
    },
    Processing,
    TrackComplete {
        id: String,
        title: String,
        creator: Option<String>,
        path: String,
    },
    TrackFailed {
        id: String,
        title: Option<String>,
        error: String,
    },
    Error {
        message: String,
    },
}

pub fn parse_event(line: &str) -> Option<SpotiFlacEvent> {
    let trimmed = line.trim();
    if trimmed.is_empty() || !trimmed.starts_with('{') {
        return None;
    }
    serde_json::from_str(trimmed).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_progress_events() {
        let event = parse_event(r#"{"event":"progress","percent":42.5,"downloadedBytes":10,"totalBytes":100,"speed":"1MB/s","eta":"00:10"}"#).unwrap();
        assert!(matches!(event, SpotiFlacEvent::Progress { percent, .. } if percent == 42.5));
    }

    #[test]
    fn parses_track_complete_events() {
        let event = parse_event(
            r#"{"event":"track_complete","id":"t1","title":"Song","creator":"Artist","path":"C:\\tmp\\song.flac"}"#,
        )
        .unwrap();
        assert!(matches!(event, SpotiFlacEvent::TrackComplete { id, .. } if id == "t1"));
    }

    #[test]
    fn ignores_non_json_lines() {
        assert!(parse_event("Loading SpotiFLAC v1.0...").is_none());
    }

    #[test]
    fn parses_track_failed_events() {
        let event =
            parse_event(r#"{"event":"track_failed","id":"t2","title":"Song","error":"403"}"#)
                .unwrap();
        assert!(matches!(event, SpotiFlacEvent::TrackFailed { id, .. } if id == "t2"));
    }

    #[test]
    fn parses_fatal_error_events() {
        let event = parse_event(r#"{"event":"error","message":"invalid link"}"#).unwrap();
        assert!(matches!(event, SpotiFlacEvent::Error { message } if message == "invalid link"));
    }
}
