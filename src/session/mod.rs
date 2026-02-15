use std::collections::{BTreeMap, VecDeque};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

use serde::Deserialize;

use crate::error::SessionError;
use crate::model::{Agent, AgentId, HookEvent, SessionArchive, SessionMeta, TaskGraph};

// ============================================================================
// FUNCTIONAL CORE: Pure functions for serialization and data transformation
// ============================================================================

/// Serialize session archive to JSON string.
/// Pure function: no side effects, deterministic.
///
/// # Arguments
/// * `archive` - The session archive to serialize
///
/// # Returns
/// * `Ok(String)` - JSON string representation
/// * `Err(SessionError)` - Serialization error
pub fn serialize_session(archive: &SessionArchive) -> Result<String, SessionError> {
    serde_json::to_string_pretty(archive).map_err(SessionError::from)
}

/// Deserialize JSON string to session archive.
/// Pure function: no side effects, deterministic.
///
/// # Arguments
/// * `content` - JSON string to parse
///
/// # Returns
/// * `Ok(SessionArchive)` - Parsed session archive
/// * `Err(SessionError)` - Deserialization error
pub fn deserialize_session(content: &str) -> Result<SessionArchive, SessionError> {
    serde_json::from_str(content).map_err(SessionError::from)
}

/// Generate deterministic filename for session archive.
/// Pure function: based solely on session metadata.
///
/// Format: `{session_id}.json`
/// Example: `s20260211-095900.json`
///
/// # Arguments
/// * `meta` - Session metadata
///
/// # Returns
/// Filename string
pub fn generate_filename(meta: &SessionMeta) -> String {
    format!("{}.json", meta.id)
}

/// Extract session metadata from full session archive.
/// Pure function: data transformation only.
///
/// # Arguments
/// * `archive` - Full session archive
///
/// # Returns
/// Session metadata
pub fn extract_metadata(archive: &SessionArchive) -> SessionMeta {
    archive.meta.clone()
}

/// Build session archive from explicit domain parameters, filtering by session_id.
/// Pure function: transforms domain data to archive format.
/// Filters events and agents to only include those belonging to this session.
///
/// # Arguments
/// * `task_graph` - Optional task graph (project-level, not session-specific)
/// * `events` - Ring buffer of hook events
/// * `agents` - Active agents keyed by agent ID
/// * `meta` - Session metadata (contains session_id for filtering)
///
/// # Returns
/// Session archive ready for serialization (contains only data for this session)
pub fn build_archive(
    task_graph: Option<&TaskGraph>,
    events: &VecDeque<HookEvent>,
    agents: &BTreeMap<AgentId, Agent>,
    meta: &SessionMeta,
) -> SessionArchive {
    let mut archive = SessionArchive::new(meta.clone());

    if let Some(tg) = task_graph {
        archive = archive.with_task_graph(tg.clone());
    }

    // Filter events by session_id before cloning
    let session_events: Vec<_> = events
        .iter()
        .filter(|e| e.session_id.as_ref() == Some(&meta.id))
        .cloned()
        .collect();
    archive = archive.with_events(session_events);

    // Filter agents by session_id before cloning
    let session_agents: BTreeMap<_, _> = agents
        .iter()
        .filter(|(_, agent)| agent.session_id.as_ref() == Some(&meta.id))
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    archive = archive.with_agents(session_agents);

    archive
}

/// Check if auto-save should trigger based on elapsed time.
/// Pure function: time comparison only.
///
/// # Arguments
/// * `last_save` - Instant of last save operation
/// * `now` - Current instant
/// * `interval_secs` - Auto-save interval in seconds
///
/// # Returns
/// `true` if interval has elapsed, `false` otherwise
pub fn should_auto_save(last_save: Instant, now: Instant, interval_secs: u64) -> bool {
    now.duration_since(last_save).as_secs() >= interval_secs
}

// ============================================================================
// IMPERATIVE SHELL: I/O operations for save/load/list/delete
// ============================================================================

/// Save session archive to disk.
/// I/O operation: writes file, creates directories if needed.
///
/// # Arguments
/// * `path` - Full path to archive file (including filename)
/// * `archive` - Session archive to save
///
/// # Returns
/// * `Ok(PathBuf)` - Path to saved file
/// * `Err(SessionError)` - I/O or serialization error
pub fn save_session(path: &Path, archive: &SessionArchive) -> Result<PathBuf, SessionError> {
    // Serialize (functional core)
    let content = serialize_session(archive)?;

    // Create parent directory if needed (I/O)
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| SessionError::Io { path: parent.display().to_string(), source: e })?;
    }

    // Write to disk (I/O)
    fs::write(path, &content)
        .map_err(|e| SessionError::Io { path: path.display().to_string(), source: e })?;

    Ok(path.to_path_buf())
}

/// Load session archive from disk.
/// I/O operation: reads file and deserializes.
///
/// # Arguments
/// * `path` - Full path to archive file
///
/// # Returns
/// * `Ok(SessionArchive)` - Loaded session archive
/// * `Err(SessionError)` - I/O or deserialization error
pub fn load_session(path: &Path) -> Result<SessionArchive, SessionError> {
    // Read from disk (I/O)
    let content = fs::read_to_string(path)
        .map_err(|e| SessionError::Io { path: path.display().to_string(), source: e })?;

    // Deserialize (functional core)
    deserialize_session(&content)
}

/// List all session archives in directory.
/// I/O operation: reads directory and parses each archive file.
/// Returns full archives so callers retain agents/events/task_graph.
///
/// Gracefully handles:
/// - Empty directory (returns empty vec)
/// - Corrupt files (returns errors in second tuple element)
/// - Missing directory (returns empty vec)
///
/// # Arguments
/// * `dir` - Directory containing session archives
///
/// # Returns
/// * `Ok((Vec<SessionArchive>, Vec<SessionError>))` - Tuple of (successful archives, errors)
///   - Archives sorted by timestamp (newest first)
///   - Errors for corrupt/unreadable files
/// * `Err(SessionError)` - I/O error reading directory itself
pub fn list_sessions(dir: &Path) -> Result<(Vec<SessionArchive>, Vec<SessionError>), SessionError> {
    if !dir.exists() {
        return Ok((Vec::new(), Vec::new()));
    }

    let entries = fs::read_dir(dir)
        .map_err(|e| SessionError::Io { path: dir.display().to_string(), source: e })?;

    let mut sessions = Vec::new();
    let mut errors = Vec::new();

    for entry in entries {
        let entry = match entry {
            Ok(e) => e,
            Err(e) => {
                errors.push(SessionError::Io {
                    path: dir.display().to_string(),
                    source: e,
                });
                continue;
            }
        };

        let path = entry.path();

        if path.extension().and_then(|s| s.to_str()) != Some("json") {
            continue;
        }

        match load_session(&path) {
            Ok(archive) => sessions.push(archive),
            Err(e) => errors.push(e),
        }
    }

    sessions.sort_by(|a, b| b.meta.timestamp.cmp(&a.meta.timestamp));

    Ok((sessions, errors))
}

/// Helper for deserializing only the `meta` field from a session archive JSON.
#[derive(Deserialize)]
struct MetaOnly {
    meta: SessionMeta,
}

/// List session metas without deserializing full archives.
/// Much faster than `list_sessions` — skips events/agents/task_graph.
/// Returns `(path, meta)` tuples so full archive can be loaded later by path.
///
/// # Returns
/// * `Ok((Vec<(PathBuf, SessionMeta)>, Vec<SessionError>))` - Tuple of (successful metas, errors)
///   - Metas sorted by timestamp (newest first)
///   - Errors for corrupt/unreadable files
/// * `Err(SessionError)` - I/O error reading directory itself
#[allow(clippy::type_complexity)]
pub fn list_session_metas(dir: &Path) -> Result<(Vec<(PathBuf, SessionMeta)>, Vec<SessionError>), SessionError> {
    if !dir.exists() {
        return Ok((Vec::new(), Vec::new()));
    }

    let entries = fs::read_dir(dir)
        .map_err(|e| SessionError::Io { path: dir.display().to_string(), source: e })?;

    let mut metas = Vec::new();
    let mut errors = Vec::new();

    for entry in entries {
        let entry = match entry {
            Ok(e) => e,
            Err(e) => {
                errors.push(SessionError::Io {
                    path: dir.display().to_string(),
                    source: e,
                });
                continue;
            }
        };

        let path = entry.path();

        if path.extension().and_then(|s| s.to_str()) != Some("json") {
            continue;
        }

        let content = match fs::read_to_string(&path) {
            Ok(c) => c,
            Err(e) => {
                errors.push(SessionError::Io {
                    path: path.display().to_string(),
                    source: e,
                });
                continue;
            }
        };

        match serde_json::from_str::<MetaOnly>(&content) {
            Ok(meta_only) => metas.push((path, meta_only.meta)),
            Err(e) => errors.push(SessionError::from(e)),
        }
    }

    metas.sort_by(|a, b| b.1.timestamp.cmp(&a.1.timestamp));

    Ok((metas, errors))
}

/// Delete session archive file.
/// I/O operation: removes file from disk.
///
/// # Arguments
/// * `path` - Full path to archive file
///
/// # Returns
/// * `Ok(())` - File deleted successfully
/// * `Err(SessionError)` - I/O error
pub fn delete_session(path: &Path) -> Result<(), SessionError> {
    fs::remove_file(path)
        .map_err(|e| SessionError::Io { path: path.display().to_string(), source: e })
}

/// Auto-save tick: save session if interval elapsed.
/// Combines pure time check with I/O save operation.
///
/// # Arguments
/// * `path` - Full path to archive file
/// * `archive` - Session archive to save
/// * `last_save` - Instant of last save operation
/// * `interval_secs` - Auto-save interval in seconds (typically 30)
///
/// # Returns
/// * `Ok(Some(Instant))` - New save timestamp if save occurred
/// * `Ok(None)` - No save needed (interval not elapsed)
/// * `Err(SessionError)` - Save operation failed
pub fn auto_save_tick(
    path: &Path,
    archive: &SessionArchive,
    last_save: Instant,
    interval_secs: u64,
) -> Result<Option<Instant>, SessionError> {
    let now = Instant::now();

    if should_auto_save(last_save, now, interval_secs) {
        // Save and return new timestamp
        save_session(path, archive)?;
        Ok(Some(now))
    } else {
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{HookEvent, HookEventKind, SessionStatus, TaskGraph};
    use chrono::Utc;
    use std::collections::{BTreeMap, VecDeque};
    use std::time::Duration;

    #[test]
    fn serialize_deserialize_round_trip() {
        let meta = SessionMeta::new("s1", Utc::now(), "/proj".to_string());
        let archive = SessionArchive::new(meta);

        let json = serialize_session(&archive).unwrap();
        let restored = deserialize_session(&json).unwrap();

        assert_eq!(archive, restored);
    }

    #[test]
    fn serialize_deserialize_round_trip_with_data() {
        let meta = SessionMeta::new("s1", Utc::now(), "/proj".to_string())
            .with_status(SessionStatus::Completed)
            .with_duration(Duration::from_secs(300));

        let task_graph = TaskGraph::empty();

        let archive = SessionArchive::new(meta)
            .with_task_graph(task_graph)
            .with_agents(BTreeMap::new());

        let json = serialize_session(&archive).unwrap();
        let restored = deserialize_session(&json).unwrap();

        assert_eq!(archive, restored);
    }

    #[test]
    fn deserialize_invalid_json() {
        let result = deserialize_session("not valid json");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("JSON"));
    }

    #[test]
    fn generate_filename_uses_session_id() {
        let meta = SessionMeta::new("s20260211-095900", Utc::now(), "/proj".to_string());
        let filename = generate_filename(&meta);
        assert_eq!(filename, "s20260211-095900.json");
    }

    #[test]
    fn extract_metadata_returns_clone() {
        let meta = SessionMeta::new("s1", Utc::now(), "/proj".to_string());
        let archive = SessionArchive::new(meta.clone());

        let extracted = extract_metadata(&archive);
        assert_eq!(extracted, meta);
    }

    #[test]
    fn build_archive_includes_task_graph() {
        let task_graph = TaskGraph::empty();
        let events = VecDeque::new();
        let agents = BTreeMap::new();
        let meta = SessionMeta::new("s1", Utc::now(), "/proj".to_string());

        let archive = build_archive(Some(&task_graph), &events, &agents, &meta);

        assert!(archive.task_graph.is_some());
    }

    #[test]
    fn build_archive_includes_events_and_agents() {
        let events = VecDeque::new();
        let mut agents = BTreeMap::new();
        let meta = SessionMeta::new("s1", Utc::now(), "/proj".to_string());

        // Agent with matching session_id
        let mut agent = Agent::new("a01", Utc::now());
        agent.session_id = Some(meta.id.clone());
        agents.insert("a01".into(), agent);

        let archive = build_archive(None, &events, &agents, &meta);

        assert_eq!(archive.agents.len(), 1);
        assert!(archive.events.is_empty());
    }

    #[test]
    fn should_auto_save_triggers_after_interval() {
        let start = Instant::now();
        let later = start + Duration::from_secs(31);

        assert!(should_auto_save(start, later, 30));
    }

    #[test]
    fn should_auto_save_does_not_trigger_before_interval() {
        let start = Instant::now();
        let later = start + Duration::from_secs(29);

        assert!(!should_auto_save(start, later, 30));
    }

    #[test]
    fn should_auto_save_triggers_exactly_at_interval() {
        let start = Instant::now();
        let later = start + Duration::from_secs(30);

        assert!(should_auto_save(start, later, 30));
    }

    #[test]
    fn build_archive_filters_events_by_session_id() {
        let meta = SessionMeta::new("s1", Utc::now(), "/proj".to_string());
        let mut events = VecDeque::new();

        // Event matching session_id
        let mut e1 = HookEvent::new(Utc::now(), HookEventKind::SessionStart);
        e1.session_id = Some(meta.id.clone());
        events.push_back(e1);

        // Event with different session_id
        let mut e2 = HookEvent::new(Utc::now(), HookEventKind::notification("test".into()));
        e2.session_id = Some("s2".into());
        events.push_back(e2);

        // Event with no session_id
        let e3 = HookEvent::new(Utc::now(), HookEventKind::notification("test2".into()));
        events.push_back(e3);

        let archive = build_archive(None, &events, &BTreeMap::new(), &meta);

        assert_eq!(archive.events.len(), 1);
        assert_eq!(archive.events[0].session_id.as_ref(), Some(&meta.id));
    }

    #[test]
    fn build_archive_filters_agents_by_session_id() {
        let meta = SessionMeta::new("s1", Utc::now(), "/proj".to_string());
        let mut agents = BTreeMap::new();

        // Agent matching session_id
        let mut a1 = Agent::new("a01", Utc::now());
        a1.session_id = Some(meta.id.clone());
        agents.insert("a01".into(), a1);

        // Agent with different session_id
        let mut a2 = Agent::new("a02", Utc::now());
        a2.session_id = Some("s2".into());
        agents.insert("a02".into(), a2);

        // Agent with no session_id
        let a3 = Agent::new("a03", Utc::now());
        agents.insert("a03".into(), a3);

        let archive = build_archive(None, &VecDeque::new(), &agents, &meta);

        assert_eq!(archive.agents.len(), 1);
        assert!(archive.agents.contains_key(&AgentId::new("a01")));
        assert!(!archive.agents.contains_key(&AgentId::new("a02")));
        assert!(!archive.agents.contains_key(&AgentId::new("a03")));
    }

    #[test]
    fn build_archive_handles_empty_domain() {
        let meta = SessionMeta::new("s1", Utc::now(), "/proj".to_string());
        let events = VecDeque::new();
        let agents = BTreeMap::new();

        let archive = build_archive(None, &events, &agents, &meta);

        assert!(archive.events.is_empty());
        assert!(archive.agents.is_empty());
        assert!(archive.task_graph.is_none());
    }

    #[test]
    fn build_archive_includes_task_graph_when_present() {
        use crate::model::{Task, TaskStatus, Wave};

        // Create task graph with 5 tasks (2 completed)
        let task_graph = TaskGraph::new(vec![
            Wave::new(
                1,
                vec![
                    Task::new("T1", "Task 1".to_string(), TaskStatus::Completed),
                    Task::new("T2", "Task 2".to_string(), TaskStatus::Completed),
                    Task::new("T3", "Task 3".to_string(), TaskStatus::Pending),
                ],
            ),
            Wave::new(
                2,
                vec![
                    Task::new("T4", "Task 4".to_string(), TaskStatus::Running),
                    Task::new("T5", "Task 5".to_string(), TaskStatus::Pending),
                ],
            ),
        ]);

        let meta = SessionMeta::new("s1", Utc::now(), "/proj".to_string());

        let archive = build_archive(
            Some(&task_graph),
            &VecDeque::new(),
            &BTreeMap::new(),
            &meta,
        );

        assert!(archive.task_graph.is_some());
        assert_eq!(archive.task_graph.as_ref().unwrap().total_tasks(), 5);
        assert_eq!(archive.task_graph.as_ref().unwrap().completed_tasks(), 2);
    }

    #[test]
    fn list_sessions_returns_errors_for_corrupt_files() {
        use tempfile::TempDir;

        let temp = TempDir::new().unwrap();
        let dir = temp.path();

        // Create a valid session file
        let meta = SessionMeta::new("s1", Utc::now(), "/proj".to_string());
        let archive = SessionArchive::new(meta);
        let valid_path = dir.join("s1.json");
        save_session(&valid_path, &archive).unwrap();

        // Create a corrupt session file
        let corrupt_path = dir.join("s2.json");
        fs::write(&corrupt_path, "not valid json").unwrap();

        // List sessions
        let (sessions, errors) = list_sessions(dir).unwrap();

        // Should have 1 successful session and 1 error
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].meta.id.as_str(), "s1");
        assert_eq!(errors.len(), 1);
        assert!(errors[0].to_string().contains("JSON"));
    }

    #[test]
    fn list_session_metas_returns_errors_for_corrupt_files() {
        use tempfile::TempDir;

        let temp = TempDir::new().unwrap();
        let dir = temp.path();

        // Create a valid session file
        let meta = SessionMeta::new("s1", Utc::now(), "/proj".to_string());
        let archive = SessionArchive::new(meta);
        let valid_path = dir.join("s1.json");
        save_session(&valid_path, &archive).unwrap();

        // Create a corrupt session file
        let corrupt_path = dir.join("s2.json");
        fs::write(&corrupt_path, "not valid json").unwrap();

        // List session metas
        let (metas, errors) = list_session_metas(dir).unwrap();

        // Should have 1 successful meta and 1 error
        assert_eq!(metas.len(), 1);
        assert_eq!(metas[0].1.id.as_str(), "s1");
        assert_eq!(errors.len(), 1);
        assert!(errors[0].to_string().contains("JSON"));
    }

    #[test]
    fn list_sessions_empty_dir_returns_empty_vecs() {
        use tempfile::TempDir;

        let temp = TempDir::new().unwrap();
        let dir = temp.path();

        let (sessions, errors) = list_sessions(dir).unwrap();
        assert!(sessions.is_empty());
        assert!(errors.is_empty());
    }

    #[test]
    fn list_session_metas_empty_dir_returns_empty_vecs() {
        use tempfile::TempDir;

        let temp = TempDir::new().unwrap();
        let dir = temp.path();

        let (metas, errors) = list_session_metas(dir).unwrap();
        assert!(metas.is_empty());
        assert!(errors.is_empty());
    }
}
