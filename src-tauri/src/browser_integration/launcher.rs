//! Lets `pully-native-host` launch Pully itself when an explicit "Open in
//! Pully" / "Quick Download" action arrives from the browser but Pully
//! isn't currently running. Deliberately narrow: the only thing this can
//! ever do is start the *exact* executable Pully itself last recorded,
//! optionally with a URL argument and the `--tray` flag — never an
//! arbitrary path, and never anything derived from browser-supplied data
//! beyond the URL (which goes through the same validation as any other
//! link before it ever gets this far).
//!
//! This is only reachable from an explicit user action (a popup button or
//! context-menu click), never from passive tab-state updates — see
//! `pully-native-host.rs` and docs/BROWSER_INTEGRATION.md.

use std::{fs, io, path::PathBuf, process::Command};

fn breadcrumb_path() -> io::Result<PathBuf> {
    dirs::data_local_dir()
        .map(|dir| dir.join("Pully").join("app-path.txt"))
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "no local app data directory"))
}

/// Called once at Pully startup so a later on-demand launch (from
/// `pully-native-host`) knows exactly where this installed copy lives.
/// Cheap enough to redo unconditionally on every launch, which also means a
/// moved/reinstalled copy of Pully self-heals this automatically.
pub fn record_app_path() {
    let Ok(path) = breadcrumb_path() else {
        return;
    };
    let Ok(exe) = std::env::current_exe() else {
        return;
    };
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let _ = fs::write(&path, exe.to_string_lossy().as_bytes());
}

/// Launches Pully with `url` (and `--tray` if `quick`, so it comes up
/// hidden rather than stealing focus for a download the user didn't ask to
/// review). Silently gives up if Pully has never recorded its own path yet
/// (it's never been run at all) — there's nothing safe to guess here.
pub fn launch(url: &str, quick: bool) {
    let Ok(path) = breadcrumb_path() else {
        return;
    };
    let exe = match fs::read_to_string(&path) {
        Ok(value) => value,
        Err(_) => {
            eprintln!(
                "[pully-native-host] Pully has never recorded its install path; \
                 can't launch it automatically. Open it manually at least once."
            );
            return;
        }
    };
    let exe = exe.trim();
    if exe.is_empty() {
        return;
    }
    let mut command = Command::new(exe);
    command.arg(url);
    if quick {
        command.arg("--tray");
    }
    if let Err(error) = command.spawn() {
        eprintln!("[pully-native-host] failed to launch Pully: {error}");
    }
}
