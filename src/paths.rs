use std::path::{Path, PathBuf};

/// Resolved paths for all loom-tui file locations.
/// Pure data structure with no I/O.
#[derive(Debug, Clone)]
pub struct Paths {
    /// Path to the task graph JSON file
    /// Example: <project_root>/.claude/state/active_task_graph.json
    pub task_graph: PathBuf,

    /// Directory containing agent transcript JSONL files
    /// Example: <project_root>/.claude/state/subagents/
    pub transcripts: PathBuf,

    /// Path to the hook events JSONL file
    /// Example: $TMPDIR/loom-tui/events.jsonl (defaults to /tmp when TMPDIR unset)
    pub events: PathBuf,

    /// Directory containing active agent marker files (*.active)
    /// Example: /tmp/claude-subagents/
    pub active_agents: PathBuf,

    /// Directory for archived session storage
    /// Example: ~/.local/share/loom-tui/sessions/
    pub archive_dir: PathBuf,
}

impl Paths {
    /// Resolves all paths relative to the given project root.
    ///
    /// Pure function: only performs path concatenation and environment variable reads.
    /// Does NOT create directories or verify file existence - that is the caller's responsibility.
    ///
    /// # Arguments
    ///
    /// * `project_root` - The root directory of the project being monitored.
    ///
    /// # Environment
    ///
    /// * `TMPDIR` - If set, used for temp directory paths. Defaults to "/tmp" if unset.
    /// * `HOME` - Used to resolve the archive directory (~/.local/share/loom-tui/sessions/).
    ///
    /// # Examples
    ///
    /// ```
    /// use std::path::Path;
    /// use loom_tui::paths::Paths;
    ///
    /// let project_root = Path::new("/home/user/project");
    /// let paths = Paths::resolve(project_root);
    ///
    /// assert_eq!(
    ///     paths.task_graph,
    ///     Path::new("/home/user/project/.claude/state/active_task_graph.json")
    /// );
    /// ```
    pub fn resolve(project_root: &Path) -> Self {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
        let home_path = PathBuf::from(home);

        Self {
            task_graph: project_root
                .join(".claude")
                .join("state")
                .join("active_task_graph.json"),

            transcripts: project_root.join(".claude").join("state").join("subagents"),

            // Always use /tmp (not $TMPDIR) so TUI and hook scripts agree on path.
            // Hooks run outside nix-shell where TMPDIR differs.
            events: PathBuf::from("/tmp").join("loom-tui").join("events.jsonl"),

            active_agents: PathBuf::from("/tmp").join("claude-subagents"),

            archive_dir: home_path
                .join(".local")
                .join("share")
                .join("loom-tui")
                .join("sessions"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    #[test]
    fn test_resolve_paths_with_default_tmpdir() {
        // Remove TMPDIR if set to test default behavior
        let _guard = TmpdirGuard::unset();

        let project_root = Path::new("/home/user/project");
        let paths = Paths::resolve(project_root);

        assert_eq!(
            paths.task_graph,
            Path::new("/home/user/project/.claude/state/active_task_graph.json")
        );

        assert_eq!(
            paths.transcripts,
            Path::new("/home/user/project/.claude/state/subagents")
        );

        assert_eq!(paths.events, Path::new("/tmp/loom-tui/events.jsonl"));

        assert_eq!(
            paths.active_agents,
            Path::new("/tmp/claude-subagents")
        );

        // archive_dir depends on HOME, which varies - just verify it ends correctly
        assert!(paths
            .archive_dir
            .to_str()
            .unwrap()
            .ends_with(".local/share/loom-tui/sessions"));
    }

    #[test]
    fn test_resolve_paths_with_custom_tmpdir() {
        let _guard = TmpdirGuard::set("/var/tmp");

        let project_root = Path::new("/projects/my-app");
        let paths = Paths::resolve(project_root);

        assert_eq!(
            paths.task_graph,
            Path::new("/projects/my-app/.claude/state/active_task_graph.json")
        );

        assert_eq!(
            paths.transcripts,
            Path::new("/projects/my-app/.claude/state/subagents")
        );

        // events always uses /tmp, not $TMPDIR (hooks run outside nix-shell)
        assert_eq!(
            paths.events,
            Path::new("/tmp/loom-tui/events.jsonl")
        );

        // active_agents always uses /tmp, not $TMPDIR
        assert_eq!(
            paths.active_agents,
            Path::new("/tmp/claude-subagents")
        );
    }

    #[test]
    fn test_paths_derive_clone() {
        let project_root = Path::new("/test");
        let paths = Paths::resolve(project_root);
        let _cloned = paths.clone();
    }

    #[test]
    fn test_paths_derive_debug() {
        let project_root = Path::new("/test");
        let paths = Paths::resolve(project_root);
        let debug_str = format!("{:?}", paths);
        assert!(debug_str.contains("Paths"));
    }

    // Test helper: RAII guard for setting/unsetting TMPDIR
    struct TmpdirGuard {
        original: Option<String>,
    }

    impl TmpdirGuard {
        fn set(value: &str) -> Self {
            let original = env::var("TMPDIR").ok();
            env::set_var("TMPDIR", value);
            Self { original }
        }

        fn unset() -> Self {
            let original = env::var("TMPDIR").ok();
            env::remove_var("TMPDIR");
            Self { original }
        }
    }

    impl Drop for TmpdirGuard {
        fn drop(&mut self) {
            match &self.original {
                Some(val) => env::set_var("TMPDIR", val),
                None => env::remove_var("TMPDIR"),
            }
        }
    }
}
