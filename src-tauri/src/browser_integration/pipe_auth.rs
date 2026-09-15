//! A lightweight same-user authentication check for the local named pipe.
//!
//! `reject_remote_clients(true)` on the pipe (see `ipc.rs`) already blocks
//! network-remote connections, but doesn't restrict *which local process*
//! can open the pipe — without this, any local process could connect
//! directly and send crafted messages, bypassing Chrome's own
//! extension-id check entirely (that check only constrains who can launch
//! `pully-native-host`, not who can talk to Pully's pipe once it exists).
//!
//! This isn't meant to resist a determined local attacker with equal-or-higher
//! privileges — a process running as the same user could read this same
//! token file just as easily as `pully-native-host` does, and a process
//! with *more* privilege (or physical/remote-desktop access to the account)
//! already has far more direct ways to cause harm than impersonating the
//! browser extension. What it closes is the gap where any other local
//! process — accidentally or not — could talk to Pully's pipe without even
//! knowing it needed to look for this file first.
//!
//! Pully writes a fresh random token to a per-user, NTFS-ACL'd location
//! (`%LOCALAPPDATA%\Pully\`, the same directory already used for the
//! native-messaging manifest and the app-path breadcrumb) on every startup.
//! `pully-native-host` reads it and sends it as the very first frame on a
//! new pipe connection, before anything else. Pully closes the connection
//! immediately if that first frame doesn't match.

use std::{fs, io, path::PathBuf};

fn token_path() -> io::Result<PathBuf> {
    dirs::data_local_dir()
        .map(|dir| dir.join("Pully").join("pipe-token.txt"))
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "no local app data directory"))
}

/// Called once at Pully startup. Generates a fresh token (invalidating
/// whatever a previous run left behind) and writes it to the breadcrumb
/// file, returning it so Pully can hold it in memory to check connections
/// against. `None` only if the local app data directory can't be
/// determined/written — in that case the pipe accepts no connections at
/// all (see `ipc.rs`) rather than silently running unauthenticated.
pub fn issue_token() -> Option<String> {
    let path = token_path().ok()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).ok()?;
    }
    let token = uuid::Uuid::new_v4().to_string();
    fs::write(&path, &token).ok()?;
    Some(token)
}

/// Called by `pully-native-host` to read the token it must present.
pub fn read_token() -> Option<String> {
    let path = token_path().ok()?;
    let contents = fs::read_to_string(path).ok()?;
    let trimmed = contents.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}
