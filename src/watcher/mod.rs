mod parsers;
mod tail;

pub use parsers::*;
pub use tail::TailState;

use crate::event::AppEvent;
use crate::paths::Paths;
use notify::{Config, RecommendedWatcher, RecursiveMode, Watcher};
use std::path::Path;
use std::sync::{mpsc, Arc, Mutex};
use std::time::Duration;

/// Result type for watcher operations
pub type WatcherResult<T> = Result<T, Box<dyn std::error::Error + Send + Sync>>;


/// Starts file watching for all paths and returns a channel for receiving events.
/// Debounces file changes at 200ms per NFR-012.
///
/// # Imperative Shell
/// This function handles I/O setup (file watching) but delegates parsing to pure functions.
///
/// # Returns
/// Channel receiver for AppEvent stream
pub fn start_watching(paths: &Paths) -> WatcherResult<mpsc::Receiver<AppEvent>> {
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

    // Keep watcher alive by moving it to a separate thread
    std::thread::spawn(move || {
        let _watcher = watcher;
        loop {
            std::thread::sleep(Duration::from_secs(1));
        }
    });

    Ok(rx)
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
            let _ = tx.send(AppEvent::ParseError {
                source: "file_watcher".to_string(),
                error: format!("Watch error: {}", e),
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
                let _ = tx.send(AppEvent::ParseError {
                    source: path.display().to_string(),
                    error: e,
                });
            }
        },
        Err(e) => {
            let _ = tx.send(AppEvent::ParseError {
                source: path.display().to_string(),
                error: format!("Failed to read file: {}", e),
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
                let _ = tx.send(AppEvent::TranscriptUpdated { agent_id, messages });
            }
            Err(e) => {
                let _ = tx.send(AppEvent::ParseError {
                    source: path.display().to_string(),
                    error: e,
                });
            }
        },
        Err(e) => {
            let _ = tx.send(AppEvent::ParseError {
                source: path.display().to_string(),
                error: format!("Failed to read file: {}", e),
            });
        }
    }
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
                let _ = tx.send(AppEvent::ParseError {
                    source: path.display().to_string(),
                    error: format!("Failed to read file: {}", e),
                });
                return;
            }
        },
        Err(e) => {
            let _ = tx.send(AppEvent::ParseError {
                source: "tail_state".to_string(),
                error: format!("Lock poisoned: {}", e),
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
                let _ = tx.send(AppEvent::HookEventReceived(event));
            }
        }
        Err(e) => {
            let _ = tx.send(AppEvent::ParseError {
                source: path.display().to_string(),
                error: e,
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
            AppEvent::ParseError { source, error } => {
                assert!(source.contains("task_graph.json"));
                assert!(error.contains("JSON"));
            }
            _ => panic!("Expected ParseError event"),
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
                assert_eq!(agent_id, "a04");
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
