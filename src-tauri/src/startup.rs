//! Resolves a URL Pully should open from how it was launched — either a raw
//! `http(s)` argument, or the path to a Windows `.url` Internet Shortcut file
//! (what Explorer passes when a registered "Download with Pully" context
//! menu entry is invoked on a link file; see `docs/WINDOWS_INTEGRATION.md`).

use std::{fs, path::Path};

pub fn resolve_launch_arg(arg: &str) -> Option<String> {
    let trimmed = arg.trim();
    if trimmed.is_empty() {
        return None;
    }
    if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
        return Some(trimmed.to_string());
    }
    let path = Path::new(trimmed);
    let is_url_file = path
        .extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("url"));
    if is_url_file {
        return read_url_shortcut(path);
    }
    None
}

fn read_url_shortcut(path: &Path) -> Option<String> {
    let contents = fs::read_to_string(path).ok()?;
    contents
        .lines()
        .find_map(|line| line.strip_prefix("URL="))
        .map(|value| value.trim().to_string())
        .filter(|value| value.starts_with("http://") || value.starts_with("https://"))
}

/// Scans process args (argv[0] is the executable path, so it's skipped) for
/// the first one that resolves to a URL.
pub fn first_url_from_args<S: AsRef<str>>(args: &[S]) -> Option<String> {
    args.iter()
        .skip(1)
        .find_map(|arg| resolve_launch_arg(arg.as_ref()))
}

/// Whether this process was launched to run hidden in the system tray
/// instead of showing its window immediately — used for a Quick Download
/// launched from the browser extension while Pully wasn't already running
/// (see `browser_integration::launcher`). Never set by a normal launch
/// (desktop icon, Start Menu, `.url` file), which always show the window.
pub fn wants_tray<S: AsRef<str>>(args: &[S]) -> bool {
    args.iter().any(|arg| arg.as_ref() == "--tray")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_a_raw_url_argument() {
        assert_eq!(
            resolve_launch_arg("https://example.com/video"),
            Some("https://example.com/video".into())
        );
        assert_eq!(resolve_launch_arg("not-a-url"), None);
    }

    #[test]
    fn reads_the_url_from_an_internet_shortcut_file() {
        let dir = tempfile::tempdir().unwrap();
        let shortcut = dir.path().join("Video.url");
        fs::write(
            &shortcut,
            "[InternetShortcut]\r\nURL=https://example.com/watch?v=1\r\nIconIndex=0\r\n",
        )
        .unwrap();
        assert_eq!(
            resolve_launch_arg(shortcut.to_str().unwrap()),
            Some("https://example.com/watch?v=1".into())
        );
    }

    #[test]
    fn ignores_a_shortcut_with_no_usable_url() {
        let dir = tempfile::tempdir().unwrap();
        let shortcut = dir.path().join("Empty.url");
        fs::write(&shortcut, "[InternetShortcut]\r\n").unwrap();
        assert_eq!(resolve_launch_arg(shortcut.to_str().unwrap()), None);
    }

    #[test]
    fn finds_the_first_url_among_launch_args() {
        let args = vec![
            "Pully.exe".to_string(),
            "--flag".to_string(),
            "https://example.com/a".to_string(),
        ];
        assert_eq!(
            first_url_from_args(&args),
            Some("https://example.com/a".into())
        );
        assert_eq!(first_url_from_args(&["Pully.exe".to_string()]), None);
    }

    #[test]
    fn recognizes_the_tray_flag() {
        assert!(wants_tray(&[
            "Pully.exe".to_string(),
            "https://example.com/a".to_string(),
            "--tray".to_string()
        ]));
        assert!(!wants_tray(&[
            "Pully.exe".to_string(),
            "https://example.com/a".to_string()
        ]));
    }
}
