use crate::{
    errors::{map_process_error, PullyError, Result},
    extractors::{is_youtube_mix_url, validate_url, MediaExtractor},
    models::{MediaFormat, MediaInfo, PlaylistItem, SubtitleTrack},
};
use serde_json::Value;
use std::{path::PathBuf, thread, time::Duration};

const ANALYSIS_ATTEMPTS: u32 = 3;

fn is_temporary_youtube_reload_error(stderr: &str) -> bool {
    stderr
        .to_ascii_lowercase()
        .contains("the page needs to be reloaded")
}

pub struct YtDlpExtractor {
    binary: PathBuf,
    deno: Option<PathBuf>,
}
impl YtDlpExtractor {
    pub fn new(binary: PathBuf, deno: Option<PathBuf>) -> Self {
        Self { binary, deno }
    }
}

impl MediaExtractor for YtDlpExtractor {
    fn analyze(&self, url: &str) -> Result<MediaInfo> {
        validate_url(url)?;
        let mut args: Vec<String> = vec![
            "--ignore-config".into(),
            "--dump-single-json".into(),
            "--no-warnings".into(),
            "--no-download".into(),
            "--extractor-retries".into(),
            "3".into(),
            "--retry-sleep".into(),
            "extractor:linear=0.5:1.5:0.5".into(),
        ];
        args.extend(crate::extractors::js_runtime_args(self.deno.as_deref()));
        args.extend(["--playlist-end".into(), "100".into()]);
        if is_youtube_mix_url(url) {
            args.push("--no-playlist".into());
        }
        args.extend(["--".into(), url.into()]);
        let mut attempt = 0;
        let output = loop {
            attempt += 1;
            let output = crate::process::hidden_command(&self.binary)
                .args(&args)
                .output()
                .map_err(|e| {
                    PullyError::DependencyMissing(format!("Could not start yt-dlp: {e}"))
                })?;
            if output.status.success() {
                break output;
            }
            let stderr = String::from_utf8_lossy(&output.stderr);
            if attempt < ANALYSIS_ATTEMPTS && is_temporary_youtube_reload_error(&stderr) {
                thread::sleep(Duration::from_millis(300 * u64::from(attempt)));
                continue;
            }
            return Err(map_process_error(&stderr));
        };
        let value: Value = serde_json::from_slice(&output.stdout).map_err(|e| {
            PullyError::Internal(format!("yt-dlp returned unreadable metadata: {e}"))
        })?;
        parse_media_info(&value, url)
    }
}

fn text(value: &Value, key: &str) -> Option<String> {
    value
        .get(key)?
        .as_str()
        .map(str::to_owned)
        .filter(|s| !s.is_empty() && s != "none")
}
fn codec_present(value: Option<&str>) -> bool {
    value.is_some_and(|c| c != "none" && c != "null")
}

pub fn parse_media_info(value: &Value, fallback_url: &str) -> Result<MediaInfo> {
    let entries = value.get("entries").and_then(Value::as_array);
    let is_playlist = entries.is_some();
    let playlist_items = entries
        .map(|items| {
            items
                .iter()
                .filter_map(|item| {
                    Some(PlaylistItem {
                        id: text(item, "id")?,
                        title: text(item, "title").unwrap_or_else(|| "Untitled".into()),
                        url: text(item, "webpage_url")
                            .or_else(|| text(item, "url"))
                            .unwrap_or_default(),
                        thumbnail: text(item, "thumbnail"),
                        duration: item.get("duration").and_then(Value::as_f64),
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    let source_value = entries.and_then(|e| e.first()).unwrap_or(value);
    let formats: Vec<MediaFormat> = source_value
        .get("formats")
        .and_then(Value::as_array)
        .map(|formats| {
            formats
                .iter()
                .filter_map(|f| {
                    let id = text(f, "format_id")?;
                    let video_codec = text(f, "vcodec");
                    let audio_codec = text(f, "acodec");
                    let has_video = codec_present(video_codec.as_deref())
                        || codec_present(text(f, "video_ext").as_deref());
                    let has_audio = codec_present(audio_codec.as_deref())
                        || codec_present(text(f, "audio_ext").as_deref());
                    if !has_video && !has_audio {
                        return None;
                    }
                    Some(MediaFormat {
                        id,
                        extension: text(f, "ext").unwrap_or_else(|| "unknown".into()),
                        resolution: text(f, "resolution"),
                        width: f.get("width").and_then(Value::as_u64),
                        height: f.get("height").and_then(Value::as_u64),
                        fps: f.get("fps").and_then(Value::as_f64),
                        video_codec,
                        audio_codec,
                        file_size: f
                            .get("filesize")
                            .or_else(|| f.get("filesize_approx"))
                            .and_then(Value::as_u64),
                        protocol: text(f, "protocol"),
                        note: text(f, "format_note"),
                        has_video,
                        has_audio,
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    let mut subtitles = Vec::new();
    collect_subtitles(value.get("subtitles"), false, &mut subtitles);
    collect_subtitles(value.get("automatic_captions"), true, &mut subtitles);
    if formats.is_empty() && !is_playlist {
        return Err(PullyError::Unsupported(
            "No media formats were returned by the extractor.".into(),
        ));
    }
    Ok(MediaInfo {
        id: text(value, "id").unwrap_or_else(|| "unknown".into()),
        url: text(value, "webpage_url").unwrap_or_else(|| fallback_url.into()),
        title: text(value, "title").unwrap_or_else(|| "Untitled media".into()),
        creator: text(value, "uploader")
            .or_else(|| text(value, "channel"))
            .or_else(|| text(value, "creator")),
        duration: value.get("duration").and_then(Value::as_f64),
        thumbnail: text(value, "thumbnail"),
        source: text(value, "extractor_key")
            .or_else(|| text(value, "extractor"))
            .unwrap_or_else(|| "Web".into()),
        is_playlist,
        playlist_count: entries.map(Vec::len),
        playlist_items,
        formats,
        subtitles,
    })
}

fn collect_subtitles(value: Option<&Value>, automatic: bool, output: &mut Vec<SubtitleTrack>) {
    let Some(map) = value.and_then(Value::as_object) else {
        return;
    };
    for (language, formats) in map {
        let mut extensions: Vec<String> = formats
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|f| text(f, "ext"))
            .collect();
        extensions.sort();
        extensions.dedup();
        output.push(SubtitleTrack {
            language: language.clone(),
            name: formats
                .as_array()
                .and_then(|v| v.first())
                .and_then(|v| text(v, "name")),
            automatic,
            extensions,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_formats_and_subtitles() {
        let value = serde_json::json!({"id":"abc","webpage_url":"https://example.com/v","title":"Example","uploader":"Maker","duration":61.0,"extractor_key":"Example","formats":[{"format_id":"137","ext":"mp4","height":1080,"vcodec":"avc1","acodec":"none","protocol":"https"},{"format_id":"140","ext":"m4a","vcodec":"none","acodec":"mp4a"}],"subtitles":{"en":[{"ext":"vtt"}]}});
        let info = parse_media_info(&value, "https://example.com/v").unwrap();
        assert_eq!(info.formats.len(), 2);
        assert!(info.formats[0].has_video);
        assert_eq!(info.formats[0].protocol.as_deref(), Some("https"));
        assert_eq!(info.subtitles[0].language, "en");
    }

    #[test]
    fn recognizes_direct_video_from_extension() {
        let value = serde_json::json!({
            "id":"direct", "title":"Direct file", "extractor_key":"Generic",
            "formats":[{"format_id":"mp4","ext":"mp4","vcodec":null,"acodec":null,"video_ext":"mp4","audio_ext":"none"}]
        });
        let info = parse_media_info(&value, "https://example.com/video.mp4").unwrap();
        assert!(info.formats[0].has_video);
        assert!(!info.formats[0].has_audio);
    }

    #[test]
    fn recognizes_the_temporary_youtube_reload_response() {
        assert!(is_temporary_youtube_reload_error(
            "ERROR: [youtube] abc: The page needs to be reloaded."
        ));
        assert!(!is_temporary_youtube_reload_error(
            "ERROR: This video is private"
        ));
    }
}
