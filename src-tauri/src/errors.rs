use serde::Serialize;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum PullyError {
    #[error("InvalidUrl: Please enter a valid HTTP or HTTPS link.")]
    InvalidUrl,
    #[error("DependencyMissing: {0}")]
    DependencyMissing(String),
    #[error("Unsupported: Pully couldn't find downloadable media at this link.\n{0}")]
    Unsupported(String),
    #[error("PermissionDenied: Pully cannot write to the selected folder.\n{0}")]
    PermissionDenied(String),
    #[error("NotFound: That download no longer exists.")]
    NotFound,
    #[error("ProcessFailed: {0}")]
    ProcessFailed(String),
    #[error("Internal: {0}")]
    Internal(String),
}

impl Serialize for PullyError {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

pub type Result<T> = std::result::Result<T, PullyError>;

pub fn map_process_error(stderr: &str) -> PullyError {
    let lower = stderr.to_lowercase();
    if lower.contains("unsupported url") || lower.contains("no video formats found") {
        PullyError::Unsupported(stderr.to_string())
    } else if lower.contains("private video")
        || lower.contains("sign in")
        || lower.contains("login")
        || lower.contains("authentication")
    {
        PullyError::ProcessFailed(format!(
            "This media requires authentication. Pully does not bypass access controls.\n{stderr}"
        ))
    } else if lower.contains("geo") && (lower.contains("block") || lower.contains("available")) {
        PullyError::ProcessFailed(format!(
            "This media is not available in your region.\n{stderr}"
        ))
    } else if lower.contains("no space left") || lower.contains("disk full") {
        PullyError::ProcessFailed(format!(
            "There is not enough disk space to finish this download.\n{stderr}"
        ))
    } else if lower.contains("permission denied") || lower.contains("access is denied") {
        PullyError::PermissionDenied(stderr.to_string())
    } else {
        PullyError::ProcessFailed(stderr.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn maps_unsupported_urls() {
        assert!(matches!(
            map_process_error("ERROR: Unsupported URL"),
            PullyError::Unsupported(_)
        ));
    }
    #[test]
    fn maps_disk_full() {
        assert!(map_process_error("No space left on device")
            .to_string()
            .contains("disk space"));
    }
}
