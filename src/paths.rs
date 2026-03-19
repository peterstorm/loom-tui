use std::path::{Path, PathBuf};

/// Resolved paths for all loom-tui file locations.
/// Pure data structure with no I/O.
#[derive(Debug, Clone)]
pub struct Paths {
    /// Path to the task graph JSON file
    /// Example: <project_root>/.claude/state/active_task_graph.json
    pub task_graph: PathBuf,

    /// Directory containing Claude Code transcript JSONL files for this project
    /// Example: ~/.claude/projects/-home-user-dev-myproject/
    pub transcript_dir: PathBuf,

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
    /// * `HOME` - Used to resolve transcript_dir (~/.claude/projects/PROJECT_HASH/)
    ///   and archive_dir (~/.local/share/loom-tui/sessions/).
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
        let home_path = PathBuf::from(&home);

        let hash = Self::project_hash(project_root);

        Self {
            task_graph: project_root
                .join(".claude")
                .join("state")
                .join("active_task_graph.json"),

            transcript_dir: home_path.join(".claude").join("projects").join(hash),

            archive_dir: home_path
                .join(".local")
                .join("share")
                .join("loom-tui")
                .join("sessions"),
        }
    }

    /// Compute the project hash from an absolute path.
    ///
    /// Replaces all forward slashes with dashes and strips the leading dash.
    /// This matches Claude Code's own project directory naming convention.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::path::Path;
    /// use loom_tui::paths::Paths;
    ///
    /// assert_eq!(
    ///     Paths::project_hash(Path::new("/home/user/dev/myproject")),
    ///     "-home-user-dev-myproject"
    /// );
    /// ```
    pub fn project_hash(project_root: &Path) -> String {
        let raw = project_root.to_string_lossy();
        raw.replace('/', "-")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    // ---------------------------------------------------------------------------
    // project_hash tests
    // ---------------------------------------------------------------------------

    #[test]
    fn project_hash_typical_path() {
        assert_eq!(
            Paths::project_hash(Path::new("/home/user/dev/myproject")),
            "-home-user-dev-myproject"
        );
    }

    #[test]
    fn project_hash_root_path() {
        assert_eq!(Paths::project_hash(Path::new("/")), "-");
    }

    #[test]
    fn project_hash_single_segment() {
        assert_eq!(Paths::project_hash(Path::new("/project")), "-project");
    }

    #[test]
    fn project_hash_deep_path() {
        assert_eq!(
            Paths::project_hash(Path::new("/home/peterstorm/dev/rust/loom-tui")),
            "-home-peterstorm-dev-rust-loom-tui"
        );
    }

    #[test]
    fn project_hash_matches_claude_code_convention() {
        // Claude Code uses leading dash: /home/user -> -home-user
        let hash = Paths::project_hash(Path::new("/any/path"));
        assert!(hash.starts_with('-'), "hash must start with dash to match Claude Code dirs");
    }

    // ---------------------------------------------------------------------------
    // transcript_dir resolution tests
    // ---------------------------------------------------------------------------

    #[test]
    fn transcript_dir_uses_project_hash() {
        let _guard = HomeGuard::set("/home/testuser");
        let project_root = Path::new("/home/testuser/dev/myproject");
        let paths = Paths::resolve(project_root);

        assert_eq!(
            paths.transcript_dir,
            Path::new("/home/testuser/.claude/projects/-home-testuser-dev-myproject")
        );
    }

    #[test]
    fn transcript_dir_fallback_when_no_home() {
        let _guard = HomeGuard::unset();
        let project_root = Path::new("/home/user/project");
        let paths = Paths::resolve(project_root);

        // When HOME is unset, falls back to /tmp
        assert_eq!(
            paths.transcript_dir,
            Path::new("/tmp/.claude/projects/-home-user-project")
        );
    }

    // ---------------------------------------------------------------------------
    // task_graph and archive_dir unchanged
    // ---------------------------------------------------------------------------

    #[test]
    fn task_graph_path_correct() {
        let project_root = Path::new("/home/user/project");
        let paths = Paths::resolve(project_root);
        assert_eq!(
            paths.task_graph,
            Path::new("/home/user/project/.claude/state/active_task_graph.json")
        );
    }

    #[test]
    fn archive_dir_uses_home() {
        assert!(Paths::resolve(Path::new("/test"))
            .archive_dir
            .to_str()
            .unwrap()
            .ends_with(".local/share/loom-tui/sessions"));
    }

    // ---------------------------------------------------------------------------
    // derive tests
    // ---------------------------------------------------------------------------

    #[test]
    fn paths_derive_clone() {
        let paths = Paths::resolve(Path::new("/test"));
        let _cloned = paths.clone();
    }

    #[test]
    fn paths_derive_debug() {
        let paths = Paths::resolve(Path::new("/test"));
        let debug_str = format!("{:?}", paths);
        assert!(debug_str.contains("Paths"));
    }

    // ---------------------------------------------------------------------------
    // Resolve tests
    // ---------------------------------------------------------------------------

    #[test]
    fn test_resolve_paths_task_graph_and_archive() {
        let project_root = Path::new("/home/user/project");
        let paths = Paths::resolve(project_root);

        assert_eq!(
            paths.task_graph,
            Path::new("/home/user/project/.claude/state/active_task_graph.json")
        );

        assert!(paths
            .archive_dir
            .to_str()
            .unwrap()
            .ends_with(".local/share/loom-tui/sessions"));
    }

    #[test]
    fn test_resolve_paths_projects_my_app() {
        let project_root = Path::new("/projects/my-app");
        let paths = Paths::resolve(project_root);

        assert_eq!(
            paths.task_graph,
            Path::new("/projects/my-app/.claude/state/active_task_graph.json")
        );
    }

    // ---------------------------------------------------------------------------
    // Helpers
    // ---------------------------------------------------------------------------

    use std::sync::Mutex;

    static HOME_LOCK: Mutex<()> = Mutex::new(());

    struct HomeGuard {
        original: Option<String>,
        _lock: std::sync::MutexGuard<'static, ()>,
    }

    impl HomeGuard {
        fn set(value: &str) -> Self {
            let lock = HOME_LOCK.lock().unwrap();
            let original = env::var("HOME").ok();
            unsafe { env::set_var("HOME", value) };
            Self { original, _lock: lock }
        }

        fn unset() -> Self {
            let lock = HOME_LOCK.lock().unwrap();
            let original = env::var("HOME").ok();
            unsafe { env::remove_var("HOME") };
            Self { original, _lock: lock }
        }
    }

    impl Drop for HomeGuard {
        fn drop(&mut self) {
            match &self.original {
                Some(val) => unsafe { env::set_var("HOME", val) },
                None => unsafe { env::remove_var("HOME") },
            }
        }
    }
}
