use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::time::Duration;

use super::ids::{AgentId, SessionId, TaskId, ToolName};
use super::serde_utils::duration_opt_millis;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct TokenUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_creation_input_tokens: u64,
    pub cache_read_input_tokens: u64,
}

impl TokenUsage {
    /// Input + output tokens (excludes cache — represents actual computation).
    pub fn api_tokens(&self) -> u64 {
        self.input_tokens + self.output_tokens
    }

    /// All tokens including cache operations.
    pub fn total(&self) -> u64 {
        self.input_tokens
            + self.output_tokens
            + self.cache_creation_input_tokens
            + self.cache_read_input_tokens
    }

    /// Context window size: input + cache tokens (matches Claude Code's reported total_tokens).
    /// Output tokens excluded because JSONL captures them at stream start (always ~1).
    pub fn context_window(&self) -> u64 {
        self.input_tokens + self.cache_creation_input_tokens + self.cache_read_input_tokens
    }

    pub fn add(&mut self, other: &TokenUsage) {
        self.input_tokens += other.input_tokens;
        self.output_tokens += other.output_tokens;
        self.cache_creation_input_tokens += other.cache_creation_input_tokens;
        self.cache_read_input_tokens += other.cache_read_input_tokens;
    }

    pub fn is_empty(&self) -> bool {
        self.total() == 0
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Agent {
    pub id: AgentId,
    #[serde(default)]
    pub task_id: Option<TaskId>,
    #[serde(default)]
    pub agent_type: Option<String>,
    /// Model the agent was spawned with (sonnet/opus/haiku, None = inherited)
    #[serde(default)]
    pub model: Option<String>,
    /// The prompt/task description the agent was spawned with
    #[serde(default)]
    pub task_description: Option<String>,
    pub started_at: DateTime<Utc>,
    #[serde(default)]
    pub finished_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub messages: Vec<AgentMessage>,
    #[serde(default)]
    pub session_id: Option<SessionId>,
    #[serde(default)]
    pub skills: Vec<String>,
    #[serde(default)]
    pub token_usage: TokenUsage,
}

impl Default for Agent {
    fn default() -> Self {
        Self {
            id: AgentId::new("unknown"),
            task_id: None,
            agent_type: None,
            model: None,
            task_description: None,
            started_at: Utc::now(),
            finished_at: None,
            messages: Vec::new(),
            session_id: None,
            skills: Vec::new(),
            token_usage: TokenUsage::default(),
        }
    }
}

impl Agent {
    pub fn new(id: impl Into<AgentId>, started_at: DateTime<Utc>) -> Self {
        Self {
            id: id.into(),
            task_id: None,
            agent_type: None,
            model: None,
            task_description: None,
            started_at,
            finished_at: None,
            messages: Vec::new(),
            session_id: None,
            skills: Vec::new(),
            token_usage: TokenUsage::default(),
        }
    }

    pub fn with_task(mut self, task_id: impl Into<TaskId>) -> Self {
        self.task_id = Some(task_id.into());
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

    pub fn with_model(mut self, model: String) -> Self {
        self.model = Some(model);
        self
    }

    pub fn finish(mut self, finished_at: DateTime<Utc>) -> Self {
        self.finished_at = Some(finished_at);
        self
    }

    /// Display name: agent_type if available, otherwise short ID
    pub fn display_name(&self) -> &str {
        self.agent_type.as_deref().unwrap_or(self.id.as_str())
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
    pub tool_name: ToolName,
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
    pub fn new(tool_name: impl Into<ToolName>, input_summary: String) -> Self {
        Self {
            tool_name: tool_name.into(),
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


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agent_builder_pattern() {
        let now = Utc::now();
        let later = now + chrono::Duration::seconds(10);

        let agent = Agent::new("a01", now)
            .with_task("T1")
            .finish(later);

        assert_eq!(agent.id.as_str(), "a01");
        assert_eq!(agent.task_id, Some(TaskId::new("T1")));
        assert_eq!(agent.finished_at, Some(later));
    }

    #[test]
    fn tool_call_serializes_duration_as_millis() {
        let call = ToolCall::new("Read", "file.rs".to_string())
            .with_duration(Duration::from_millis(250));

        let json = serde_json::to_string(&call).unwrap();
        assert!(json.contains("\"duration\":250"));
    }
}
