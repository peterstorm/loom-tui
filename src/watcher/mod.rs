mod parsers;
mod tail;

pub use parsers::*;
pub use tail::TailState;

use crate::error::WatcherError;
use crate::event::AppEvent;
use crate::paths::Paths;
use crate::session;
use notify::{Config, RecommendedWatcher, RecursiveMode, Watcher};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::Duration;

/// Result type for watcher operations
pub type WatcherResult<T> = Result<T, WatcherError>;

/// Commands sent to the tail worker thread
enum TailCommand {
    ReadFile(PathBuf),
    /// Sentinel: initial file replay done, safe to run stale cleanup
    ReplayDone,
}

/// Start a dedicated worker thread that owns TailState and processes file read requests.
/// Eliminates Arc<Mutex<TailState>> anti-pattern by using message passing.
///
/// # Returns
/// - Sender for file paths to read
fn start_tail_worker(tx: mpsc::Sender<AppEvent>) -> mpsc::Sender<TailCommand> {
    let (cmd_tx, cmd_rx) = mpsc::channel();

    std::thread::spawn(move || {
        let mut tail_state = TailState::new();

        while let Ok(cmd) = cmd_rx.recv() {
            match cmd {
                TailCommand::ReplayDone => {
                    let _ = tx.send(AppEvent::ReplayComplete);
                }
                TailCommand::ReadFile(path) => {
                    let new_content = match tail_state.read_new_lines(&path) {
                        Ok(content) => content,
                        Err(e) => {
                            if tx.send(AppEvent::Error {
                                source: path.display().to_string(),
                                error: WatcherError::Io(e.to_string()).into(),
                            }).is_err() {
                                return;
                            }
                            continue;
                        }
                    };

                    if new_content.is_empty() {
                        continue;
                    }

                    match parsers::parse_hook_events(&new_content) {
                        Ok(events) => {
                            for event in events {
                                let enriched = if matches!(event.kind, crate::model::HookEventKind::SessionStart) {
                                    enrich_session_start_event(event)
                                } else {
                                    event
                                };
                                if tx.send(AppEvent::HookEventReceived(enriched)).is_err() {
                                    return;
                                }
                            }
                        }
                        Err(e) => {
                            if tx.send(AppEvent::Error {
                                source: path.display().to_string(),
                                error: WatcherError::Parse(e).into(),
                            }).is_err() {
                                return;
                            }
                        }
                    }
                }
            }
        }
    });

    cmd_tx
}

/// Resolve the Claude Code transcript directory for a given project root.
/// Returns ~/.claude/projects/{project_hash}/ where project_hash = cwd with / → -
pub fn transcript_dir_for_project(project_root: &Path) -> Option<PathBuf> {
    let project_hash = project_root.display().to_string().replace('/', "-");
    let home = std::env::var("HOME").ok()?;
    let dir = PathBuf::from(format!("{}/.claude/projects/{}", home, project_hash));
    if dir.is_dir() { Some(dir) } else { None }
}

/// Derive transcript path from cwd + session_id when not provided by hook.
/// Claude Code stores transcripts at ~/.claude/projects/{project_hash}/{session_id}.jsonl
/// where project_hash is the cwd with `/` replaced by `-`.
///
/// # Imperative Shell
/// This function performs filesystem I/O (Path::exists check) and belongs in the shell.
pub fn derive_transcript_path(cwd: &str, session_id: &str) -> Option<String> {
    if cwd.is_empty() || session_id.is_empty() {
        return None;
    }
    let project_hash = cwd.replace('/', "-");
    let home = std::env::var("HOME").ok()?;
    let path = format!("{}/.claude/projects/{}/{}.jsonl", home, project_hash, session_id);
    if std::path::Path::new(&path).exists() {
        Some(path)
    } else {
        None
    }
}

/// Starts file watching for all paths and returns a channel for receiving events.
/// Debounces file changes at 200ms.
///
/// # Imperative Shell
/// This function handles I/O setup (file watching) but delegates parsing to pure functions.
///
/// # Returns
/// Channel receiver for AppEvent stream
pub fn start_watching(
    paths: &Paths,
    transcript_dir: Option<PathBuf>,
) -> WatcherResult<mpsc::Receiver<AppEvent>> {
    let (tx, rx) = mpsc::channel();

    // Start dedicated worker thread that owns TailState
    let tail_worker = start_tail_worker(tx.clone());

    // Clone tx for watcher callback; keep original for initial reads
    let tx_watcher = tx.clone();

    // Clone paths for move into watcher thread
    let task_graph_path = paths.task_graph.clone();
    let transcripts_dir = paths.transcripts.clone();
    let events_path = paths.events.clone();
    let tail_worker_watcher = tail_worker.clone();

    // Create notify watcher with 200ms debounce
    let mut watcher = RecommendedWatcher::new(
        move |res: Result<notify::Event, notify::Error>| {
            handle_watch_event(
                res,
                &task_graph_path,
                &transcripts_dir,
                &events_path,
                &tail_worker_watcher,
                &tx_watcher,
            );
        },
        Config::default().with_poll_interval(Duration::from_millis(200)),
    )?;

    // Watch all paths (no longer watching active_agents — derived from hook events)
    watch_path(&mut watcher, &paths.task_graph)?;
    watch_path(&mut watcher, &paths.transcripts)?;

    // Watch events file's parent dir so we catch file creation + modifications
    // even if events.jsonl doesn't exist yet at startup
    if let Some(events_dir) = paths.events.parent() {
        std::fs::create_dir_all(events_dir)?;
        watch_path(&mut watcher, events_dir)?;
    }

    // Initial read of existing files
    load_existing_files(paths, &tail_worker, &tx);

    // Periodic polling of events file — ensures real-time updates even if inotify
    // misses appends (common on tmpfs). TailState deduplicates so no double-processing.
    let events_path_poll = paths.events.clone();
    let tail_worker_poll = tail_worker.clone();

    std::thread::spawn(move || {
        loop {
            std::thread::sleep(Duration::from_millis(200));
            if events_path_poll.exists()
                && tail_worker_poll.send(TailCommand::ReadFile(events_path_poll.clone())).is_err() {
                    return;
                }
        }
    });

    // Poll Claude Code transcript files for assistant reasoning text
    if let Some(dir) = transcript_dir {
        start_transcript_polling(dir, tx);
    }

    // Keep watcher alive by moving it to a separate thread
    std::thread::spawn(move || {
        let _watcher = watcher;
        loop {
            std::thread::sleep(Duration::from_secs(1));
        }
    });

    Ok(rx)
}

/// Start polling Claude Code transcript files for assistant reasoning text.
/// Polls files every 200ms, rescans directory every 2s for recently modified .jsonl files,
/// tails new content, and emits AssistantText events.
/// Discovers subagent transcripts in `subagents/` subdirs and emits metadata events.
///
/// Scans ALL project directories under `~/.claude/projects/` (not just the current project)
/// because hook events from all Claude Code sessions arrive in the same events.jsonl.
/// Without cross-project scanning, subagent tool events from other projects never get
/// agent_id attribution via dedup upgrade.
fn start_transcript_polling(transcript_dir: PathBuf, tx: mpsc::Sender<AppEvent>) {
    // Derive ~/.claude/projects/ root from the primary transcript dir
    let projects_root = transcript_dir.parent().map(|p| p.to_path_buf());

    std::thread::spawn(move || {
        let mut tail_state = TailState::new();
        // value: (modified_time, is_subagent)
        let mut known_files: BTreeMap<PathBuf, (std::time::SystemTime, bool)> = BTreeMap::new();
        let mut scan_counter: u32 = 0;
        let mut is_metadata_tick;

        loop {
            std::thread::sleep(Duration::from_millis(200));
            scan_counter = scan_counter.wrapping_add(1);
            is_metadata_tick = scan_counter % 10 == 1;

            // Rescan directories every ~2s (10 iterations) to discover new files
            if scan_counter % 10 == 1 {
                let cutoff = std::time::SystemTime::now() - Duration::from_secs(3600);

                // Collect all project dirs to scan: primary + all others under ~/.claude/projects/
                let mut project_dirs = vec![transcript_dir.clone()];
                if let Some(ref root) = projects_root {
                    if let Ok(entries) = std::fs::read_dir(root) {
                        for entry in entries.flatten() {
                            let path = entry.path();
                            if path.is_dir() && path != transcript_dir {
                                project_dirs.push(path);
                            }
                        }
                    }
                }

                for proj_dir in &project_dirs {
                    scan_project_dir(proj_dir, cutoff, &mut known_files);
                }
            }

            // Tail known files for new content
            let paths: Vec<(PathBuf, bool)> = known_files
                .iter()
                .map(|(p, (_, is_sub))| (p.clone(), *is_sub))
                .collect();

            for (path, is_subagent) in paths {
                if !path.exists() {
                    continue;
                }
                let new_content = match tail_state.read_new_lines(&path) {
                    Ok(c) => c,
                    Err(e) => {
                        if tx.send(AppEvent::Error {
                            source: path.display().to_string(),
                            error: WatcherError::Io(e.to_string()).into(),
                        }).is_err() {
                            return;
                        }
                        continue;
                    }
                };

                // For subagent transcripts ({session_id}/subagents/agent-{id}.jsonl),
                // extract session_id from grandparent dir, not filename.
                let session_id: String = if is_subagent {
                    path.parent()                         // subagents/
                        .and_then(|p| p.parent())         // {session_id}/
                        .and_then(|p| p.file_name())
                        .and_then(|s| s.to_str())
                        .unwrap_or("unknown")
                        .to_string()
                } else {
                    path.file_stem()
                        .and_then(|s| s.to_str())
                        .unwrap_or("unknown")
                        .to_string()
                };

                // Parse incremental content for assistant text + tool progress
                if !new_content.is_empty() {
                    let events =
                        parsers::parse_claude_transcript_incremental(&new_content, &session_id);
                    for event in events {
                        if tx.send(AppEvent::HookEventReceived(event)).is_err() {
                            return;
                        }
                    }

                    // Note: parse_agent_progress_tool_calls removed — subagent transcripts
                    // now produce properly-attributed tool events via
                    // parse_claude_transcript_incremental (assistant entries with agentId).
                }

                // For subagent transcripts, parse metadata from the FULL file on the ~2s
                // rescan tick (not every 200ms). Not gated on new_content — the agent
                // might not be registered yet when the first chunk arrives. Full-file parse
                // is cheap and the update handler uses SET semantics (idempotent).
                if is_subagent && is_metadata_tick {
                    let full_content = match std::fs::read_to_string(&path) {
                        Ok(c) => c,
                        Err(_) => continue,
                    };
                    let metadata = parsers::parse_transcript_metadata(&full_content);
                    if metadata.model.is_some() || !metadata.token_usage.is_empty() || !metadata.skills.is_empty() {
                        // Extract agent_id from filename (agent-{id}.jsonl)
                        let file_stem = path.file_stem()
                            .and_then(|s| s.to_str())
                            .unwrap_or("unknown");
                        let agent_id = file_stem
                            .strip_prefix("agent-")
                            .unwrap_or(file_stem)
                            .to_string();
                        if tx.send(AppEvent::AgentMetadataUpdated {
                            agent_id: agent_id.into(),
                            metadata,
                        }).is_err() {
                            return;
                        }
                    }
                }
            }
        }
    });
}

/// Scan a single project dir for .jsonl transcripts and per-session subagent dirs.
fn scan_project_dir(
    proj_dir: &Path,
    cutoff: std::time::SystemTime,
    known_files: &mut BTreeMap<PathBuf, (std::time::SystemTime, bool)>,
) {
    let entries = match std::fs::read_dir(proj_dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) == Some("jsonl") {
            let modified = entry
                .metadata()
                .ok()
                .and_then(|m| m.modified().ok())
                .unwrap_or(std::time::UNIX_EPOCH);
            if modified > cutoff {
                known_files.entry(path).or_insert((modified, false));
            }
        } else if path.is_dir() {
            // Check for flat subagents/ at project root
            if path.file_name().and_then(|n| n.to_str()) == Some("subagents") {
                scan_subagents_dir(&path, cutoff, known_files);
            } else {
                // Per-session dir: {session_id}/subagents/
                let subagents_dir = path.join("subagents");
                if subagents_dir.is_dir() {
                    scan_subagents_dir(&subagents_dir, cutoff, known_files);
                }
            }
        }
    }
}

/// Scan a subagents/ directory for .jsonl files and add them to known_files.
fn scan_subagents_dir(
    dir: &Path,
    cutoff: std::time::SystemTime,
    known_files: &mut BTreeMap<PathBuf, (std::time::SystemTime, bool)>,
) {
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) != Some("jsonl") {
                continue;
            }
            let modified = entry
                .metadata()
                .ok()
                .and_then(|m| m.modified().ok())
                .unwrap_or(std::time::UNIX_EPOCH);
            if modified > cutoff {
                known_files.entry(path).or_insert((modified, true));
            }
        }
    }
}

/// Read existing files on startup so the TUI doesn't start empty.
/// Agent lifecycle is derived from hook events (SubagentStart/SubagentStop),
/// not from .active marker files.
fn load_existing_files(
    paths: &Paths,
    tail_worker: &mpsc::Sender<TailCommand>,
    tx: &mpsc::Sender<AppEvent>,
) {
    if paths.task_graph.exists() {
        handle_task_graph_update(&paths.task_graph, tx);
    }

    if paths.transcripts.is_dir() {
        if let Ok(entries) = std::fs::read_dir(&paths.transcripts) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension() == Some("jsonl".as_ref()) {
                    handle_transcript_update(&path, tx);
                }
            }
        }
    }

    if paths.events.exists()
        && tail_worker.send(TailCommand::ReadFile(paths.events.clone())).is_err() {
            return;
        }

    // Signal that initial replay is done (processed after ReadFile in FIFO order)
    let _ = tail_worker.send(TailCommand::ReplayDone);

    // Load archived session metas (lightweight — skips events/agents/task_graph)
    match session::list_session_metas(&paths.archive_dir) {
        Ok((metas, errors)) => {
            // Report errors from corrupt session files
            for error in errors {
                if tx.send(AppEvent::Error {
                    source: "sessions".to_string(),
                    error: error.into(),
                }).is_err() {
                    return;
                }
            }
            // Send metas if any successfully loaded
            if !metas.is_empty()
                && tx.send(AppEvent::SessionMetasLoaded(metas)).is_err() {
                }
        }
        Err(e) => {
            if tx.send(AppEvent::Error {
                source: "sessions".to_string(),
                error: e.into(),
            }).is_err() {
            }
        }
    }
}

/// Watch a single path (file or directory)
fn watch_path(watcher: &mut RecommendedWatcher, path: &Path) -> WatcherResult<()> {
    if path.exists() {
        watcher.watch(path, RecursiveMode::Recursive)?;
    } else {
        eprintln!("Warning: watch path does not exist: {}", path.display());
    }
    Ok(())
}

/// Handles a single watch event and emits appropriate AppEvent
fn handle_watch_event(
    res: Result<notify::Event, notify::Error>,
    task_graph_path: &Path,
    transcripts_dir: &Path,
    events_path: &Path,
    tail_worker: &mpsc::Sender<TailCommand>,
    tx: &mpsc::Sender<AppEvent>,
) {
    match res {
        Ok(event) => {
            for path in event.paths {
                // Task graph updated
                if path == task_graph_path {
                    handle_task_graph_update(&path, tx);
                }
                // Transcript file updated
                else if path.starts_with(transcripts_dir)
                    && path.extension() == Some("jsonl".as_ref())
                {
                    handle_transcript_update(&path, tx);
                }
                // Hook events file updated (send to worker thread)
                else if path == events_path
                    && tail_worker.send(TailCommand::ReadFile(path)).is_err() {
                        return;
                    }
            }
        }
        Err(e) => {
            if tx.send(AppEvent::Error {
                source: "file_watcher".to_string(),
                error: WatcherError::Notify(e.to_string()).into(),
            }).is_err() {
            }
        }
    }
}

/// Handle task graph file update (I/O shell calls pure parser)
fn handle_task_graph_update(path: &Path, tx: &mpsc::Sender<AppEvent>) {
    match std::fs::read_to_string(path) {
        Ok(content) => match parsers::parse_task_graph(&content) {
            Ok(graph) => {
                if tx.send(AppEvent::TaskGraphUpdated(graph)).is_err() {
                }
            }
            Err(e) => {
                if tx.send(AppEvent::Error {
                    source: path.display().to_string(),
                    error: WatcherError::Parse(e).into(),
                }).is_err() {
                }
            }
        },
        Err(e) => {
            if tx.send(AppEvent::Error {
                source: path.display().to_string(),
                error: WatcherError::Io(e.to_string()).into(),
            }).is_err() {
            }
        }
    }
}

/// Handle transcript file update (I/O shell calls pure parser)
fn handle_transcript_update(path: &Path, tx: &mpsc::Sender<AppEvent>) {
    // Extract agent ID from filename (e.g., "agent-a04.jsonl" -> "a04")
    let agent_id = path
        .file_stem()
        .and_then(|s| s.to_str())
        .and_then(|s| s.strip_prefix("agent-"))
        .unwrap_or("unknown")
        .to_string();

    match std::fs::read_to_string(path) {
        Ok(content) => match parsers::parse_transcript(&content) {
            Ok(messages) => {
                if tx.send(AppEvent::TranscriptUpdated { agent_id: agent_id.into(), messages }).is_err() {
                }
            }
            Err(e) => {
                if tx.send(AppEvent::Error {
                    source: path.display().to_string(),
                    error: WatcherError::Parse(e).into(),
                }).is_err() {
                }
            }
        },
        Err(e) => {
            if tx.send(AppEvent::Error {
                source: path.display().to_string(),
                error: WatcherError::Io(e.to_string()).into(),
            }).is_err() {
            }
        }
    }
}

/// Enrich SessionStart event with transcript_path and git_branch if not already present.
/// Performs filesystem I/O to derive and verify the transcript path and get git branch.
fn enrich_session_start_event(mut event: crate::model::HookEvent) -> crate::model::HookEvent {
    let needs_transcript = event.raw.get("transcript_path").is_none();
    let needs_git_branch = event.raw.get("git_branch").is_none();

    if !needs_transcript && !needs_git_branch {
        return event;
    }

    // Extract cwd and session_id from the event (clone to avoid borrow conflicts)
    let cwd = event.raw.get("cwd")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .unwrap_or_default();
    let session_id = event.session_id.as_ref().map(|s| s.as_str()).unwrap_or("");

    if let serde_json::Value::Object(ref mut map) = event.raw {
        // Derive transcript path (I/O happens here, in the shell)
        if needs_transcript {
            if let Some(transcript_path) = derive_transcript_path(&cwd, session_id) {
                map.insert("transcript_path".to_string(), serde_json::Value::String(transcript_path));
            }
        }

        // Get git branch (I/O happens here, in the shell)
        if needs_git_branch {
            if let Some(git_branch) = get_current_git_branch() {
                map.insert("git_branch".to_string(), serde_json::Value::String(git_branch));
            }
        }
    }

    event
}

/// Get current git branch by executing git command.
/// Returns None if not in a git repo or git command fails.
fn get_current_git_branch() -> Option<String> {
    std::process::Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_watch_path_nonexistent() {
        let mut watcher =
            RecommendedWatcher::new(|_| {}, Config::default()).expect("create watcher");

        let result = watch_path(&mut watcher, Path::new("/nonexistent/path"));
        assert!(result.is_ok());
    }

    #[test]
    fn test_handle_task_graph_update_valid() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("task_graph.json");

        let json = r#"{
            "waves": [
                {
                    "number": 1,
                    "tasks": [
                        {
                            "id": "T1",
                            "description": "Test task",
                            "status": "pending"
                        }
                    ]
                }
            ],
            "total_tasks": 1,
            "completed_tasks": 0
        }"#;

        fs::write(&path, json).unwrap();

        let (tx, rx) = mpsc::channel();
        handle_task_graph_update(&path, &tx);

        let event = rx.recv_timeout(Duration::from_secs(1)).unwrap();
        match event {
            AppEvent::TaskGraphUpdated(graph) => {
                assert_eq!(graph.total_tasks(), 1);
                assert_eq!(graph.waves.len(), 1);
            }
            _ => panic!("Expected TaskGraphUpdated event"),
        }
    }

    #[test]
    fn test_handle_task_graph_update_invalid() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("task_graph.json");
        fs::write(&path, "invalid json").unwrap();

        let (tx, rx) = mpsc::channel();
        handle_task_graph_update(&path, &tx);

        let event = rx.recv_timeout(Duration::from_secs(1)).unwrap();
        match event {
            AppEvent::Error { source, error } => {
                assert!(source.contains("task_graph.json"));
                assert!(error.to_string().contains("JSON"));
            }
            _ => panic!("Expected Error event"),
        }
    }

    #[test]
    fn test_handle_transcript_update_extracts_agent_id() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("agent-a04.jsonl");

        let jsonl = r#"{"timestamp":"2026-02-11T10:00:00Z","type":"reasoning","content":"test"}"#;
        fs::write(&path, jsonl).unwrap();

        let (tx, rx) = mpsc::channel();
        handle_transcript_update(&path, &tx);

        let event = rx.recv_timeout(Duration::from_secs(1)).unwrap();
        match event {
            AppEvent::TranscriptUpdated { agent_id, messages } => {
                assert_eq!(agent_id.as_str(), "a04");
                assert_eq!(messages.len(), 1);
            }
            _ => panic!("Expected TranscriptUpdated event"),
        }
    }

    #[test]
    fn test_scan_counter_wraps_at_u32_max() {
        // Verify wrapping_add prevents overflow and modulo still works
        let mut counter: u32 = u32::MAX - 5;

        for _ in 0..10 {
            counter = counter.wrapping_add(1);
        }

        // Counter should have wrapped around: MAX-5 + 10 = 4 (after wrap)
        assert_eq!(counter, 4);

        // Modulo should still work correctly
        assert_eq!(counter % 10, 4);
        assert_eq!((counter.wrapping_add(1)) % 10, 5);
        assert_eq!((counter.wrapping_add(6)) % 10, 0);
    }

    #[test]
    fn test_watch_path_warns_on_nonexistent() {
        let mut watcher =
            RecommendedWatcher::new(|_| {}, Config::default()).expect("create watcher");

        // Should return Ok even for nonexistent path (with warning logged)
        let result = watch_path(&mut watcher, Path::new("/nonexistent/definitely/not/here"));
        assert!(result.is_ok());
    }

    #[test]
    fn test_channel_send_failure_exits_thread() {
        // Simulate channel closed scenario
        let (tx, rx) = mpsc::channel();
        drop(rx); // Close receiver

        // Sending should fail
        let result = tx.send(AppEvent::TaskGraphUpdated(crate::model::TaskGraph::empty()));

        assert!(result.is_err(), "Send should fail when channel closed");
    }

}
