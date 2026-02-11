mod parsers;
mod tail;

pub use parsers::*;
pub use tail::TailState;

use crate::event::AppEvent;
use crate::paths::Paths;
use notify::{Config, RecommendedWatcher, RecursiveMode, Watcher};
use std::path::Path;
use std::sync::mpsc;
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

    // Clone paths for move into watcher thread
    let task_graph_path = paths.task_graph.clone();
    let transcripts_dir = paths.transcripts.clone();
    let events_path = paths.events.clone();
    let active_agents_dir = paths.active_agents.clone();

    // Create notify watcher with 200ms debounce
    let mut watcher = RecommendedWatcher::new(
        move |res: Result<notify::Event, notify::Error>| {
            handle_watch_event(
                res,
                &task_graph_path,
                &transcripts_dir,
                &events_path,
                &active_agents_dir,
                &tx,
            );
        },
        Config::default().with_poll_interval(Duration::from_millis(200)),
    )?;

    // Watch all paths
    watch_path(&mut watcher, &paths.task_graph)?;
    watch_path(&mut watcher, &paths.transcripts)?;
    watch_path(&mut watcher, &paths.events)?;
    watch_path(&mut watcher, &paths.active_agents)?;

    // Keep watcher alive by moving it to a separate thread
    std::thread::spawn(move || {
        let _watcher = watcher;
        loop {
            std::thread::sleep(Duration::from_secs(1));
        }
    });

    Ok(rx)
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
    active_agents_dir: &Path,
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
                else if path.starts_with(transcripts_dir) && path.extension() == Some("jsonl".as_ref())
                {
                    handle_transcript_update(&path, tx);
                }
                // Hook events file updated
                else if path == events_path {
                    handle_events_update(&path, tx);
                }
                // Active agent marker file
                else if path.starts_with(active_agents_dir) && path.extension() == Some("active".as_ref())
                {
                    handle_active_agent_change(&path, &event.kind, tx);
                }
            }
        }
        Err(e) => {
            // Emit parse error for watch failures
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

/// Handle hook events file update (I/O shell calls pure parser)
fn handle_events_update(path: &Path, tx: &mpsc::Sender<AppEvent>) {
    match std::fs::read_to_string(path) {
        Ok(content) => match parsers::parse_hook_events(&content) {
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
        },
        Err(e) => {
            let _ = tx.send(AppEvent::ParseError {
                source: path.display().to_string(),
                error: format!("Failed to read file: {}", e),
            });
        }
    }
}

/// Handle active agent marker file changes
fn handle_active_agent_change(
    path: &Path,
    event_kind: &notify::EventKind,
    tx: &mpsc::Sender<AppEvent>,
) {
    // Extract agent ID from filename (e.g., "a04.active" -> "a04")
    let agent_id = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown")
        .to_string();

    match event_kind {
        notify::EventKind::Create(_) => {
            let _ = tx.send(AppEvent::AgentStarted(agent_id));
        }
        notify::EventKind::Remove(_) => {
            let _ = tx.send(AppEvent::AgentStopped(agent_id));
        }
        _ => {}
    }
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
        assert!(result.is_ok()); // Should not error on nonexistent paths
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
    fn test_handle_active_agent_change_create() {
        let (tx, rx) = mpsc::channel();
        let path = Path::new("/tmp/claude-subagents/a04.active");
        let event_kind = notify::EventKind::Create(notify::event::CreateKind::File);

        handle_active_agent_change(&path, &event_kind, &tx);

        let event = rx.recv_timeout(Duration::from_secs(1)).unwrap();
        match event {
            AppEvent::AgentStarted(id) => assert_eq!(id, "a04"),
            _ => panic!("Expected AgentStarted event"),
        }
    }

    #[test]
    fn test_handle_active_agent_change_remove() {
        let (tx, rx) = mpsc::channel();
        let path = Path::new("/tmp/claude-subagents/a04.active");
        let event_kind = notify::EventKind::Remove(notify::event::RemoveKind::File);

        handle_active_agent_change(&path, &event_kind, &tx);

        let event = rx.recv_timeout(Duration::from_secs(1)).unwrap();
        match event {
            AppEvent::AgentStopped(id) => assert_eq!(id, "a04"),
            _ => panic!("Expected AgentStopped event"),
        }
    }
}
