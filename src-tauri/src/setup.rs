//! Downloads yt-dlp and FFmpeg into Pully's own app-data folder when they
//! aren't already found elsewhere (bundled sidecar, PATH, or a previous
//! download here). Never runs implicitly — only in response to the user
//! clicking "Install automatically" on the missing-dependency banner (see
//! `commands::install_dependencies`), so Pully's documented "no network
//! calls you didn't ask for" stance (`PRIVACY.md`) stays true.
//!
//! SpotiFLAC is deliberately not auto-installed here — see
//! `docs/SPOTIFLAC.md`. Its maintained lossless CLI is a Python module with
//! provider extensions, so Pully validates a user-supplied installation
//! instead of silently installing a Python runtime and third-party providers.

use futures_util::StreamExt;
use serde::Serialize;
use std::{fs, io::Write, path::Path, path::PathBuf};
use tauri::{AppHandle, Emitter, Manager};

const YT_DLP_URL: &str = "https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp.exe";
/// gyan.dev's "essentials" build always redirects to whatever the current
/// stable Windows FFmpeg build is — the same URL widely used by CI systems
/// for this purpose. LGPL by default (no `--enable-gpl` in "essentials").
const FFMPEG_ZIP_URL: &str = "https://www.gyan.dev/ffmpeg/builds/ffmpeg-release-essentials.zip";

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SetupProgress {
    pub tool: &'static str,
    pub percent: f64,
    pub stage: &'static str, // "downloading" | "extracting" | "done" | "error"
    pub message: Option<String>,
}

fn emit(
    app: &AppHandle,
    tool: &'static str,
    percent: f64,
    stage: &'static str,
    message: Option<String>,
) {
    let _ = app.emit(
        "setup-progress",
        SetupProgress {
            tool,
            percent,
            stage,
            message,
        },
    );
}

/// Where auto-downloaded tools live. Checked by `commands::discover_dependencies`
/// after bundled sidecars but before PATH, so a download here is picked up
/// without needing a restart once `install_dependencies` finishes.
pub fn tools_dir(app: &AppHandle) -> Option<PathBuf> {
    app.path().app_data_dir().ok().map(|dir| dir.join("tools"))
}

async fn download_to_file(
    app: &AppHandle,
    tool: &'static str,
    url: &str,
    destination: &Path,
) -> Result<(), String> {
    let response = reqwest::get(url)
        .await
        .map_err(|e| format!("Could not reach the download server: {e}"))?;
    if !response.status().is_success() {
        return Err(format!(
            "Download failed with HTTP status {}.",
            response.status()
        ));
    }
    let total = response.content_length();
    let mut downloaded: u64 = 0;
    let temp_path = destination.with_extension("part");
    let mut file = fs::File::create(&temp_path).map_err(|e| e.to_string())?;
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| e.to_string())?;
        file.write_all(&chunk).map_err(|e| e.to_string())?;
        downloaded += chunk.len() as u64;
        let percent = total
            .filter(|t| *t > 0)
            .map(|t| (downloaded as f64 / t as f64) * 100.0)
            .unwrap_or(0.0);
        emit(app, tool, percent, "downloading", None);
    }
    drop(file);
    fs::rename(&temp_path, destination).map_err(|e| e.to_string())?;
    Ok(())
}

async fn install_yt_dlp(app: &AppHandle, dir: &Path) -> Result<(), String> {
    download_to_file(app, "yt-dlp", YT_DLP_URL, &dir.join("yt-dlp.exe")).await
}

async fn install_ffmpeg(app: &AppHandle, dir: &Path) -> Result<(), String> {
    let zip_path = std::env::temp_dir().join("pully-ffmpeg-download.part.zip");
    download_to_file(app, "ffmpeg", FFMPEG_ZIP_URL, &zip_path).await?;
    emit(app, "ffmpeg", 100.0, "extracting", None);
    let destination = dir.join("ffmpeg.exe");
    let extract_zip_path = zip_path.clone();
    tauri::async_runtime::spawn_blocking(move || -> Result<(), String> {
        let file = fs::File::open(&extract_zip_path).map_err(|e| e.to_string())?;
        let mut archive = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;
        let mut found = false;
        for i in 0..archive.len() {
            let mut entry = archive.by_index(i).map_err(|e| e.to_string())?;
            let name = entry.name().replace('\\', "/");
            if name.to_ascii_lowercase().ends_with("bin/ffmpeg.exe") {
                let mut out = fs::File::create(&destination).map_err(|e| e.to_string())?;
                std::io::copy(&mut entry, &mut out).map_err(|e| e.to_string())?;
                found = true;
                break;
            }
        }
        if found {
            Ok(())
        } else {
            Err("The downloaded FFmpeg archive didn't contain ffmpeg.exe where expected.".into())
        }
    })
    .await
    .map_err(|e| e.to_string())??;
    let _ = fs::remove_file(&zip_path);
    Ok(())
}

/// Downloads whichever of yt-dlp/FFmpeg `dependencies` (a fresh
/// `discover_dependencies` snapshot) didn't already resolve to a path.
/// Emits `setup-progress` events throughout; the caller re-runs
/// `discover_dependencies` afterwards to pick up the results.
pub async fn install_missing(
    app: &AppHandle,
    need_yt_dlp: bool,
    need_ffmpeg: bool,
) -> Result<(), String> {
    let dir = tools_dir(app)
        .ok_or_else(|| "Could not resolve Pully's app data directory.".to_string())?;
    fs::create_dir_all(&dir).map_err(|e| format!("Could not create {}: {e}", dir.display()))?;

    if need_yt_dlp {
        if let Err(error) = install_yt_dlp(app, &dir).await {
            emit(app, "yt-dlp", 0.0, "error", Some(error.clone()));
            return Err(format!("yt-dlp: {error}"));
        }
        emit(app, "yt-dlp", 100.0, "done", None);
    }
    if need_ffmpeg {
        if let Err(error) = install_ffmpeg(app, &dir).await {
            emit(app, "ffmpeg", 0.0, "error", Some(error.clone()));
            return Err(format!("FFmpeg: {error}"));
        }
        emit(app, "ffmpeg", 100.0, "done", None);
    }
    Ok(())
}
