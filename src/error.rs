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

#[derive(Debug, thiserror::Error)]
pub enum HookInstallError {
    #[error("failed to create hooks directory {path}: {source}")]
    CreateDir {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to write hook script {path}: {source}")]
    WriteScript {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to set executable permission on {path}: {source}")]
    SetPermissions {
        path: String,
        #[source]
        source: std::io::Error,
    },
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hook_install_error_create_dir_display() {
        let error = HookInstallError::CreateDir {
            path: "/project/.claude/hooks".to_string(),
            source: std::io::Error::new(std::io::ErrorKind::PermissionDenied, "permission denied"),
        };
        let display = error.to_string();
        assert!(display.contains("failed to create hooks directory"));
        assert!(display.contains("/project/.claude/hooks"));
        assert!(display.contains("permission denied"));
    }

    #[test]
    fn test_hook_install_error_write_script_display() {
        let error = HookInstallError::WriteScript {
            path: "/project/.claude/hooks/send_event.sh".to_string(),
            source: std::io::Error::new(std::io::ErrorKind::PermissionDenied, "permission denied"),
        };
        let display = error.to_string();
        assert!(display.contains("failed to write hook script"));
        assert!(display.contains("/project/.claude/hooks/send_event.sh"));
        assert!(display.contains("permission denied"));
    }

    #[test]
    fn test_hook_install_error_set_permissions_display() {
        let error = HookInstallError::SetPermissions {
            path: "/project/.claude/hooks/send_event.sh".to_string(),
            source: std::io::Error::new(std::io::ErrorKind::PermissionDenied, "permission denied"),
        };
        let display = error.to_string();
        assert!(display.contains("failed to set executable permission"));
        assert!(display.contains("/project/.claude/hooks/send_event.sh"));
        assert!(display.contains("permission denied"));
    }

    #[test]
    fn test_hook_install_error_preserves_io_error_kind() {
        let error = HookInstallError::CreateDir {
            path: "/test/path".to_string(),
            source: std::io::Error::new(std::io::ErrorKind::NotFound, "not found"),
        };

        // Verify we can access the source error
        let source_err = std::error::Error::source(&error).unwrap();
        let io_err = source_err.downcast_ref::<std::io::Error>().unwrap();
        assert_eq!(io_err.kind(), std::io::ErrorKind::NotFound);
    }
}
