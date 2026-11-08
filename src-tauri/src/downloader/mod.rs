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
use tauri::{AppHandle, Emitter, Manager};
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
use spotiflac_protocol::{parse_event, parse_report, SpotiFlacEvent};

#[derive(Clone)]
struct Job {
    request: DownloadRequest,
    provider: Provider,
    progress: DownloadProgress,
    cancel: Arc<AtomicBool>,
    pid: Arc<AtomicU32>,
    archive: Option<PathBuf>,
    sync_files: HashMap<String, (String, PathBuf)>,
    spotify_sync: Option<(String, bool, usize)>,
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

fn playlist_selection_args(request: &DownloadRequest) -> Result<Vec<String>> {
    match &request.playlist_indices {
        None => Ok(Vec::new()),
        Some(indices) if !request.is_playlist || indices.is_empty() || indices.contains(&0) => Err(
            PullyError::Internal("Select at least one valid collection item.".into()),
        ),
        Some(indices) => {
            let mut indices = indices.clone();
            indices.sort_unstable();
            indices.dedup();
            Ok(vec![
                "--playlist-items".into(),
                indices
                    .iter()
                    .map(usize::to_string)
                    .collect::<Vec<_>>()
                    .join(","),
            ])
        }
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
    spotify_destinations: Arc<Mutex<HashMap<PathBuf, Arc<Mutex<()>>>>>,
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
            spotify_destinations: Arc::new(Mutex::new(HashMap::new())),
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
        self.enqueue_with_archive(app, request, None)
    }

    pub fn enqueue_with_archive(
        &self,
        app: AppHandle,
        request: DownloadRequest,
        archive: Option<PathBuf>,
    ) -> Result<String> {
        self.enqueue_internal(app, request, archive, None)
    }

    pub fn enqueue_spotify_sync(
        &self,
        app: AppHandle,
        request: DownloadRequest,
        sync_id: String,
        write_m3u: bool,
        expected_tracks: usize,
    ) -> Result<String> {
        if Uuid::parse_str(&sync_id).is_err() {
            return Err(PullyError::Internal("Invalid sync identifier.".into()));
        }
        self.enqueue_internal(
            app,
            request,
            None,
            Some((sync_id, write_m3u, expected_tracks)),
        )
    }

    fn enqueue_internal(
        &self,
        app: AppHandle,
        request: DownloadRequest,
        archive: Option<PathBuf>,
        spotify_sync: Option<(String, bool, usize)>,
    ) -> Result<String> {
        let provider = extractors::route(&request.url)?;
        if spotify_sync.is_some() && provider != Provider::SpotiFlac {
            return Err(PullyError::Unsupported(
                "Expected a Spotify playlist.".into(),
            ));
        }
        playlist_selection_args(&request)?;
        if request.playlist_indices.is_some() && provider == Provider::SpotiFlac {
            return Err(PullyError::Unsupported(
                "Track selection is unavailable for SpotiFLAC.".into(),
            ));
        }
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
        if spotify_sync.is_some() {
            let binary = paths.spotiflac.as_deref().ok_or_else(|| {
                PullyError::DependencyMissing("Install SpotiFLAC from Settings first.".into())
            })?;
            verify_spotiflac_sync_contract(binary).map_err(PullyError::DependencyMissing)?;
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
            sync_managed: archive.is_some() || spotify_sync.is_some(),
            sync_finalized: archive.is_none() && spotify_sync.is_none(),
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
            archive,
            sync_files: HashMap::new(),
            spotify_sync,
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

    pub fn spotiflac_path(&self) -> Option<PathBuf> {
        self.paths().spotiflac
    }

    pub fn analyze_playlist(&self, url: &str) -> Result<crate::models::MediaInfo> {
        use crate::extractors::MediaExtractor;
        let paths = self.paths();
        extractors::yt_dlp(&paths.yt_dlp, paths.deno.as_deref()).analyze(url)
    }

    pub fn sync_files(&self, id: &str) -> Option<HashMap<String, (String, PathBuf)>> {
        self.inner
            .lock()
            .ok()?
            .jobs
            .get(id)
            .map(|job| job.sync_files.clone())
    }

    pub fn progress(&self, id: &str) -> Option<DownloadProgress> {
        self.inner
            .lock()
            .ok()?
            .jobs
            .get(id)
            .map(|job| job.progress.clone())
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
                if tauri::async_runtime::spawn_blocking(move || runner.run_job(&runner_app, &id))
                    .await
                    .is_err()
                {
                    manager.fail(
                        &app_handle,
                        &next,
                        "The download worker stopped unexpectedly. Retry the sync.".into(),
                    );
                }
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
            Provider::SpotiFlac if job.spotify_sync.is_some() => {
                self.run_spotiflac_sync_job(app, id, &job, &directory)
            }
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
        if job.archive.is_some() {
            args.extend([
                "--print".into(),
                "after_move:PULLY_SYNC|%(extractor_key)s|%(id)s|%(filepath)j".into(),
            ]);
        }
        if let Some(archive) = &job.archive {
            args.extend([
                "--download-archive".into(),
                archive.to_string_lossy().into_owned(),
            ]);
        }
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
        } else {
            args.push("--no-write-thumbnail".into());
            // Embedding still downloads an image. Keep it (including any
            // playlist cover) inside this job's cleanup folder, so a failed
            // embed or thumbnail conversion cannot leave an output sidecar.
            args.extend([
                "-P".into(),
                format!("thumbnail:{}", temp_directory.to_string_lossy()),
                "-P".into(),
                format!("pl_thumbnail:{}", temp_directory.to_string_lossy()),
            ]);
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
        match playlist_selection_args(&job.request) {
            Ok(selection) => args.extend(selection),
            Err(error) => {
                self.fail(app, id, error.to_string());
                return;
            }
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
        // Cancellation can arrive after spawn but before the PID is published.
        if job.cancel.load(Ordering::SeqCst) {
            kill_process(child.id());
        }
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
                } else if let Some(data) = line.strip_prefix("PULLY_SYNC|") {
                    let mut fields = data.splitn(3, '|');
                    if let (Some(extractor), Some(item_id), Some(path)) =
                        (fields.next(), fields.next(), fields.next())
                    {
                        if let Ok(path) = serde_json::from_str::<String>(path) {
                            if let Ok(mut inner) = self.inner.lock() {
                                if let Some(job) = inner.jobs.get_mut(id) {
                                    job.sync_files.insert(
                                        item_id.to_string(),
                                        (extractor.to_ascii_lowercase(), PathBuf::from(path)),
                                    );
                                }
                            }
                        }
                    }
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

    /// SpotiFLAC's playlist mode indexes the selected output directory and
    /// skips files it can match, without a second Pully downloader.
    fn run_spotiflac_sync_job(&self, app: &AppHandle, id: &str, job: &Job, directory: &Path) {
        let Some(binary) = self.paths().spotiflac else {
            self.fail(app, id, "SpotiFLAC is not installed.".into());
            return;
        };
        let Some((sync_id, make_m3u, expected_tracks)) = job.spotify_sync.as_ref() else {
            return;
        };
        // Playlist mode indexes the destination recursively. Serialize syncs
        // sharing a selected directory so they cannot race to write a track.
        let destination_lock = match self.spotify_destinations.lock() {
            Ok(mut locks) => locks.entry(directory.to_path_buf()).or_default().clone(),
            Err(_) => {
                self.fail(app, id, "Could not coordinate playlist downloads.".into());
                return;
            }
        };
        let _destination_guard = loop {
            if job.cancel.load(Ordering::SeqCst) {
                self.update(app, id, |p| p.status = DownloadStatus::Cancelled);
                return;
            }
            match destination_lock.try_lock() {
                Ok(guard) => break guard,
                Err(std::sync::TryLockError::WouldBlock) => {
                    std::thread::sleep(std::time::Duration::from_millis(100))
                }
                Err(std::sync::TryLockError::Poisoned(_)) => {
                    self.fail(app, id, "Could not coordinate playlist downloads.".into());
                    return;
                }
            }
        };
        let folder = spotify_sync_folder(directory, sync_id, job.request.playlist_folder);
        if let Err(error) = fs::create_dir_all(&folder) {
            self.fail(
                app,
                id,
                format!("Could not create playlist folder: {error}"),
            );
            return;
        }
        let folder = match fs::canonicalize(&folder) {
            Ok(path) if path.starts_with(directory) => path,
            _ => {
                self.fail(
                    app,
                    id,
                    "The playlist folder is outside the selected destination.".into(),
                );
                return;
            }
        };
        let mut args = spotiflac_playlist_args(&job.request.url, &folder, *make_m3u);
        let help = match spotiflac_cli_help(&binary) {
            Ok(value) => value,
            Err(error) => {
                self.fail(app, id, error);
                return;
            }
        };
        args.extend(spotiflac_optional_args(&help));
        self.update(app, id, |p| p.status = DownloadStatus::Downloading);
        let mut child = match crate::process::hidden_command(&binary)
            .args(&args)
            .env("PYTHONUTF8", "1")
            .env("PYTHONIOENCODING", "utf-8")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .stdin(Stdio::null())
            .spawn()
        {
            Ok(child) => child,
            Err(error) => {
                self.fail(app, id, format!("Could not start SpotiFLAC: {error}"));
                return;
            }
        };
        let process_guard = match crate::process::ProcessTreeGuard::attach(&child) {
            Ok(guard) => guard,
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                self.fail(
                    app,
                    id,
                    format!("Could not safely start the playlist process: {error}"),
                );
                return;
            }
        };
        job.pid.store(child.id(), Ordering::SeqCst);
        // Cancellation can arrive after spawn but before the PID is published.
        if job.cancel.load(Ordering::SeqCst) {
            kill_process(child.id());
        }
        let stderr = child.stderr.take();
        let progress_app = app.clone();
        let progress_id = id.to_owned();
        let error_thread = std::thread::spawn(move || {
            stderr
                .map(|pipe| {
                    BufReader::new(pipe)
                        .lines()
                        .map_while(std::result::Result::ok)
                        .inspect(|line| {
                            if let Some(percent) = spotiflac_playlist_progress(line) {
                                progress_app.state::<DownloadManager>().update(
                                    &progress_app,
                                    &progress_id,
                                    |progress| progress.percent = percent,
                                );
                            }
                        })
                        .collect::<Vec<_>>()
                        .join("\n")
                })
                .unwrap_or_default()
        });
        // Drain concurrently, but do not wait for EOF before waiting for the
        // main process: a descendant may inherit the pipe and keep it open.
        let stdout = child.stdout.take();
        let output_thread = std::thread::spawn(move || {
            stdout
                .map(|pipe| {
                    BufReader::new(pipe)
                        .lines()
                        .map_while(std::result::Result::ok)
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default()
        });
        // The CLI's timeout applies to individual provider requests, not to a
        // stalled metadata fetch or a wedged subprocess. Allow for all tracks
        // and retries, while putting a ceiling on the whole queue job.
        let timeout_seconds = 300u64
            .saturating_add((*expected_tracks as u64).saturating_mul(420))
            .min(24 * 60 * 60);
        let status = crate::process::wait_with_timeout(
            &mut child,
            std::time::Duration::from_secs(timeout_seconds),
        );
        drop(process_guard);
        if status.is_err() {
            let _ = child.kill();
            let _ = child.wait();
        }
        job.pid.store(0, Ordering::SeqCst);
        let stdout = output_thread.join().unwrap_or_default();
        let stderr = error_thread.join().unwrap_or_default();
        if job.cancel.load(Ordering::Relaxed) {
            self.update(app, id, |p| {
                p.status = DownloadStatus::Cancelled;
                p.speed = None;
                p.eta = None;
            });
            return;
        }
        let status = match status {
            Ok(status) => status,
            Err(error) => {
                self.fail(
                    app,
                    id,
                    if error.kind() == std::io::ErrorKind::TimedOut {
                        "SpotiFLAC timed out while synchronizing the playlist.".into()
                    } else {
                        format!("Could not wait for SpotiFLAC: {error}")
                    },
                );
                return;
            }
        };
        let Some(plan) = parse_spotiflac_sync_plan(&stderr) else {
            self.fail(
                app,
                id,
                format!(
                    "SpotiFLAC did not confirm a playlist sync. {}",
                    spotiflac_failure_details(&stdout, &stderr)
                ),
            );
            return;
        };
        let Some(report) = parse_report(&stdout) else {
            self.fail(
                app,
                id,
                "SpotiFLAC did not return a supported playlist report.".into(),
            );
            return;
        };
        // SpotiFLAC also merges distinct Spotify IDs that represent the same
        // recording, so its unique recording count can be lower than our ID snapshot.
        if plan.0 == 0 || plan.0 > *expected_tracks {
            self.fail(app, id, "The Spotify playlist changed during synchronization. Sync again to refresh its contents.".into());
            return;
        }
        if let Err(error) = validate_spotiflac_report(&report, plan.2) {
            self.fail(app, id, error);
            return;
        }
        let ffprobe = find_ffprobe(self.paths().ffmpeg.as_deref());
        let mut failures = Vec::new();
        for track in report.tracks {
            match track.status.as_str() {
                "downloaded" | "skipped" => {
                    let valid = track
                        .file_path
                        .as_deref()
                        .map(PathBuf::from)
                        .and_then(|p| fs::canonicalize(p).ok())
                        .filter(|path| {
                            path.starts_with(&folder) && path.is_file() && is_flac_stream(path)
                        });
                    match valid {
                        Some(path)
                            if ffprobe.as_deref().is_none_or(|probe| {
                                validate_flac_with_ffprobe(probe, &path).is_ok()
                            }) =>
                        {
                            let rendered = path.to_string_lossy().into_owned();
                            self.update(app, id, |p| p.output_path = Some(rendered));
                        }
                        _ => failures.push(format!(
                            "{}: output could not be verified as FLAC",
                            track.title
                        )),
                    }
                }
                "failed" => failures.push(format!(
                    "{}: {}",
                    track.title,
                    track.error.unwrap_or_else(|| "provider failed".into())
                )),
                _ => failures.push(format!("{}: unknown provider result", track.title)),
            }
        }
        if !status.success() {
            failures.push("SpotiFLAC exited unsuccessfully.".into());
        }
        let provider_error = spotiflac_sync_error_details(&stderr);
        if !provider_error.is_empty() {
            failures.push(provider_error);
        }
        if plan.1 > 0 && !*make_m3u {
            match verify_existing_sync_audio(
                &folder,
                ffprobe.as_deref(),
                job.request.playlist_folder,
            ) {
                Ok(count) if count >= plan.1 => {}
                Ok(_) => failures.push(
                    "Some tracks marked as present could not be verified in the playlist folder."
                        .into(),
                ),
                Err(error) => failures.push(error),
            }
        }
        if *make_m3u && failures.is_empty() {
            if let Err(error) = verify_spotiflac_m3u(&folder, plan.0, &stderr, ffprobe.as_deref()) {
                failures.push(error);
            }
        }
        if job.cancel.load(Ordering::Relaxed) {
            self.update(app, id, |p| {
                p.status = DownloadStatus::Cancelled;
                p.speed = None;
                p.eta = None;
            });
            return;
        }
        if !failures.is_empty() {
            self.fail(
                app,
                id,
                format!("Playlist sync incomplete:\n{}", failures.join("\n")),
            );
            return;
        }
        self.update(app, id, |p| {
            p.status = DownloadStatus::Completed;
            p.percent = 100.0;
            p.speed = None;
            p.eta = None;
        });
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
        let ffprobe = find_ffprobe(self.paths().ffmpeg.as_deref());
        let mut args = spotiflac_args(&job.request.url, &temp_directory);
        if let Ok(help) = spotiflac_cli_help(&binary) {
            args.extend(spotiflac_optional_args(&help));
        }

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
        // Cancellation can arrive after spawn but before the PID is published.
        if job.cancel.load(Ordering::SeqCst) {
            kill_process(child.id());
        }
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
        let mut skipped = 0usize;
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
                            &temp_directory,
                            ffprobe.as_deref(),
                        ) {
                            Ok(Some(final_path)) => {
                                successes += 1;
                                let rendered = final_path.to_string_lossy().into_owned();
                                self.update(app, id, |p| p.output_path = Some(rendered));
                            }
                            Ok(None) => {
                                skipped += 1;
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
                        kill_process(child.id());
                        break;
                    }
                }
            }
        }
        let status = child.wait();
        job.pid.store(0, Ordering::SeqCst);
        let stderr = error_thread.join().unwrap_or_default();
        if !job.cancel.load(Ordering::Relaxed) {
            let report = parse_report(&plain_output);
            let has_report = report.is_some();
            if let Some(report) = report {
                for track in report.tracks {
                    match track.status.as_str() {
                        "downloaded" => {
                            if let Some(path) = track.file_path {
                                match place_finished_track(
                                    &job.request,
                                    directory,
                                    playlist_folder,
                                    &track.id,
                                    &track.title,
                                    track.artists.as_deref(),
                                    &path,
                                    &temp_directory,
                                    ffprobe.as_deref(),
                                ) {
                                    Ok(Some(final_path)) => {
                                        successes += 1;
                                        let rendered = final_path.to_string_lossy().into_owned();
                                        self.update(app, id, |p| p.output_path = Some(rendered));
                                    }
                                    Ok(None) => skipped += 1,
                                    Err(error) => {
                                        failures.push(format!("{}: {error}", track.title))
                                    }
                                }
                            } else {
                                failures.push(format!(
                                    "{}: provider did not report an output file",
                                    track.title
                                ));
                            }
                        }
                        "failed" => failures.push(format!(
                            "{}: {}",
                            track.title,
                            track
                                .error
                                .unwrap_or_else(|| "provider could not download this track".into())
                        )),
                        "skipped" => { /* The CLI did not prove the destination file is valid. */ }
                        other => failures.push(format!(
                            "{}: unrecognized provider status {other}",
                            track.title
                        )),
                    }
                }
            }
            if !has_report
                && plain_output
                    .iter()
                    .any(|line| line.contains("\"schema_version\""))
            {
                failures.push("SpotiFLAC returned an unsupported or unreadable run report.".into());
            } else if !has_report {
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
                                &temp_directory,
                                ffprobe.as_deref(),
                            ) {
                                Ok(Some(final_path)) => {
                                    successes += 1;
                                    let rendered = final_path.to_string_lossy().into_owned();
                                    self.update(app, id, |p| p.output_path = Some(rendered));
                                }
                                Ok(None) => skipped += 1,
                                Err(error) => failures.push(format!("{title}: {error}")),
                            }
                        }
                    }
                    Err(error) => failures.push(error.to_string()),
                }
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
        if successes == 0 && skipped == 0 {
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
        if !status.as_ref().is_ok_and(|exit| exit.success()) {
            failures.push("SpotiFLAC exited unsuccessfully; some tracks may be missing.".into());
        }
        self.update(app, id, |p| {
            p.status = DownloadStatus::Completed;
            p.percent = 100.0;
            p.speed = None;
            p.eta = None;
            if !failures.is_empty() {
                p.error = Some(format!(
                    "{} issue(s) while processing {} track(s):\n{}",
                    failures.len(),
                    successes + skipped,
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
        let (pid, was_pending) = {
            let mut inner = self
                .inner
                .lock()
                .map_err(|_| PullyError::Internal("Download queue unavailable.".into()))?;
            let job = inner.jobs.get(id).ok_or(PullyError::NotFound)?;
            job.cancel.store(true, Ordering::SeqCst);
            let pid = job.pid.load(Ordering::SeqCst);
            let was_pending = inner.pending.iter().any(|queued| queued == id);
            inner.pending.retain(|queued| queued != id);
            (pid, was_pending)
        };
        if pid > 0 {
            kill_process(pid);
        }
        // A running job owns its terminal status until the process and cleanup
        // have finished. Premature cancellation permits another sync to overlap.
        if was_pending {
            self.update(app, id, |p| p.status = DownloadStatus::Cancelled);
        }
        Ok(())
    }

    pub fn remove(&self, id: &str) -> Result<()> {
        let mut inner = self
            .inner
            .lock()
            .map_err(|_| PullyError::Internal("Download queue unavailable.".into()))?;
        let job = inner.jobs.get(id).ok_or(PullyError::NotFound)?;
        if job.progress.sync_managed && !job.progress.sync_finalized {
            return Err(PullyError::ProcessFailed(
                "The sync result is still being recorded. Please try again shortly.".into(),
            ));
        }
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
    pub fn finalize_sync(&self, app: &AppHandle, id: &str) {
        self.update(app, id, |progress| progress.sync_finalized = true);
    }
    pub fn retry(&self, app: AppHandle, id: &str) -> Result<String> {
        let request = {
            let inner = self
                .inner
                .lock()
                .map_err(|_| PullyError::Internal("Download queue unavailable.".into()))?;
            let job = inner.jobs.get(id).ok_or(PullyError::NotFound)?;
            if job.progress.sync_managed {
                return Err(PullyError::Unsupported(
                    "Retry this job using Sync now on the Synced playlists page.".into(),
                ));
            }
            job.request.clone()
        };
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

/// Reject missing, incomplete, or escaping entries before treating a requested
/// provider-generated playlist as successful.
fn spotify_sync_folder(directory: &Path, sync_id: &str, use_subfolder: bool) -> PathBuf {
    if use_subfolder {
        directory.join(format!("Spotify playlist [{sync_id}]"))
    } else {
        directory.to_path_buf()
    }
}

fn verify_spotiflac_m3u(
    folder: &Path,
    expected: usize,
    stderr: &str,
    ffprobe: Option<&Path>,
) -> std::result::Result<(), String> {
    let summary = stderr
        .split("PLAYLIST SYNC")
        .last()
        .filter(|_| stderr.contains("PLAYLIST SYNC"))
        .ok_or("SpotiFLAC did not report a completed playlist file.")?;
    let lines: Vec<_> = summary.lines().collect();
    let mut candidates = 0;
    for entry in fs::read_dir(folder)
        .map_err(|error| format!("Could not inspect playlist files: {error}"))?
    {
        let entry = entry.map_err(|error| error.to_string())?;
        let path = entry.path();
        if entry
            .file_type()
            .map_err(|error| error.to_string())?
            .is_symlink()
            || !path
                .extension()
                .and_then(|value| value.to_str())
                .is_some_and(|ext| ext.eq_ignore_ascii_case("m3u8"))
        {
            continue;
        }
        // The CLI truncates displayed names to 56 characters. Match its
        // summary row and the following status row so a stale playlist from an
        // earlier name cannot count as the current run's output.
        let Some(name) = path.file_name().and_then(|value| value.to_str()) else {
            continue;
        };
        let displayed: String = name.chars().take(56).collect();
        if !lines.windows(2).any(|rows| {
            rows[0].trim().trim_matches('║').trim() == displayed
                && ["created", "updated", "unchanged"]
                    .iter()
                    .any(|status| rows[1].contains(status))
                && rows[1].contains("track(s)")
                && !rows[1].contains("missing")
        }) {
            continue;
        }
        candidates += 1;
        let content = fs::read_to_string(&path)
            .map_err(|error| format!("Could not read the generated playlist: {error}"))?;
        if !content
            .trim_start_matches('\u{feff}')
            .starts_with("#EXTM3U")
        {
            return Err("SpotiFLAC produced an invalid M3U8 playlist.".into());
        }
        let mut count = 0usize;
        let mut valid = true;
        for line in content
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty() && !line.starts_with('#'))
        {
            let target = folder.join(line);
            match fs::canonicalize(target) {
                Ok(path) if path.starts_with(folder) && path.is_file() && is_flac_stream(&path) => {
                    if let Some(probe) = ffprobe {
                        validate_flac_with_ffprobe(probe, &path).map_err(|error| {
                            format!(
                                "Could not verify playlist track {}: {error}",
                                path.display()
                            )
                        })?;
                    }
                    count += 1;
                }
                _ => {
                    valid = false;
                    break;
                }
            }
        }
        if valid && count >= expected && candidates == 1 {
            continue;
        }
        return Err("SpotiFLAC produced an incomplete or ambiguous M3U8 playlist.".into());
    }
    if candidates != 1 {
        return Err(
            "SpotiFLAC did not produce a complete M3U8 playlist with valid local files.".into(),
        );
    }
    Ok(())
}

fn verify_existing_sync_audio(
    directory: &Path,
    ffprobe: Option<&Path>,
    strict: bool,
) -> std::result::Result<usize, String> {
    let mut verified = 0usize;
    for entry in fs::read_dir(directory)
        .map_err(|error| format!("Could not inspect existing files: {error}"))?
    {
        let entry = entry.map_err(|error| error.to_string())?;
        let kind = entry.file_type().map_err(|error| error.to_string())?;
        if kind.is_symlink() {
            continue;
        }
        if kind.is_dir() {
            verified += verify_existing_sync_audio(&entry.path(), ffprobe, strict)?;
            continue;
        }
        let path = entry.path();
        let extension = path
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        if matches!(
            extension.as_str(),
            "flac" | "mp3" | "m4a" | "aac" | "ogg" | "opus" | "wav"
        ) && (extension != "flac" || !is_flac_stream(&path))
        {
            if !strict {
                continue;
            }
            return Err(format!(
                "An existing audio file is not a verified FLAC stream: {}",
                path.display()
            ));
        }
        if extension == "flac" {
            if let Some(probe) = ffprobe {
                if let Err(error) = validate_flac_with_ffprobe(probe, &path) {
                    if strict {
                        return Err(format!(
                            "Could not verify existing audio {}: {error}",
                            path.display()
                        ));
                    }
                    continue;
                }
            }
            verified += 1;
        }
    }
    Ok(verified)
}

fn spotiflac_playlist_args(url: &str, directory: &Path, m3u: bool) -> Vec<String> {
    vec![
        "--playlist".into(),
        url.into(),
        directory.to_string_lossy().into_owned(),
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
        "--m3u".into(),
        if m3u { "m3u8" } else { "none" }.into(),
    ]
}

fn spotiflac_sync_error_details(stderr: &str) -> String {
    stderr
        .lines()
        .filter(|line| line.contains("[ERROR]") || line.contains("Critical error"))
        .take(3)
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(1200)
        .collect()
}

/// SpotiFLAC announces the track it is starting, rather than one it finished.
/// Reserve the last step for playlist writing and Pully's output verification.
fn spotiflac_playlist_progress(line: &str) -> Option<f64> {
    let rest = line.trim().strip_prefix("Track [")?;
    let (position, rest) = rest.split_once('/')?;
    let (total, _) = rest.split_once(']')?;
    let position: usize = position.parse().ok()?;
    let total: usize = total.parse().ok()?;
    if position == 0 || total == 0 || position > total {
        return None;
    }
    Some(((position - 1) as f64 / total as f64) * 95.0)
}

fn validate_spotiflac_report(
    report: &spotiflac_protocol::RunReport,
    pending: usize,
) -> std::result::Result<(), String> {
    let mut ids = std::collections::HashSet::new();
    if report.tracks.len() != pending
        || report
            .tracks
            .iter()
            .any(|track| track.id.trim().is_empty() || !ids.insert(&track.id))
    {
        return Err("SpotiFLAC returned an incomplete or duplicate track report; sync again to retry missing tracks.".into());
    }
    Ok(())
}

fn parse_spotiflac_sync_plan(stderr: &str) -> Option<(usize, usize, usize)> {
    let line = stderr
        .lines()
        .find(|line| line.contains("[SYNC]") && line.contains("unique track(s)"))?;
    let number_before = |marker: &str| -> Option<usize> {
        line.split(marker)
            .next()?
            .split_whitespace()
            .last()?
            .parse()
            .ok()
    };
    let unique = number_before(" unique track(s)")?;
    let existing = number_before(" already on disk")?;
    let pending = number_before(" to download")?;
    (unique == existing + pending).then_some((unique, existing, pending))
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
    verify_spotiflac_cli(binary).is_ok()
}

pub(crate) fn verify_spotiflac_cli(binary: &Path) -> std::result::Result<(), String> {
    spotiflac_cli_help(binary).map(|_| ())
}

/// Check playlist support and exercise the subscription command in an
/// isolated database before reporting the provider as ready for sync.
pub(crate) fn verify_spotiflac_sync_contract(binary: &Path) -> std::result::Result<(), String> {
    let help = spotiflac_cli_help(binary)?;
    if !["--playlist", "--m3u", "--no-fallback"]
        .iter()
        .all(|flag| spotiflac_help_has_flag(&help, flag))
    {
        return Err("Spotify sync needs a newer SpotiFLAC with playlist support. Update or repair it in Settings ? Status.".into());
    }
    let directory = tempfile::tempdir().map_err(|error| {
        format!("Could not prepare the SpotiFLAC verification workspace: {error}")
    })?;
    let output = crate::process::output_with_timeout(
        crate::process::hidden_command(binary)
            .args(["--subscriptions", "--json"])
            .env("SPOTIFLAC_DB_PATH", directory.path().join("verify.db"))
            .env("PYTHONUTF8", "1")
            .env("PYTHONIOENCODING", "utf-8"),
        std::time::Duration::from_secs(30),
    )
    .map_err(|error| format!("Could not verify SpotiFLAC subscriptions: {error}"))?;
    let subscriptions = serde_json::from_slice::<serde_json::Value>(&output.stdout).ok();
    if !output.status.success()
        || !subscriptions
            .as_ref()
            .is_some_and(|value| value.as_array().is_some_and(Vec::is_empty))
    {
        return Err("SpotiFLAC could not run its subscription command. Update or repair it in Settings ? Status.".into());
    }
    // Subscription options have a separate parser and are absent from the
    // main help. Exercise the backfill check against the isolated empty DB.
    let output = crate::process::output_with_timeout(
        crate::process::hidden_command(binary)
            .args(["--check-subscriptions", "--subscribe-backfill", "--json"])
            .env("SPOTIFLAC_DB_PATH", directory.path().join("verify.db"))
            .env("PYTHONUTF8", "1")
            .env("PYTHONIOENCODING", "utf-8"),
        std::time::Duration::from_secs(30),
    )
    .map_err(|error| format!("Could not verify playlist checks: {error}"))?;
    if !output.status.success()
        || !serde_json::from_slice::<serde_json::Value>(&output.stdout)
            .ok()
            .is_some_and(|value| value.as_array().is_some_and(Vec::is_empty))
    {
        return Err(
            "SpotiFLAC cannot check playlist subscriptions. Update or repair it in Settings."
                .into(),
        );
    }
    Ok(())
}

fn spotiflac_help_has_flag(help: &str, flag: &str) -> bool {
    help.match_indices(flag).any(|(index, _)| {
        let before = help[..index].chars().next_back();
        let after = help[index + flag.len()..].chars().next();
        before.is_none_or(|c| !c.is_ascii_alphanumeric() && c != '-')
            && after.is_none_or(|c| !c.is_ascii_alphanumeric() && c != '-')
    })
}

fn spotiflac_cli_help(binary: &Path) -> std::result::Result<String, String> {
    let output = crate::process::output_with_timeout(
        crate::process::hidden_command(binary)
            .arg("--help")
            .env("PYTHONUTF8", "1")
            .env("PYTHONIOENCODING", "utf-8"),
        std::time::Duration::from_secs(20),
    )
    .map_err(|error| format!("Could not check SpotiFLAC: {error}"))?;
    if !output.status.success() {
        let details = String::from_utf8_lossy(&output.stderr);
        let details = details
            .lines()
            .rev()
            .take(4)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect::<Vec<_>>()
            .join("\n");
        return Err(format!(
            "SpotiFLAC could not start. Try reinstalling it from Settings.\n{}",
            details.chars().take(1200).collect::<String>()
        ));
    }
    let help = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    if !spotiflac_help_supports_contract(&help) {
        return Err("The installed SpotiFLAC CLI does not provide --quality, --registries, and --json. Install a compatible SpotiFLAC version.".into());
    }
    Ok(help)
}

fn spotiflac_optional_args(help: &str) -> Vec<String> {
    let mut args = Vec::new();
    if help.contains("--no-fallback") {
        args.push("--no-fallback".into());
    }
    if help.contains("--retries") {
        args.extend(["--retries".into(), "2".into()]);
    }
    if help.contains("--timeout") {
        args.extend(["--timeout".into(), "120".into()]);
    }
    args
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

fn find_ffprobe(ffmpeg: Option<&Path>) -> Option<PathBuf> {
    let name = if cfg!(windows) {
        "ffprobe.exe"
    } else {
        "ffprobe"
    };
    let adjacent = ffmpeg
        .and_then(Path::parent)
        .map(|parent| parent.join(name));
    adjacent.filter(|path| path.is_file()).or_else(|| {
        std::env::var_os("PATH").and_then(|paths| {
            std::env::split_paths(&paths)
                .map(|dir| dir.join(name))
                .find(|path| path.is_file())
        })
    })
}

fn validate_flac_with_ffprobe(binary: &Path, source: &Path) -> Result<()> {
    let output = crate::process::output_with_timeout(
        crate::process::hidden_command(binary)
            .args([
                "-v",
                "error",
                "-select_streams",
                "a:0",
                "-show_entries",
                "stream=codec_name",
                "-of",
                "json",
            ])
            .arg(source),
        std::time::Duration::from_secs(30),
    )
    .map_err(|error| PullyError::ProcessFailed(format!("Could not verify audio: {error}")))?;
    let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap_or_default();
    let valid = output.status.success()
        && result
            .get("streams")
            .and_then(|s| s.as_array())
            .is_some_and(|streams| {
                streams.iter().any(|stream| {
                    stream.get("codec_name").and_then(|codec| codec.as_str()) == Some("flac")
                })
            });
    if !valid {
        return Err(PullyError::ProcessFailed(
            "FFprobe could not verify a FLAC audio stream.".into(),
        ));
    }
    Ok(())
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
    temp_directory: &Path,
    ffprobe: Option<&Path>,
) -> Result<Option<PathBuf>> {
    let source = PathBuf::from(source_path);
    let root = fs::canonicalize(temp_directory).map_err(|e| PullyError::Internal(e.to_string()))?;
    let canonical = fs::canonicalize(&source)
        .map_err(|e| PullyError::Internal(format!("Cannot inspect downloaded track: {e}")))?;
    if !canonical.starts_with(&root) || !canonical.is_file() || !is_flac_stream(&canonical) {
        return Err(PullyError::Internal(
            "SpotiFLAC returned an invalid FLAC file or a path outside its temporary directory."
                .into(),
        ));
    }
    if let Some(ffprobe) = ffprobe {
        validate_flac_with_ffprobe(ffprobe, &canonical)?;
    }
    let source = canonical;
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
            playlist_indices: None,
            existing_file_behavior: ExistingFileBehavior::Skip,
            embed_metadata: false,
            embed_thumbnail: false,
            save_thumbnail: false,
            download_subtitles: false,
            embed_subtitles: false,
        }
    }

    #[test]
    fn collection_selection_passes_only_requested_indices() {
        let mut selected = request(true);
        selected.is_playlist = true;
        selected.playlist_indices = Some(vec![5, 2, 5]);
        assert_eq!(
            playlist_selection_args(&selected).unwrap(),
            vec!["--playlist-items", "2,5"]
        );
        selected.playlist_indices = Some(vec![]);
        assert!(playlist_selection_args(&selected).is_err());
        selected.playlist_indices = Some(vec![0]);
        assert!(playlist_selection_args(&selected).is_err());
        selected.playlist_indices = Some(vec![1]);
        selected.is_playlist = false;
        assert!(playlist_selection_args(&selected).is_err());
        selected.playlist_indices = None;
        assert!(playlist_selection_args(&selected).unwrap().is_empty());
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
            playlist_indices: None,
            existing_file_behavior,
            embed_metadata: false,
            embed_thumbnail: false,
            save_thumbnail: false,
            download_subtitles: false,
            embed_subtitles: false,
        }
    }

    #[test]
    fn playlist_mode_uses_native_incremental_flags() {
        let args = spotiflac_playlist_args(
            "https://open.spotify.com/playlist/abc",
            Path::new("C:\\Music"),
            true,
        );
        assert_eq!(
            &args[..3],
            &[
                "--playlist",
                "https://open.spotify.com/playlist/abc",
                "C:\\Music"
            ]
        );
        assert!(args.windows(2).any(|pair| pair == ["--m3u", "m3u8"]));
        assert_eq!(
            parse_spotiflac_sync_plan(
                "[SYNC] 12 unique track(s) ? 10 already on disk ? 2 to download"
            ),
            Some((12, 10, 2))
        );
        assert_eq!(
            parse_spotiflac_sync_plan(
                "[SYNC] 12 unique track(s) ? 9 already on disk ? 2 to download"
            ),
            None
        );
    }

    #[test]
    fn spotify_sync_respects_the_selected_folder_layout() {
        let root = Path::new("music");
        assert_eq!(spotify_sync_folder(root, "id", false), root);
        assert_eq!(
            spotify_sync_folder(root, "id", true),
            root.join("Spotify playlist [id]")
        );
    }

    #[test]
    fn reports_progress_when_spotiflac_starts_a_track() {
        assert_eq!(spotiflac_playlist_progress("Track [1/4] First"), Some(0.0));
        assert_eq!(spotiflac_playlist_progress("Track [3/4] Third"), Some(47.5));
        assert_eq!(spotiflac_playlist_progress("Track [5/4] Invalid"), None);
        assert_eq!(spotiflac_playlist_progress("Track [1/0] Invalid"), None);
        assert_eq!(spotiflac_playlist_progress("unrelated output"), None);
    }

    #[test]
    #[ignore = "Requires a locally generated real SpotiFLAC playlist fixture"]
    fn real_spotiflac_playlist_output_passes_validation() {
        let root = std::env::var_os("PULLY_TEST_SPOTIFLAC_FIXTURE").expect("fixture directory");
        let root = PathBuf::from(root);
        let stderr = fs::read_to_string(root.join("stderr.log")).unwrap();
        let stdout = fs::read_to_string(root.join("stdout.log")).unwrap();
        let lines = stdout.lines().map(str::to_owned).collect::<Vec<_>>();
        let plan = parse_spotiflac_sync_plan(&stderr).expect("playlist plan");
        let report = parse_report(&lines).expect("JSON report");
        assert!(plan.0 > 0);
        assert_eq!(plan.2, 0);
        validate_spotiflac_report(&report, plan.2).unwrap();
        assert!(spotiflac_sync_error_details(&stderr).is_empty());
        let output = fs::canonicalize(root.join("output")).unwrap();
        verify_spotiflac_m3u(&output, plan.0, &stderr, find_ffprobe(None).as_deref()).unwrap();
        let failed_stderr = fs::read_to_string(root.join("stderr-failed.log")).unwrap();
        let failed_stdout = fs::read_to_string(root.join("stdout-failed.log")).unwrap();
        let failed_lines = failed_stdout.lines().map(str::to_owned).collect::<Vec<_>>();
        let failed_plan = parse_spotiflac_sync_plan(&failed_stderr).unwrap();
        let failed_report = parse_report(&failed_lines).unwrap();
        assert_eq!(failed_plan.2, 1);
        assert!(validate_spotiflac_report(&failed_report, failed_plan.2).is_err());
    }

    #[test]
    fn rejects_missing_or_duplicate_spotiflac_track_results() {
        let report = |tracks: &str| {
            parse_report(&[format!(r#"{{"schema_version":1,"tracks":[{tracks}]}}"#)]).unwrap()
        };
        let one = r#"{"id":"a","title":"A","status":"downloaded"}"#;
        let two = r#"{"id":"b","title":"B","status":"failed"}"#;
        assert!(validate_spotiflac_report(&report(&format!("{one},{two}")), 2).is_ok());
        assert!(validate_spotiflac_report(&report(one), 2).is_err());
        assert!(validate_spotiflac_report(&report(&format!("{one},{one}")), 2).is_err());
        assert!(validate_spotiflac_report(&report(""), 0).is_ok());
    }

    #[test]
    fn exposes_provider_setup_errors_from_a_zero_exit_sync() {
        let stderr = "[SYNC] 50 unique track(s) ? 0 already on disk ? 50 to download\n[ERROR] SpotiFLAC: No extensions found";
        assert_eq!(parse_spotiflac_sync_plan(stderr), Some((50, 0, 50)));
        assert!(spotiflac_sync_error_details(stderr).contains("No extensions found"));
    }

    #[test]
    fn ffprobe_rejects_a_fake_flac_header() {
        let Some(ffprobe) = find_ffprobe(None) else {
            return;
        };
        let dir = tempfile::tempdir().unwrap();
        let fake = dir.path().join("fake.flac");
        fs::write(&fake, b"fLaC incomplete").unwrap();
        assert!(validate_flac_with_ffprobe(&ffprobe, &fake).is_err());
    }

    #[test]
    fn validates_provider_m3u_without_allowing_missing_or_external_files() {
        let dir = tempfile::tempdir().unwrap();
        let root = fs::canonicalize(dir.path()).unwrap();
        fs::write(root.join("song.flac"), b"fLaC fixture").unwrap();
        fs::write(
            root.join("playlist.m3u8"),
            "#EXTM3U\n#EXTINF:1,Song\nsong.flac\n",
        )
        .unwrap();
        let summary = "║  PLAYLIST SYNC  ║\n║  playlist.m3u8     ║\n║    unchanged · 1 track(s)  ║";
        fs::write(root.join("unrelated.flac"), b"corrupted orphan").unwrap();
        assert!(verify_spotiflac_m3u(&root, 1, summary, None).is_ok());
        assert!(verify_spotiflac_m3u(&root, 2, summary, None).is_err());
        assert!(verify_spotiflac_m3u(
            &root,
            1,
            "PLAYLIST SYNC\nother.m3u8\ncreated · 1 track(s)",
            None
        )
        .is_err());
        assert!(verify_spotiflac_m3u(
            &root,
            1,
            "PLAYLIST SYNC\nplaylist.m3u8\nupdated · 1 track(s) · 1 missing",
            None
        )
        .is_err());
        fs::write(root.join("playlist.m3u8"), "#EXTM3U\n../outside.flac\n").unwrap();
        assert!(verify_spotiflac_m3u(&root, 1, summary, None).is_err());
        fs::write(root.join("playlist.m3u8"), "#EXTM3U\nunrelated.flac\n").unwrap();
        assert!(verify_spotiflac_m3u(&root, 1, summary, None).is_err());
    }

    #[test]
    fn rejects_invalid_existing_playlist_audio() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("bad.flac"), b"not flac").unwrap();
        assert!(verify_existing_sync_audio(dir.path(), None, true).is_err());
        assert_eq!(
            verify_existing_sync_audio(dir.path(), None, false).unwrap(),
            0
        );
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
    fn distinguishes_a_spotiflac_startup_crash_from_missing_options() {
        let directory = tempfile::tempdir().unwrap();
        let binary = directory.path().join(if cfg!(windows) {
            "broken-cli.cmd"
        } else {
            "broken-cli"
        });
        #[cfg(windows)]
        fs::write(
            &binary,
            "@echo off\r\necho --quality --registries --json\r\necho ModuleNotFoundError: _cffi_backend 1>&2\r\nexit /b 1\r\n",
        )
        .unwrap();
        #[cfg(not(windows))]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::write(&binary, "#!/bin/sh\necho '--quality --registries --json'\necho 'ModuleNotFoundError: _cffi_backend' >&2\nexit 1\n").unwrap();
            fs::set_permissions(&binary, fs::Permissions::from_mode(0o700)).unwrap();
        }
        let error = verify_spotiflac_cli(&binary).unwrap_err();
        assert!(error.contains("could not start"));
        assert!(error.contains("_cffi_backend"));
        assert!(!spotiflac_supports_lossless_contract(&binary));
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
    fn verifies_playlist_commands_in_an_isolated_database() {
        assert!(!spotiflac_help_has_flag(
            "--subscribe-backfill",
            "--subscribe"
        ));
        assert!(spotiflac_help_has_flag("[--subscribe URL]", "--subscribe"));
        let dir = tempfile::tempdir().unwrap();
        let binary = dir.path().join(if cfg!(windows) {
            "provider.cmd"
        } else {
            "provider"
        });
        #[cfg(windows)]
        fs::write(&binary, "@echo off\r\nif \"%1\"==\"--help\" (echo --quality --registries --json --playlist --m3u --no-fallback --subscribe --subscriptions --check-subscriptions --subscribe-backfill) else (echo [])\r\n").unwrap();
        #[cfg(not(windows))]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::write(&binary, "#!/bin/sh\nif [ \"$1\" = \"--help\" ]; then echo '--quality --registries --json --playlist --m3u --no-fallback --subscribe --subscriptions --check-subscriptions --subscribe-backfill'; else echo '[]'; fi\n").unwrap();
            fs::set_permissions(&binary, fs::Permissions::from_mode(0o700)).unwrap();
        }
        assert!(verify_spotiflac_sync_contract(&binary).is_ok());
    }

    #[test]
    fn enables_retry_and_lossless_guard_only_when_advertised() {
        let args = spotiflac_optional_args("--no-fallback --retries N --timeout SECONDS");
        assert_eq!(
            args,
            vec!["--no-fallback", "--retries", "2", "--timeout", "120"]
        );
        assert!(spotiflac_optional_args("--quality --json").is_empty());
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
        fs::write(&temp, b"fLaC audio").unwrap();
        let request = audio_request(ExistingFileBehavior::Skip);
        let placed = place_finished_track(
            &request,
            dir.path(),
            false,
            "t1",
            "Song",
            Some("Artist"),
            temp.to_str().unwrap(),
            dir.path(),
            None,
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
        fs::write(&temp, b"fLaC new").unwrap();
        let request = audio_request(ExistingFileBehavior::Skip);
        let placed = place_finished_track(
            &request,
            dir.path(),
            false,
            "t1",
            "Song",
            Some("Artist"),
            temp.to_str().unwrap(),
            dir.path(),
            None,
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
        fs::write(&temp, b"fLaC new").unwrap();
        let request = audio_request(ExistingFileBehavior::Overwrite);
        let placed = place_finished_track(
            &request,
            dir.path(),
            false,
            "t1",
            "Song",
            Some("Artist"),
            temp.to_str().unwrap(),
            dir.path(),
            None,
        )
        .unwrap();
        assert!(placed.is_some());
        assert_eq!(fs::read(&existing).unwrap(), b"fLaC new");
    }

    #[test]
    fn rejects_invalid_or_external_provider_output() {
        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let invalid = root.path().join("invalid.flac");
        let external = outside.path().join("external.flac");
        fs::write(&invalid, b"not flac").unwrap();
        fs::write(&external, b"fLaC data").unwrap();
        let request = audio_request(ExistingFileBehavior::Skip);
        for path in [&invalid, &external] {
            assert!(place_finished_track(
                &request,
                root.path(),
                false,
                "id",
                "Song",
                None,
                path.to_str().unwrap(),
                root.path(),
                None,
            )
            .is_err());
            assert!(path.exists());
        }
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
