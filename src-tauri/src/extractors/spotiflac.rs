//! Adapter for the locally installed SpotiFLAC command-line application.
//!
//! The public CLI accepts a Spotify URL and output directory as positional
//! arguments but does not expose a metadata-only mode. Analysis therefore uses
//! Spotify's public oEmbed response for the real title and artwork. A best-effort
//! page metadata read adds the creator and duration when Spotify exposes them.

use crate::{
    errors::{PullyError, Result},
    extractors::{validate_url, MediaExtractor},
    models::{MediaFormat, MediaInfo},
};
use serde::Deserialize;
use std::{path::PathBuf, time::Duration};

pub struct SpotiFlacExtractor {
    _binary: PathBuf,
}

impl SpotiFlacExtractor {
    pub fn new(binary: PathBuf) -> Self {
        Self { _binary: binary }
    }
}

impl MediaExtractor for SpotiFlacExtractor {
    fn analyze(&self, url: &str) -> Result<MediaInfo> {
        validate_url(url)?;
        let (kind, id) = spotify_resource(url)?;
        let metadata = spotify_metadata(url)?;
        let is_playlist = matches!(kind, "album" | "playlist");

        Ok(MediaInfo {
            id,
            url: url.to_string(),
            title: metadata.title,
            creator: metadata.creator,
            duration: metadata.duration,
            thumbnail: metadata.thumbnail,
            source: "Spotify".into(),
            is_playlist,
            playlist_count: None,
            playlist_items: Vec::new(),
            formats: vec![lossless_audio_format()],
            subtitles: Vec::new(),
        })
    }
}

#[derive(Debug, Deserialize)]
struct OEmbedResponse {
    title: String,
    thumbnail_url: Option<String>,
}

struct SpotifyMetadata {
    title: String,
    creator: Option<String>,
    duration: Option<f64>,
    thumbnail: Option<String>,
}

fn spotify_metadata(value: &str) -> Result<SpotifyMetadata> {
    let mut endpoint = url::Url::parse("https://open.spotify.com/oembed")
        .map_err(|error| PullyError::Internal(error.to_string()))?;
    endpoint.query_pairs_mut().append_pair("url", value);
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(15))
        .user_agent("Pully/0.1")
        .build()
        .map_err(|error| {
            PullyError::Internal(format!(
                "Could not prepare Spotify metadata request: {error}"
            ))
        })?;
    let response = client
        .get(endpoint)
        .send()
        .and_then(reqwest::blocking::Response::error_for_status)
        .map_err(|error| {
            PullyError::ProcessFailed(format!("Could not fetch Spotify metadata: {error}"))
        })?;
    let body = response.text().map_err(|error| {
        PullyError::ProcessFailed(format!("Could not read Spotify metadata: {error}"))
    })?;
    let oembed: OEmbedResponse = serde_json::from_str(&body).map_err(|error| {
        PullyError::ProcessFailed(format!("Spotify returned unreadable metadata: {error}"))
    })?;
    if oembed.title.trim().is_empty() {
        return Err(PullyError::ProcessFailed(
            "Spotify returned an empty media title.".into(),
        ));
    }

    let page = client
        .get(value)
        .send()
        .and_then(reqwest::blocking::Response::error_for_status)
        .ok()
        .and_then(|response| response.text().ok());
    let creator = page
        .as_deref()
        .and_then(|html| meta_content(html, "og:description"))
        .and_then(|description| {
            description
                .split('·')
                .next()
                .map(str::trim)
                .map(str::to_string)
        })
        .filter(|value| !value.is_empty());
    let duration = page
        .as_deref()
        .and_then(|html| meta_content(html, "music:duration"))
        .and_then(|value| value.parse::<f64>().ok());

    Ok(SpotifyMetadata {
        title: oembed.title,
        creator,
        duration,
        thumbnail: oembed.thumbnail_url,
    })
}

fn meta_content(html: &str, property: &str) -> Option<String> {
    for tag in html.split('<').skip(1) {
        let Some(name) = tag.get(..4) else {
            continue;
        };
        if !name.eq_ignore_ascii_case("meta")
            || !tag.as_bytes().get(4).is_some_and(u8::is_ascii_whitespace)
        {
            continue;
        }
        let mut attributes = &tag[4..];
        let mut matches = false;
        let mut content = None;
        while !attributes.is_empty() {
            attributes = attributes.trim_start();
            if attributes.starts_with(['>', '/']) {
                break;
            }
            let end = attributes
                .find(|c: char| c.is_ascii_whitespace() || matches!(c, '=' | '>' | '/'))
                .unwrap_or(attributes.len());
            if end == 0 {
                break;
            }
            let name = &attributes[..end];
            attributes = attributes[end..].trim_start();
            if !attributes.starts_with('=') {
                continue;
            }
            attributes = attributes[1..].trim_start();
            let value;
            if attributes.starts_with(['\'', '"']) {
                let quote = attributes.as_bytes()[0] as char;
                attributes = &attributes[1..];
                let Some(end) = attributes.find(quote) else {
                    break;
                };
                value = &attributes[..end];
                attributes = &attributes[end + 1..];
            } else {
                let end = attributes
                    .find(|c: char| c.is_ascii_whitespace() || c == '>')
                    .unwrap_or(attributes.len());
                value = &attributes[..end];
                attributes = &attributes[end..];
            }
            if name.eq_ignore_ascii_case("property") || name.eq_ignore_ascii_case("name") {
                matches |= value.eq_ignore_ascii_case(property);
            } else if name.eq_ignore_ascii_case("content") && content.is_none() {
                content = Some(value);
            }
        }
        if matches && content.is_some() {
            return content.map(decode_html);
        }
    }
    None
}

fn decode_html(value: &str) -> String {
    value
        .replace("&#x27;", "'")
        .replace("&quot;", "\"")
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
}

fn spotify_resource(value: &str) -> Result<(&'static str, String)> {
    let parsed = url::Url::parse(value).map_err(|_| PullyError::InvalidUrl)?;
    let host = parsed.host_str().ok_or(PullyError::InvalidUrl)?;
    if host != "spotify.com" && !host.ends_with(".spotify.com") {
        return Err(PullyError::InvalidUrl);
    }
    let segments: Vec<_> = parsed
        .path_segments()
        .into_iter()
        .flatten()
        .filter(|segment| !segment.is_empty())
        .collect();
    for kind in ["track", "album", "playlist"] {
        if let Some(index) = segments.iter().position(|segment| *segment == kind) {
            if let Some(id) = segments.get(index + 1).filter(|id| !id.is_empty()) {
                return Ok((kind, (*id).to_string()));
            }
        }
    }
    Err(PullyError::Unsupported(
        "SpotiFLAC supports Spotify track, album, and playlist links.".into(),
    ))
}

fn lossless_audio_format() -> MediaFormat {
    MediaFormat {
        id: "spotiflac-lossless".into(),
        extension: "flac".into(),
        resolution: None,
        width: None,
        height: None,
        fps: None,
        video_codec: None,
        audio_codec: Some("flac".into()),
        file_size: None,
        protocol: None,
        note: Some("Lossless via SpotiFLAC".into()),
        has_video: false,
        has_audio: true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_spotify_page_metadata() {
        let html = r#"<meta property="og:description" content="Seafret · Album · Song · 2016"><meta name="music:duration" content="230">"#;
        assert_eq!(
            meta_content(html, "og:description").as_deref(),
            Some("Seafret · Album · Song · 2016")
        );
        assert_eq!(meta_content(html, "music:duration").as_deref(), Some("230"));
    }

    #[test]
    fn parses_reordered_and_differently_quoted_metadata() {
        for html in [
            r#"<meta content="Artist &amp; Guests" property="og:description">"#,
            "<META CONTENT = 'Artist &amp; Guests' PROPERTY = 'og:description'>",
            "<meta content='unrelated'><meta content='Artist &amp; Guests' name='og:description'>",
        ] {
            assert_eq!(
                meta_content(html, "og:description").as_deref(),
                Some("Artist & Guests")
            );
        }
        assert_eq!(
            meta_content("<meta content=230 name=music:duration>", "music:duration").as_deref(),
            Some("230")
        );
        assert_eq!(
            meta_content(
                "<meta content='Artist > Guest' property='og:description'>",
                "og:description"
            )
            .as_deref(),
            Some("Artist > Guest")
        );
    }

    #[test]
    fn does_not_read_metadata_from_other_tags_or_attribute_values() {
        assert!(meta_content(
            r#"<div property="og:description" content="wrong">"#,
            "og:description"
        )
        .is_none());
        assert!(meta_content(
            r#"<meta data-note='property="og:description"' content="wrong">"#,
            "og:description"
        )
        .is_none());
        assert!(meta_content(
            "<meta property='og:description'><meta content='wrong'>",
            "og:description"
        )
        .is_none());
    }

    #[test]
    fn recognizes_locale_prefixed_album_and_playlist_links() {
        assert_eq!(
            spotify_resource("https://open.spotify.com/intl-de/album/album123")
                .unwrap()
                .0,
            "album"
        );
        assert_eq!(
            spotify_resource("https://open.spotify.com/playlist/list123")
                .unwrap()
                .0,
            "playlist"
        );
    }

    #[test]
    fn rejects_unsupported_spotify_pages() {
        assert!(spotify_resource("https://open.spotify.com/artist/abc").is_err());
    }
}
