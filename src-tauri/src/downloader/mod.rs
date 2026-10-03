mod progress;
mod spotiflac_protocol;

use std::{
    collections::{HashMap, VecDeque},
    fs,
    io::{BufRead, BufReader},
    path::{Path, PathBuf},
    process::Stdio,
    sync::{
        atomic::{AtomicBool, AtomicU32, Ordering},
        Arc, Mutex,
    },
};
use tauri::{AppHandle, Emitter};
use uuid::Uuid;

use crate::{
    errors::{map_process_error, PullyError, Result},
    extractors::{self, Provider},
    filesystem::{
        filename_template, is_safe_format_id, output_directory, render_track_filename, TrackMeta,
    },
    models::{
        DownloadMode, DownloadProgress, DownloadRequest, DownloadStatus, ExistingFileBehavior,
    },
};
use progress::parse_progress;
use spotiflac_protocol::{parse_event, SpotiFlacEvent};

#[derive(Clone)]
struct Job {
    request: DownloadRequest,
    provider: Provider,
    progress: DownloadProgress,
    cancel: Arc<AtomicBool>,
    pid: Arc<AtomicU32>,
}

struct Inner {
    jobs: HashMap<String, Job>,
    pending: VecDeque<String>,
    active: usize,
    concurrency: usize,
}

fn take_next(inner: &mut Inner) -> Option<String> {
    if inner.active >= inner.concurrency {
        return None;
    }
    let id = inner.pending.pop_front()?;
    inner.active += 1;
    Some(id)
}

fn format_selector(request: &DownloadRequest) -> String {
    match request.format_id.as_deref().filter(|id| *id != "best") {
        Some(id) if request.format_has_audio.unwrap_or(false) => id.to_string(),
        Some(id) => format!("{id}+bestaudio/best"),
        None => "bestvideo+bestaudio/best".into(),
    }
}

fn supports_thumbnail_embedding(request: &DownloadRequest) -> bool {
    match request.mode {
        DownloadMode::Video => matches!(request.output_format.as_str(), "mp4" | "mkv" | "mov"),
        DownloadMode::Audio => matches!(
            request.output_format.as_str(),
            "mp3" | "m4a" | "flac" | "opus" | "vorbis" | "alac"
        ),
    }
}

fn valid_output_format(mode: &DownloadMode, format: &str) -> bool {
    match mode {
        DownloadMode::Video => matches!(format, "mp4" | "webm" | "mkv" | "mov" | "original"),
        DownloadMode::Audio => matches!(
            format,
            "original" | "mp3" | "m4a" | "aac" | "flac" | "alac" | "opus" | "vorbis" | "wav"
        ),
    }
}

fn valid_target_fps(mode: &DownloadMode, output_format: &str, target_fps: Option<u32>) -> bool {
    match target_fps {
        None => true,
        Some(fps) => {
            matches!(mode, DownloadMode::Video)
                && output_format != "original"
                && (1..=240).contains(&fps)
        }
    }
}

#[derive(Clone)]
struct ToolPaths {
    yt_dlp: PathBuf,
    spotiflac: Option<PathBuf>,
    ffmpeg: Option<PathBuf>,
    deno: Option<PathBuf>,
}

#[derive(Clone)]
pub struct DownloadManager {
    inner: Arc<Mutex<Inner>>,
    paths: Arc<Mutex<ToolPaths>>,
}

impl DownloadManager {
    pub fn new(
        yt_dlp: PathBuf,
        spotiflac: Option<PathBuf>,
        ffmpeg: Option<PathBuf>,
        deno: Option<PathBuf>,
    ) -> Self {
        Self {
            inner: Arc::new(Mutex::new(Inner {
                jobs: HashMap::new(),
                pending: VecDeque::new(),
                active: 0,
                concurrency: 2,
            })),
            paths: Arc::new(Mutex::new(ToolPaths {
                yt_dlp,
                spotiflac,
                ffmpeg,
                deno,
            })),
        }
    }

    /// Called once a first-run (or re-triggered) dependency download
    /// finishes, so already-queued and future jobs pick up the newly
    /// installed `yt-dlp`/`ffmpeg` without requiring an app restart.
    pub fn update_paths(
        &self,
        yt_dlp: PathBuf,
        spotiflac: Option<PathBuf>,
        ffmpeg: Option<PathBuf>,
        deno: Option<PathBuf>,
    ) {
        if let Ok(mut paths) = self.paths.lock() {
            *paths = ToolPaths {
                yt_dlp,
                spotiflac,
                ffmpeg,
                deno,
            };
        }
    }

    fn paths(&self) -> ToolPaths {
        self.paths
            .lock()
            .map(|p| p.clone())
            .unwrap_or_else(|_| ToolPaths {
                yt_dlp: PathBuf::from("yt-dlp"),
                spotiflac: None,
                ffmpeg: None,
                deno: None,
            })
    }

    pub fn set_concurrency(&self, app: AppHandle, concurrency: usize) -> Result<()> {
        if !(1..=6).contains(&concurrency) {
            return Err(PullyError::Internal(
                "Concurrent downloads must be between 1 and 6.".into(),
            ));
        }
        self.inner
            .lock()
            .map_err(|_| PullyError::Internal("Download queue unavailable.".into()))?
            .concurrency = concurrency;
        self.schedule(app);
        Ok(())
    }

    pub fn enqueue(&self, app: AppHandle, request: DownloadRequest) -> Result<String> {
        let provider = extractors::route(&request.url)?;
        let paths = self.paths();
        if provider == Provider::SpotiFlac && paths.spotiflac.is_none() {
            return Err(PullyError::DependencyMissing(
                "SpotiFLAC is not installed. Install a SpotiFLAC-compatible CLI named \
                 `spotiflac` on your PATH to download Spotify links."
                    .into(),
            ));
        }
        if provider == Provider::SpotiFlac
            && paths
                .spotiflac
                .as_deref()
                .is_some_and(|path| !spotiflac_supports_lossless_contract(path))
        {
            return Err(PullyError::DependencyMissing(
                "The installed SpotiFLAC is too old for verified lossless downloads. Update it with `python -m pip install --upgrade \"SpotiFLAC>=4.1.2\"`."
                    .into(),
            ));
        }
        if provider == Provider::SpotiFlac
            && !matches!(request.output_format.as_str(), "original" | "flac")
        {
            return Err(PullyError::Unsupported(
                "This SpotiFLAC CLI produces FLAC audio. Choose Original or FLAC.".into(),
            ));
        }
        if let Some(id) = request.format_id.as_deref() {
            if !is_safe_format_id(id) {
                return Err(PullyError::Internal("Invalid format selection.".into()));
            }
        }
        if !valid_output_format(&request.mode, &request.output_format) {
            return Err(PullyError::Internal("Invalid output format.".into()));
        }
        if !valid_target_fps(&request.mode, &request.output_format, request.target_fps) {
            return Err(PullyError::Internal(
                "Invalid frame-rate conversion request.".into(),
            ));
        }
        if request.target_fps.is_some() && paths.ffmpeg.is_none() {
            return Err(PullyError::DependencyMissing(
                "FFmpeg is required to change frame rate.".into(),
            ));
        }
        let id = Uuid::new_v4().to_string();
        let progress = DownloadProgress {
            id: id.clone(),
            title: request.title.clone(),
            thumbnail: request.thumbnail.clone(),
            status: DownloadStatus::Waiting,
            percent: 0.0,
            downloaded_bytes: None,
            total_bytes: None,
            speed: None,
            eta: None,
            output_path: None,
            error: None,
        };
        let job = Job {
            request,
            provider,
            progress: progress.clone(),
            cancel: Arc::new(AtomicBool::new(false)),
            pid: Arc::new(AtomicU32::new(0)),
        };
        {
            let mut inner = self
                .inner
                .lock()
                .map_err(|_| PullyError::Internal("Download queue unavailable.".into()))?;
            inner.jobs.insert(id.clone(), job);
            inner.pending.push_back(id.clone());
        }
        let _ = app.emit("download-progress", progress);
        self.schedule(app);
        Ok(id)
    }

    fn schedule(&self, app: AppHandle) {
        loop {
            let next = {
                let mut inner = match self.inner.lock() {
                    Ok(v) => v,
                    Err(_) => return,
                };
                match take_next(&mut inner) {
                    Some(id) => id,
                    None => return,
                }
            };
            let manager = self.clone();
            let app_handle = app.clone();
            tauri::async_runtime::spawn(async move {
                let runner = manager.clone();
                let runner_app = app_handle.clone();
                let id = next.clone();
                let _ =
                    tauri::async_runtime::spawn_blocking(move || runner.run_job(&runner_app, &id))
                        .await;
                {
                    if let Ok(mut inner) = manager.inner.lock() {
                        inner.active = inner.active.saturating_sub(1);
                    }
                }
                manager.schedule(app_handle);
            });
        }
    }

    fn update(&self, app: &AppHandle, id: &str, mutate: impl FnOnce(&mut DownloadProgress)) {
        let value = {
            let mut inner = match self.inner.lock() {
                Ok(v) => v,
                Err(_) => return,
            };
            let Some(job) = inner.jobs.get_mut(id) else {
                return;
            };
            mutate(&mut job.progress);
            job.progress.clone()
        };
        let _ = app.emit("download-progress", value);
    }

    fn run_job(&self, app: &AppHandle, id: &str) {
        let Some(job) = self.inner.lock().ok().and_then(|i| i.jobs.get(id).cloned()) else {
            return;
        };
        if job.cancel.load(Ordering::Relaxed) {
            self.update(app, id, |p| p.status = DownloadStatus::Cancelled);
            return;
        }
        let directory = match output_directory(job.request.output_directory.as_deref()) {
            Ok(v) => v,
            Err(e) => {
                self.fail(app, id, e.to_string());
                return;
            }
        };
        match job.provider {
            Provider::YtDlp => self.run_ytdlp_job(app, id, &job, &directory),
            Provider::SpotiFlac => self.run_spotiflac_job(app, id, &job, &directory),
        }
    }

    fn run_ytdlp_job(&self, app: &AppHandle, id: &str, job: &Job, directory: &Path) {
        let paths = self.paths();
        let output_template = match filename_template(
            directory,
            job.request.filename_template.as_deref(),
            job.request.is_playlist && job.request.playlist_folder,
        ) {
            Ok(value) => value,
            Err(error) => {
                self.fail(app, id, error.to_string());
                return;
            }
        };
        // Keep the template relative so yt-dlp applies -P to both the final
        // file and its temporary files. An absolute -o silently ignores -P.
        let relative_template = output_template
            .strip_prefix(directory)
            .expect("validated output template stays inside the download folder");
        let temp_directory = std::env::temp_dir().join("Pully").join(id);
        if let Err(error) = fs::create_dir_all(&temp_directory) {
            self.fail(
                app,
                id,
                format!("Could not create a temporary download folder: {error}"),
            );
            return;
        }
        let existing_file_arg = match job.request.existing_file_behavior {
            ExistingFileBehavior::Skip => "--no-overwrites",
            ExistingFileBehavior::Overwrite => "--force-overwrites",
        };
        let mut args: Vec<String> = vec!["--ignore-config".into(), "--newline".into(), "--no-colors".into(), existing_file_arg.into(), "--windows-filenames".into(), "--ignore-errors".into(), "--extractor-retries".into(), "3".into(), "--retry-sleep".into(), "extractor:linear=0.5:1.5:0.5".into(), "--progress".into(), "--progress-template".into(), "download:PULLY_PROGRESS|%(progress._percent_str)s|%(progress.downloaded_bytes)s|%(progress.total_bytes)s|%(progress.total_bytes_estimate)s|%(progress._speed_str)s|%(progress._eta_str)s".into(), "--progress-template".into(), "postprocess:PULLY_PROCESSING".into(), "--print".into(), "after_move:PULLY_FILE|%(filepath)s".into(), "-P".into(), directory.to_string_lossy().into_owned(), "-P".into(), format!("temp:{}", temp_directory.to_string_lossy()), "-o".into(), relative_template.to_string_lossy().into_owned()];
        args.extend(extractors::js_runtime_args(paths.deno.as_deref()));
        if extractors::is_youtube_url(&job.request.url) {
            // Prefer direct HTTPS media over YouTube's HLS variants, which
            // can advertise a format and then reject segment requests.
            args.extend(["--format-sort".into(), "proto:https".into()]);
        }
        if let Some(ffmpeg) = &paths.ffmpeg {
            if let Some(parent) = ffmpeg.parent() {
                args.extend([
                    "--ffmpeg-location".into(),
                    parent.to_string_lossy().into_owned(),
                ]);
            }
        }
        match job.request.mode {
            DownloadMode::Video => {
                let selection = format_selector(&job.request);
                args.extend(["-f".into(), selection]);
                if let Some(target_fps) = job.request.target_fps {
                    let format = job.request.output_format.clone();
                    let intermediate = if format == "mkv" { "mp4" } else { "mkv" };
                    args.extend(["--merge-output-format".into(), intermediate.into()]);
                    args.extend(["--remux-video".into(), intermediate.into()]);
                    args.extend(["--recode-video".into(), format]);
                    args.extend([
                        "--postprocessor-args".into(),
                        format!("VideoConvertor+ffmpeg_o:-r {target_fps}"),
                    ]);
                } else if job.request.output_format != "original" {
                    let format = job.request.output_format.clone();
                    args.extend(["--merge-output-format".into(), format.clone()]);
                    args.extend(["--remux-video".into(), format]);
                }
            }
            DownloadMode::Audio => {
                args.extend(["-f".into(), "bestaudio/best".into()]);
                if job.request.output_format != "original" {
                    args.extend([
                        "-x".into(),
                        "--audio-format".into(),
                        job.request.output_format.clone(),
                    ]);
                }
            }
        }
        if job.request.embed_metadata {
            args.push("--embed-metadata".into());
        }
        if job.request.embed_thumbnail && supports_thumbnail_embedding(&job.request) {
            args.push("--embed-thumbnail".into());
            args.extend(["--convert-thumbnails".into(), "jpg".into()]);
        }
        if job.request.save_thumbnail {
            args.push("--write-thumbnail".into());
        }
        if job.request.download_subtitles || job.request.embed_subtitles {
            args.extend(["--write-subs".into(), "--write-auto-subs".into()]);
        }
        if job.request.embed_subtitles {
            args.push("--embed-subs".into());
        }
        if crate::extractors::is_youtube_mix_url(&job.request.url) {
            args.push("--no-playlist".into());
        }
        args.push("--".into());
        args.push(job.request.url.clone());
        self.update(app, id, |p| p.status = DownloadStatus::Downloading);
        let mut child = match crate::process::hidden_command(&paths.yt_dlp)
            .args(&args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .stdin(Stdio::null())
            .spawn()
        {
            Ok(v) => v,
            Err(e) => {
                cleanup_temp_directory(&temp_directory);
                self.fail(app, id, e.to_string());
                return;
            }
        };
        job.pid.store(child.id(), Ordering::SeqCst);
        let stdout = child.stdout.take();
        let stderr = child.stderr.take();
        let error_thread = std::thread::spawn(move || {
            stderr
                .map(|s| {
                    BufReader::new(s)
                        .lines()
                        .map_while(std::result::Result::ok)
                        .collect::<Vec<_>>()
                        .join("\n")
                })
                .unwrap_or_default()
        });
        // With `--ignore-errors` above, a playlist keeps going past an
        // unavailable/geo-blocked item instead of aborting entirely — but
        // yt-dlp's own exit code still reflects "were there any errors",
        // so a job with some real successes still exits non-zero. Track
        // completed files ourselves so a partial playlist failure reports
        // Completed-with-a-warning instead of a flat Failed that would
        // hide the files that *did* download successfully.
        let mut files_completed = 0usize;
        if let Some(out) = stdout {
            for line in BufReader::new(out)
                .lines()
                .map_while(std::result::Result::ok)
            {
                if job.cancel.load(Ordering::Relaxed) {
                    break;
                }
                if let Some(value) = parse_progress(&line) {
                    self.update(app, id, |p| {
                        p.status = DownloadStatus::Downloading;
                        p.percent = value.percent;
                        p.downloaded_bytes = value.downloaded_bytes;
                        p.total_bytes = value.total_bytes;
                        p.speed = value.speed;
                        p.eta = value.eta;
                    });
                } else if let Some(path) = line.strip_prefix("PULLY_FILE|") {
                    files_completed += 1;
                    self.update(app, id, |p| p.output_path = Some(path.to_string()));
                } else if line.starts_with("PULLY_PROCESSING") {
                    self.update(app, id, |p| p.status = DownloadStatus::Processing);
                }
            }
        }
        let status = child.wait();
        job.pid.store(0, Ordering::SeqCst);
        let stderr = error_thread.join().unwrap_or_default();
        cleanup_temp_directory(&temp_directory);
        let succeeded = status.map(|s| s.success()).unwrap_or(false);
        if job.cancel.load(Ordering::Relaxed) {
            self.update(app, id, |p| {
                p.status = DownloadStatus::Cancelled;
                p.speed = None;
                p.eta = None;
            });
        } else if succeeded {
            self.update(app, id, |p| {
                p.status = DownloadStatus::Completed;
                p.percent = 100.0;
                p.speed = None;
                p.eta = None;
            });
        } else if files_completed > 0 {
            // At least one item made it through despite a non-zero exit —
            // treat this as a completed batch with a note, the same pattern
            // already used for SpotiFLAC's own partial-failure handling,
            // rather than discarding a partially-successful playlist.
            self.update(app, id, |p| {
                p.status = DownloadStatus::Completed;
                p.percent = 100.0;
                p.speed = None;
                p.eta = None;
                p.error = Some(format!(
                    "Some items in this playlist could not be downloaded.\n{}",
                    map_process_error(&stderr)
                ));
            });
        } else {
            self.fail(app, id, map_process_error(&stderr).to_string());
        }
    }

    /// Runs a Spotify download through the SpotiFLAC adapter. Structurally
    /// mirrors `run_ytdlp_job` (same temp-dir pattern, same progress events,
    /// same cancellation and cleanup), but SpotiFLAC's own process is
    /// responsible for fetching and encoding audio — Pully only orchestrates
    /// it and moves finished tracks into place using its own filename
    /// template (see `extractors::spotiflac` for the validated CLI contract).
    fn run_spotiflac_job(&self, app: &AppHandle, id: &str, job: &Job, directory: &Path) {
        let Some(binary) = self.paths().spotiflac else {
            self.fail(
                app,
                id,
                "SpotiFLAC is not installed. Install a SpotiFLAC-compatible CLI named \
                 `spotiflac` on your PATH to download Spotify links."
                    .into(),
            );
            return;
        };
        let temp_directory = std::env::temp_dir().join("Pully").join(id);
        if let Err(error) = fs::create_dir_all(&temp_directory) {
            self.fail(
                app,
                id,
                format!("Could not create a temporary download folder: {error}"),
            );
            return;
        }
        let playlist_folder = job.request.is_playlist && job.request.playlist_folder;
        let args = spotiflac_args(&job.request.url, &temp_directory);

        self.update(app, id, |p| p.status = DownloadStatus::Downloading);
        let mut child = match crate::process::hidden_command(&binary)
            .args(&args)
            // Older Windows builds of SpotiFLAC print Unicode status symbols.
            // Without UTF-8 mode Python can abort its resolver mid-download.
            .env("PYTHONUTF8", "1")
            .env("PYTHONIOENCODING", "utf-8")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .stdin(Stdio::null())
            .spawn()
        {
            Ok(v) => v,
            Err(e) => {
                cleanup_temp_directory(&temp_directory);
                self.fail(app, id, format!("Could not start SpotiFLAC: {e}"));
                return;
            }
        };
        job.pid.store(child.id(), Ordering::SeqCst);
        let stdout = child.stdout.take();
        let stderr = child.stderr.take();
        let error_thread = std::thread::spawn(move || {
            stderr
                .map(|s| {
                    BufReader::new(s)
                        .lines()
                        .map_while(std::result::Result::ok)
                        .collect::<Vec<_>>()
                        .join("\n")
                })
                .unwrap_or_default()
        });

        let mut successes = 0usize;
        let mut failures: Vec<String> = Vec::new();
        let mut fatal_error: Option<String> = None;
        let mut plain_output: Vec<String> = Vec::new();
        if let Some(out) = stdout {
            for line in BufReader::new(out)
                .lines()
                .map_while(std::result::Result::ok)
            {
                if job.cancel.load(Ordering::Relaxed) {
                    break;
                }
                let Some(event) = parse_event(&line) else {
                    if !line.trim().is_empty() {
                        plain_output.push(line);
                    }
                    continue;
                };
                match event {
                    SpotiFlacEvent::Progress {
                        percent,
                        downloaded_bytes,
                        total_bytes,
                        speed,
                        eta,
                    } => {
                        self.update(app, id, |p| {
                            p.status = DownloadStatus::Downloading;
                            p.percent = percent.clamp(0.0, 100.0);
                            p.downloaded_bytes = downloaded_bytes;
                            p.total_bytes = total_bytes;
                            p.speed = speed.clone();
                            p.eta = eta.clone();
                        });
                    }
                    SpotiFlacEvent::Processing => {
                        self.update(app, id, |p| p.status = DownloadStatus::Processing);
                    }
                    SpotiFlacEvent::TrackComplete {
                        id: track_id,
                        title,
                        creator,
                        path,
                    } => {
                        match place_finished_track(
                            &job.request,
                            directory,
                            playlist_folder,
                            &track_id,
                            &title,
                            creator.as_deref(),
                            &path,
                        ) {
                            Ok(Some(final_path)) => {
                                successes += 1;
                                let rendered = final_path.to_string_lossy().into_owned();
                                self.update(app, id, |p| p.output_path = Some(rendered));
                            }
                            Ok(None) => {
                                // Existing-file behavior said to skip this track.
                            }
                            Err(error) => {
                                failures.push(format!("{title}: {error}"));
                            }
                        }
                    }
                    SpotiFlacEvent::TrackFailed { id, title, error } => {
                        let label = title.filter(|t| !t.is_empty()).unwrap_or(id);
                        failures.push(format!("{label}: {error}"));
                    }
                    SpotiFlacEvent::Error { message } => {
                        fatal_error = Some(message);
                        break;
                    }
                }
            }
        }
        let status = child.wait();
        job.pid.store(0, Ordering::SeqCst);
        let stderr = error_thread.join().unwrap_or_default();
        if successes == 0 {
            match collect_spotiflac_outputs(&temp_directory) {
                Ok(paths) => {
                    let base_id = spotify_resource_id(&job.request.url);
                    for (index, path) in paths.into_iter().enumerate() {
                        let discovered_title = path
                            .file_stem()
                            .and_then(|value| value.to_str())
                            .unwrap_or("Spotify track");
                        let title = if !job.request.is_playlist && index == 0 {
                            job.request.title.as_str()
                        } else {
                            discovered_title
                        };
                        let track_id = if index == 0 {
                            base_id.clone()
                        } else {
                            format!("{base_id}-{}", index + 1)
                        };
                        match place_finished_track(
                            &job.request,
                            directory,
                            playlist_folder,
                            &track_id,
                            title,
                            (!job.request.is_playlist && index == 0)
                                .then_some(job.request.creator.as_deref())
                                .flatten(),
                            &path.to_string_lossy(),
                        ) {
                            Ok(Some(final_path)) => {
                                successes += 1;
                                let rendered = final_path.to_string_lossy().into_owned();
                                self.update(app, id, |p| p.output_path = Some(rendered));
                            }
                            Ok(None) => {}
                            Err(error) => failures.push(format!("{title}: {error}")),
                        }
                    }
                }
                Err(error) => failures.push(error.to_string()),
            }
        }
        cleanup_temp_directory(&temp_directory);

        if job.cancel.load(Ordering::Relaxed) {
            self.update(app, id, |p| {
                p.status = DownloadStatus::Cancelled;
                p.speed = None;
                p.eta = None;
            });
            return;
        }
        if let Some(message) = fatal_error {
            self.fail(app, id, message);
            return;
        }
        if successes == 0 {
            let message = if !failures.is_empty() {
                format!("Every track failed:\n{}", failures.join("\n"))
            } else {
                let details = spotiflac_failure_details(&plain_output, &stderr);
                if details.is_empty() && status.map(|s| s.success()).unwrap_or(false) {
                    "SpotiFLAC finished without producing any audio files.".into()
                } else {
                    map_process_error(&details).to_string()
                }
            };
            self.fail(app, id, message);
            return;
        }
        self.update(app, id, |p| {
            p.status = DownloadStatus::Completed;
            p.percent = 100.0;
            p.speed = None;
            p.eta = None;
            if !failures.is_empty() {
                p.error = Some(format!(
                    "{} of {} tracks failed:\n{}",
                    failures.len(),
                    successes + failures.len(),
                    failures.join("\n")
                ));
            }
        });
    }

    fn fail(&self, app: &AppHandle, id: &str, error: String) {
        self.update(app, id, |p| {
            p.status = DownloadStatus::Failed;
            p.error = Some(error);
            p.speed = None;
            p.eta = None;
        });
    }

    pub fn cancel(&self, app: &AppHandle, id: &str) -> Result<()> {
        let pid = {
            let mut inner = self
                .inner
                .lock()
                .map_err(|_| PullyError::Internal("Download queue unavailable.".into()))?;
            let job = inner.jobs.get(id).ok_or(PullyError::NotFound)?;
            job.cancel.store(true, Ordering::SeqCst);
            let pid = job.pid.load(Ordering::SeqCst);
            inner.pending.retain(|queued| queued != id);
            pid
        };
        if pid > 0 {
            kill_process(pid);
        }
        self.update(app, id, |p| p.status = DownloadStatus::Cancelled);
        Ok(())
    }

    pub fn remove(&self, id: &str) -> Result<()> {
        let mut inner = self
            .inner
            .lock()
            .map_err(|_| PullyError::Internal("Download queue unavailable.".into()))?;
        let job = inner.jobs.get(id).ok_or(PullyError::NotFound)?;
        if matches!(
            job.progress.status,
            DownloadStatus::Downloading | DownloadStatus::Processing | DownloadStatus::Waiting
        ) {
            return Err(PullyError::ProcessFailed(
                "Cancel this download before removing it.".into(),
            ));
        }
        inner.jobs.remove(id);
        Ok(())
    }
    pub fn retry(&self, app: AppHandle, id: &str) -> Result<String> {
        let request = self
            .inner
            .lock()
            .map_err(|_| PullyError::Internal("Download queue unavailable.".into()))?
            .jobs
            .get(id)
            .ok_or(PullyError::NotFound)?
            .request
            .clone();
        self.enqueue(app, request)
    }
    pub fn output_path(&self, id: &str) -> Result<PathBuf> {
        self.inner
            .lock()
            .map_err(|_| PullyError::Internal("Download queue unavailable.".into()))?
            .jobs
            .get(id)
            .and_then(|j| j.progress.output_path.as_ref())
            .map(PathBuf::from)
            .ok_or(PullyError::NotFound)
    }
}

fn spotify_resource_id(value: &str) -> String {
    url::Url::parse(value)
        .ok()
        .and_then(|parsed| {
            parsed
                .path_segments()
                .and_then(|segments| segments.filter(|part| !part.is_empty()).next_back())
                .map(str::to_string)
        })
        .unwrap_or_else(|| "spotify".into())
}

fn spotiflac_args(url: &str, output_directory: &Path) -> Vec<String> {
    vec![
        url.to_string(),
        output_directory.to_string_lossy().into_owned(),
        "--registries".into(),
        "https://raw.githubusercontent.com/zarzet/SpotiFLAC-Extension/main/registry.json".into(),
        "--service".into(),
        "ext:tidal-web".into(),
        "--quality".into(),
        "LOSSLESS".into(),
        "--json".into(),
        "--no-lyrics".into(),
        "--no-enrich".into(),
        "--max-concurrent".into(),
        "1".into(),
    ]
}

pub(crate) fn spotiflac_supports_lossless_contract(binary: &Path) -> bool {
    crate::process::hidden_command(binary)
        .arg("--help")
        .env("PYTHONUTF8", "1")
        .env("PYTHONIOENCODING", "utf-8")
        .output()
        .ok()
        .map(|output| {
            let text = format!(
                "{}\n{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            spotiflac_help_supports_contract(&text)
        })
        .unwrap_or(false)
}

fn spotiflac_help_supports_contract(help: &str) -> bool {
    ["--quality", "--registries", "--json"]
        .iter()
        .all(|flag| help.contains(flag))
}

fn spotiflac_failure_details(output: &[String], stderr: &str) -> String {
    if !stderr.trim().is_empty() {
        return stderr.trim().to_string();
    }
    let relevant: Vec<_> = output
        .iter()
        .filter(|line| {
            let lower = line.to_ascii_lowercase();
            lower.contains("failed") || lower.contains("error:")
        })
        .rev()
        .take(8)
        .cloned()
        .collect();
    relevant.into_iter().rev().collect::<Vec<_>>().join("\n")
}

fn collect_spotiflac_outputs(directory: &Path) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    let entries = fs::read_dir(directory).map_err(|error| {
        PullyError::Internal(format!("Could not inspect SpotiFLAC output: {error}"))
    })?;
    for entry in entries {
        let path = entry
            .map_err(|error| PullyError::Internal(error.to_string()))?
            .path();
        if path.is_dir() {
            files.extend(collect_spotiflac_outputs(&path)?);
        } else if path
            .extension()
            .and_then(|value| value.to_str())
            .is_some_and(|extension| extension.eq_ignore_ascii_case("flac"))
            && is_flac_stream(&path)
        {
            files.push(path);
        }
    }
    files.sort();
    Ok(files)
}

fn is_flac_stream(path: &Path) -> bool {
    use std::io::Read;
    let Ok(mut file) = fs::File::open(path) else {
        return false;
    };
    let mut signature = [0u8; 4];
    file.read_exact(&mut signature).is_ok() && signature == *b"fLaC"
}

/// Moves one SpotiFLAC-downloaded track from its temp path into Pully's
/// configured download folder, rendering the final filename from Pully's own
/// template rather than trusting SpotiFLAC's naming. Returns `Ok(None)` when
/// the existing-file setting says to skip a file that's already there.
fn place_finished_track(
    request: &DownloadRequest,
    directory: &Path,
    playlist_folder: bool,
    track_id: &str,
    title: &str,
    creator: Option<&str>,
    source_path: &str,
) -> Result<Option<PathBuf>> {
    let source = PathBuf::from(source_path);
    let extension = source
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("flac");
    let meta = TrackMeta {
        title,
        creator,
        id: track_id,
        extension,
    };
    let destination = render_track_filename(
        directory,
        request.filename_template.as_deref(),
        playlist_folder,
        playlist_folder.then_some(request.title.as_str()),
        &meta,
    )?;
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent).map_err(|e| PullyError::PermissionDenied(e.to_string()))?;
    }
    if destination.exists() {
        match request.existing_file_behavior {
            ExistingFileBehavior::Skip => {
                let _ = fs::remove_file(&source);
                return Ok(None);
            }
            ExistingFileBehavior::Overwrite => {
                fs::remove_file(&destination)
                    .map_err(|e| PullyError::PermissionDenied(e.to_string()))?;
            }
        }
    }
    if fs::rename(&source, &destination).is_err() {
        fs::copy(&source, &destination).map_err(|e| PullyError::PermissionDenied(e.to_string()))?;
        let _ = fs::remove_file(&source);
    }
    Ok(Some(destination))
}

fn cleanup_temp_directory(path: &std::path::Path) {
    let expected_parent = std::env::temp_dir().join("Pully");
    if path.parent() == Some(expected_parent.as_path()) {
        let _ = fs::remove_dir_all(path);
    }
}

#[cfg(windows)]
fn kill_process(pid: u32) {
    let _ = crate::process::hidden_command("taskkill")
        .args(["/PID", &pid.to_string(), "/T", "/F"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}
#[cfg(not(windows))]
fn kill_process(pid: u32) {
    let _ = crate::process::hidden_command("kill")
        .args(["-TERM", &pid.to_string()])
        .status();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(has_audio: bool) -> DownloadRequest {
        DownloadRequest {
            url: "https://example.com/media".into(),
            title: "Example".into(),
            creator: None,
            thumbnail: None,
            mode: DownloadMode::Video,
            format_id: Some("137".into()),
            format_has_audio: Some(has_audio),
            target_fps: None,
            output_format: "mp4".into(),
            output_directory: None,
            filename_template: Some("{title} [{id}].{ext}".into()),
            is_playlist: false,
            playlist_folder: true,
            existing_file_behavior: ExistingFileBehavior::Skip,
            embed_metadata: false,
            embed_thumbnail: false,
            save_thumbnail: false,
            download_subtitles: false,
            embed_subtitles: false,
        }
    }

    fn audio_request(existing_file_behavior: ExistingFileBehavior) -> DownloadRequest {
        DownloadRequest {
            url: "https://open.spotify.com/track/abc".into(),
            title: "Album Title".into(),
            creator: Some("Artist".into()),
            thumbnail: None,
            mode: DownloadMode::Audio,
            format_id: None,
            format_has_audio: None,
            target_fps: None,
            output_format: "original".into(),
            output_directory: None,
            filename_template: Some("{creator} - {title}.{ext}".into()),
            is_playlist: false,
            playlist_folder: true,
            existing_file_behavior,
            embed_metadata: false,
            embed_thumbnail: false,
            save_thumbnail: false,
            download_subtitles: false,
            embed_subtitles: false,
        }
    }

    #[test]
    fn uses_the_installed_spotiflac_positional_cli_contract() {
        let args = spotiflac_args(
            "https://open.spotify.com/track/abc",
            Path::new("C:\\Temp\\Pully"),
        );
        assert_eq!(args[0], "https://open.spotify.com/track/abc");
        assert_eq!(args[1], "C:\\Temp\\Pully");
        assert!(args
            .windows(2)
            .any(|pair| pair == ["--quality", "LOSSLESS"]));
        assert!(args
            .windows(2)
            .any(|pair| pair == ["--service", "ext:tidal-web"]));
        assert!(args.iter().any(|arg| arg == "--json"));
        assert!(!args.iter().any(|arg| arg == "info"));
    }

    #[test]
    fn extracts_useful_errors_from_a_zero_exit_spotiflac_run() {
        let output = vec![
            "Metadata fetched successfully.".into(),
            "[X] tidal failed: all APIs failed".into(),
            "Status: Download completed!".into(),
            "Error: all APIs failed".into(),
        ];
        let details = spotiflac_failure_details(&output, "");
        assert!(details.contains("tidal failed"));
        assert!(details.contains("Error: all APIs failed"));
    }

    #[test]
    fn rejects_spotiflac_versions_without_the_lossless_contract() {
        assert!(spotiflac_help_supports_contract(
            "--quality LOSSLESS --registries URL --json"
        ));
        assert!(!spotiflac_help_supports_contract(
            "--service tidal url output_dir"
        ));
    }

    #[test]
    fn discovers_nested_spotiflac_audio_outputs_only() {
        let root = tempfile::tempdir().unwrap();
        let album = root.path().join("Album");
        fs::create_dir(&album).unwrap();
        fs::write(album.join("song.flac"), b"fLaC audio").unwrap();
        fs::write(album.join("renamed-lossy.flac"), b"not really flac").unwrap();
        fs::write(album.join("youtube.m4a"), b"audio").unwrap();
        fs::write(album.join("cover.jpg"), b"image").unwrap();
        let files = collect_spotiflac_outputs(root.path()).unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].file_name().unwrap(), "song.flac");
    }

    #[test]
    fn places_a_finished_track_at_its_rendered_destination() {
        let dir = tempfile::tempdir().unwrap();
        let temp = dir.path().join("source.flac");
        fs::write(&temp, b"audio").unwrap();
        let request = audio_request(ExistingFileBehavior::Skip);
        let placed = place_finished_track(
            &request,
            dir.path(),
            false,
            "t1",
            "Song",
            Some("Artist"),
            temp.to_str().unwrap(),
        )
        .unwrap();
        let destination = placed.expect("track should be placed");
        assert_eq!(destination.file_name().unwrap(), "Artist - Song.flac");
        assert!(destination.exists());
        assert!(!temp.exists());
    }

    #[test]
    fn skips_an_existing_track_without_overwriting_it() {
        let dir = tempfile::tempdir().unwrap();
        let existing = dir.path().join("Artist - Song.flac");
        fs::write(&existing, b"original").unwrap();
        let temp = dir.path().join("source.flac");
        fs::write(&temp, b"new").unwrap();
        let request = audio_request(ExistingFileBehavior::Skip);
        let placed = place_finished_track(
            &request,
            dir.path(),
            false,
            "t1",
            "Song",
            Some("Artist"),
            temp.to_str().unwrap(),
        )
        .unwrap();
        assert!(placed.is_none());
        assert!(!temp.exists());
        assert_eq!(fs::read(&existing).unwrap(), b"original");
    }

    #[test]
    fn overwrites_an_existing_track_when_requested() {
        let dir = tempfile::tempdir().unwrap();
        let existing = dir.path().join("Artist - Song.flac");
        fs::write(&existing, b"original").unwrap();
        let temp = dir.path().join("source.flac");
        fs::write(&temp, b"new").unwrap();
        let request = audio_request(ExistingFileBehavior::Overwrite);
        let placed = place_finished_track(
            &request,
            dir.path(),
            false,
            "t1",
            "Song",
            Some("Artist"),
            temp.to_str().unwrap(),
        )
        .unwrap();
        assert!(placed.is_some());
        assert_eq!(fs::read(&existing).unwrap(), b"new");
    }

    #[test]
    fn selects_audio_only_when_needed() {
        assert_eq!(format_selector(&request(false)), "137+bestaudio/best");
        assert_eq!(format_selector(&request(true)), "137");
    }

    #[test]
    fn skips_thumbnail_embedding_for_unsupported_containers() {
        let mut video = request(false);
        assert!(supports_thumbnail_embedding(&video));
        video.output_format = "webm".into();
        assert!(!supports_thumbnail_embedding(&video));
        video.output_format = "original".into();
        assert!(!supports_thumbnail_embedding(&video));
        let mut audio = audio_request(ExistingFileBehavior::Skip);
        audio.output_format = "flac".into();
        assert!(supports_thumbnail_embedding(&audio));
        audio.output_format = "wav".into();
        assert!(!supports_thumbnail_embedding(&audio));
    }

    #[test]
    fn accepts_only_supported_output_formats() {
        for format in ["mp4", "webm", "mkv", "mov", "original"] {
            assert!(valid_output_format(&DownloadMode::Video, format));
        }
        for format in [
            "original", "mp3", "m4a", "aac", "flac", "alac", "opus", "vorbis", "wav",
        ] {
            assert!(valid_output_format(&DownloadMode::Audio, format));
        }
        assert!(!valid_output_format(&DownloadMode::Video, "avi"));
        assert!(!valid_output_format(&DownloadMode::Audio, "wma"));
    }

    #[test]
    fn validates_frame_rate_conversion_requests() {
        assert!(valid_target_fps(&DownloadMode::Video, "mp4", None));
        assert!(valid_target_fps(&DownloadMode::Video, "mp4", Some(30)));
        assert!(!valid_target_fps(
            &DownloadMode::Video,
            "original",
            Some(30)
        ));
        assert!(!valid_target_fps(&DownloadMode::Audio, "mp3", Some(30)));
        assert!(!valid_target_fps(&DownloadMode::Video, "mp4", Some(0)));
    }

    #[test]
    fn queue_respects_concurrency() {
        let mut inner = Inner {
            jobs: HashMap::new(),
            pending: VecDeque::from(["a".into(), "b".into(), "c".into()]),
            active: 0,
            concurrency: 2,
        };
        assert_eq!(take_next(&mut inner).as_deref(), Some("a"));
        assert_eq!(take_next(&mut inner).as_deref(), Some("b"));
        assert_eq!(take_next(&mut inner), None);
        assert_eq!(inner.pending.len(), 1);
    }
}
