/// Git context data for a session — commits and diffstat within its time range.
#[derive(Debug, Clone)]
pub struct GitContext {
    pub commits: Vec<GitCommit>,
    pub diff_stat: DiffStat,
    pub file_changes: Vec<FileChange>,
}

impl Default for GitContext {
    fn default() -> Self {
        Self {
            commits: Vec::new(),
            diff_stat: DiffStat::default(),
            file_changes: Vec::new(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct GitCommit {
    pub hash: String,
    pub subject: String,
}

#[derive(Debug, Clone, Default)]
pub struct DiffStat {
    pub files_changed: u32,
    pub insertions: u32,
    pub deletions: u32,
}

#[derive(Debug, Clone)]
pub struct FileChange {
    pub path: String,
    pub insertions: u32,
    pub deletions: u32,
}

#[derive(Debug, Clone)]
pub enum GitError {
    NotARepo,
    GitNotFound,
    CommandFailed(String),
}
