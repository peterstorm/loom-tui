use std::process::Command;

use chrono::{DateTime, Utc};

use crate::model::git_context::{DiffStat, FileChange, GitCommit, GitContext, GitError};

/// Fetch git context (commits + diffstat) for a project within a time range.
pub fn fetch_git_context(
    project_path: &str,
    since: &DateTime<Utc>,
    until: Option<&DateTime<Utc>>,
) -> Result<GitContext, GitError> {
    let after_arg = format!("--after={}", since.to_rfc3339());
    let until_arg = until.map(|u| format!("--before={}", u.to_rfc3339()));

    let mut log_args = vec![
        "-C",
        project_path,
        "log",
        "--oneline",
        "--format=%h %s",
        &after_arg,
    ];

    if let Some(ref u) = until_arg {
        log_args.push(u);
    }

    let log_output = git(&log_args)?;
    let commits: Vec<GitCommit> = log_output
        .lines()
        .filter(|l| !l.is_empty())
        .filter_map(parse_log_line)
        .collect();

    if commits.is_empty() {
        return Ok(GitContext::default());
    }

    // diff --stat from first^..last
    let last_hash = &commits[0].hash;
    let first_hash = &commits[commits.len() - 1].hash;
    let diff_range = format!("{first_hash}^..{last_hash}");
    let diff_args = vec!["-C", project_path, "diff", "--stat", &diff_range];

    let diff_output = match git(&diff_args) {
        Ok(out) => out,
        Err(_) => {
            // first commit has no parent — use diff against empty tree
            let diff_args_alt = vec![
                "-C",
                project_path,
                "diff",
                "--stat",
                "4b825dc642cb6eb9a060e54bf899d15f3f9382256e",
                last_hash,
            ];
            git(&diff_args_alt).unwrap_or_default()
        }
    };

    let diff_lines: Vec<&str> = diff_output.lines().collect();
    let mut file_changes = Vec::new();
    let mut diff_stat = DiffStat::default();

    for line in &diff_lines {
        if let Some(fc) = parse_diff_stat_line(line) {
            file_changes.push(fc);
        } else if let Some(ds) = parse_diff_stat_summary(line) {
            diff_stat = ds;
        }
    }

    Ok(GitContext {
        commits,
        diff_stat,
        file_changes,
    })
}

fn git(args: &[&str]) -> Result<String, GitError> {
    let output = Command::new("git")
        .args(args)
        .output()
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                GitError::GitNotFound
            } else {
                GitError::CommandFailed(format!("git exec failed: {e}"))
            }
        })?;

    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        if stderr.contains("not a git repository") {
            Err(GitError::NotARepo)
        } else {
            Err(GitError::CommandFailed(
                format!("git {}: {}", args.join(" "), stderr.trim()),
            ))
        }
    }
}

/// Parse a `git log --format='%h %s'` line into hash + subject.
fn parse_log_line(line: &str) -> Option<GitCommit> {
    let (hash, subject) = line.split_once(' ')?;
    Some(GitCommit {
        hash: hash.to_string(),
        subject: subject.to_string(),
    })
}

/// Parse a per-file diff stat line like ` src/main.rs | 10 ++++------`.
fn parse_diff_stat_line(line: &str) -> Option<FileChange> {
    let pipe_idx = line.find('|')?;
    let path = line[..pipe_idx].trim().to_string();
    if path.is_empty() {
        return None;
    }

    let rest = line[pipe_idx + 1..].trim();
    // Binary files show "Bin X -> Y bytes"
    if rest.starts_with("Bin") {
        return Some(FileChange {
            path,
            insertions: 0,
            deletions: 0,
        });
    }

    let insertions = rest.matches('+').count() as u32;
    let deletions = rest.matches('-').count() as u32;

    Some(FileChange {
        path,
        insertions,
        deletions,
    })
}

/// Parse the summary line like ` 3 files changed, 42 insertions(+), 17 deletions(-)`.
fn parse_diff_stat_summary(line: &str) -> Option<DiffStat> {
    let trimmed = line.trim();
    if !trimmed.contains("changed") {
        return None;
    }

    let files_changed = extract_number_before(trimmed, "file")
        .or_else(|| extract_number_before(trimmed, "files"))?;
    let insertions = extract_number_before(trimmed, "insertion").unwrap_or(0);
    let deletions = extract_number_before(trimmed, "deletion").unwrap_or(0);

    Some(DiffStat {
        files_changed,
        insertions,
        deletions,
    })
}

/// Extract a number preceding a keyword, e.g. "42 insertions" → 42.
fn extract_number_before(text: &str, keyword: &str) -> Option<u32> {
    let idx = text.find(keyword)?;
    let before = text[..idx].trim();
    let num_str = before.rsplit(|c: char| !c.is_ascii_digit()).next()?;
    num_str.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_log_line_valid() {
        let commit = parse_log_line("abc1234 fix: handle edge case").unwrap();
        assert_eq!(commit.hash, "abc1234");
        assert_eq!(commit.subject, "fix: handle edge case");
    }

    #[test]
    fn parse_log_line_minimal() {
        let commit = parse_log_line("a b").unwrap();
        assert_eq!(commit.hash, "a");
        assert_eq!(commit.subject, "b");
    }

    #[test]
    fn parse_log_line_empty() {
        assert!(parse_log_line("").is_none());
    }

    #[test]
    fn parse_log_line_no_space() {
        assert!(parse_log_line("abc1234").is_none());
    }

    #[test]
    fn parse_diff_stat_line_typical() {
        let fc = parse_diff_stat_line(" src/main.rs | 5 +++--").unwrap();
        assert_eq!(fc.path, "src/main.rs");
        assert_eq!(fc.insertions, 3);
        assert_eq!(fc.deletions, 2);
    }

    #[test]
    fn parse_diff_stat_line_insertions_only() {
        let fc = parse_diff_stat_line(" src/lib.rs | 3 +++").unwrap();
        assert_eq!(fc.insertions, 3);
        assert_eq!(fc.deletions, 0);
    }

    #[test]
    fn parse_diff_stat_line_deletions_only() {
        let fc = parse_diff_stat_line(" old.rs | 2 --").unwrap();
        assert_eq!(fc.insertions, 0);
        assert_eq!(fc.deletions, 2);
    }

    #[test]
    fn parse_diff_stat_line_binary() {
        let fc = parse_diff_stat_line(" image.png | Bin 0 -> 1234 bytes").unwrap();
        assert_eq!(fc.path, "image.png");
        assert_eq!(fc.insertions, 0);
        assert_eq!(fc.deletions, 0);
    }

    #[test]
    fn parse_diff_stat_line_no_pipe() {
        assert!(parse_diff_stat_line("no pipe here").is_none());
    }

    #[test]
    fn parse_diff_stat_summary_full() {
        let ds =
            parse_diff_stat_summary(" 3 files changed, 42 insertions(+), 17 deletions(-)")
                .unwrap();
        assert_eq!(ds.files_changed, 3);
        assert_eq!(ds.insertions, 42);
        assert_eq!(ds.deletions, 17);
    }

    #[test]
    fn parse_diff_stat_summary_one_file() {
        let ds =
            parse_diff_stat_summary(" 1 file changed, 10 insertions(+)").unwrap();
        assert_eq!(ds.files_changed, 1);
        assert_eq!(ds.insertions, 10);
        assert_eq!(ds.deletions, 0);
    }

    #[test]
    fn parse_diff_stat_summary_deletions_only() {
        let ds =
            parse_diff_stat_summary(" 2 files changed, 5 deletions(-)").unwrap();
        assert_eq!(ds.files_changed, 2);
        assert_eq!(ds.insertions, 0);
        assert_eq!(ds.deletions, 5);
    }

    #[test]
    fn parse_diff_stat_summary_not_summary() {
        assert!(parse_diff_stat_summary(" src/main.rs | 5 +++--").is_none());
    }

    #[test]
    fn extract_number_before_found() {
        assert_eq!(extract_number_before("42 insertions(+)", "insertion"), Some(42));
    }

    #[test]
    fn extract_number_before_not_found() {
        assert_eq!(extract_number_before("no keyword here", "insertion"), None);
    }
}
