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

#[derive(Debug, thiserror::Error)]
pub enum SessionError {
    #[error("JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("I/O {path}: {source}")]
    Io { path: String, source: std::io::Error },
}

#[derive(Debug, Clone, thiserror::Error)]
pub enum WatcherError {
    #[error("notify: {0}")]
    Notify(String),
    #[error("I/O: {0}")]
    Io(String),
    #[error("parse: {0}")]
    Parse(#[from] ParseError),
    #[error("lock poisoned")]
    LockPoisoned,
}

impl From<notify::Error> for WatcherError {
    fn from(e: notify::Error) -> Self {
        WatcherError::Notify(e.to_string())
    }
}

impl From<std::io::Error> for WatcherError {
    fn from(e: std::io::Error) -> Self {
        WatcherError::Io(e.to_string())
    }
}

#[derive(Debug, Clone, thiserror::Error)]
pub enum LoomError {
    #[error("session: {0}")]
    Session(String),
    #[error(transparent)]
    Watcher(#[from] WatcherError),
}

impl From<SessionError> for LoomError {
    fn from(e: SessionError) -> Self {
        LoomError::Session(e.to_string())
    }
}
