use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

use serde::Deserialize;

use crate::app::state::DomainState;
use crate::error::SessionError;
use crate::model::{SessionArchive, SessionMeta};

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

/// Build session archive from domain state, filtering by session_id.
/// Pure function: transforms domain state to archive format.
/// Filters events and agents to only include those belonging to this session.
///
/// # Arguments
/// * `domain` - Domain state (agents, events, task graph)
/// * `meta` - Session metadata (contains session_id for filtering)
///
/// # Returns
/// Session archive ready for serialization (contains only data for this session)
pub fn build_archive(domain: &DomainState, meta: &SessionMeta) -> SessionArchive {
    let mut archive = SessionArchive::new(meta.clone());

    if let Some(ref task_graph) = domain.task_graph {
        archive = archive.with_task_graph(task_graph.clone());
    }

    // Filter events by session_id before cloning
    let session_events: Vec<_> = domain.events
        .iter()
        .filter(|e| e.session_id.as_ref() == Some(&meta.id))
        .cloned()
        .collect();
    archive = archive.with_events(session_events);

    // Filter agents by session_id before cloning
    let session_agents: BTreeMap<_, _> = domain.agents
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
/// - Corrupt files (skips them)
/// - Missing directory (returns empty vec)
///
/// # Arguments
/// * `dir` - Directory containing session archives
///
/// # Returns
/// * `Ok(Vec<SessionArchive>)` - Full archives sorted by timestamp (newest first)
/// * `Err(SessionError)` - I/O error reading directory
pub fn list_sessions(dir: &Path) -> Result<Vec<SessionArchive>, SessionError> {
    if !dir.exists() {
        return Ok(Vec::new());
    }

    let entries = fs::read_dir(dir)
        .map_err(|e| SessionError::Io { path: dir.display().to_string(), source: e })?;

    let mut sessions = Vec::new();

    for entry in entries {
        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue,
        };

        let path = entry.path();

        if path.extension().and_then(|s| s.to_str()) != Some("json") {
            continue;
        }

        if let Ok(archive) = load_session(&path) {
            sessions.push(archive);
        }
    }

    sessions.sort_by(|a, b| b.meta.timestamp.cmp(&a.meta.timestamp));

    Ok(sessions)
}

/// Helper for deserializing only the `meta` field from a session archive JSON.
#[derive(Deserialize)]
struct MetaOnly {
    meta: SessionMeta,
}

/// List session metas without deserializing full archives.
/// Much faster than `list_sessions` — skips events/agents/task_graph.
/// Returns `(path, meta)` tuples so full archive can be loaded later by path.
pub fn list_session_metas(dir: &Path) -> Result<Vec<(PathBuf, SessionMeta)>, SessionError> {
    if !dir.exists() {
        return Ok(Vec::new());
    }

    let entries = fs::read_dir(dir)
        .map_err(|e| SessionError::Io { path: dir.display().to_string(), source: e })?;

    let mut metas = Vec::new();

    for entry in entries {
        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue,
        };

        let path = entry.path();

        if path.extension().and_then(|s| s.to_str()) != Some("json") {
            continue;
        }

        let content = match fs::read_to_string(&path) {
            Ok(c) => c,
            Err(_) => continue,
        };

        if let Ok(meta_only) = serde_json::from_str::<MetaOnly>(&content) {
            metas.push((path, meta_only.meta));
        }
    }

    metas.sort_by(|a, b| b.1.timestamp.cmp(&a.1.timestamp));

    Ok(metas)
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
/// * `Some(Instant)` - New save timestamp if save occurred
/// * `None` - No save needed (interval not elapsed)
pub fn auto_save_tick(
    path: &Path,
    archive: &SessionArchive,
    last_save: Instant,
    interval_secs: u64,
) -> Option<Instant> {
    let now = Instant::now();

    if should_auto_save(last_save, now, interval_secs) {
        // Save and return new timestamp
        if save_session(path, archive).is_ok() {
            Some(now)
        } else {
            None
        }
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::AppState;
    use crate::model::{Agent, SessionStatus, TaskGraph};
    use chrono::Utc;
    use std::collections::BTreeMap;
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

        let task_graph = TaskGraph {
            waves: vec![],
            total_tasks: 0,
            completed_tasks: 0,
        };

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
        let mut state = AppState::new();
        state.domain.task_graph = Some(TaskGraph {
            waves: vec![],
            total_tasks: 0,
            completed_tasks: 0,
        });

        let meta = SessionMeta::new("s1", Utc::now(), "/proj".to_string());
        let archive = build_archive(&state.domain, &meta);

        assert!(archive.task_graph.is_some());
    }

    #[test]
    fn build_archive_includes_events_and_agents() {
        let mut state = AppState::new();
        let meta = SessionMeta::new("s1", Utc::now(), "/proj".to_string());

        // Agent with matching session_id
        let mut agent = Agent::new("a01", Utc::now());
        agent.session_id = Some(meta.id.clone());
        state.domain.agents.insert("a01".into(), agent);

        let archive = build_archive(&state.domain, &meta);

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
}
