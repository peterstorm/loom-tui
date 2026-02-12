use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Agent {
    pub id: String,
    #[serde(default)]
    pub task_id: Option<String>,
    #[serde(default)]
    pub agent_type: Option<String>,
    /// The prompt/task description the agent was spawned with
    #[serde(default)]
    pub task_description: Option<String>,
    pub started_at: DateTime<Utc>,
    #[serde(default)]
    pub finished_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub messages: Vec<AgentMessage>,
    #[serde(default)]
    pub session_id: Option<String>,
}

impl Default for Agent {
    fn default() -> Self {
        Self {
            id: String::new(),
            task_id: None,
            agent_type: None,
            task_description: None,
            started_at: Utc::now(),
            finished_at: None,
            messages: Vec::new(),
            session_id: None,
        }
    }
}

impl Agent {
    pub fn new(id: String, started_at: DateTime<Utc>) -> Self {
        Self {
            id,
            task_id: None,
            agent_type: None,
            task_description: None,
            started_at,
            finished_at: None,
            messages: Vec::new(),
            session_id: None,
        }
    }

    pub fn with_task(mut self, task_id: String) -> Self {
        self.task_id = Some(task_id);
        self
    }

    pub fn add_message(mut self, message: AgentMessage) -> Self {
        self.messages.push(message);
        self
    }

    pub fn with_agent_type(mut self, agent_type: String) -> Self {
        self.agent_type = Some(agent_type);
        self
    }

    pub fn finish(mut self, finished_at: DateTime<Utc>) -> Self {
        self.finished_at = Some(finished_at);
        self
    }

    /// Display name: agent_type if available, otherwise short ID
    pub fn display_name(&self) -> &str {
        self.agent_type.as_deref().unwrap_or(&self.id)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgentMessage {
    pub timestamp: DateTime<Utc>,
    #[serde(flatten)]
    pub kind: MessageKind,
}

impl AgentMessage {
    pub fn reasoning(timestamp: DateTime<Utc>, content: String) -> Self {
        Self {
            timestamp,
            kind: MessageKind::Reasoning { content },
        }
    }

    pub fn tool(timestamp: DateTime<Utc>, call: ToolCall) -> Self {
        Self {
            timestamp,
            kind: MessageKind::Tool(call),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum MessageKind {
    Reasoning { content: String },
    Tool(ToolCall),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolCall {
    pub tool_name: String,
    pub input_summary: String,
    #[serde(default)]
    pub result_summary: Option<String>,
    #[serde(
        default,
        with = "duration_opt_millis",
        skip_serializing_if = "Option::is_none"
    )]
    pub duration: Option<Duration>,
    #[serde(default)]
    pub success: Option<bool>,
}

impl ToolCall {
    pub fn new(tool_name: String, input_summary: String) -> Self {
        Self {
            tool_name,
            input_summary,
            result_summary: None,
            duration: None,
            success: None,
        }
    }

    pub fn with_result(mut self, result_summary: String, success: bool) -> Self {
        self.result_summary = Some(result_summary);
        self.success = Some(success);
        self
    }

    pub fn with_duration(mut self, duration: Duration) -> Self {
        self.duration = Some(duration);
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
    fn agent_builder_pattern() {
        let now = Utc::now();
        let later = now + chrono::Duration::seconds(10);

        let agent = Agent::new("a01".into(), now)
            .with_task("T1".into())
            .finish(later);

        assert_eq!(agent.id, "a01");
        assert_eq!(agent.task_id, Some("T1".into()));
        assert_eq!(agent.finished_at, Some(later));
    }

    #[test]
    fn tool_call_serializes_duration_as_millis() {
        let call = ToolCall::new("Read".into(), "file.rs".into())
            .with_duration(Duration::from_millis(250));

        let json = serde_json::to_string(&call).unwrap();
        assert!(json.contains("\"duration\":250"));
    }
}
