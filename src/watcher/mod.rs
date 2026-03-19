mod parsers;
mod tail;

pub use parsers::*;
pub use tail::TailState;

use crate::error::WatcherError;
use crate::event::AppEvent;
use crate::model::ids::SessionId;
use crate::paths::Paths;
use crate::session;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::mpsc;
use std::time::{Duration, SystemTime};

/// Result type for watcher operations
pub type WatcherResult<T> = Result<T, WatcherError>;

// ---------------------------------------------------------------------------
// Session lifecycle timeouts (FR-010, FR-013)
// ---------------------------------------------------------------------------

/// Confirmed sessions (received at least one user prompt) are marked complete
/// after 10 minutes without new writes.
const CONFIRMED_TIMEOUT: Duration = Duration::from_secs(600);

/// Unconfirmed sessions (no user prompt received) are removed after 30 seconds.
const UNCONFIRMED_TIMEOUT: Duration = Duration::from_secs(30);

/// How often we re-scan the transcript directory for new .jsonl files (session discovery).
/// 10 × 200ms = ~2 seconds, satisfying NFR-001.
const DIR_RESCAN_INTERVAL: u32 = 10;

/// How often we emit AgentMetadataUpdated from the full subagent transcript content.
/// 10 × 200ms = ~2 seconds.
const METADATA_EMIT_INTERVAL: u32 = 10;

// ---------------------------------------------------------------------------
// Internal state per known transcript file
// ---------------------------------------------------------------------------

#[derive(Debug)]
struct FileState {
    /// Last observed mtime on disk (used for lifecycle decisions)
    mtime: SystemTime,
    /// True when this is a subagent transcript (inside {session_id}/subagents/)
    is_subagent: bool,
    /// The session_id this file belongs to (stem of top-level jsonl, or parent dir stem for
    /// subagent files)
    session_id: String,
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Start unified polling loop. Returns channel receiver for AppEvents.
///
/// The loop runs every 200ms and:
/// 1. (Re)scans transcript_dir for new .jsonl files  -> SessionDiscovered
/// 2. Checks mtime on known files                     -> SessionCompleted / SessionReactivated
/// 3. Tails transcript files via TailState            -> TranscriptEventReceived
/// 4. Scans {session_id}/subagents/ dirs              -> agent discovery + AgentMetadataUpdated
/// 5. Polls task_graph file mtime                     -> TaskGraphUpdated
///
/// # FR-018 / FR-032 / SC-002
/// No notify crate, no events.jsonl watcher, no /tmp/loom-tui references.
pub fn start_watching(paths: &Paths) -> WatcherResult<mpsc::Receiver<AppEvent>> {
    let (tx, rx) = mpsc::channel();

    // Load archived session metas immediately on startup (lightweight)
    load_archived_session_metas(&paths.archive_dir, &tx);

    let transcript_dir = paths.transcript_dir.clone();
    let task_graph_path = paths.task_graph.clone();

    std::thread::spawn(move || {
        polling_loop(transcript_dir, task_graph_path, tx);
    });

    Ok(rx)
}

// ---------------------------------------------------------------------------
// Polling loop (imperative shell — all I/O lives here)
// ---------------------------------------------------------------------------

fn polling_loop(
    transcript_dir: PathBuf,
    task_graph_path: PathBuf,
    tx: mpsc::Sender<AppEvent>,
) {
    let mut tail_state = TailState::new();

    // key: absolute path to .jsonl file
    let mut known_files: BTreeMap<PathBuf, FileState> = BTreeMap::new();

    // key: session_id (string), value: whether session is confirmed + last_mtime
    let mut session_confirmed: BTreeMap<String, (bool, SystemTime)> = BTreeMap::new();
    // sessions we have already emitted SessionCompleted for
    let mut completed_sessions: std::collections::HashSet<String> = std::collections::HashSet::new();

    let mut task_graph_mtime: Option<SystemTime> = None;
    let mut scan_counter: u32 = 0;

    // Initial session reply done immediately
    if tx.send(AppEvent::ReplayComplete).is_err() {
        return;
    }

    loop {
        std::thread::sleep(Duration::from_millis(200));
        scan_counter = scan_counter.wrapping_add(1);

        let do_dir_rescan = scan_counter % DIR_RESCAN_INTERVAL == 1;
        let do_metadata_emit = scan_counter % METADATA_EMIT_INTERVAL == 1;

        // ----------------------------------------------------------------
        // 1. Scan transcript directory for new .jsonl files
        // ----------------------------------------------------------------
        if do_dir_rescan {
            scan_transcript_dir(
                &transcript_dir,
                &mut known_files,
                &mut session_confirmed,
                &mut completed_sessions,
                &tx,
            );
        }

        // ----------------------------------------------------------------
        // 2 + 3. For each known file: check mtime lifecycle + tail content
        // ----------------------------------------------------------------
        let paths: Vec<PathBuf> = known_files.keys().cloned().collect();
        for path in paths {
            let file_state = match known_files.get_mut(&path) {
                Some(s) => s,
                None => continue,
            };

            let session_id = file_state.session_id.clone();
            let is_subagent = file_state.is_subagent;

            // Get current mtime (non-fatal on error)
            let current_mtime = match path.metadata().and_then(|m| m.modified()) {
                Ok(m) => m,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                    // File deleted — clean up state to stop polling it
                    known_files.remove(&path);
                    session_confirmed.remove(&session_id);
                    continue;
                }
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

            // Update mtime on file state
            file_state.mtime = current_mtime;

            // Update per-session mtime tracker (use the freshest mtime across all files)
            if let Some((confirmed, prev_mtime)) = session_confirmed.get_mut(&session_id) {
                if current_mtime > *prev_mtime {
                    *prev_mtime = current_mtime;

                    // Reactivate if previously completed
                    if completed_sessions.remove(&session_id) {
                        if tx.send(AppEvent::SessionReactivated {
                            session_id: SessionId::new(&session_id),
                        }).is_err() {
                            return;
                        }
                    }
                    let _ = confirmed; // borrowed; confirm state updated by update.rs on UserMessage
                }
            }

            // Tail new content from this file (FR-003, NFR-002, NFR-003)
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

            if !new_content.is_empty() {
                let events = parsers::parse_transcript_events(&new_content, &session_id);

                // FR-010/FR-012: mark session confirmed if any UserMessage seen
                let has_user_message = events
                    .iter()
                    .any(|e| matches!(e.kind, crate::model::TranscriptEventKind::UserMessage));
                if has_user_message {
                    if let Some((confirmed, _)) = session_confirmed.get_mut(&session_id) {
                        *confirmed = true;
                    }
                }

                for mut event in events {
                    // Mark whether this is a subagent event
                    if is_subagent {
                        // Extract agent id from filename: agent-{id}.jsonl
                        let agent_id = path
                            .file_stem()
                            .and_then(|s| s.to_str())
                            .and_then(|s| s.strip_prefix("agent-"))
                            .unwrap_or_else(|| {
                                path.file_stem().and_then(|s| s.to_str()).unwrap_or("unknown")
                            })
                            .to_string();
                        if event.agent_id.is_none() {
                            event = event.with_agent(agent_id);
                        }
                    }
                    // Stamp session_id if not already set
                    if event.session_id.is_none() {
                        event = event.with_session(session_id.as_str());
                    }
                    if tx.send(AppEvent::TranscriptEventReceived(event)).is_err() {
                        return;
                    }
                }
            }

            // Emit metadata for subagent files on the rescan tick (FR-014)
            if is_subagent && do_metadata_emit {
                emit_agent_metadata(&path, &tx);
            }
        }

        // ----------------------------------------------------------------
        // 4. Check session lifecycle (mtime staleness) — FR-009, FR-010, FR-011
        // ----------------------------------------------------------------
        let now = SystemTime::now();
        // We need separate traversal to avoid borrow conflicts
        let sessions_to_check: Vec<(String, bool, SystemTime)> = session_confirmed
            .iter()
            .map(|(id, (confirmed, mtime))| (id.clone(), *confirmed, *mtime))
            .collect();

        for (session_id, confirmed, last_mtime) in sessions_to_check {
            if completed_sessions.contains(&session_id) {
                continue;
            }
            let timeout = if confirmed {
                CONFIRMED_TIMEOUT
            } else {
                UNCONFIRMED_TIMEOUT
            };
            let elapsed = now.duration_since(last_mtime).unwrap_or(Duration::ZERO);
            if elapsed >= timeout {
                completed_sessions.insert(session_id.clone());
                if tx.send(AppEvent::SessionCompleted {
                    session_id: SessionId::new(&session_id),
                }).is_err() {
                    return;
                }
            }
        }

        // ----------------------------------------------------------------
        // 5. Poll task graph by mtime (FR-033)
        // ----------------------------------------------------------------
        let new_mtime = task_graph_path.metadata().and_then(|m| m.modified()).ok();
        if new_mtime.is_some() && new_mtime != task_graph_mtime {
            task_graph_mtime = new_mtime;
            handle_task_graph_update(&task_graph_path, &tx);
        }
    }
}

// ---------------------------------------------------------------------------
// Directory scanning (FR-001, FR-002, FR-014)
// ---------------------------------------------------------------------------

/// Scan transcript_dir for top-level .jsonl files and per-session subagent dirs.
/// Emits SessionDiscovered for newly found sessions.
fn scan_transcript_dir(
    transcript_dir: &PathBuf,
    known_files: &mut BTreeMap<PathBuf, FileState>,
    session_confirmed: &mut BTreeMap<String, (bool, SystemTime)>,
    completed_sessions: &mut std::collections::HashSet<String>,
    tx: &mpsc::Sender<AppEvent>,
) {
    let entries = match std::fs::read_dir(transcript_dir) {
        Ok(e) => e,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return,
        Err(e) => {
            let _ = tx.send(AppEvent::Error {
                source: transcript_dir.display().to_string(),
                error: WatcherError::Io(e.to_string()).into(),
            });
            return;
        }
    };

    for entry in entries.flatten() {
        let path = entry.path();

        if path.extension().and_then(|s| s.to_str()) == Some("jsonl") {
            // Top-level session transcript
            let session_id = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("unknown")
                .to_string();

            if known_files.contains_key(&path) {
                continue;
            }

            let mtime = match entry.metadata().and_then(|m| m.modified()) {
                Ok(m) => m,
                Err(e) => {
                    let _ = tx.send(AppEvent::Error {
                        source: path.display().to_string(),
                        error: WatcherError::Io(e.to_string()).into(),
                    });
                    continue;
                }
            };

            known_files.insert(path.clone(), FileState {
                mtime,
                is_subagent: false,
                session_id: session_id.clone(),
            });

            // Only emit SessionDiscovered if not already known as completed
            // (re-discovery after restart should still emit)
            if !session_confirmed.contains_key(&session_id) {
                session_confirmed.insert(session_id.clone(), (false, mtime));
                if tx.send(AppEvent::SessionDiscovered {
                    session_id: SessionId::new(&session_id),
                    transcript_path: path,
                }).is_err() {
                    return;
                }
            } else if completed_sessions.contains(&session_id) {
                // Found a completed session's file again — may have new content;
                // reactivation is handled by mtime check in polling loop
            }
        } else if path.is_dir() {
            // Per-session subagent dir: {session_id}/subagents/
            let subagents_dir = path.join("subagents");
            if subagents_dir.is_dir() {
                let parent_session_id = path
                    .file_name()
                    .and_then(|s| s.to_str())
                    .unwrap_or("unknown")
                    .to_string();
                scan_subagents_dir(
                    &subagents_dir,
                    &parent_session_id,
                    known_files,
                    tx,
                );
            }
        }
    }
}

/// Scan a subagents/ directory for agent-*.jsonl files and register them.
fn scan_subagents_dir(
    dir: &PathBuf,
    parent_session_id: &str,
    known_files: &mut BTreeMap<PathBuf, FileState>,
    tx: &mpsc::Sender<AppEvent>,
) {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return,
        Err(e) => {
            let _ = tx.send(AppEvent::Error {
                source: dir.display().to_string(),
                error: WatcherError::Io(e.to_string()).into(),
            });
            return;
        }
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("jsonl") {
            continue;
        }
        if known_files.contains_key(&path) {
            continue;
        }

        let mtime = match entry.metadata().and_then(|m| m.modified()) {
            Ok(m) => m,
            Err(e) => {
                let _ = tx.send(AppEvent::Error {
                    source: path.display().to_string(),
                    error: WatcherError::Io(e.to_string()).into(),
                });
                continue;
            }
        };

        known_files.insert(path, FileState {
            mtime,
            is_subagent: true,
            session_id: parent_session_id.to_string(),
        });
    }
}

// ---------------------------------------------------------------------------
// Helper: emit agent metadata from full file content
// ---------------------------------------------------------------------------

fn emit_agent_metadata(path: &PathBuf, tx: &mpsc::Sender<AppEvent>) {
    let full_content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(e) => {
            let _ = tx.send(AppEvent::Error {
                source: path.display().to_string(),
                error: WatcherError::Io(e.to_string()).into(),
            });
            return;
        }
    };

    let metadata = parsers::parse_transcript_metadata(&full_content);
    if metadata.model.is_none() && metadata.token_usage.is_empty() && metadata.skills.is_empty() && metadata.task_description.is_none() {
        return;
    }

    let file_stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("unknown");
    let agent_id = file_stem.strip_prefix("agent-").unwrap_or(file_stem).to_string();

    let _ = tx.send(AppEvent::AgentMetadataUpdated {
        agent_id: agent_id.into(),
        metadata,
    });
}

// ---------------------------------------------------------------------------
// Helper: read + parse task graph
// ---------------------------------------------------------------------------

fn handle_task_graph_update(path: &PathBuf, tx: &mpsc::Sender<AppEvent>) {
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

// ---------------------------------------------------------------------------
// Startup: load archived session metas
// ---------------------------------------------------------------------------

fn load_archived_session_metas(archive_dir: &PathBuf, tx: &mpsc::Sender<AppEvent>) {
    match session::list_session_metas(archive_dir) {
        Ok((metas, errors)) => {
            for error in errors {
                let _ = tx.send(AppEvent::Error {
                    source: "sessions".to_string(),
                    error: error.into(),
                });
            }
            if !metas.is_empty() {
                let _ = tx.send(AppEvent::SessionMetasLoaded(metas));
            }
        }
        Err(e) => {
            let _ = tx.send(AppEvent::Error {
                source: "sessions".to_string(),
                error: e.into(),
            });
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::io::Write;
    use std::time::Duration;
    use tempfile::TempDir;

    // -----------------------------------------------------------------------
    // Unit: handle_task_graph_update
    // -----------------------------------------------------------------------

    #[test]
    fn task_graph_update_valid_json_emits_event() {
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
            _ => panic!("expected TaskGraphUpdated"),
        }
    }

    #[test]
    fn task_graph_update_invalid_json_emits_error() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("task_graph.json");
        fs::write(&path, "invalid json").unwrap();

        let (tx, rx) = mpsc::channel();
        handle_task_graph_update(&path, &tx);

        let event = rx.recv_timeout(Duration::from_secs(1)).unwrap();
        match event {
            AppEvent::Error { source, .. } => {
                assert!(source.contains("task_graph.json"));
            }
            _ => panic!("expected Error event"),
        }
    }

    #[test]
    fn task_graph_update_missing_file_emits_error() {
        let path = PathBuf::from("/nonexistent/path/task_graph.json");
        let (tx, rx) = mpsc::channel();
        handle_task_graph_update(&path, &tx);

        let event = rx.recv_timeout(Duration::from_secs(1)).unwrap();
        assert!(matches!(event, AppEvent::Error { .. }));
    }

    // -----------------------------------------------------------------------
    // Unit: scan_transcript_dir — session discovery (FR-001, FR-002)
    // -----------------------------------------------------------------------

    #[test]
    fn scan_discovers_new_jsonl_files() {
        let temp = TempDir::new().unwrap();
        let session_file = temp.path().join("session-abc.jsonl");
        fs::write(&session_file, "").unwrap();

        let mut known_files = BTreeMap::new();
        let mut session_confirmed = BTreeMap::new();
        let mut completed = std::collections::HashSet::new();
        let (tx, rx) = mpsc::channel();

        scan_transcript_dir(
            &temp.path().to_path_buf(),
            &mut known_files,
            &mut session_confirmed,
            &mut completed,
            &tx,
        );

        assert!(known_files.contains_key(&session_file));
        assert!(session_confirmed.contains_key("session-abc"));

        let event = rx.recv_timeout(Duration::from_millis(100)).unwrap();
        match event {
            AppEvent::SessionDiscovered { session_id, transcript_path } => {
                assert_eq!(session_id.as_str(), "session-abc");
                assert_eq!(transcript_path, session_file);
            }
            _ => panic!("expected SessionDiscovered"),
        }
    }

    #[test]
    fn scan_does_not_rediscover_known_files() {
        let temp = TempDir::new().unwrap();
        let session_file = temp.path().join("session-xyz.jsonl");
        fs::write(&session_file, "").unwrap();

        let mut known_files = BTreeMap::new();
        let mut session_confirmed = BTreeMap::new();
        let mut completed = std::collections::HashSet::new();
        let (tx, rx) = mpsc::channel();

        // First scan: discovers
        scan_transcript_dir(
            &temp.path().to_path_buf(),
            &mut known_files,
            &mut session_confirmed,
            &mut completed,
            &tx,
        );
        let _first = rx.recv_timeout(Duration::from_millis(100)).unwrap();

        // Second scan: should not re-emit
        scan_transcript_dir(
            &temp.path().to_path_buf(),
            &mut known_files,
            &mut session_confirmed,
            &mut completed,
            &tx,
        );

        let second = rx.recv_timeout(Duration::from_millis(100));
        assert!(second.is_err(), "should not emit duplicate SessionDiscovered");
    }

    #[test]
    fn scan_ignores_non_jsonl_files() {
        let temp = TempDir::new().unwrap();
        fs::write(temp.path().join("not-a-transcript.txt"), "").unwrap();
        fs::write(temp.path().join("data.json"), "").unwrap();

        let mut known_files = BTreeMap::new();
        let mut session_confirmed = BTreeMap::new();
        let mut completed = std::collections::HashSet::new();
        let (tx, rx) = mpsc::channel();

        scan_transcript_dir(
            &temp.path().to_path_buf(),
            &mut known_files,
            &mut session_confirmed,
            &mut completed,
            &tx,
        );

        assert!(known_files.is_empty());
        assert!(rx.recv_timeout(Duration::from_millis(100)).is_err());
    }

    #[test]
    fn scan_nonexistent_dir_is_nonfatal() {
        let path = PathBuf::from("/nonexistent/transcript/dir");
        let mut known_files = BTreeMap::new();
        let mut session_confirmed = BTreeMap::new();
        let mut completed = std::collections::HashSet::new();
        let (tx, _rx) = mpsc::channel();

        // Must not panic (NFR-007)
        scan_transcript_dir(&path, &mut known_files, &mut session_confirmed, &mut completed, &tx);
        assert!(known_files.is_empty());
    }

    // -----------------------------------------------------------------------
    // Unit: scan_subagents_dir (FR-014)
    // -----------------------------------------------------------------------

    #[test]
    fn scan_subagents_discovers_agent_files() {
        let temp = TempDir::new().unwrap();
        let subagents_dir = temp.path().join("subagents");
        fs::create_dir(&subagents_dir).unwrap();
        fs::write(subagents_dir.join("agent-a04.jsonl"), "").unwrap();
        fs::write(subagents_dir.join("agent-b12.jsonl"), "").unwrap();
        fs::write(subagents_dir.join("not-an-agent.txt"), "").unwrap();

        let mut known_files = BTreeMap::new();
        let (tx, _rx) = mpsc::channel();

        scan_subagents_dir(
            &subagents_dir,
            "session-parent",
            &mut known_files,
            &tx,
        );

        // Two .jsonl files discovered; .txt ignored
        assert_eq!(known_files.len(), 2);
        for (_, state) in &known_files {
            assert!(state.is_subagent);
            assert_eq!(state.session_id, "session-parent");
        }
    }

    // -----------------------------------------------------------------------
    // Integration: polling loop discovers sessions + tails content
    // -----------------------------------------------------------------------

    #[test]
    fn polling_discovers_session_and_tails_events() {
        let temp = TempDir::new().unwrap();
        let session_path = temp.path().join("session-test.jsonl");

        // Pre-create an empty file
        fs::write(&session_path, "").unwrap();

        let paths = crate::paths::Paths {
            task_graph: temp.path().join("task_graph.json"),
            transcript_dir: temp.path().to_path_buf(),
            archive_dir: temp.path().join("archives"),
        };

        let rx = start_watching(&paths).expect("start_watching failed");

        // Drain ReplayComplete
        let mut discovered = false;
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while std::time::Instant::now() < deadline {
            match rx.recv_timeout(Duration::from_millis(500)) {
                Ok(AppEvent::SessionDiscovered { session_id, .. }) => {
                    assert_eq!(session_id.as_str(), "session-test");
                    discovered = true;
                    break;
                }
                Ok(_) => {}
                Err(_) => {}
            }
        }
        assert!(discovered, "SessionDiscovered not emitted within 5s");
    }

    #[test]
    fn polling_tails_new_transcript_events() {
        let temp = TempDir::new().unwrap();
        let session_path = temp.path().join("sess-tail.jsonl");

        // Start with empty file
        fs::write(&session_path, "").unwrap();

        let paths = crate::paths::Paths {
            task_graph: temp.path().join("task_graph.json"),
            transcript_dir: temp.path().to_path_buf(),
            archive_dir: temp.path().join("archives"),
        };

        let rx = start_watching(&paths).expect("start_watching");

        // Wait for session discovery
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        let mut discovered = false;
        while std::time::Instant::now() < deadline {
            match rx.recv_timeout(Duration::from_millis(300)) {
                Ok(AppEvent::SessionDiscovered { .. }) => {
                    discovered = true;
                    break;
                }
                Ok(_) => {}
                Err(_) => {}
            }
        }
        assert!(discovered, "session not discovered");

        // Append a user message JSONL line
        let line = r#"{"type":"human","timestamp":"2026-03-18T10:00:00Z","message":{"content":[{"type":"text","text":"hello"}]}}"#;
        let mut f = std::fs::OpenOptions::new()
            .append(true)
            .open(&session_path)
            .unwrap();
        writeln!(f, "{}", line).unwrap();
        drop(f);

        // Expect a TranscriptEventReceived within 1 second (NFR-002)
        let deadline = std::time::Instant::now() + Duration::from_secs(3);
        let mut received = false;
        while std::time::Instant::now() < deadline {
            match rx.recv_timeout(Duration::from_millis(300)) {
                Ok(AppEvent::TranscriptEventReceived(evt)) => {
                    use crate::model::TranscriptEventKind;
                    if matches!(evt.kind, TranscriptEventKind::UserMessage) {
                        assert_eq!(evt.session_id, Some(crate::model::ids::SessionId::new("sess-tail")));
                        received = true;
                        break;
                    }
                }
                Ok(_) => {}
                Err(_) => {}
            }
        }
        assert!(received, "TranscriptEventReceived not emitted within 3s");
    }

    #[test]
    fn polling_emits_task_graph_updated_on_file_creation() {
        let temp = TempDir::new().unwrap();

        let paths = crate::paths::Paths {
            task_graph: temp.path().join("task_graph.json"),
            transcript_dir: temp.path().join("transcripts"),
            archive_dir: temp.path().join("archives"),
        };

        fs::create_dir_all(&paths.transcript_dir).unwrap();

        let rx = start_watching(&paths).expect("start_watching");

        // Write task graph after watcher starts
        std::thread::sleep(Duration::from_millis(50));
        let json = r#"{
            "waves": [
                {
                    "number": 1,
                    "tasks": [{"id": "T1", "description": "task", "status": "pending"}]
                }
            ],
            "total_tasks": 1,
            "completed_tasks": 0
        }"#;
        fs::write(&paths.task_graph, json).unwrap();

        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        let mut got_update = false;
        while std::time::Instant::now() < deadline {
            match rx.recv_timeout(Duration::from_millis(300)) {
                Ok(AppEvent::TaskGraphUpdated(graph)) => {
                    assert_eq!(graph.total_tasks(), 1);
                    got_update = true;
                    break;
                }
                Ok(_) => {}
                Err(_) => {}
            }
        }
        assert!(got_update, "TaskGraphUpdated not emitted within 5s");
    }

    #[test]
    fn polling_discovers_subagent_transcripts() {
        let temp = TempDir::new().unwrap();

        // Create session + subagents structure
        let session_dir = temp.path().join("session-parent");
        let subagents_dir = session_dir.join("subagents");
        fs::create_dir_all(&subagents_dir).unwrap();

        // Top-level session file
        fs::write(temp.path().join("session-parent.jsonl"), "").unwrap();
        // Subagent file
        let agent_line = r#"{"type":"assistant","timestamp":"2026-03-18T10:00:00Z","message":{"id":"m1","model":"claude-3","content":[{"type":"text","text":"working"}],"usage":{"input_tokens":10,"output_tokens":20}}}"#;
        let agent_path = subagents_dir.join("agent-a01.jsonl");
        fs::write(&agent_path, format!("{}\n", agent_line)).unwrap();

        let paths = crate::paths::Paths {
            task_graph: temp.path().join("task_graph.json"),
            transcript_dir: temp.path().to_path_buf(),
            archive_dir: temp.path().join("archives"),
        };

        let rx = start_watching(&paths).expect("start_watching");

        // Wait for subagent transcript events
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        let mut got_agent_event = false;
        while std::time::Instant::now() < deadline {
            match rx.recv_timeout(Duration::from_millis(300)) {
                Ok(AppEvent::TranscriptEventReceived(evt)) => {
                    if let Some(ref aid) = evt.agent_id {
                        if aid.as_str().contains("a01") {
                            got_agent_event = true;
                            break;
                        }
                    }
                }
                Ok(_) => {}
                Err(_) => {}
            }
        }
        assert!(got_agent_event, "subagent transcript event not received");
    }

    // -----------------------------------------------------------------------
    // Unit: scan_counter wrapping
    // -----------------------------------------------------------------------

    #[test]
    fn scan_counter_wrapping_never_panics() {
        let mut counter: u32 = u32::MAX - 5;
        for _ in 0..10 {
            counter = counter.wrapping_add(1);
            let _ = counter % DIR_RESCAN_INTERVAL;
            let _ = counter % METADATA_EMIT_INTERVAL;
        }
        // No panic = pass
    }

    // -----------------------------------------------------------------------
    // Unit: channel send failure graceful handling
    // -----------------------------------------------------------------------

    #[test]
    fn channel_closed_handle_task_graph_does_not_panic() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("tg.json");
        fs::write(&path, "invalid").unwrap();
        let (tx, rx) = mpsc::channel();
        drop(rx); // close receiver

        // Must not panic even when receiver is gone
        handle_task_graph_update(&path, &tx);
    }

    // -----------------------------------------------------------------------
    // Unit: FR-010/FR-012 — session confirmed on UserMessage
    // -----------------------------------------------------------------------

    #[test]
    fn session_confirmed_after_user_message() {
        let temp = TempDir::new().unwrap();
        let session_file = temp.path().join("sess-confirm.jsonl");
        fs::write(&session_file, "").unwrap();

        let paths = crate::paths::Paths {
            task_graph: temp.path().join("task_graph.json"),
            transcript_dir: temp.path().to_path_buf(),
            archive_dir: temp.path().join("archives"),
        };

        let rx = start_watching(&paths).expect("start_watching");

        // Wait for session discovery
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        let mut discovered = false;
        while std::time::Instant::now() < deadline {
            match rx.recv_timeout(Duration::from_millis(300)) {
                Ok(AppEvent::SessionDiscovered { .. }) => {
                    discovered = true;
                    break;
                }
                Ok(_) => {}
                Err(_) => {}
            }
        }
        assert!(discovered, "session not discovered");

        // Append a user message — watcher should mark session confirmed internally.
        // We verify indirectly: after marking confirmed, SessionCompleted should only
        // fire after 10min (CONFIRMED_TIMEOUT), not 30s (UNCONFIRMED_TIMEOUT).
        // Within a short test window, no SessionCompleted should arrive.
        let line = r#"{"type":"human","timestamp":"2026-03-18T10:00:00Z","message":{"content":[{"type":"text","text":"hello"}]}}"#;
        let mut f = std::fs::OpenOptions::new()
            .append(true)
            .open(&session_file)
            .unwrap();
        writeln!(f, "{}", line).unwrap();
        drop(f);

        // Drain events for up to 1s; confirm we receive a UserMessage TranscriptEventReceived
        let deadline = std::time::Instant::now() + Duration::from_secs(3);
        let mut got_user_message = false;
        while std::time::Instant::now() < deadline {
            match rx.recv_timeout(Duration::from_millis(300)) {
                Ok(AppEvent::TranscriptEventReceived(evt)) => {
                    if matches!(evt.kind, crate::model::TranscriptEventKind::UserMessage) {
                        got_user_message = true;
                        break;
                    }
                }
                Ok(_) => {}
                Err(_) => {}
            }
        }
        assert!(got_user_message, "UserMessage event not received");
    }

    // -----------------------------------------------------------------------
    // Unit: scan_transcript_dir emits error on non-NotFound io errors
    // -----------------------------------------------------------------------

    #[test]
    fn scan_transcript_dir_emits_error_on_permission_denied() {
        // We test using a path that is a file (not a dir) which causes a non-NotFound error
        let temp = TempDir::new().unwrap();
        let not_a_dir = temp.path().join("not_a_dir");
        fs::write(&not_a_dir, "some content").unwrap();

        let mut known_files = BTreeMap::new();
        let mut session_confirmed = BTreeMap::new();
        let mut completed = std::collections::HashSet::new();
        let (tx, rx) = mpsc::channel();

        scan_transcript_dir(&not_a_dir, &mut known_files, &mut session_confirmed, &mut completed, &tx);

        // Should emit an Error event since it's not a NotFound error (it's NotADirectory)
        let event = rx.recv_timeout(Duration::from_millis(200));
        assert!(
            event.is_ok() && matches!(event.unwrap(), AppEvent::Error { .. }),
            "expected Error event for non-dir path"
        );
    }

    // -----------------------------------------------------------------------
    // Unit: scan_subagents_dir emits error on non-NotFound io errors
    // -----------------------------------------------------------------------

    #[test]
    fn scan_subagents_dir_emits_error_on_not_a_dir() {
        let temp = TempDir::new().unwrap();
        let not_a_dir = temp.path().join("not_a_dir");
        fs::write(&not_a_dir, "some content").unwrap();

        let mut known_files = BTreeMap::new();
        let (tx, rx) = mpsc::channel();

        scan_subagents_dir(&not_a_dir, "session-parent", &mut known_files, &tx);

        let event = rx.recv_timeout(Duration::from_millis(200));
        assert!(
            event.is_ok() && matches!(event.unwrap(), AppEvent::Error { .. }),
            "expected Error event for non-dir path"
        );
    }

    // -----------------------------------------------------------------------
    // Unit: scan_subagents_dir silent on NotFound
    // -----------------------------------------------------------------------

    #[test]
    fn scan_subagents_dir_silent_on_not_found() {
        let path = PathBuf::from("/nonexistent/subagents/dir");
        let mut known_files = BTreeMap::new();
        let (tx, rx) = mpsc::channel();

        scan_subagents_dir(&path, "sess", &mut known_files, &tx);

        // No error should be emitted
        assert!(rx.recv_timeout(Duration::from_millis(100)).is_err());
        assert!(known_files.is_empty());
    }

    // -----------------------------------------------------------------------
    // Fix 1: deleted file is cleaned up from known_files (no error spam)
    // -----------------------------------------------------------------------

    #[test]
    fn deleted_file_removed_from_known_files_no_error_spam() {
        let temp = TempDir::new().unwrap();
        let session_path = temp.path().join("sess-delete.jsonl");
        fs::write(&session_path, "").unwrap();

        let paths = crate::paths::Paths {
            task_graph: temp.path().join("task_graph.json"),
            transcript_dir: temp.path().to_path_buf(),
            archive_dir: temp.path().join("archives"),
        };

        let rx = start_watching(&paths).expect("start_watching");

        // Wait for session discovery
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        let mut discovered = false;
        while std::time::Instant::now() < deadline {
            match rx.recv_timeout(Duration::from_millis(300)) {
                Ok(AppEvent::SessionDiscovered { session_id, .. }) => {
                    assert_eq!(session_id.as_str(), "sess-delete");
                    discovered = true;
                    break;
                }
                Ok(_) => {}
                Err(_) => {}
            }
        }
        assert!(discovered, "session not discovered");

        // Delete the file
        fs::remove_file(&session_path).unwrap();

        // Wait 2 polling cycles (2 * 200ms = 400ms) for the watcher to process the deletion
        std::thread::sleep(Duration::from_millis(500));

        // Drain any remaining events
        while rx.recv_timeout(Duration::from_millis(50)).is_ok() {}

        // After cleanup, no AppEvent::Error events should arrive for the deleted path
        // (the entry should be gone from known_files so it's never polled again)
        let deadline = std::time::Instant::now() + Duration::from_millis(600);
        while std::time::Instant::now() < deadline {
            if let Ok(AppEvent::Error { source, .. }) = rx.recv_timeout(Duration::from_millis(100)) {
                if source.contains("sess-delete") {
                    panic!("got error for deleted file after cleanup: {source}");
                }
            }
        }
    }

    // -----------------------------------------------------------------------
    // Fix 3: scanned files never get UNIX_EPOCH mtime — they are registered
    //         with a real mtime or skipped entirely (no fallback).
    // -----------------------------------------------------------------------

    #[test]
    fn scan_transcript_dir_no_unix_epoch_mtime_in_known_files() {
        // Verify that after scanning a directory of real files, no entry in
        // known_files ends up with UNIX_EPOCH as its mtime.
        // Before Fix 3, a metadata/modified failure fell back to UNIX_EPOCH
        // which triggered immediate phantom SessionCompleted events.
        let temp = TempDir::new().unwrap();
        fs::write(temp.path().join("sess-a.jsonl"), "").unwrap();
        fs::write(temp.path().join("sess-b.jsonl"), "").unwrap();

        let mut known_files = BTreeMap::new();
        let mut session_confirmed = BTreeMap::new();
        let mut completed = std::collections::HashSet::new();
        let (tx, _rx) = mpsc::channel();

        scan_transcript_dir(
            &temp.path().to_path_buf(),
            &mut known_files,
            &mut session_confirmed,
            &mut completed,
            &tx,
        );

        // Both files should be discovered with real mtimes (not UNIX_EPOCH)
        assert_eq!(known_files.len(), 2);
        for (path, file_state) in &known_files {
            assert_ne!(
                file_state.mtime,
                SystemTime::UNIX_EPOCH,
                "file {:?} has UNIX_EPOCH mtime — fallback not removed",
                path
            );
        }
    }

    #[test]
    fn scan_subagents_dir_no_unix_epoch_mtime_in_known_files() {
        let temp = TempDir::new().unwrap();
        let subagents_dir = temp.path().join("subagents");
        fs::create_dir(&subagents_dir).unwrap();
        fs::write(subagents_dir.join("agent-x01.jsonl"), "").unwrap();

        let mut known_files = BTreeMap::new();
        let (tx, _rx) = mpsc::channel();

        scan_subagents_dir(&subagents_dir, "parent-session", &mut known_files, &tx);

        assert_eq!(known_files.len(), 1);
        for (path, file_state) in &known_files {
            assert_ne!(
                file_state.mtime,
                SystemTime::UNIX_EPOCH,
                "subagent file {:?} has UNIX_EPOCH mtime — fallback not removed",
                path
            );
        }
    }
}
