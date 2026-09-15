pub mod spotiflac;
mod yt_dlp;
use crate::{
    errors::{PullyError, Result},
    models::MediaInfo,
};
use std::path::Path;

pub trait MediaExtractor {
    fn analyze(&self, url: &str) -> Result<MediaInfo>;
}

/// Which backend a URL should be handled by. Routing happens once, before
/// analysis or download, so the rest of Pully (UI, download manager, progress
/// events) never needs to know which provider is in play.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Provider {
    YtDlp,
    SpotiFlac,
}

/// Decides which provider owns a URL. Spotify links go to SpotiFLAC;
/// everything else keeps going through yt-dlp, unchanged.
pub fn route(value: &str) -> Result<Provider> {
    validate_url(value)?;
    Ok(if is_spotify_url(value) {
        Provider::SpotiFlac
    } else {
        Provider::YtDlp
    })
}

pub fn is_spotify_url(value: &str) -> bool {
    let Ok(parsed) = url::Url::parse(value) else {
        return false;
    };
    let Some(host) = parsed.host_str() else {
        return false;
    };
    host == "spotify.com" || host.ends_with(".spotify.com")
}

pub fn validate_url(value: &str) -> Result<()> {
    let parsed = url::Url::parse(value).map_err(|_| PullyError::InvalidUrl)?;
    if !matches!(parsed.scheme(), "http" | "https") || parsed.host_str().is_none() {
        return Err(PullyError::InvalidUrl);
    }
    Ok(())
}

pub fn is_youtube_mix_url(value: &str) -> bool {
    let Ok(parsed) = url::Url::parse(value) else {
        return false;
    };
    let Some(host) = parsed.host_str() else {
        return false;
    };
    let is_youtube = host == "youtube.com" || host.ends_with(".youtube.com");
    let is_short_link = host == "youtu.be" || host.ends_with(".youtu.be");
    if !is_youtube && !is_short_link {
        return false;
    }

    let has_video = if is_short_link {
        parsed
            .path_segments()
            .is_some_and(|mut parts| parts.next().is_some_and(|part| !part.is_empty()))
    } else {
        parsed
            .query_pairs()
            .any(|(key, value)| key == "v" && !value.is_empty())
    };
    let is_mix = parsed
        .query_pairs()
        .any(|(key, value)| key == "list" && value.starts_with("RD"));
    has_video && is_mix
}

pub fn yt_dlp(path: &Path) -> yt_dlp::YtDlpExtractor {
    yt_dlp::YtDlpExtractor::new(path.to_path_buf())
}

pub fn spotiflac(path: &Path) -> spotiflac::SpotiFlacExtractor {
    spotiflac::SpotiFlacExtractor::new(path.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn accepts_web_urls_only() {
        assert!(validate_url("https://example.com/v").is_ok());
        assert!(validate_url("file:///etc/passwd").is_err());
        assert!(validate_url("not a url").is_err());
    }
    #[test]
    fn routes_spotify_links_to_spotiflac() {
        assert_eq!(
            route("https://open.spotify.com/track/abc123").unwrap(),
            Provider::SpotiFlac
        );
        assert_eq!(
            route("https://open.spotify.com/album/abc123").unwrap(),
            Provider::SpotiFlac
        );
        assert_eq!(
            route("https://open.spotify.com/playlist/abc123").unwrap(),
            Provider::SpotiFlac
        );
    }
    #[test]
    fn routes_everything_else_to_yt_dlp() {
        assert_eq!(
            route("https://www.youtube.com/watch?v=abc").unwrap(),
            Provider::YtDlp
        );
        assert_eq!(
            route("https://soundcloud.com/artist/track").unwrap(),
            Provider::YtDlp
        );
    }
    #[test]
    fn rejects_invalid_urls_before_routing() {
        assert!(route("not a url").is_err());
    }

    #[test]
    fn recognizes_youtube_mix_watch_links() {
        assert!(is_youtube_mix_url(
            "https://www.youtube.com/watch?v=G5RpJwCJDqc&list=RDG5RpJwCJDqc&start_radio=1"
        ));
        assert!(is_youtube_mix_url(
            "https://youtu.be/G5RpJwCJDqc?list=RDG5RpJwCJDqc"
        ));
        assert!(!is_youtube_mix_url(
            "https://www.youtube.com/watch?v=G5RpJwCJDqc&list=PL123"
        ));
        assert!(!is_youtube_mix_url(
            "https://www.youtube.com/playlist?list=RDG5RpJwCJDqc"
        ));
    }
}
