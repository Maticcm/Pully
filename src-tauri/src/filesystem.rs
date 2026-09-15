use crate::errors::{PullyError, Result};
use std::{
    fs,
    path::{Path, PathBuf},
};

pub fn output_directory(requested: Option<&str>) -> Result<PathBuf> {
    let path = requested
        .map(PathBuf::from)
        .or_else(|| dirs::download_dir().map(|p| p.join("Pully")))
        .ok_or_else(|| PullyError::Internal("Could not locate your Downloads folder.".into()))?;
    fs::create_dir_all(&path).map_err(|e| PullyError::PermissionDenied(e.to_string()))?;
    let canonical = path
        .canonicalize()
        .map_err(|e| PullyError::PermissionDenied(e.to_string()))?;
    if !canonical.is_dir() {
        return Err(PullyError::PermissionDenied(
            "The selected location is not a folder.".into(),
        ));
    }
    Ok(canonical)
}

pub fn is_safe_format_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 100
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '+'))
}

fn validated_template(requested: Option<&str>) -> Result<String> {
    let template = requested.unwrap_or("{title} [{id}].{ext}").trim();
    if template.is_empty()
        || template.len() > 180
        || !template.contains("{ext}")
        || template.contains("..")
        || template
            .chars()
            .any(|c| matches!(c, '/' | '\\' | ':' | '*' | '?' | '\"' | '<' | '>' | '|'))
    {
        return Err(PullyError::Internal(
            "The filename template contains unsafe characters.".into(),
        ));
    }
    let mut remaining = template.to_string();
    for token in ["{title}", "{creator}", "{id}", "{resolution}", "{ext}"] {
        remaining = remaining.replace(token, "");
    }
    if remaining.contains('{')
        || remaining.contains('}')
        || !remaining.chars().all(|c| {
            c.is_alphanumeric()
                || c.is_whitespace()
                || matches!(c, '-' | '_' | '.' | '(' | ')' | '[' | ']')
        })
    {
        return Err(PullyError::Internal(
            "The filename template contains an unsupported token.".into(),
        ));
    }
    Ok(template.to_string())
}

fn within_directory(directory: &Path, path: PathBuf) -> Result<PathBuf> {
    if !path.starts_with(directory) {
        return Err(PullyError::Internal(
            "The filename escaped the selected download folder.".into(),
        ));
    }
    Ok(path)
}

/// Builds a yt-dlp output template (`-o`) by translating Pully's `{token}`
/// syntax into yt-dlp's own `%(...)s` syntax. yt-dlp resolves these itself
/// from the live metadata it fetches while downloading.
pub fn filename_template(
    directory: &Path,
    requested: Option<&str>,
    playlist_folder: bool,
) -> Result<PathBuf> {
    let template = validated_template(requested)?;
    let rendered = template
        .replace("{title}", "%(title).180B")
        .replace("{creator}", "%(uploader,channel,creator|Unknown)s")
        .replace("{id}", "%(id)s")
        .replace("{resolution}", "%(resolution|unknown)s")
        .replace("{ext}", "%(ext)s");
    let path = if playlist_folder {
        directory
            .join("%(playlist_title).120B [%(playlist_id)s]")
            .join(rendered)
    } else {
        directory.join(rendered)
    };
    within_directory(directory, path)
}

/// Metadata for one already-downloaded track, used to render its final
/// filename directly (no external templating engine involved).
pub struct TrackMeta<'a> {
    pub title: &'a str,
    pub creator: Option<&'a str>,
    pub id: &'a str,
    pub extension: &'a str,
}

/// Windows device names, reserved case-insensitively as a whole segment or
/// as the part before the first `.` (e.g. both `con` and `con.mp3` are
/// blocked by the OS) — yt-dlp's own `--windows-filenames` flag already
/// handles this for the yt-dlp path; this covers the SpotiFLAC path, which
/// renders filenames directly instead of going through yt-dlp's sanitizer.
const RESERVED_WINDOWS_NAMES: [&str; 22] = [
    "CON", "PRN", "AUX", "NUL", "COM0", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7",
    "COM8", "COM9", "LPT0", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7",
];

fn is_reserved_windows_name(segment: &str) -> bool {
    let stem = segment.split('.').next().unwrap_or(segment);
    RESERVED_WINDOWS_NAMES
        .iter()
        .any(|reserved| stem.eq_ignore_ascii_case(reserved))
}

fn sanitize_path_segment(value: &str) -> String {
    let cleaned: String = value
        .chars()
        .map(|c| {
            if matches!(c, '/' | '\\' | ':' | '*' | '?' | '\"' | '<' | '>' | '|') {
                '_'
            } else {
                c
            }
        })
        .collect();
    let trimmed = cleaned.trim().trim_matches('.');
    if trimmed.is_empty() {
        "untitled".into()
    } else {
        let truncated: String = trimmed.chars().take(180).collect();
        if is_reserved_windows_name(&truncated) {
            format!("_{truncated}")
        } else {
            truncated
        }
    }
}

/// Renders a filename directly from known track metadata, for providers
/// (like SpotiFLAC) that don't share yt-dlp's `%(...)s` templating engine.
/// Applies the same `{token}` syntax and safety rules as `filename_template`.
pub fn render_track_filename(
    directory: &Path,
    requested: Option<&str>,
    playlist_folder: bool,
    playlist_title: Option<&str>,
    track: &TrackMeta,
) -> Result<PathBuf> {
    let template = validated_template(requested)?;
    let rendered = template
        .replace("{title}", &sanitize_path_segment(track.title))
        .replace(
            "{creator}",
            &sanitize_path_segment(track.creator.unwrap_or("Unknown")),
        )
        .replace("{id}", &sanitize_path_segment(track.id))
        .replace("{resolution}", "audio")
        .replace("{ext}", track.extension.trim_start_matches('.'));
    let path = match (playlist_folder, playlist_title) {
        (true, Some(title)) => directory.join(sanitize_path_segment(title)).join(rendered),
        _ => directory.join(rendered),
    };
    within_directory(directory, path)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn validates_format_ids() {
        assert!(is_safe_format_id("137+bestaudio"));
        assert!(!is_safe_format_id("137; rm -rf"));
        assert!(!is_safe_format_id("../bad"));
    }
    #[test]
    fn output_is_bounded_by_directory() {
        let d = tempfile::tempdir().unwrap();
        assert!(filename_template(d.path(), None, false)
            .unwrap()
            .starts_with(d.path()));
    }
    #[test]
    fn renders_track_filenames_directly() {
        let d = tempfile::tempdir().unwrap();
        let track = TrackMeta {
            title: "Song / Title",
            creator: Some("Artist"),
            id: "t1",
            extension: "flac",
        };
        let path = render_track_filename(
            d.path(),
            Some("{creator} - {title}.{ext}"),
            false,
            None,
            &track,
        )
        .unwrap();
        assert_eq!(path.file_name().unwrap(), "Artist - Song _ Title.flac");
        assert!(path.starts_with(d.path()));
    }
    #[test]
    fn avoids_reserved_windows_device_names() {
        let d = tempfile::tempdir().unwrap();
        for title in ["con", "NUL", "Com1", "lpt3"] {
            let track = TrackMeta {
                title,
                creator: None,
                id: "t1",
                extension: "flac",
            };
            let path = render_track_filename(d.path(), Some("{title}.{ext}"), false, None, &track)
                .unwrap();
            let name = path.file_name().unwrap().to_str().unwrap();
            assert!(
                !is_reserved_windows_name(name.split('.').next().unwrap()),
                "{name} should not be a reserved device name"
            );
        }
    }
    #[test]
    fn nests_track_filenames_under_a_playlist_folder() {
        let d = tempfile::tempdir().unwrap();
        let track = TrackMeta {
            title: "Track",
            creator: None,
            id: "t1",
            extension: "flac",
        };
        let path = render_track_filename(
            d.path(),
            Some("{title}.{ext}"),
            true,
            Some("My Album"),
            &track,
        )
        .unwrap();
        assert!(path.starts_with(d.path().join("My Album")));
    }
    #[test]
    fn rejects_unsafe_filename_templates() {
        let d = tempfile::tempdir().unwrap();
        assert!(filename_template(d.path(), Some("../{title}.{ext}"), false).is_err());
        assert!(filename_template(d.path(), Some("{unknown}.{ext}"), false).is_err());
        assert!(
            filename_template(d.path(), Some("{creator} - {title}.{ext}"), true)
                .unwrap()
                .starts_with(d.path())
        );
    }
}
