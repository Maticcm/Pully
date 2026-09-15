use crate::{
    browser_integration::{
        ipc::{self, BrowserIntegration},
        messages::{Envelope, PROTOCOL_VERSION},
        state::{BrowserStatus, DetectedTab},
    },
    downloader::DownloadManager,
    errors::{PullyError, Result},
    extractors::{self, MediaExtractor},
    models::{DependencyInfo, DownloadRequest, MediaInfo},
    PendingLaunch, PendingUrl, TraySettings,
};
use std::time::Duration;
use std::{
    path::{Path, PathBuf},
    process::Command,
    sync::Mutex,
};
use tauri::{AppHandle, Manager, State};

#[derive(Clone)]
pub struct Dependencies {
    pub yt_dlp: Option<PathBuf>,
    pub ffmpeg: Option<PathBuf>,
    /// Optional. Only needed for Spotify links — see `extractors::spotiflac`
    /// for the CLI contract Pully expects from this binary.
    pub spotiflac: Option<PathBuf>,
    /// Pully's own native-messaging host, built alongside the main app (see
    /// `src/bin/pully-native-host.rs`) — not a user-installed tool.
    pub native_host: Option<PathBuf>,
}

fn command_version(path: &Path) -> Option<String> {
    for flag in ["--version", "-version"] {
        let output = Command::new(path).arg(flag).output().ok()?;
        if output.status.success() {
            let text = if output.stdout.is_empty() {
                String::from_utf8_lossy(&output.stderr)
            } else {
                String::from_utf8_lossy(&output.stdout)
            };
            if let Some(line) = text.lines().find(|line| !line.trim().is_empty()) {
                return Some(line.trim().to_string());
            }
        }
    }
    None
}

pub fn discover_dependencies(app: &AppHandle) -> Dependencies {
    fn locate(app: &AppHandle, name: &str) -> Option<PathBuf> {
        let exe = if cfg!(windows) {
            format!("{name}.exe")
        } else {
            name.to_string()
        };
        let mut candidates = Vec::new();
        if let Ok(resources) = app.path().resource_dir() {
            candidates.push(resources.join("binaries").join(&exe));
            candidates.push(resources.join(&exe));
        }
        // Tools Pully downloaded itself via `install_dependencies` — checked
        // before PATH so a version Pully fetched and knows about wins over
        // some unrelated system installation.
        if let Some(tools) = crate::setup::tools_dir(app) {
            if name == "spotiflac" && cfg!(windows) {
                candidates.push(tools.join("spotiflac-env").join("Scripts").join(&exe));
            }
            candidates.push(tools.join(&exe));
        }
        if let Ok(current) = std::env::current_exe() {
            if let Some(parent) = current.parent() {
                candidates.push(parent.join("binaries").join(&exe));
                // `cargo build`/`cargo tauri dev` places every `[[bin]]`
                // target (including `pully-native-host`) directly alongside
                // the main executable, with no `binaries/` subfolder — only
                // release bundling copies things into `binaries/`.
                candidates.push(parent.join(&exe));
            }
        }
        if let Ok(path) = std::env::var("PATH") {
            for folder in std::env::split_paths(&path) {
                candidates.push(folder.join(&exe));
            }
        }
        candidates.into_iter().find(|p| p.is_file())
    }
    Dependencies {
        yt_dlp: locate(app, "yt-dlp"),
        ffmpeg: locate(app, "ffmpeg"),
        spotiflac: locate(app, "spotiflac"),
        native_host: locate(app, "pully-native-host"),
    }
}

#[tauri::command]
pub async fn analyze_url(
    url: String,
    dependencies: State<'_, Mutex<Dependencies>>,
) -> Result<MediaInfo> {
    let snapshot = dependencies
        .lock()
        .map_err(|_| PullyError::Internal("Dependency state unavailable.".into()))?
        .clone();
    match extractors::route(&url)? {
        extractors::Provider::YtDlp => {
            let binary = snapshot.yt_dlp.clone().ok_or_else(|| {
                PullyError::DependencyMissing("yt-dlp is missing or corrupted.".into())
            })?;
            tauri::async_runtime::spawn_blocking(move || extractors::yt_dlp(&binary).analyze(&url))
                .await
                .map_err(|e| PullyError::Internal(e.to_string()))?
        }
        extractors::Provider::SpotiFlac => {
            let binary = snapshot.spotiflac.clone().ok_or_else(|| {
                PullyError::DependencyMissing(
                    "SpotiFLAC is not installed. Install a SpotiFLAC-compatible CLI named \
                     `spotiflac` on your PATH to analyze Spotify links."
                        .into(),
                )
            })?;
            tauri::async_runtime::spawn_blocking(move || {
                extractors::spotiflac(&binary).analyze(&url)
            })
            .await
            .map_err(|e| PullyError::Internal(e.to_string()))?
        }
    }
}

fn dependency_info_snapshot(dependencies: &Dependencies) -> DependencyInfo {
    let yt = dependencies.yt_dlp.as_deref().and_then(command_version);
    let ff = dependencies.ffmpeg.as_deref().and_then(command_version);
    let spotiflac = dependencies.spotiflac.as_deref().and_then(command_version);
    let mut issues = Vec::new();
    if yt.is_none() {
        issues.push("yt-dlp was not found.".into());
    }
    if ff.is_none() {
        issues.push("FFmpeg was not found or did not pass its version check; merging and conversion will fail.".into());
    }
    DependencyInfo {
        pully: env!("CARGO_PKG_VERSION").into(),
        yt_dlp: yt,
        ffmpeg: ff,
        spoti_flac: spotiflac,
        ready: issues.is_empty(),
        issues,
    }
}

#[tauri::command]
pub fn dependency_info(dependencies: State<'_, Mutex<Dependencies>>) -> DependencyInfo {
    let guard = dependencies.lock().unwrap();
    dependency_info_snapshot(&guard)
}

/// Downloads whichever of yt-dlp/FFmpeg aren't already found, into Pully's
/// own app-data folder (see `setup::install_missing`), then refreshes the
/// managed `Dependencies` state and the running `DownloadManager` in place
/// so newly installed tools are usable immediately, with no restart.
/// SpotiFLAC is never touched here — see `docs/SPOTIFLAC.md`.
#[tauri::command]
pub async fn install_dependencies(
    app: AppHandle,
    dependencies: State<'_, Mutex<Dependencies>>,
    manager: State<'_, DownloadManager>,
) -> Result<DependencyInfo> {
    let (need_yt_dlp, need_ffmpeg) = {
        let guard = dependencies
            .lock()
            .map_err(|_| PullyError::Internal("Dependency state unavailable.".into()))?;
        (guard.yt_dlp.is_none(), guard.ffmpeg.is_none())
    };
    if need_yt_dlp || need_ffmpeg {
        crate::setup::install_missing(&app, need_yt_dlp, need_ffmpeg)
            .await
            .map_err(PullyError::Internal)?;
    }
    let fresh = discover_dependencies(&app);
    manager.update_paths(
        fresh
            .yt_dlp
            .clone()
            .unwrap_or_else(|| PathBuf::from("yt-dlp")),
        fresh.spotiflac.clone(),
        fresh.ffmpeg.clone(),
    );
    let snapshot = dependency_info_snapshot(&fresh);
    if let Ok(mut guard) = dependencies.lock() {
        *guard = fresh;
    }
    Ok(snapshot)
}
#[tauri::command]
pub fn start_download(
    app: AppHandle,
    manager: State<'_, DownloadManager>,
    request: DownloadRequest,
) -> Result<String> {
    manager.enqueue(app, request)
}
#[tauri::command]
pub fn cancel_download(
    app: AppHandle,
    manager: State<'_, DownloadManager>,
    id: String,
) -> Result<()> {
    manager.cancel(&app, &id)
}
#[tauri::command]
pub fn remove_download(manager: State<'_, DownloadManager>, id: String) -> Result<()> {
    manager.remove(&id)
}
#[tauri::command]
pub fn retry_download(
    app: AppHandle,
    manager: State<'_, DownloadManager>,
    id: String,
) -> Result<String> {
    manager.retry(app, &id)
}

#[tauri::command]
pub fn set_concurrency(
    app: AppHandle,
    manager: State<'_, DownloadManager>,
    concurrency: usize,
) -> Result<()> {
    manager.set_concurrency(app, concurrency)
}

#[tauri::command]
pub fn open_download(manager: State<'_, DownloadManager>, id: String, reveal: bool) -> Result<()> {
    let path = manager.output_path(&id)?;
    if !path.is_file() {
        return Err(PullyError::NotFound);
    }
    #[cfg(windows)]
    {
        let mut cmd = Command::new("explorer.exe");
        if reveal {
            cmd.arg("/select,").arg(&path);
        } else {
            cmd.arg(&path);
        }
        cmd.spawn()
            .map_err(|e| PullyError::ProcessFailed(e.to_string()))?;
    }
    #[cfg(target_os = "macos")]
    {
        let mut cmd = Command::new("open");
        if reveal {
            cmd.arg("-R");
        }
        cmd.arg(&path)
            .spawn()
            .map_err(|e| PullyError::ProcessFailed(e.to_string()))?;
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let target = if reveal {
            path.parent().unwrap_or(&path)
        } else {
            &path
        };
        Command::new("xdg-open")
            .arg(target)
            .spawn()
            .map_err(|e| PullyError::ProcessFailed(e.to_string()))?;
    }
    Ok(())
}

/// Returns (and clears) the URL Pully was launched with, if any — set when
/// launched directly with a link, via a registered "Download with Pully"
/// shell command on a `.url` file, or by `pully-native-host` launching Pully
/// on the browser's behalf (see `browser_integration::launcher`), in which
/// case `quick` says whether it should go straight through Quick Mode
/// rather than the normal review screen. The frontend calls this once on
/// startup; later relaunches while already running arrive as an
/// `external-url` event instead (see `lib.rs`).
#[tauri::command]
pub fn take_pending_url(pending: State<'_, PendingUrl>) -> Option<PendingLaunch> {
    pending.0.lock().ok().and_then(|mut value| value.take())
}

/// Mirrors the user's "Run in system tray" setting into `TraySettings` (see
/// `lib.rs`). Called once on startup with the persisted setting, and again
/// any time the user flips it in Settings.
#[tauri::command]
pub fn set_run_in_tray(tray: State<'_, TraySettings>, enabled: bool) -> Result<()> {
    tray.run_in_tray
        .store(enabled, std::sync::atomic::Ordering::Relaxed);
    Ok(())
}

/// A browser session counts as "connected" if it's been heard from within
/// this window — the extension pings roughly this often (see
/// browser-extension/src/background/service-worker.ts).
const CONNECTION_FRESHNESS: Duration = Duration::from_secs(20);

#[tauri::command]
pub async fn browser_tabs(integration: State<'_, BrowserIntegration>) -> Result<Vec<DetectedTab>> {
    Ok(integration.store.lock().await.snapshot())
}

#[tauri::command]
pub async fn browser_connection_status(
    integration: State<'_, BrowserIntegration>,
) -> Result<Vec<BrowserStatus>> {
    Ok(integration
        .store
        .lock()
        .await
        .connection_status(CONNECTION_FRESHNESS))
}

/// Strict opt-in gate for the whole feature (see `BrowserIntegration::enabled`).
/// Called once on startup with the persisted setting, and again any time the
/// user flips it in Settings.
#[tauri::command]
pub fn set_browser_integration_enabled(
    integration: State<'_, BrowserIntegration>,
    enabled: bool,
) -> Result<()> {
    integration
        .enabled
        .store(enabled, std::sync::atomic::Ordering::Relaxed);
    Ok(())
}

/// The frontend's already-fully-resolved theme (see `colorChannels`/CSS
/// custom properties in `App.tsx`) — Rust never interprets or derives any
/// of this, it's just relayed verbatim to connected browser sessions.
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThemeSnapshot {
    pub accent_color: String,
    pub accent_foreground: String,
    pub canvas: String,
    pub surface: String,
    pub dark: bool,
    pub font_family: String,
}

const MAX_THEME_FIELD_LEN: usize = 256;

/// Pushed on an interval by the frontend (see `useEffect` in `App.tsx`) so
/// the browser extension's popup can mirror Pully's actual applied theme —
/// accent color, base color, font, and light/dark — instead of a fixed
/// look-alike palette. Broadcasts immediately to every connected
/// `pully-native-host` session; there is no separate Rust-side timer.
#[tauri::command]
pub async fn push_theme(app: AppHandle, theme: ThemeSnapshot) -> Result<()> {
    for field in [
        &theme.accent_color,
        &theme.accent_foreground,
        &theme.canvas,
        &theme.surface,
        &theme.font_family,
    ] {
        if field.len() > MAX_THEME_FIELD_LEN {
            return Err(PullyError::Internal("Theme value is too long.".into()));
        }
    }
    ipc::broadcast(
        &app,
        Envelope::ThemeUpdate {
            protocol_version: PROTOCOL_VERSION,
            accent_color: theme.accent_color,
            accent_foreground: theme.accent_foreground,
            canvas: theme.canvas,
            surface: theme.surface,
            dark: theme.dark,
            font_family: theme.font_family,
        },
    )
    .await;
    Ok(())
}
