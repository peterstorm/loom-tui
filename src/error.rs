//! Error types for loom-tui
//!
//! Domain-specific error enums using thiserror for exhaustive error handling.
//! Follows rust-patterns.md guidelines: no stringly-typed errors, transparent
//! wrappers where appropriate, Display impls via thiserror macros.

#[derive(Debug, Clone, thiserror::Error)]
pub enum ParseError {
    #[error("JSON parse: {0}")]
    Json(String),
    #[error("invalid format: {0}")]
    InvalidFormat(String),
}

#[derive(Debug, Clone, thiserror::Error)]
pub enum SessionError {
    #[error("JSON: {0}")]
    Json(String),
    #[error("I/O {path}: {message}")]
    Io { path: String, message: String },
}

impl From<serde_json::Error> for SessionError {
    fn from(e: serde_json::Error) -> Self {
        SessionError::Json(e.to_string())
    }
}

#[derive(Debug, Clone, thiserror::Error)]
pub enum WatcherError {
    #[error("I/O: {0}")]
    Io(String),
    #[error("parse: {0}")]
    Parse(#[from] ParseError),
}

impl From<std::io::Error> for WatcherError {
    fn from(e: std::io::Error) -> Self {
        WatcherError::Io(e.to_string())
    }
}

#[derive(Debug, Clone, thiserror::Error)]
pub enum LoomError {
    #[error("session: {0}")]
    Session(#[from] SessionError),
    #[error(transparent)]
    Watcher(#[from] WatcherError),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn watcher_error_io_display() {
        let error = WatcherError::Io("disk error".to_string());
        assert!(error.to_string().contains("disk error"));
    }

    #[test]
    fn watcher_error_from_io_error() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "not found");
        let watcher_err = WatcherError::from(io_err);
        assert!(watcher_err.to_string().contains("not found"));
    }

    #[test]
    fn loom_error_from_watcher_error() {
        let watcher_err = WatcherError::Io("x".to_string());
        let loom_err = LoomError::from(watcher_err);
        assert!(loom_err.to_string().contains("x"));
    }
}
