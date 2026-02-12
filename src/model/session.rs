use super::{Agent, HookEvent, TaskGraph};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::time::Duration;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SessionMeta {
    pub id: String,
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
    pub failed_tasks: Vec<String>,
    #[serde(default)]
    pub transcript_path: Option<String>,
}

impl SessionMeta {
    pub fn new(id: String, timestamp: DateTime<Utc>, project_path: String) -> Self {
        Self {
            id,
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
    #[serde(default)]
    pub task_graph: Option<TaskGraph>,
    #[serde(default)]
    pub events: Vec<HookEvent>,
    #[serde(default)]
    pub agents: BTreeMap<String, Agent>,
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
    pub fn new(meta: SessionMeta) -> Self {
        Self {
            meta,
            task_graph: None,
            events: Vec::new(),
            agents: BTreeMap::new(),
        }
    }

    pub fn with_task_graph(mut self, task_graph: TaskGraph) -> Self {
        self.task_graph = Some(task_graph);
        self
    }

    pub fn with_events(mut self, events: Vec<HookEvent>) -> Self {
        self.events = events;
        self
    }

    pub fn with_agents(mut self, agents: BTreeMap<String, Agent>) -> Self {
        self.agents = agents;
        self
    }
}

// Custom serde for Duration as milliseconds
mod duration_opt_millis {
    use serde::{Deserialize, Deserializer, Serializer};
    use std::time::Duration;

    pub fn serialize<S>(duration: &Option<Duration>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match duration {
            Some(d) => serializer.serialize_some(&d.as_millis()),
            None => serializer.serialize_none(),
        }
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Option<Duration>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let millis: Option<u64> = Option::deserialize(deserializer)?;
        Ok(millis.map(Duration::from_millis))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_meta_builder_pattern() {
        let now = Utc::now();
        let meta = SessionMeta::new("s1".into(), now, "/proj".into())
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
        let meta = SessionMeta::new("s1".into(), Utc::now(), "/proj".into());
        let archive = SessionArchive::new(meta);

        let json = serde_json::to_string(&archive).unwrap();
        let restored: SessionArchive = serde_json::from_str(&json).unwrap();

        assert_eq!(archive, restored);
    }
}
