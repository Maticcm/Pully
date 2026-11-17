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

/// The public CLI emits one complete JSON report at exit, not NDJSON events.
#[derive(Debug, Deserialize)]
pub struct RunReport {
    pub schema_version: u32,
    pub tracks: Vec<TrackRecord>,
}

#[derive(Debug, Deserialize)]
pub struct TrackRecord {
    pub id: String,
    pub title: String,
    pub artists: Option<String>,
    pub status: String,
    pub file_path: Option<String>,
    pub error: Option<String>,
}

pub fn parse_report(lines: &[String]) -> Option<RunReport> {
    let output = lines.join("\n");
    // SpotiFLAC 5.0.6 also attaches its logging handler to stdout, so
    // warnings from the resolver or extensions can surround the JSON report.
    // Decode a complete document instead of requiring stdout to be only JSON.
    let mut offset = 0;
    for line in output.split_inclusive('\n') {
        let candidate = line.trim_start_matches('\u{feff}').trim_start();
        if candidate.starts_with('{') {
            let start = offset + line.len() - candidate.len();
            let mut documents = serde_json::Deserializer::from_str(&output[start..])
                .into_iter::<serde_json::Value>();
            if let Some(Ok(value)) = documents.next() {
                if value.get("schema_version").is_some() {
                    let report: RunReport = serde_json::from_value(value).ok()?;
                    return (report.schema_version == 1).then_some(report);
                }
            }
        }
        offset += line.len();
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_public_cli_multiline_report() {
        let lines = ["{", "\"schema_version\": 1,", "\"tracks\": [{\"id\":\"abc\",\"title\":\"Song\",\"artists\":\"Artist\",\"status\":\"failed\",\"file_path\":null,\"error\":\"unavailable\"}]", "}"].map(str::to_string);
        let report = parse_report(&lines).unwrap();
        assert_eq!(report.tracks[0].status, "failed");
        assert_eq!(report.tracks[0].error.as_deref(), Some("unavailable"));
    }

    #[test]
    fn parses_report_surrounded_by_provider_logs() {
        let lines = [
            "WARNING: provider request failed; retrying",
            "extension output: {not JSON}",
            "{",
            r#"  "schema_version": 1,"#,
            r#"  "tracks": [{"id":"abc","title":"Song {live}","artists":"Artist","status":"downloaded","file_path":"C:\\tmp\\song.flac","error":null}]"#,
            "}",
            "INFO: finished",
        ].map(str::to_string);
        let report = parse_report(&lines).unwrap();
        assert_eq!(report.tracks[0].title, "Song {live}");
        assert_eq!(report.tracks[0].status, "downloaded");
        assert_eq!(
            report.tracks[0].file_path.as_deref(),
            Some(r"C:\tmp\song.flac")
        );
    }

    #[test]
    fn parses_compact_report_after_unrelated_json() {
        let lines = [
            r#"{"message":"provider initialized"}"#,
            "\u{feff}  {\"schema_version\":1,\"tracks\":[]}",
        ]
        .map(str::to_string);
        assert!(parse_report(&lines).unwrap().tracks.is_empty());
    }

    #[test]
    fn rejects_unsupported_or_malformed_reports_even_with_logs() {
        for document in [
            r#"{"schema_version":2,"tracks":[]}"#,
            r#"{"schema_version":1,"tracks":[{"status":"downloaded"}]}"#,
            r#"{"schema_version":1,"tracks":[]"#,
        ] {
            let lines = [
                "WARNING: provider message".to_string(),
                document.to_string(),
            ];
            assert!(parse_report(&lines).is_none());
        }
    }

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
