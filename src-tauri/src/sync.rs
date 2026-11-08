use crate::{
    downloader::DownloadManager,
    extractors::{self, Provider},
    models::{DownloadRequest, DownloadStatus, MediaInfo},
};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::Mutex,
    time::{SystemTime, UNIX_EPOCH},
};
use tauri::{AppHandle, Emitter, Manager, State};
use uuid::Uuid;

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncHistoryEntry {
    finished_at: u64,
    outcome: String,
    details: Option<String>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaylistSnapshot {
    pub(crate) provider: String,
    pub(crate) playlist_id: String,
    pub(crate) item_ids: Vec<String>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncedPlaylist {
    id: String,
    request: DownloadRequest,
    interval_minutes: u64,
    paused: bool,
    next_run: u64,
    last_success: Option<u64>,
    error: Option<String>,
    #[serde(default)]
    history: Vec<SyncHistoryEntry>,
    #[serde(default)]
    snapshot: Option<PlaylistSnapshot>,
    #[serde(default)]
    changes: Option<String>,
    #[serde(default)]
    files: std::collections::HashMap<String, (String, PathBuf)>,
    #[serde(default)]
    write_m3u: bool,
    #[serde(default)]
    spotify_download: bool,
    #[serde(skip_deserializing, default)]
    job_id: Option<String>,
    #[serde(skip)]
    pending_snapshot: Option<PlaylistSnapshot>,
    #[serde(skip)]
    pending_changes: Option<String>,
}

pub struct SyncManager {
    path: PathBuf,
    entries: Mutex<Vec<SyncedPlaylist>>,
    load_error: Option<String>,
}
fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
impl SyncManager {
    pub fn load(path: PathBuf) -> Self {
        let backup = path.with_extension("bak");
        let read = |file: &Path| {
            fs::read(file)
                .ok()
                .and_then(|bytes| serde_json::from_slice::<Vec<SyncedPlaylist>>(&bytes).ok())
        };
        let (entries, load_error) = if let Some(entries) = read(&path) {
            (entries, None)
        } else if let Some(entries) = read(&backup) {
            // Keep both files untouched until the user can recover them. In particular,
            // rotating a corrupt primary would destroy the only valid backup.
            (entries, Some("Sync state requires recovery: restore synced-playlists.bak as synced-playlists.json while Pully is closed.".into()))
        } else if !path.exists() && !backup.exists() {
            (Vec::new(), None)
        } else {
            (
                Vec::new(),
                Some(
                    "Could not read sync state or its backup; existing files were preserved."
                        .into(),
                ),
            )
        };
        let mut entries = entries;
        for entry in &mut entries {
            if extractors::is_spotify_url(&entry.request.url) && !entry.spotify_download {
                if entry
                    .request
                    .output_directory
                    .as_deref()
                    .is_some_and(|path| !path.trim().is_empty())
                {
                    entry.spotify_download = true;
                    entry.request.mode = crate::models::DownloadMode::Audio;
                    entry.request.output_format = "flac".into();
                } else {
                    entry.paused = true;
                    entry.error = Some("This older Spotify subscription has no destination. Remove and re-add it to select a folder.".into());
                }
            }
        }
        Self {
            path,
            entries: Mutex::new(entries),
            load_error,
        }
    }
    fn ensure_writable(&self) -> std::result::Result<(), String> {
        match &self.load_error {
            Some(error) => Err(error.clone()),
            None => Ok(()),
        }
    }
    fn save(&self, entries: &[SyncedPlaylist]) -> std::result::Result<(), String> {
        self.ensure_writable()?;
        save_entries(&self.path, entries)
    }
}
fn save_entries(path: &Path, entries: &[SyncedPlaylist]) -> std::result::Result<(), String> {
    let parent = path.parent().ok_or("Invalid sync state path")?;
    fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    let pending = path.with_extension("new");
    let backup = path.with_extension("bak");
    let bytes = serde_json::to_vec(entries).map_err(|e| e.to_string())?;
    fs::write(&pending, bytes).map_err(|e| e.to_string())?;
    if path.exists() {
        if backup.exists() {
            fs::remove_file(&backup).map_err(|e| e.to_string())?;
        }
        fs::rename(path, &backup).map_err(|e| e.to_string())?;
    }
    if let Err(e) = fs::rename(&pending, path) {
        if backup.exists() {
            let _ = fs::rename(&backup, path);
        }
        return Err(e.to_string());
    }
    Ok(())
}
fn publish(app: &AppHandle, entries: &[SyncedPlaylist]) {
    let _ = app.emit("sync-playlists-changed", entries);
}

fn canonical_spotify_playlist(value: &str) -> Option<String> {
    let url = url::Url::parse(value).ok()?;
    if url.scheme() != "https" || url.host_str()? != "open.spotify.com" {
        return None;
    }
    let mut parts = url.path_segments()?.filter(|part| !part.is_empty());
    let mut kind = parts.next()?;
    if kind.starts_with("intl-") {
        kind = parts.next()?;
    }
    let id = parts.next()?;
    if kind != "playlist"
        || parts.next().is_some()
        || id.len() > 128
        || !id.bytes().all(|c| c.is_ascii_alphanumeric())
    {
        return None;
    }
    Some(format!("https://open.spotify.com/playlist/{id}"))
}

fn same_destination(a: Option<&str>, b: Option<&str>) -> bool {
    match (a, b) {
        (Some(a), Some(b)) if a == b => true,
        (Some(a), Some(b)) => match (fs::canonicalize(a), fs::canonicalize(b)) {
            (Ok(a), Ok(b)) => a == b,
            _ => false,
        },
        _ => false,
    }
}

#[tauri::command]
pub fn list_synced_playlists(
    manager: State<'_, SyncManager>,
) -> std::result::Result<Vec<SyncedPlaylist>, String> {
    manager.ensure_writable()?;
    Ok(manager.entries.lock().map_err(|e| e.to_string())?.clone())
}
#[tauri::command]
pub fn add_synced_playlist(
    app: AppHandle,
    manager: State<'_, SyncManager>,
    mut request: DownloadRequest,
    interval_minutes: u64,
    write_m3u: bool,
) -> std::result::Result<(), String> {
    if !request.is_playlist {
        return Err("Enter a playlist or collection URL.".into());
    }
    let provider = extractors::route(&request.url).map_err(|e| e.to_string())?;
    if !(15..=10080).contains(&interval_minutes) {
        return Err("Choose an interval between 15 minutes and 7 days.".into());
    }
    if request
        .output_directory
        .as_deref()
        .is_none_or(|p| p.trim().is_empty())
    {
        return Err("Choose a destination folder.".into());
    }
    if provider == Provider::SpotiFlac {
        request.url = canonical_spotify_playlist(&request.url)
            .ok_or("Enter a valid Spotify playlist link.")?;
        request.mode = crate::models::DownloadMode::Audio;
        request.output_format = "flac".into();
    }
    request.existing_file_behavior = crate::models::ExistingFileBehavior::Skip;
    request.playlist_indices = None;
    let mut entries = manager.entries.lock().map_err(|e| e.to_string())?;
    if entries.iter().any(|entry| {
        (entry.request.url == request.url
            || (provider == Provider::SpotiFlac
                && canonical_spotify_playlist(&entry.request.url).as_deref()
                    == Some(request.url.as_str())))
            && same_destination(
                entry.request.output_directory.as_deref(),
                request.output_directory.as_deref(),
            )
    }) {
        return Err("This playlist is already synchronized to that folder.".into());
    }
    let mut next = entries.clone();
    next.push(SyncedPlaylist {
        id: Uuid::new_v4().to_string(),
        request,
        interval_minutes,
        paused: false,
        next_run: now(),
        last_success: None,
        error: None,
        history: Vec::new(),
        snapshot: None,
        changes: None,
        files: Default::default(),
        write_m3u,
        spotify_download: provider == Provider::SpotiFlac,
        job_id: None,
        pending_snapshot: None,
        pending_changes: None,
    });
    manager.save(&next)?;
    *entries = next;
    publish(&app, &entries);
    Ok(())
}
#[tauri::command]
pub fn change_synced_playlist(
    app: AppHandle,
    manager: State<'_, SyncManager>,
    id: String,
    action: String,
) -> std::result::Result<(), String> {
    let mut entries = manager.entries.lock().map_err(|e| e.to_string())?;
    let mut next = entries.clone();
    let index = next
        .iter()
        .position(|e| e.id == id)
        .ok_or("Playlist not found.")?;
    match action.as_str() {
        "pause" => next[index].paused = true,
        "resume" => {
            next[index].paused = false;
            next[index].next_run = now();
        }
        "sync" if next[index].job_id.is_none() => {
            next[index].paused = false;
            next[index].next_run = now();
        }
        "folder" | "root" if next[index].job_id.is_none() => {
            next[index].request.playlist_folder = action == "folder";
        }
        "remove" if next[index].job_id.is_none() => {
            next.remove(index);
        }
        "sync" | "remove" | "folder" | "root" => {
            return Err("Wait for the current sync to finish.".into())
        }
        _ => return Err("Unknown action.".into()),
    }
    manager.save(&next)?;
    *entries = next;
    publish(&app, &entries);
    Ok(())
}
pub fn start(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        loop {
            let worker = app.clone();
            let _ = tauri::async_runtime::spawn_blocking(move || tick(&worker)).await;
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
        }
    });
}
fn tick(app: &AppHandle) {
    let sync = app.state::<SyncManager>();
    let downloads = app.state::<DownloadManager>();
    if sync.load_error.is_some() {
        return;
    }
    let Ok(mut entries) = sync.entries.lock() else {
        return;
    };
    let mut changed = false;
    let mut finished_jobs = Vec::new();
    for entry in entries.iter_mut() {
        if let Some(id) = entry.job_id.clone() {
            if id.starts_with("metadata-") {
                continue;
            }
            if let Some(progress) = downloads.progress(&id) {
                if matches!(
                    progress.status,
                    DownloadStatus::Completed | DownloadStatus::Failed | DownloadStatus::Cancelled
                ) {
                    let previous = entry.clone();
                    if let Some(files) = downloads.sync_files(&id) {
                        for (item_id, (extractor, path)) in files {
                            if safe_output_path(&entry.request, &path) {
                                entry.files.insert(item_id, (extractor, path));
                            }
                        }
                    }
                    entry.error = progress.error;
                    if progress.status == DownloadStatus::Completed && entry.error.is_none() {
                        // A provider result must be successful before its snapshot becomes
                        // the baseline; a failed run must not hide changes on retry.
                        if entry.write_m3u && !entry.spotify_download {
                            if let Some(snapshot) = &entry.pending_snapshot {
                                if let Err(error) = write_m3u_for_snapshot(entry, snapshot) {
                                    entry.error =
                                        Some(format!("Could not update playlist file: {error}"));
                                }
                            } else {
                                entry.error = Some(
                                    "Playlist metadata was unavailable after the sync.".into(),
                                );
                            }
                        }
                        if entry.error.is_none() {
                            commit_pending_snapshot(entry);
                            entry.last_success = Some(now());
                        }
                    }
                    entry.pending_snapshot = None;
                    entry.pending_changes = None;
                    if progress.status != DownloadStatus::Completed && entry.error.is_none() {
                        entry.error = Some("Sync was cancelled.".into());
                    }
                    record_history(
                        entry,
                        if progress.status == DownloadStatus::Completed && entry.error.is_none() {
                            "success"
                        } else {
                            "failed"
                        },
                    );
                    entry.job_id = None;
                    finished_jobs.push((id.clone(), previous));
                    entry.next_run = now() + entry.interval_minutes * 60;
                    changed = true;
                }
            } else {
                entry.error = Some(
                    "The sync job was removed from Downloads before its result was recorded."
                        .into(),
                );
                record_history(entry, "failed");
                entry.job_id = None;
                entry.pending_snapshot = None;
                entry.pending_changes = None;
                entry.next_run = now() + entry.interval_minutes * 60;
                changed = true;
            }
        } else if !entry.paused && entry.next_run <= now() {
            let reservation = format!("metadata-{}", Uuid::new_v4());
            entry.job_id = Some(reservation.clone());
            let id = entry.id.clone();
            let url = entry.request.url.clone();
            let handle = app.clone();
            tauri::async_runtime::spawn(async move {
                let worker = handle.clone();
                let worker_id = id.clone();
                let worker_reservation = reservation.clone();
                if tauri::async_runtime::spawn_blocking(move || {
                    prepare_sync(&worker, &worker_id, &worker_reservation, &url)
                })
                .await
                .is_err()
                {
                    let sync = handle.state::<SyncManager>();
                    if let Ok(mut entries) = sync.entries.lock() {
                        if let Some(entry) = entries.iter_mut().find(|entry| {
                            entry.id == id && entry.job_id.as_deref() == Some(&reservation)
                        }) {
                            entry.job_id = None;
                            entry.pending_snapshot = None;
                            entry.pending_changes = None;
                            entry.error = Some(
                                "Playlist analysis stopped unexpectedly. Try syncing again.".into(),
                            );
                            entry.next_run = now() + entry.interval_minutes * 60;
                            record_history(entry, "failed");
                            if let Err(error) = sync.save(&entries) {
                                eprintln!("Could not save playlist sync state: {error}");
                            }
                            publish(&handle, &entries);
                        }
                    };
                }
            });
            changed = true;
        }
    }

    if changed {
        match sync.save(&entries) {
            Ok(()) => {
                for (id, _) in finished_jobs {
                    downloads.finalize_sync(app, &id);
                }
            }
            Err(error) => {
                // Retry recording terminal jobs on the next tick if storage failed.
                for (_, previous) in finished_jobs {
                    if let Some(entry) = entries.iter_mut().find(|entry| entry.id == previous.id) {
                        *entry = previous;
                    }
                }
                eprintln!("Could not save playlist sync state: {error}");
            }
        }
        publish(app, &entries);
    }
}

fn snapshot_from_media(media: &MediaInfo) -> std::result::Result<PlaylistSnapshot, String> {
    if !media.is_playlist
        || media.id.trim().is_empty()
        || media.source.trim().is_empty()
        || media.playlist_count != Some(media.playlist_items.len())
        || media
            .playlist_items
            .iter()
            .any(|item| item.id.trim().is_empty())
    {
        return Err(
            "The provider returned an incomplete playlist; the previous snapshot was retained."
                .into(),
        );
    }
    Ok(PlaylistSnapshot {
        provider: media.source.clone(),
        playlist_id: media.id.clone(),
        item_ids: media
            .playlist_items
            .iter()
            .map(|item| item.id.clone())
            .collect(),
    })
}

fn describe_changes(
    old: Option<&PlaylistSnapshot>,
    new: &PlaylistSnapshot,
) -> std::result::Result<String, String> {
    let Some(old) = old else {
        return Ok(format!("Initial snapshot: {} items", new.item_ids.len()));
    };
    if old.provider != new.provider || old.playlist_id != new.playlist_id {
        return Err("The provider returned a different playlist identity; the previous snapshot was retained.".into());
    }
    use std::collections::HashMap;
    let mut counts: HashMap<&str, i64> = HashMap::new();
    for id in &old.item_ids {
        *counts.entry(id).or_default() -= 1;
    }
    for id in &new.item_ids {
        *counts.entry(id).or_default() += 1;
    }
    let added: i64 = counts.values().filter(|n| **n > 0).sum();
    let removed: i64 = counts.values().filter(|n| **n < 0).map(|n| -n).sum();
    // Compare the relative order of occurrences shared by both snapshots;
    // additions or removals alone should not count as a reorder.
    fn occurrences(ids: &[String]) -> Vec<(String, usize)> {
        let mut seen = HashMap::<&str, usize>::new();
        ids.iter()
            .map(|id| {
                let number = seen.entry(id).or_default();
                *number += 1;
                (id.clone(), *number)
            })
            .collect()
    }
    let old_items = occurrences(&old.item_ids);
    let new_items = occurrences(&new.item_ids);
    let old_set: std::collections::HashSet<_> = old_items.iter().collect();
    let new_set: std::collections::HashSet<_> = new_items.iter().collect();
    let old_shared: Vec<_> = old_items
        .iter()
        .filter(|item| new_set.contains(item))
        .collect();
    let new_shared: Vec<_> = new_items
        .iter()
        .filter(|item| old_set.contains(item))
        .collect();
    let reordered = old_shared != new_shared;
    Ok(format!(
        "{added} added, {removed} removed{}",
        if reordered { "; order changed" } else { "" }
    ))
}

fn prepare_sync(app: &AppHandle, id: &str, reservation: &str, url: &str) {
    let downloads = app.state::<DownloadManager>();
    let spotify = extractors::is_spotify_url(url);
    let analysis = if spotify {
        downloads
            .spotiflac_path()
            .ok_or_else(|| "Install SpotiFLAC from Settings first.".to_string())
            .and_then(|binary| spotiflac_snapshot(&binary, url))
    } else {
        downloads
            .analyze_playlist(url)
            .map_err(|error| error.to_string())
            .and_then(|media| snapshot_from_media(&media))
    };
    let sync = app.state::<SyncManager>();
    let Ok(mut entries) = sync.entries.lock() else {
        return;
    };
    let Some(index) = entries
        .iter()
        .position(|entry| entry.id == id && entry.job_id.as_deref() == Some(reservation))
    else {
        return;
    };
    if entries[index].paused {
        entries[index].job_id = None;
        publish(app, &entries);
        return;
    }
    if spotify && !entries[index].spotify_download {
        entries[index].job_id = None;
        entries[index].paused = true;
        entries[index].error = Some("This older Spotify subscription has no destination. Remove and re-add it to select a folder.".into());
        if let Err(error) = sync.save(&entries) {
            eprintln!("Could not save playlist sync state: {error}");
        }
        publish(app, &entries);
        return;
    }
    let result = analysis.and_then(|snapshot| {
        let summary = describe_changes(entries[index].snapshot.as_ref(), &snapshot)?;
        entries[index].pending_snapshot = Some(snapshot);
        entries[index].pending_changes = Some(summary);
        if spotify {
            return downloads
                .enqueue_spotify_sync(
                    app.clone(),
                    entries[index].request.clone(),
                    entries[index].id.clone(),
                    entries[index].write_m3u,
                    entries[index]
                        .pending_snapshot
                        .as_ref()
                        .unwrap()
                        .item_ids
                        .len(),
                )
                .map_err(|error| error.to_string());
        }
        let archive = sync
            .path
            .parent()
            .ok_or("Sync state directory unavailable")?
            .join("sync-archives")
            .join(format!("{}.txt", id));
        fs::create_dir_all(archive.parent().unwrap()).map_err(|error| error.to_string())?;
        restore_missing_archive_entries(
            &archive,
            &entries[index],
            entries[index].pending_snapshot.as_ref(),
        )?;
        downloads
            .enqueue_with_archive(app.clone(), entries[index].request.clone(), Some(archive))
            .map_err(|error| error.to_string())
    });
    match result {
        Ok(job) => {
            entries[index].job_id = Some(job);
            entries[index].error = None;
        }
        Err(error) => {
            entries[index].job_id = None;
            entries[index].pending_snapshot = None;
            entries[index].pending_changes = None;
            entries[index].error = Some(error);
            entries[index].next_run = now() + entries[index].interval_minutes * 60;
            record_history(&mut entries[index], "failed");
        }
    }
    if let Err(error) = sync.save(&entries) {
        eprintln!("Could not save playlist sync state: {error}");
    }
    publish(app, &entries);
}

#[derive(Deserialize)]
struct SpotiFlacSubscriptionResult {
    url: String,
    kind: String,
    total_releases: usize,
    new_releases: Vec<SpotiFlacSubscriptionItem>,
    error: String,
}
#[derive(Deserialize)]
struct SpotiFlacSubscriptionItem {
    id: String,
    #[serde(rename = "type")]
    kind: String,
}

fn parse_spotiflac_snapshot(
    bytes: &[u8],
    expected_url: &str,
) -> std::result::Result<PlaylistSnapshot, String> {
    let mut results: Vec<SpotiFlacSubscriptionResult> =
        serde_json::from_slice(bytes).map_err(|_| {
            "SpotiFLAC returned unreadable playlist metadata. Update or repair it in Settings."
                .to_string()
        })?;
    if results.len() != 1 {
        return Err("SpotiFLAC returned an unexpected subscription result.".into());
    }
    let result = results.remove(0);
    if !result.error.is_empty() {
        return Err(format!(
            "SpotiFLAC could not read the playlist: {}",
            result.error
        ));
    }
    if result.url != expected_url
        || result.kind != "playlist"
        || result.total_releases == 0
        || result.total_releases != result.new_releases.len()
    {
        return Err("SpotiFLAC returned an empty or incomplete playlist; the previous snapshot was retained.".into());
    }
    let playlist_id = url::Url::parse(expected_url)
        .ok()
        .and_then(|u| {
            u.path_segments()
                .and_then(|mut segments| segments.rfind(|s| !s.is_empty()).map(str::to_owned))
        })
        .ok_or("Invalid Spotify playlist URL.")?;
    let mut item_ids = Vec::with_capacity(result.new_releases.len());
    for item in result.new_releases {
        if item.kind != "track" || item.id.is_empty() {
            return Err("SpotiFLAC returned an item without a stable track ID.".into());
        }
        item_ids.push(item.id);
    }
    Ok(PlaylistSnapshot {
        provider: "SpotiFLAC".into(),
        playlist_id,
        item_ids,
    })
}

fn spotiflac_snapshot(binary: &Path, url: &str) -> std::result::Result<PlaylistSnapshot, String> {
    let dir = tempfile::tempdir()
        .map_err(|error| format!("Could not create a metadata workspace: {error}"))?;
    let database = dir.path().join("subscriptions.db");
    let command = |flag: &str| {
        let mut process = crate::process::hidden_command(binary);
        process
            .arg(flag)
            .env("SPOTIFLAC_DB_PATH", &database)
            .env("PYTHONUTF8", "1")
            .env("PYTHONIOENCODING", "utf-8");
        process
    };
    let mut subscribe = command("--subscribe");
    subscribe.args([url, "--json"]);
    let output =
        crate::process::output_with_timeout(&mut subscribe, std::time::Duration::from_secs(30))
            .map_err(|error| format!("Could not start the SpotiFLAC playlist check: {error}"))?;
    if !output.status.success() {
        return Err("SpotiFLAC does not support playlist subscription checks. Update or repair it in Settings.".into());
    }
    let mut check = command("--check-subscriptions");
    check.args(["--subscribe-backfill", "--json"]);
    let output =
        crate::process::output_with_timeout(&mut check, std::time::Duration::from_secs(120))
            .map_err(|error| format!("SpotiFLAC playlist metadata check failed: {error}"))?;
    if !output.status.success() {
        return Err(
            "SpotiFLAC could not read this playlist. Check the URL and provider availability."
                .into(),
        );
    }
    parse_spotiflac_snapshot(&output.stdout, url)
}

fn safe_output_path(request: &DownloadRequest, path: &Path) -> bool {
    let Some(root) = request.output_directory.as_deref() else {
        return false;
    };
    match (fs::canonicalize(root), fs::canonicalize(path)) {
        (Ok(root), Ok(file)) => file.starts_with(root) && file.is_file(),
        _ => false,
    }
}

fn restore_missing_archive_entries(
    archive: &Path,
    entry: &SyncedPlaylist,
    snapshot: Option<&PlaylistSnapshot>,
) -> std::result::Result<(), String> {
    let Some(snapshot) = snapshot else {
        return Ok(());
    };
    let current: std::collections::HashSet<&str> =
        snapshot.item_ids.iter().map(String::as_str).collect();
    let missing: std::collections::HashSet<String> = entry
        .files
        .iter()
        .filter(|(id, (_, path))| current.contains(id.as_str()) && !path.exists())
        .map(|(id, (extractor, _))| format!("{} {}", extractor.to_ascii_lowercase(), id))
        .collect();
    if missing.is_empty() || !archive.exists() {
        return Ok(());
    }
    let data = fs::read_to_string(archive).map_err(|e| e.to_string())?;
    let mut updated = String::new();
    for line in data.lines() {
        if !missing.contains(line.trim()) {
            updated.push_str(line);
            updated.push('\n');
        }
    }
    let pending = archive.with_extension("new");
    let backup = archive.with_extension("bak");
    fs::write(&pending, updated).map_err(|e| e.to_string())?;
    if backup.exists() {
        fs::remove_file(&backup).map_err(|e| e.to_string())?;
    }
    fs::rename(archive, &backup).map_err(|e| e.to_string())?;
    if let Err(error) = fs::rename(&pending, archive) {
        let _ = fs::rename(&backup, archive);
        return Err(error.to_string());
    }
    Ok(())
}

fn write_m3u_for_snapshot(
    entry: &SyncedPlaylist,
    snapshot: &PlaylistSnapshot,
) -> std::result::Result<(), String> {
    let root = PathBuf::from(
        entry
            .request
            .output_directory
            .as_deref()
            .ok_or("Destination unavailable")?,
    );
    let output = root.join(format!("Pully-{}.m3u8", entry.id));
    let pending = root.join(format!(".Pully-{}.m3u8.new", entry.id));
    let mut contents = String::from("#EXTM3U\n");
    for id in &snapshot.item_ids {
        if let Some((_, path)) = entry.files.get(id) {
            if safe_output_path(&entry.request, path) {
                let value = path.to_string_lossy();
                if !value.contains(['\r', '\n']) {
                    contents.push_str(&value);
                    contents.push('\n');
                }
            }
        }
    }
    fs::write(&pending, contents).map_err(|e| e.to_string())?;
    let backup = root.join(format!(".Pully-{}.m3u8.bak", entry.id));
    if output.exists() {
        if backup.exists() {
            fs::remove_file(&backup).map_err(|e| e.to_string())?;
        }
        fs::rename(&output, &backup).map_err(|e| e.to_string())?;
    }
    if let Err(error) = fs::rename(&pending, &output) {
        if backup.exists() {
            let _ = fs::rename(&backup, &output);
        }
        return Err(error.to_string());
    }
    Ok(())
}

fn commit_pending_snapshot(entry: &mut SyncedPlaylist) {
    if let Some(snapshot) = entry.pending_snapshot.take() {
        entry.snapshot = Some(snapshot);
    }
    if let Some(changes) = entry.pending_changes.take() {
        entry.changes = Some(changes);
    }
}

fn record_history(entry: &mut SyncedPlaylist, outcome: &str) {
    entry.history.push(SyncHistoryEntry {
        finished_at: now(),
        outcome: outcome.into(),
        details: entry.error.clone(),
    });
    if entry.history.len() > 50 {
        entry.history.drain(..entry.history.len() - 50);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spotify_playlist_links_have_a_stable_identity() {
        let expected = Some("https://open.spotify.com/playlist/abc123".to_string());
        assert_eq!(
            canonical_spotify_playlist("https://open.spotify.com/playlist/abc123?si=tracking"),
            expected
        );
        assert_eq!(
            canonical_spotify_playlist("https://open.spotify.com/intl-en/playlist/abc123/"),
            expected
        );
        assert!(canonical_spotify_playlist("https://open.spotify.com/track/abc123").is_none());
        assert!(canonical_spotify_playlist("https://other.spotify.com/playlist/abc123").is_none());
        assert!(canonical_spotify_playlist("https://open.spotify.com/playlist/abc-123").is_none());
        let folder = tempfile::tempdir().unwrap();
        let direct = folder.path().to_string_lossy().into_owned();
        let equivalent = folder.path().join(".").to_string_lossy().into_owned();
        assert!(same_destination(Some(&direct), Some(&equivalent)));
    }
    fn example_playlist() -> SyncedPlaylist {
        let request: DownloadRequest = serde_json::from_value(serde_json::json!({
            "url": "https://example.com/playlist", "title": "Example", "mode": "audio",
            "outputFormat": "original", "isPlaylist": true, "playlistFolder": true,
            "existingFileBehavior": "skip", "embedMetadata": false, "embedThumbnail": false,
            "saveThumbnail": false, "downloadSubtitles": false, "embedSubtitles": false
        }))
        .unwrap();
        SyncedPlaylist {
            id: "test".into(),
            request,
            interval_minutes: 123,
            paused: true,
            next_run: 10,
            last_success: None,
            error: None,
            history: Vec::new(),
            snapshot: None,
            changes: None,
            files: Default::default(),
            write_m3u: false,
            spotify_download: false,
            job_id: Some("old-process-job".into()),
            pending_snapshot: None,
            pending_changes: None,
        }
    }

    #[test]
    fn history_and_custom_interval_survive_restart_but_job_does_not() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sync.json");
        let mut entry = example_playlist();
        entry.error = Some("network error".into());
        record_history(&mut entry, "failed");
        save_entries(&path, &[entry]).unwrap();
        let manager = SyncManager::load(path);
        let entries = manager.entries.lock().unwrap();
        assert_eq!(entries[0].interval_minutes, 123);
        assert!(entries[0].paused);
        assert!(entries[0].job_id.is_none());
        assert_eq!(
            entries[0].history[0].details.as_deref(),
            Some("network error")
        );
    }

    fn snap(ids: &[&str]) -> PlaylistSnapshot {
        PlaylistSnapshot {
            provider: "YoutubeTab".into(),
            playlist_id: "list".into(),
            item_ids: ids.iter().map(|id| id.to_string()).collect(),
        }
    }

    #[test]
    fn detects_changes_and_duplicate_occurrences() {
        assert_eq!(
            describe_changes(Some(&snap(&["a", "b", "a"])), &snap(&["a", "c"])).unwrap(),
            "1 added, 2 removed"
        );
        assert_eq!(
            describe_changes(Some(&snap(&["a", "b"])), &snap(&["b", "a"])).unwrap(),
            "0 added, 0 removed; order changed"
        );
        assert_eq!(
            describe_changes(Some(&snap(&["a", "b"])), &snap(&["b", "c", "a"])).unwrap(),
            "1 added, 0 removed; order changed"
        );
        assert_eq!(
            describe_changes(Some(&snap(&["a", "b"])), &snap(&["a", "c", "b"])).unwrap(),
            "1 added, 0 removed"
        );
        assert!(describe_changes(
            Some(&snap(&["a"])),
            &PlaylistSnapshot {
                playlist_id: "other".into(),
                ..snap(&["a"])
            }
        )
        .is_err());
    }

    #[test]
    #[ignore = "Requires a compatible local SpotiFLAC and public network access"]
    fn live_spotiflac_playlist_snapshots_are_stable() {
        let binary = std::env::var_os("PULLY_TEST_SPOTIFLAC")
            .expect("set PULLY_TEST_SPOTIFLAC to the compatible executable");
        crate::downloader::verify_spotiflac_sync_contract(Path::new(&binary))
            .expect("provider playlist capability check");
        let url = std::env::var("PULLY_TEST_PLAYLIST_URL")
            .unwrap_or_else(|_| "https://open.spotify.com/playlist/37i9dQZF1DXcBWIGoYBM5M".into());
        let first = spotiflac_snapshot(Path::new(&binary), &url).expect("first metadata snapshot");
        let second =
            spotiflac_snapshot(Path::new(&binary), &url).expect("second metadata snapshot");
        assert!(!first.item_ids.is_empty());
        assert_eq!(first.playlist_id, second.playlist_id);
        // The remote list can change; the comparison must still accept both snapshots.
        assert!(describe_changes(Some(&first), &second).is_ok());
    }

    #[test]
    fn accepts_complete_spotiflac_subscription_report_and_rejects_partial_reports() {
        let url = "https://open.spotify.com/playlist/list";
        let report = serde_json::json!([{ "url": url, "kind": "playlist", "total_releases": 2,
            "new_releases": [{"id": "first", "type": "track"}, {"id": "second", "type": "track"}], "error": "" }]);
        let snapshot =
            parse_spotiflac_snapshot(&serde_json::to_vec(&report).unwrap(), url).unwrap();
        assert_eq!(snapshot.item_ids, vec!["first", "second"]);
        let mut incomplete = report.clone();
        incomplete[0]["total_releases"] = 3.into();
        assert!(parse_spotiflac_snapshot(&serde_json::to_vec(&incomplete).unwrap(), url).is_err());
        let mut failed = report;
        failed[0]["error"] = "unavailable".into();
        assert!(parse_spotiflac_snapshot(&serde_json::to_vec(&failed).unwrap(), url).is_err());
    }

    #[test]
    fn legacy_spotify_subscriptions_require_a_destination() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sync.json");
        let mut entry = example_playlist();
        entry.request.url = "https://open.spotify.com/playlist/example".into();
        entry.request.output_directory = None;
        entry.spotify_download = false;
        entry.paused = false;
        save_entries(&path, &[entry]).unwrap();
        let manager = SyncManager::load(path);
        let entries = manager.entries.lock().unwrap();
        assert!(entries[0].paused);
        assert!(entries[0]
            .error
            .as_deref()
            .unwrap()
            .contains("no destination"));
    }

    #[test]
    fn incomplete_metadata_is_never_a_valid_snapshot() {
        let media: MediaInfo = serde_json::from_value(serde_json::json!({
            "id":"list", "url":"https://example.com/list", "title":"list", "source":"Example", "isPlaylist":true,
            "playlistCount":2, "playlistItems":[], "formats":[], "subtitles":[]
        })).unwrap();
        assert!(snapshot_from_media(&media).is_err());
    }

    #[test]
    fn archive_restoration_only_retries_missing_current_items() {
        let dir = tempfile::tempdir().unwrap();
        let archive = dir.path().join("archive.txt");
        let kept = dir.path().join("kept.mp3");
        fs::write(&kept, b"audio").unwrap();
        fs::write(
            &archive,
            "youtube a\nyoutube b\nyoutube removed\nother unrelated\n",
        )
        .unwrap();
        let mut entry = example_playlist();
        entry.snapshot = Some(snap(&["a", "b"]));
        entry.files.insert("a".into(), ("youtube".into(), kept));
        entry.files.insert(
            "b".into(),
            ("youtube".into(), dir.path().join("missing.mp3")),
        );
        entry.files.insert(
            "removed".into(),
            ("youtube".into(), dir.path().join("removed.mp3")),
        );
        restore_missing_archive_entries(&archive, &entry, entry.snapshot.as_ref()).unwrap();
        assert_eq!(
            fs::read_to_string(&archive).unwrap(),
            "youtube a\nyoutube removed\nother unrelated\n"
        );
    }

    #[test]
    fn m3u_uses_snapshot_order_and_omits_missing_and_external_files() {
        let dir = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let a = dir.path().join("a.mp3");
        let b = dir.path().join("b.mp3");
        let foreign = outside.path().join("foreign.mp3");
        for path in [&a, &b, &foreign] {
            fs::write(path, b"audio").unwrap();
        }
        let mut entry = example_playlist();
        entry.request.output_directory = Some(dir.path().to_string_lossy().into_owned());
        entry.snapshot = Some(snap(&["b", "foreign", "missing", "a", "b"]));
        entry.files.insert("a".into(), ("yt".into(), a.clone()));
        entry.files.insert("b".into(), ("yt".into(), b.clone()));
        entry.files.insert("foreign".into(), ("yt".into(), foreign));
        write_m3u_for_snapshot(&entry, entry.snapshot.as_ref().unwrap()).unwrap();
        let output = fs::read_to_string(dir.path().join("Pully-test.m3u8")).unwrap();
        assert_eq!(
            output,
            format!(
                "#EXTM3U\n{}\n{}\n{}\n",
                b.display(),
                a.display(),
                b.display()
            )
        );
    }

    #[test]
    fn pending_snapshot_is_not_persisted_before_success() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sync.json");
        let mut entry = example_playlist();
        entry.snapshot = Some(snap(&["old"]));
        entry.pending_snapshot = Some(snap(&["new"]));
        entry.pending_changes = Some("1 added, 1 removed".into());
        save_entries(&path, &[entry.clone()]).unwrap();
        let manager = SyncManager::load(path);
        let entries = manager.entries.lock().unwrap();
        assert_eq!(entries[0].snapshot.as_ref().unwrap().item_ids, vec!["old"]);
        assert!(entries[0].pending_snapshot.is_none());
        drop(entries);
        commit_pending_snapshot(&mut entry);
        assert_eq!(entry.snapshot.unwrap().item_ids, vec!["new"]);
    }

    #[test]
    fn history_is_bounded() {
        let mut entry = example_playlist();
        for _ in 0..75 {
            record_history(&mut entry, "success");
        }
        assert_eq!(entry.history.len(), 50);
    }

    #[test]
    fn restores_backup_after_interrupted_save() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sync.json");
        save_entries(&path, &[]).unwrap();
        fs::rename(&path, path.with_extension("bak")).unwrap();
        assert!(SyncManager::load(path).entries.lock().unwrap().is_empty());
    }
}
