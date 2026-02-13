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
use std::sync::{mpsc, Arc, Mutex};
use std::time::Duration;

/// Result type for watcher operations
pub type WatcherResult<T> = Result<T, WatcherError>;


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
/// Debounces file changes at 200ms per NFR-012.
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

    // Shared tail state for incremental event reads
    let tail_state = Arc::new(Mutex::new(TailState::new()));

    // Clone tx for watcher callback; keep original for initial reads
    let tx_watcher = tx.clone();

    // Clone paths for move into watcher thread
    let task_graph_path = paths.task_graph.clone();
    let transcripts_dir = paths.transcripts.clone();
    let events_path = paths.events.clone();
    let tail_state_watcher = Arc::clone(&tail_state);

    // Create notify watcher with 200ms debounce
    let mut watcher = RecommendedWatcher::new(
        move |res: Result<notify::Event, notify::Error>| {
            handle_watch_event(
                res,
                &task_graph_path,
                &transcripts_dir,
                &events_path,
                &tail_state_watcher,
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
        std::fs::create_dir_all(events_dir).ok();
        watch_path(&mut watcher, events_dir)?;
    }

    // Initial read of existing files
    load_existing_files(paths, &tail_state, &tx);

    // Periodic polling of events file — ensures real-time updates even if inotify
    // misses appends (common on tmpfs). TailState deduplicates so no double-processing.
    let events_path_poll = paths.events.clone();
    let tail_state_poll = Arc::clone(&tail_state);
    let tx_poll = tx.clone();

    std::thread::spawn(move || {
        loop {
            std::thread::sleep(Duration::from_millis(200));
            if events_path_poll.exists() {
                handle_events_incremental(&events_path_poll, &tail_state_poll, &tx_poll);
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
/// Scans the transcript directory every 500ms for recently modified .jsonl files,
/// tails new content, and emits AssistantText events.
/// Discovers subagent transcripts automatically (each gets its own .jsonl file).
fn start_transcript_polling(transcript_dir: PathBuf, tx: mpsc::Sender<AppEvent>) {
    std::thread::spawn(move || {
        let mut tail_state = TailState::new();
        let mut known_files: BTreeMap<PathBuf, std::time::SystemTime> = BTreeMap::new();
        let mut scan_counter: u32 = 0;

        loop {
            std::thread::sleep(Duration::from_millis(200));
            scan_counter += 1;

            // Rescan directory every ~2s (10 iterations) to discover new files
            if scan_counter % 10 == 1 {
                if let Ok(entries) = std::fs::read_dir(&transcript_dir) {
                    let cutoff = std::time::SystemTime::now()
                        - Duration::from_secs(3600);
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
                            known_files.entry(path).or_insert(modified);
                        }
                    }
                }
            }

            // Tail known files for new content
            for path in known_files.keys() {
                if !path.exists() {
                    continue;
                }
                let new_content = match tail_state.read_new_lines(path) {
                    Ok(c) if !c.is_empty() => c,
                    _ => continue,
                };

                let session_id = path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("unknown");

                let events =
                    parsers::parse_claude_transcript_incremental(&new_content, session_id);
                for event in events {
                    if tx.send(AppEvent::HookEventReceived(event)).is_err() {
                        return;
                    }
                }

                // Also parse agent_progress entries (parent transcripts contain
                // tool calls with agentId for proper multi-agent attribution)
                let progress_events =
                    parsers::parse_agent_progress_tool_calls(&new_content, session_id);
                for event in progress_events {
                    if tx.send(AppEvent::HookEventReceived(event)).is_err() {
                        return;
                    }
                }
            }
        }
    });
}

/// Read existing files on startup so the TUI doesn't start empty.
/// Agent lifecycle is derived from hook events (SubagentStart/SubagentStop),
/// not from .active marker files.
fn load_existing_files(
    paths: &Paths,
    tail_state: &Arc<Mutex<TailState>>,
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

    if paths.events.exists() {
        handle_events_incremental(&paths.events, tail_state, tx);
    }

    // Load archived session metas (lightweight — skips events/agents/task_graph)
    match session::list_session_metas(&paths.archive_dir) {
        Ok(metas) if !metas.is_empty() => {
            let _ = tx.send(AppEvent::SessionMetasLoaded(metas));
        }
        Ok(_) => {}
        Err(e) => {
            let _ = tx.send(AppEvent::Error {
                source: "sessions".to_string(),
                error: e.into(),
            });
        }
    }
}

/// Watch a single path (file or directory)
fn watch_path(watcher: &mut RecommendedWatcher, path: &Path) -> WatcherResult<()> {
    if path.exists() {
        watcher.watch(path, RecursiveMode::Recursive)?;
    }
    Ok(())
}

/// Handles a single watch event and emits appropriate AppEvent
fn handle_watch_event(
    res: Result<notify::Event, notify::Error>,
    task_graph_path: &Path,
    transcripts_dir: &Path,
    events_path: &Path,
    tail_state: &Arc<Mutex<TailState>>,
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
                // Hook events file updated (incremental via TailState)
                else if path == events_path {
                    handle_events_incremental(&path, tail_state, tx);
                }
            }
        }
        Err(e) => {
            let _ = tx.send(AppEvent::Error {
                source: "file_watcher".to_string(),
                error: WatcherError::Notify(e.to_string()).into(),
            });
        }
    }
}

/// Handle task graph file update (I/O shell calls pure parser)
fn handle_task_graph_update(path: &Path, tx: &mpsc::Sender<AppEvent>) {
    match std::fs::read_to_string(path) {
        Ok(content) => match parsers::parse_task_graph(&content) {
            Ok(graph) => {
                let _ = tx.send(AppEvent::TaskGraphUpdated(graph));
            }
            Err(e) => {
                let _ = tx.send(AppEvent::Error {
                    source: path.display().to_string(),
                    error: WatcherError::Parse(e).into(),
                });
            }
        },
        Err(e) => {
            let _ = tx.send(AppEvent::Error {
                source: path.display().to_string(),
                error: WatcherError::Io(e.to_string()).into(),
            });
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
                let _ = tx.send(AppEvent::TranscriptUpdated { agent_id: agent_id.into(), messages });
            }
            Err(e) => {
                let _ = tx.send(AppEvent::Error {
                    source: path.display().to_string(),
                    error: WatcherError::Parse(e).into(),
                });
            }
        },
        Err(e) => {
            let _ = tx.send(AppEvent::Error {
                source: path.display().to_string(),
                error: WatcherError::Io(e.to_string()).into(),
            });
        }
    }
}

/// Enrich SessionStart event with transcript_path if not already present.
/// Performs filesystem I/O to derive and verify the transcript path.
fn enrich_session_start_event(mut event: crate::model::HookEvent) -> crate::model::HookEvent {
    // Skip if transcript_path already provided
    if event.raw.get("transcript_path").is_some() {
        return event;
    }

    // Extract cwd and session_id from the event
    let cwd = event.raw.get("cwd")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let session_id = event.session_id.as_ref().map(|s| s.as_str()).unwrap_or("");

    // Derive transcript path (I/O happens here, in the shell)
    if let Some(transcript_path) = derive_transcript_path(cwd, session_id) {
        // Add to raw JSON so update() can access it
        if let serde_json::Value::Object(ref mut map) = event.raw {
            map.insert("transcript_path".to_string(), serde_json::Value::String(transcript_path));
        }
    }

    event
}

/// Handle hook events file update incrementally via TailState.
/// Only reads new content since last read, avoiding duplicate event processing.
fn handle_events_incremental(
    path: &Path,
    tail_state: &Arc<Mutex<TailState>>,
    tx: &mpsc::Sender<AppEvent>,
) {
    let new_content = match tail_state.lock() {
        Ok(mut ts) => match ts.read_new_lines(path) {
            Ok(content) => content,
            Err(e) => {
                let _ = tx.send(AppEvent::Error {
                    source: path.display().to_string(),
                    error: WatcherError::Io(e.to_string()).into(),
                });
                return;
            }
        },
        Err(_) => {
            let _ = tx.send(AppEvent::Error {
                source: "tail_state".to_string(),
                error: WatcherError::LockPoisoned.into(),
            });
            return;
        }
    };

    if new_content.is_empty() {
        return;
    }

    match parsers::parse_hook_events(&new_content) {
        Ok(events) => {
            for event in events {
                // Enrich SessionStart events with transcript_path (I/O belongs in shell)
                let enriched_event = if matches!(event.kind, crate::model::HookEventKind::SessionStart) {
                    enrich_session_start_event(event)
                } else {
                    event
                };
                if tx.send(AppEvent::HookEventReceived(enriched_event)).is_err() {
                    return;
                }
            }
        }
        Err(e) => {
            let _ = tx.send(AppEvent::Error {
                source: path.display().to_string(),
                error: WatcherError::Parse(e).into(),
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::io::Write;
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
                assert_eq!(graph.total_tasks, 1);
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
    fn test_handle_events_incremental_initial_read() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("events.jsonl");

        let jsonl = r#"{"timestamp":"2026-02-11T10:00:00Z","event":"session_start"}
{"timestamp":"2026-02-11T10:01:00Z","event":"notification","message":"hello"}"#;
        fs::write(&path, jsonl).unwrap();

        let tail_state = Arc::new(Mutex::new(TailState::new()));
        let (tx, rx) = mpsc::channel();
        handle_events_incremental(&path, &tail_state, &tx);

        // Should receive both events
        let _e1 = rx.recv_timeout(Duration::from_secs(1)).unwrap();
        let _e2 = rx.recv_timeout(Duration::from_secs(1)).unwrap();
        assert!(rx.recv_timeout(Duration::from_millis(100)).is_err());
    }

    #[test]
    fn test_handle_events_incremental_only_new() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("events.jsonl");

        // Write initial content
        fs::write(
            &path,
            r#"{"timestamp":"2026-02-11T10:00:00Z","event":"session_start"}"#,
        )
        .unwrap();

        let tail_state = Arc::new(Mutex::new(TailState::new()));
        let (tx, rx) = mpsc::channel();

        // First read
        handle_events_incremental(&path, &tail_state, &tx);
        let _e1 = rx.recv_timeout(Duration::from_secs(1)).unwrap();

        // Append new event
        let mut f = fs::OpenOptions::new().append(true).open(&path).unwrap();
        writeln!(
            f,
            r#"
{{"timestamp":"2026-02-11T10:01:00Z","event":"notification","message":"new"}}"#
        )
        .unwrap();

        // Second read — should only get the new event
        handle_events_incremental(&path, &tail_state, &tx);
        let e2 = rx.recv_timeout(Duration::from_secs(1)).unwrap();
        match e2 {
            AppEvent::HookEventReceived(he) => {
                assert!(matches!(
                    he.kind,
                    crate::model::HookEventKind::Notification { .. }
                ));
            }
            _ => panic!("Expected HookEventReceived"),
        }
        // No more events
        assert!(rx.recv_timeout(Duration::from_millis(100)).is_err());
    }

    #[test]
    fn test_handle_events_incremental_no_duplicates() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("events.jsonl");

        fs::write(
            &path,
            r#"{"timestamp":"2026-02-11T10:00:00Z","event":"session_start"}"#,
        )
        .unwrap();

        let tail_state = Arc::new(Mutex::new(TailState::new()));
        let (tx, rx) = mpsc::channel();

        // Read twice without file change
        handle_events_incremental(&path, &tail_state, &tx);
        handle_events_incremental(&path, &tail_state, &tx);

        // Should only get one event
        let _e1 = rx.recv_timeout(Duration::from_secs(1)).unwrap();
        assert!(rx.recv_timeout(Duration::from_millis(100)).is_err());
    }
}
