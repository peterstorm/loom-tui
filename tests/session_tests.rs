use chrono::Utc;
use loom_tui::app::AppState;
use loom_tui::model::{SessionArchive, SessionMeta, SessionStatus, TaskGraph};
use loom_tui::session::{
    auto_save_tick, build_archive, delete_session, generate_filename, list_session_metas,
    list_sessions, load_session, save_session,
};
use std::time::{Duration, Instant};
use tempfile::TempDir;

// ============================================================================
// Integration Tests: I/O operations with tempdir
// ============================================================================

#[test]
fn save_and_load_session() {
    let tmpdir = TempDir::new().unwrap();
    let archive_path = tmpdir.path().join("s1.json");

    // Create archive
    let meta = SessionMeta::new("s1".into(), Utc::now(), "/proj".into());
    let archive = SessionArchive::new(meta);

    // Save to disk
    let saved_path = save_session(&archive_path, &archive).unwrap();
    assert_eq!(saved_path, archive_path);
    assert!(archive_path.exists());

    // Load from disk
    let loaded = load_session(&archive_path).unwrap();
    assert_eq!(loaded, archive);
}

#[test]
fn save_creates_parent_directory() {
    let tmpdir = TempDir::new().unwrap();
    let nested_path = tmpdir.path().join("sessions").join("s1.json");

    let meta = SessionMeta::new("s1".into(), Utc::now(), "/proj".into());
    let archive = SessionArchive::new(meta);

    // Parent dir does not exist yet
    assert!(!nested_path.parent().unwrap().exists());

    // Save should create parent
    save_session(&nested_path, &archive).unwrap();
    assert!(nested_path.exists());
    assert!(nested_path.parent().unwrap().exists());
}

#[test]
fn load_nonexistent_file_returns_error() {
    let tmpdir = TempDir::new().unwrap();
    let path = tmpdir.path().join("nonexistent.json");

    let result = load_session(&path);
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("failed to read"));
}

#[test]
fn load_corrupt_file_returns_error() {
    let tmpdir = TempDir::new().unwrap();
    let path = tmpdir.path().join("corrupt.json");

    // Write invalid JSON
    std::fs::write(&path, "not valid json").unwrap();

    let result = load_session(&path);
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("deserialize error"));
}

#[test]
fn list_sessions_in_empty_directory() {
    let tmpdir = TempDir::new().unwrap();
    let sessions_dir = tmpdir.path().join("sessions");
    std::fs::create_dir(&sessions_dir).unwrap();

    let sessions = list_sessions(&sessions_dir).unwrap();
    assert!(sessions.is_empty());
}

#[test]
fn list_sessions_missing_directory() {
    let tmpdir = TempDir::new().unwrap();
    let nonexistent = tmpdir.path().join("nonexistent");

    let sessions = list_sessions(&nonexistent).unwrap();
    assert!(sessions.is_empty());
}

#[test]
fn list_sessions_returns_multiple_archives() {
    let tmpdir = TempDir::new().unwrap();

    // Create multiple session archives
    let meta1 = SessionMeta::new("s1".into(), Utc::now(), "/proj".into());
    let meta2 = SessionMeta::new("s2".into(), Utc::now(), "/proj".into());

    let archive1 = SessionArchive::new(meta1);
    let archive2 = SessionArchive::new(meta2);

    save_session(&tmpdir.path().join("s1.json"), &archive1).unwrap();
    save_session(&tmpdir.path().join("s2.json"), &archive2).unwrap();

    // List sessions
    let sessions = list_sessions(tmpdir.path()).unwrap();
    assert_eq!(sessions.len(), 2);
}

#[test]
fn list_sessions_sorted_by_timestamp_newest_first() {
    let tmpdir = TempDir::new().unwrap();

    // Create sessions with different timestamps
    let now = Utc::now();
    let earlier = now - chrono::Duration::hours(1);
    let later = now + chrono::Duration::hours(1);

    let meta1 = SessionMeta::new("s1".into(), earlier, "/proj".into());
    let meta2 = SessionMeta::new("s2".into(), now, "/proj".into());
    let meta3 = SessionMeta::new("s3".into(), later, "/proj".into());

    save_session(&tmpdir.path().join("s1.json"), &SessionArchive::new(meta1)).unwrap();
    save_session(&tmpdir.path().join("s2.json"), &SessionArchive::new(meta2)).unwrap();
    save_session(&tmpdir.path().join("s3.json"), &SessionArchive::new(meta3)).unwrap();

    // List should be sorted newest first
    let sessions = list_sessions(tmpdir.path()).unwrap();
    assert_eq!(sessions.len(), 3);
    assert_eq!(sessions[0].meta.id, "s3");
    assert_eq!(sessions[1].meta.id, "s2");
    assert_eq!(sessions[2].meta.id, "s1");
}

#[test]
fn list_sessions_skips_corrupt_files() {
    let tmpdir = TempDir::new().unwrap();

    // Create valid session
    let meta = SessionMeta::new("s1".into(), Utc::now(), "/proj".into());
    save_session(&tmpdir.path().join("s1.json"), &SessionArchive::new(meta)).unwrap();

    // Create corrupt file
    std::fs::write(tmpdir.path().join("corrupt.json"), "invalid json").unwrap();

    // List should return only valid session
    let sessions = list_sessions(tmpdir.path()).unwrap();
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].meta.id, "s1");
}

#[test]
fn list_sessions_skips_non_json_files() {
    let tmpdir = TempDir::new().unwrap();

    // Create valid session
    let meta = SessionMeta::new("s1".into(), Utc::now(), "/proj".into());
    save_session(&tmpdir.path().join("s1.json"), &SessionArchive::new(meta)).unwrap();

    // Create non-JSON file
    std::fs::write(tmpdir.path().join("readme.txt"), "not a session").unwrap();

    // List should return only JSON files
    let sessions = list_sessions(tmpdir.path()).unwrap();
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].meta.id, "s1");
}

#[test]
fn delete_session_removes_file() {
    let tmpdir = TempDir::new().unwrap();
    let path = tmpdir.path().join("s1.json");

    // Create session
    let meta = SessionMeta::new("s1".into(), Utc::now(), "/proj".into());
    save_session(&path, &SessionArchive::new(meta)).unwrap();
    assert!(path.exists());

    // Delete session
    delete_session(&path).unwrap();
    assert!(!path.exists());
}

#[test]
fn delete_nonexistent_file_returns_error() {
    let tmpdir = TempDir::new().unwrap();
    let path = tmpdir.path().join("nonexistent.json");

    let result = delete_session(&path);
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("file not found"));
}

#[test]
fn auto_save_tick_saves_when_interval_elapsed() {
    let tmpdir = TempDir::new().unwrap();
    let path = tmpdir.path().join("s1.json");

    let meta = SessionMeta::new("s1".into(), Utc::now(), "/proj".into());
    let archive = SessionArchive::new(meta);

    // Simulate last save 31 seconds ago
    let last_save = Instant::now() - Duration::from_secs(31);

    // Auto-save should trigger
    let result = auto_save_tick(&path, &archive, last_save, 30);
    assert!(result.is_some());
    assert!(path.exists());
}

#[test]
fn auto_save_tick_does_not_save_before_interval() {
    let tmpdir = TempDir::new().unwrap();
    let path = tmpdir.path().join("s1.json");

    let meta = SessionMeta::new("s1".into(), Utc::now(), "/proj".into());
    let archive = SessionArchive::new(meta);

    // Simulate last save 5 seconds ago
    let last_save = Instant::now() - Duration::from_secs(5);

    // Auto-save should not trigger
    let result = auto_save_tick(&path, &archive, last_save, 30);
    assert!(result.is_none());
    assert!(!path.exists());
}

#[test]
fn build_archive_from_app_state() {
    let mut state = AppState::new();
    state.task_graph = Some(TaskGraph {
        waves: vec![],
        total_tasks: 0,
        completed_tasks: 0,
    });

    let meta = SessionMeta::new("s1".into(), Utc::now(), "/proj".into())
        .with_status(SessionStatus::Active);

    let archive = build_archive(&state, meta.clone());

    assert_eq!(archive.meta, meta);
    assert!(archive.task_graph.is_some());
}

#[test]
fn full_workflow_save_list_load_delete() {
    let tmpdir = TempDir::new().unwrap();

    // Build archive from state
    let state = AppState::new();
    let meta = SessionMeta::new("s1".into(), Utc::now(), "/proj".into())
        .with_status(SessionStatus::Completed);
    let archive = build_archive(&state, meta.clone());

    // Generate filename and save
    let filename = generate_filename(&meta);
    let path = tmpdir.path().join(filename);
    save_session(&path, &archive).unwrap();

    // List sessions
    let sessions = list_sessions(tmpdir.path()).unwrap();
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].meta.id, "s1");

    // Load session
    let loaded = load_session(&path).unwrap();
    assert_eq!(loaded.meta.id, "s1");

    // Delete session
    delete_session(&path).unwrap();
    let sessions = list_sessions(tmpdir.path()).unwrap();
    assert!(sessions.is_empty());
}

// ============================================================================
// list_session_metas tests
// ============================================================================

#[test]
fn list_session_metas_returns_correct_meta_and_path() {
    let tmpdir = TempDir::new().unwrap();

    let meta1 = SessionMeta::new("s1".into(), Utc::now(), "/proj".into());
    let meta2 = SessionMeta::new("s2".into(), Utc::now(), "/proj".into());

    save_session(&tmpdir.path().join("s1.json"), &SessionArchive::new(meta1)).unwrap();
    save_session(&tmpdir.path().join("s2.json"), &SessionArchive::new(meta2)).unwrap();

    let metas = list_session_metas(tmpdir.path()).unwrap();
    assert_eq!(metas.len(), 2);

    // Each entry has correct path and meta
    for (path, meta) in &metas {
        assert!(path.exists());
        assert!(path.extension().unwrap() == "json");
        assert!(!meta.id.is_empty());
    }
}

#[test]
fn list_session_metas_sorted_newest_first() {
    let tmpdir = TempDir::new().unwrap();

    let now = Utc::now();
    let earlier = now - chrono::Duration::hours(1);
    let later = now + chrono::Duration::hours(1);

    let m1 = SessionMeta::new("s1".into(), earlier, "/proj".into());
    let m2 = SessionMeta::new("s2".into(), now, "/proj".into());
    let m3 = SessionMeta::new("s3".into(), later, "/proj".into());

    save_session(&tmpdir.path().join("s1.json"), &SessionArchive::new(m1)).unwrap();
    save_session(&tmpdir.path().join("s2.json"), &SessionArchive::new(m2)).unwrap();
    save_session(&tmpdir.path().join("s3.json"), &SessionArchive::new(m3)).unwrap();

    let metas = list_session_metas(tmpdir.path()).unwrap();
    assert_eq!(metas.len(), 3);
    assert_eq!(metas[0].1.id, "s3"); // newest
    assert_eq!(metas[1].1.id, "s2");
    assert_eq!(metas[2].1.id, "s1"); // oldest
}

#[test]
fn list_session_metas_empty_dir() {
    let tmpdir = TempDir::new().unwrap();
    let metas = list_session_metas(tmpdir.path()).unwrap();
    assert!(metas.is_empty());
}

#[test]
fn list_session_metas_missing_dir() {
    let tmpdir = TempDir::new().unwrap();
    let metas = list_session_metas(&tmpdir.path().join("nonexistent")).unwrap();
    assert!(metas.is_empty());
}

#[test]
fn list_session_metas_skips_corrupt_files() {
    let tmpdir = TempDir::new().unwrap();

    let meta = SessionMeta::new("s1".into(), Utc::now(), "/proj".into());
    save_session(&tmpdir.path().join("s1.json"), &SessionArchive::new(meta)).unwrap();
    std::fs::write(tmpdir.path().join("corrupt.json"), "not json").unwrap();

    let metas = list_session_metas(tmpdir.path()).unwrap();
    assert_eq!(metas.len(), 1);
    assert_eq!(metas[0].1.id, "s1");
}
