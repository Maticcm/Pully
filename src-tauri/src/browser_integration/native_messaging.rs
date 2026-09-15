//! Registers (or re-registers, idempotently, on every Pully startup) the
//! native-messaging host manifest Chrome/Brave/Edge require before they'll
//! let the Pully browser extension launch `pully-native-host`.
//!
//! Deliberately done here at runtime rather than by the installer: the
//! manifest just needs to point at wherever *this* copy of
//! `pully-native-host` actually lives, which Rust can determine and write
//! correctly (via `serde_json`, so path escaping is never a concern) far
//! more simply than generating the same JSON from installer script. It's
//! cheap enough to redo unconditionally on every launch, which also means a
//! moved/reinstalled copy of Pully self-heals its registration automatically.
//!
//! Windows-first, matching the rest of browser integration — see
//! docs/BROWSER_INTEGRATION.md for the macOS/Linux gap and the exact
//! extension-id derivation this module's `EXTENSION_ID` must stay in sync with.

use serde::Serialize;
use std::path::Path;

pub const NATIVE_HOST_ID: &str = "com.pully.native_host";

/// Must match the `key` embedded in `browser-extension/manifest.json` — that
/// key is what makes the extension's id deterministic, and this constant is
/// simply the id Chrome derives from it. See docs/BROWSER_INTEGRATION.md.
pub const EXTENSION_ID: &str = "englocjbholjnjfkjkbbklebfhllfmfd";

#[derive(Serialize)]
struct NativeHostManifest {
    name: &'static str,
    description: &'static str,
    path: String,
    #[serde(rename = "type")]
    kind: &'static str,
    allowed_origins: Vec<String>,
}

/// `current_exe()`/`canonicalize()` on Windows can return a `\\?\`-prefixed
/// (or `\\?\UNC\`-prefixed) extended-length path. That prefix is fine for
/// Win32 APIs in general, but Chrome's native-messaging host loader rejects
/// it outright — the host silently never launches, which looks exactly like
/// "Pully isn't connected" with no other symptom. Strip it before it ever
/// reaches the manifest.
fn normalize_windows_path(path: &Path) -> String {
    let raw = path.to_string_lossy();
    raw.strip_prefix(r"\\?\UNC\")
        .map(|rest| format!(r"\\{rest}"))
        .or_else(|| raw.strip_prefix(r"\\?\").map(str::to_string))
        .unwrap_or_else(|| raw.into_owned())
}

fn manifest_json(host_binary: &Path) -> serde_json::Result<Vec<u8>> {
    let manifest = NativeHostManifest {
        name: NATIVE_HOST_ID,
        description: "Bridges the Pully browser extension to the Pully desktop app",
        path: normalize_windows_path(host_binary),
        kind: "stdio",
        allowed_origins: vec![format!("chrome-extension://{EXTENSION_ID}/")],
    };
    serde_json::to_vec_pretty(&manifest)
}

#[cfg(windows)]
pub fn register(host_binary: &Path) {
    if let Err(error) = windows_impl::try_register(host_binary) {
        eprintln!("[browser-integration] failed to register native messaging host: {error}");
    }
}

#[cfg(not(windows))]
pub fn register(_host_binary: &Path) {
    eprintln!(
        "[browser-integration] native messaging host registration is not implemented on this \
         platform yet; the browser extension will not be able to connect. See \
         docs/BROWSER_INTEGRATION.md."
    );
}

#[cfg(windows)]
mod windows_impl {
    use super::{manifest_json, NATIVE_HOST_ID};
    use std::{fs, io, path::Path, process::Command};

    const REGISTRY_HIVES: [&str; 3] = [
        r"Software\Google\Chrome\NativeMessagingHosts",
        r"Software\BraveSoftware\Brave-Browser\NativeMessagingHosts",
        r"Software\Microsoft\Edge\NativeMessagingHosts",
    ];

    pub fn try_register(host_binary: &Path) -> io::Result<()> {
        let manifest_path = manifest_file_path()?;
        if let Some(parent) = manifest_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&manifest_path, manifest_json(host_binary)?)?;

        for hive_subkey in REGISTRY_HIVES {
            let key = format!(r"HKCU\{hive_subkey}\{NATIVE_HOST_ID}");
            // `reg.exe add` here never touches a shell and every argument is
            // either a fixed literal or a path Pully derived itself — no
            // user- or browser-supplied data ever reaches this command.
            let status = Command::new("reg.exe")
                .args([
                    "add",
                    &key,
                    "/ve",
                    "/d",
                    &manifest_path.to_string_lossy(),
                    "/f",
                ])
                .status();
            if let Err(error) = status {
                eprintln!("[browser-integration] could not write registry key {key}: {error}");
            }
        }
        Ok(())
    }

    fn manifest_file_path() -> io::Result<std::path::PathBuf> {
        dirs::data_local_dir()
            .map(|dir| {
                dir.join("Pully")
                    .join("native-messaging")
                    .join(format!("{NATIVE_HOST_ID}.json"))
            })
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "no local app data directory"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_only_allows_the_pinned_extension_id() {
        let json = manifest_json(Path::new(r"C:\Pully\pully-native-host.exe")).unwrap();
        let value: serde_json::Value = serde_json::from_slice(&json).unwrap();
        assert_eq!(value["type"], "stdio");
        assert_eq!(
            value["allowed_origins"][0],
            format!("chrome-extension://{EXTENSION_ID}/")
        );
        assert_eq!(value["allowed_origins"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn strips_the_windows_extended_length_prefix() {
        assert_eq!(
            normalize_windows_path(Path::new(r"\\?\D:\Projects\Pully\pully-native-host.exe")),
            r"D:\Projects\Pully\pully-native-host.exe"
        );
        assert_eq!(
            normalize_windows_path(Path::new(r"\\?\UNC\server\share\pully-native-host.exe")),
            r"\\server\share\pully-native-host.exe"
        );
        assert_eq!(
            normalize_windows_path(Path::new(r"D:\Projects\Pully\pully-native-host.exe")),
            r"D:\Projects\Pully\pully-native-host.exe"
        );
    }

    #[test]
    fn manifest_path_never_carries_the_extended_length_prefix() {
        let json = manifest_json(Path::new(r"\\?\D:\Pully\pully-native-host.exe")).unwrap();
        let value: serde_json::Value = serde_json::from_slice(&json).unwrap();
        assert_eq!(value["path"], r"D:\Pully\pully-native-host.exe");
    }
}
