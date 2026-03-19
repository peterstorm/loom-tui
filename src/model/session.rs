use super::ids::{AgentId, SessionId, TaskId};
use super::serde_utils::{deserialize_vec_or_empty, duration_opt_millis};
use super::{Agent, TaskGraph, TranscriptEvent};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::time::Duration;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionMeta {
    pub id: SessionId,
    pub timestamp: DateTime<Utc>,
    #[serde(
        default,
        with = "duration_opt_millis",
        skip_serializing_if = "Option::is_none"
    )]
    pub duration: Option<Duration>,
    pub status: SessionStatus,
    pub agent_count: u32,
    pub task_count: u32,
    pub event_count: u32,
    pub project_path: String,
    #[serde(default)]
    pub git_branch: Option<String>,
    #[serde(default)]
    pub loom_plan_id: Option<String>,
    #[serde(default)]
    pub wave_count: Option<u32>,
    #[serde(default)]
    pub failed_tasks: Vec<TaskId>,
    #[serde(default)]
    pub transcript_path: Option<String>,
    /// Last time an event was received for this session (for stale session cleanup)
    #[serde(skip)]
    pub last_event_at: Option<DateTime<Utc>>,
    /// Whether a real user prompt was received (filters out subagent phantom sessions)
    #[serde(skip)]
    pub confirmed: bool,
}

impl PartialEq for SessionMeta {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
            && self.timestamp == other.timestamp
            && self.duration == other.duration
            && self.status == other.status
            && self.agent_count == other.agent_count
            && self.task_count == other.task_count
            && self.event_count == other.event_count
            && self.project_path == other.project_path
            && self.git_branch == other.git_branch
            && self.loom_plan_id == other.loom_plan_id
            && self.wave_count == other.wave_count
            && self.failed_tasks == other.failed_tasks
            && self.transcript_path == other.transcript_path
        // last_event_at, confirmed intentionally excluded (runtime-only, not serialized)
    }
}

impl SessionMeta {
    pub fn new(id: impl Into<SessionId>, timestamp: DateTime<Utc>, project_path: String) -> Self {
        Self {
            id: id.into(),
            timestamp,
            duration: None,
            status: SessionStatus::Active,
            agent_count: 0,
            task_count: 0,
            event_count: 0,
            project_path,
            git_branch: None,
            loom_plan_id: None,
            wave_count: None,
            failed_tasks: Vec::new(),
            transcript_path: None,
            last_event_at: Some(timestamp),
            confirmed: false,
        }
    }

    pub fn with_status(mut self, status: SessionStatus) -> Self {
        self.status = status;
        self
    }

    pub fn with_duration(mut self, duration: Duration) -> Self {
        self.duration = Some(duration);
        self
    }

    pub fn with_git_branch(mut self, branch: String) -> Self {
        self.git_branch = Some(branch);
        self
    }

    pub fn with_loom_plan(mut self, plan_id: String) -> Self {
        self.loom_plan_id = Some(plan_id);
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum SessionStatus {
    Active,
    Completed,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SessionArchive {
    pub meta: SessionMeta,
    /// Archive format version. 2 = TranscriptEvent format. Old-format archives are gracefully skipped.
    #[serde(default)]
    pub version: u32,
    #[serde(default)]
    pub task_graph: Option<TaskGraph>,
    /// Events in TranscriptEvent format. Old-format archives are silently
    /// skipped — the custom deserializer falls back to Vec::new() on type mismatch (FR-026, SC-008).
    #[serde(default, deserialize_with = "deserialize_vec_or_empty")]
    pub events: Vec<TranscriptEvent>,
    #[serde(default)]
    pub agents: BTreeMap<AgentId, Agent>,
}

/// Lightweight session index entry. Meta is always available; full archive loaded on demand.
#[derive(Debug, Clone)]
pub struct ArchivedSession {
    pub meta: SessionMeta,
    pub path: std::path::PathBuf,
    /// None = not yet loaded from disk
    pub data: Option<SessionArchive>,
}

impl ArchivedSession {
    pub fn new(meta: SessionMeta, path: std::path::PathBuf) -> Self {
        Self {
            meta,
            path,
            data: None,
        }
    }

    pub fn with_data(mut self, archive: SessionArchive) -> Self {
        self.data = Some(archive);
        self
    }
}

impl SessionArchive {
    /// Current archive format version.
    pub const VERSION: u32 = 2;

    pub fn new(meta: SessionMeta) -> Self {
        Self {
            meta,
            version: Self::VERSION,
            task_graph: None,
            events: Vec::new(),
            agents: BTreeMap::new(),
        }
    }

    pub fn with_task_graph(mut self, task_graph: TaskGraph) -> Self {
        self.task_graph = Some(task_graph);
        self
    }

    pub fn with_events(mut self, events: Vec<TranscriptEvent>) -> Self {
        self.events = events;
        self
    }

    pub fn with_agents(mut self, agents: BTreeMap<AgentId, Agent>) -> Self {
        self.agents = agents;
        self
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::transcript_event::{TranscriptEvent, TranscriptEventKind};
    use chrono::TimeZone;

    fn ts() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 3, 18, 10, 0, 0).unwrap()
    }

    #[test]
    fn session_meta_builder_pattern() {
        let now = Utc::now();
        let meta = SessionMeta::new("s1", now, "/proj".to_string())
            .with_status(SessionStatus::Completed)
            .with_duration(Duration::from_secs(300))
            .with_git_branch("main".into());

        assert_eq!(meta.status, SessionStatus::Completed);
        assert_eq!(meta.duration, Some(Duration::from_secs(300)));
        assert_eq!(meta.git_branch, Some("main".into()));
    }

    #[test]
    fn session_status_serializes_lowercase() {
        let status = SessionStatus::Active;
        let json = serde_json::to_string(&status).unwrap();
        assert_eq!(json, "\"active\"");
    }

    #[test]
    fn session_archive_round_trip() {
        let meta = SessionMeta::new("s1", Utc::now(), "/proj".to_string());
        let archive = SessionArchive::new(meta);

        let json = serde_json::to_string(&archive).unwrap();
        let restored: SessionArchive = serde_json::from_str(&json).unwrap();

        assert_eq!(archive, restored);
    }

    /// FR-025: New archive format stores TranscriptEvents, version=2, round-trips correctly.
    #[test]
    fn session_archive_v2_round_trip_with_events() {
        let meta = SessionMeta::new("s1", ts(), "/proj".to_string());
        let events = vec![
            TranscriptEvent::new(ts(), TranscriptEventKind::UserMessage)
                .with_session("s1"),
            TranscriptEvent::new(
                ts(),
                TranscriptEventKind::AssistantMessage {
                    content: "Hello".to_string(),
                },
            )
            .with_session("s1"),
            TranscriptEvent::new(
                ts(),
                TranscriptEventKind::ToolUse {
                    tool_name: "Read".into(),
                    input_summary: "src/main.rs".to_string(),
                },
            )
            .with_session("s1"),
        ];

        let archive = SessionArchive::new(meta).with_events(events.clone());

        assert_eq!(archive.version, SessionArchive::VERSION);
        assert_eq!(archive.version, 2);
        assert_eq!(archive.events.len(), 3);

        let json = serde_json::to_string_pretty(&archive).unwrap();
        let restored: SessionArchive = serde_json::from_str(&json).unwrap();

        assert_eq!(archive, restored);
        assert_eq!(restored.version, 2);
        assert_eq!(restored.events.len(), 3);
        assert_eq!(restored.events[0].kind, TranscriptEventKind::UserMessage);
    }

    /// FR-025: version field is serialized to JSON.
    #[test]
    fn session_archive_v2_includes_version_in_json() {
        let meta = SessionMeta::new("s1", ts(), "/proj".to_string());
        let archive = SessionArchive::new(meta);

        let json = serde_json::to_string(&archive).unwrap();
        assert!(json.contains(r#""version":2"#), "json={json}");
    }

    /// FR-026, SC-008: Old-format archives produce empty events — no crash.
    #[test]
    fn old_format_archive_returns_empty_events() {
        // This JSON uses an old event format in events array.
        // The `event` tag value "session_start" doesn't match any TranscriptEventKind variant,
        // and the shape differs, so serde cannot deserialize it into TranscriptEvent.
        // With #[serde(default)] on events, the whole array is silently replaced with Vec::new().
        let old_format_json = r#"{
            "meta": {
                "id": "s-old-1",
                "timestamp": "2026-01-01T00:00:00Z",
                "status": "completed",
                "agent_count": 0,
                "task_count": 0,
                "event_count": 5,
                "project_path": "/old/project"
            },
            "events": [
                {
                    "timestamp": "2026-01-01T00:00:01Z",
                    "event": "session_start"
                },
                {
                    "timestamp": "2026-01-01T00:00:02Z",
                    "event": "pre_tool_use",
                    "tool_name": "Read",
                    "input_summary": "file.rs"
                }
            ]
        }"#;

        let result: Result<SessionArchive, _> = serde_json::from_str(old_format_json);
        // Must not crash — FR-026
        assert!(result.is_ok(), "old archive must not fail: {:?}", result.err());
        let archive = result.unwrap();
        // Old events silently dropped — SC-008
        assert!(
            archive.events.is_empty(),
            "old-format events must be empty, got: {:?}",
            archive.events
        );
        // meta is still accessible
        assert_eq!(archive.meta.id.as_str(), "s-old-1");
    }

    /// FR-026: Old-format archive without version field also produces empty events.
    #[test]
    fn old_format_without_version_field_returns_empty_events() {
        let old_format_json = r#"{
            "meta": {
                "id": "s-old-2",
                "timestamp": "2026-01-01T00:00:00Z",
                "status": "active",
                "agent_count": 1,
                "task_count": 2,
                "event_count": 10,
                "project_path": "/old"
            }
        }"#;

        let result: Result<SessionArchive, _> = serde_json::from_str(old_format_json);
        assert!(result.is_ok());
        let archive = result.unwrap();
        // version defaults to 0 (old format)
        assert_eq!(archive.version, 0);
        // events defaults to empty
        assert!(archive.events.is_empty());
    }

    /// Partial corruption: one bad element in a v2 events array is skipped; valid events survive.
    #[test]
    fn v2_archive_with_one_corrupted_event_preserves_valid_events() {
        // Two valid TranscriptEvents surrounding one corrupted entry (missing timestamp).
        let json = r#"{
            "meta": {
                "id": "s-partial",
                "timestamp": "2026-03-18T10:00:00Z",
                "status": "completed",
                "agent_count": 1,
                "task_count": 0,
                "event_count": 3,
                "project_path": "/proj"
            },
            "version": 2,
            "events": [
                {
                    "timestamp": "2026-03-18T10:00:01Z",
                    "event": "user_message"
                },
                {
                    "NOT_A_VALID_FIELD": true
                },
                {
                    "timestamp": "2026-03-18T10:00:03Z",
                    "event": "user_message"
                }
            ]
        }"#;

        let result: Result<SessionArchive, _> = serde_json::from_str(json);
        assert!(result.is_ok(), "partial-corruption archive must not fail: {:?}", result.err());
        let archive = result.unwrap();
        // Only the two valid events survive; the corrupted one is silently dropped.
        assert_eq!(
            archive.events.len(),
            2,
            "expected 2 valid events, got: {:?}",
            archive.events
        );
        assert_eq!(archive.events[0].kind, TranscriptEventKind::UserMessage);
        assert_eq!(archive.events[1].kind, TranscriptEventKind::UserMessage);
    }

    /// FR-027: No migration — old archives remain as-is (empty events, not transformed).
    #[test]
    fn no_migration_of_old_archives() {
        let old_format_json = r#"{
            "meta": {
                "id": "s-no-migrate",
                "timestamp": "2026-01-01T00:00:00Z",
                "status": "completed",
                "agent_count": 0,
                "task_count": 0,
                "event_count": 3,
                "project_path": "/proj"
            },
            "events": [
                {"timestamp": "2026-01-01T00:00:01Z", "event": "session_start"},
                {"timestamp": "2026-01-01T00:00:02Z", "event": "session_end"}
            ]
        }"#;

        let archive: SessionArchive = serde_json::from_str(old_format_json).unwrap();
        // Events are empty (skipped), not migrated to TranscriptEvent equivalents
        assert!(archive.events.is_empty());
        // version is 0 (old format marker)
        assert_eq!(archive.version, 0);
    }
}
