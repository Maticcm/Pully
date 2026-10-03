use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct MediaFormat {
    pub id: String,
    pub extension: String,
    pub resolution: Option<String>,
    pub width: Option<u64>,
    pub height: Option<u64>,
    pub fps: Option<f64>,
    pub video_codec: Option<String>,
    pub audio_codec: Option<String>,
    pub file_size: Option<u64>,
    pub protocol: Option<String>,
    pub note: Option<String>,
    pub has_video: bool,
    pub has_audio: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SubtitleTrack {
    pub language: String,
    pub name: Option<String>,
    pub automatic: bool,
    pub extensions: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PlaylistItem {
    pub id: String,
    pub title: String,
    pub url: String,
    pub thumbnail: Option<String>,
    pub duration: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct MediaInfo {
    pub id: String,
    pub url: String,
    pub title: String,
    pub creator: Option<String>,
    pub duration: Option<f64>,
    pub thumbnail: Option<String>,
    pub source: String,
    pub is_playlist: bool,
    pub playlist_count: Option<usize>,
    pub playlist_items: Vec<PlaylistItem>,
    pub formats: Vec<MediaFormat>,
    pub subtitles: Vec<SubtitleTrack>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadRequest {
    pub url: String,
    pub title: String,
    pub creator: Option<String>,
    pub thumbnail: Option<String>,
    pub mode: DownloadMode,
    pub format_id: Option<String>,
    pub format_has_audio: Option<bool>,
    pub target_fps: Option<u32>,
    pub output_format: String,
    pub output_directory: Option<String>,
    pub filename_template: Option<String>,
    pub is_playlist: bool,
    pub playlist_folder: bool,
    pub existing_file_behavior: ExistingFileBehavior,
    pub embed_metadata: bool,
    pub embed_thumbnail: bool,
    pub save_thumbnail: bool,
    pub download_subtitles: bool,
    pub embed_subtitles: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DownloadMode {
    Video,
    Audio,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ExistingFileBehavior {
    Skip,
    Overwrite,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum DownloadStatus {
    Waiting,
    Downloading,
    Processing,
    Completed,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadProgress {
    pub id: String,
    pub title: String,
    pub thumbnail: Option<String>,
    pub status: DownloadStatus,
    pub percent: f64,
    pub downloaded_bytes: Option<u64>,
    pub total_bytes: Option<u64>,
    pub speed: Option<String>,
    pub eta: Option<String>,
    pub output_path: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DependencyInfo {
    pub pully: String,
    pub yt_dlp: Option<String>,
    pub ffmpeg: Option<String>,
    /// Optional: only required when the user opens a Spotify link. Absence never
    /// blocks app readiness, since most links route through yt-dlp.
    pub spoti_flac: Option<String>,
    pub ready: bool,
    pub issues: Vec<String>,
}
