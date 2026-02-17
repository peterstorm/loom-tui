use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::ids::{AgentId, SessionId, ToolName};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HookEvent {
    pub timestamp: DateTime<Utc>,
    #[serde(flatten)]
    pub kind: HookEventKind,
    #[serde(default)]
    pub session_id: Option<SessionId>,
    #[serde(default)]
    pub agent_id: Option<AgentId>,
    #[serde(default)]
    pub raw: Value,
}

impl HookEvent {
    pub fn new(timestamp: DateTime<Utc>, kind: HookEventKind) -> Self {
        Self {
            timestamp,
            kind,
            session_id: None,
            agent_id: None,
            raw: Value::Null,
        }
    }

    pub fn with_session(mut self, session_id: impl Into<SessionId>) -> Self {
        self.session_id = Some(session_id.into());
        self
    }

    pub fn with_agent(mut self, agent_id: impl Into<AgentId>) -> Self {
        self.agent_id = Some(agent_id.into());
        self
    }

    pub fn with_raw(mut self, raw: Value) -> Self {
        self.raw = raw;
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum HookEventKind {
    SessionStart,
    SessionEnd,
    SubagentStart {
        #[serde(default)]
        agent_type: Option<String>,
        #[serde(default)]
        task_description: Option<String>,
    },
    SubagentStop,
    PreToolUse {
        tool_name: ToolName,
        input_summary: String,
        /// Full prompt text for Task tool calls (used to correlate with SubagentStart)
        #[serde(default)]
        task_prompt: Option<String>,
        /// Model specified for Task tool calls (sonnet/opus/haiku)
        #[serde(default)]
        task_model: Option<String>,
    },
    PostToolUse {
        tool_name: ToolName,
        result_summary: String,
        #[serde(default)]
        duration_ms: Option<u64>,
    },
    Stop {
        #[serde(default)]
        reason: Option<String>,
    },
    Notification {
        message: String,
    },
    UserPromptSubmit,
    AssistantText {
        content: String,
    },
}

impl HookEventKind {
    pub fn session_start() -> Self {
        Self::SessionStart
    }

    pub fn session_end() -> Self {
        Self::SessionEnd
    }

    pub fn subagent_start(task_description: Option<String>) -> Self {
        Self::SubagentStart {
            agent_type: None,
            task_description,
        }
    }

    pub fn subagent_start_full(
        agent_type: Option<String>,
        task_description: Option<String>,
    ) -> Self {
        Self::SubagentStart {
            agent_type,
            task_description,
        }
    }

    pub fn subagent_stop() -> Self {
        Self::SubagentStop
    }

    pub fn pre_tool_use(tool_name: impl Into<ToolName>, input_summary: String) -> Self {
        Self::PreToolUse {
            tool_name: tool_name.into(),
            input_summary,
            task_prompt: None,
            task_model: None,
        }
    }

    pub fn post_tool_use(
        tool_name: impl Into<ToolName>,
        result_summary: String,
        duration_ms: Option<u64>,
    ) -> Self {
        Self::PostToolUse {
            tool_name: tool_name.into(),
            result_summary,
            duration_ms,
        }
    }

    pub fn stop(reason: Option<String>) -> Self {
        Self::Stop { reason }
    }

    pub fn notification(message: String) -> Self {
        Self::Notification { message }
    }

    pub fn user_prompt_submit() -> Self {
        Self::UserPromptSubmit
    }

    pub fn assistant_text(content: String) -> Self {
        Self::AssistantText { content }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hook_event_serializes_with_event_tag() {
        let event = HookEvent::new(Utc::now(), HookEventKind::session_start());
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("\"event\":\"session_start\""));
    }

    #[test]
    fn hook_event_kind_with_data() {
        let kind = HookEventKind::pre_tool_use("Read", "file.rs".to_string());
        let json = serde_json::to_value(&kind).unwrap();
        assert_eq!(json["event"], "pre_tool_use");
        assert_eq!(json["tool_name"], "Read");
    }

    #[test]
    fn hook_event_deserializes() {
        let json = r#"{
            "timestamp": "2026-02-11T10:00:00Z",
            "event": "notification",
            "message": "Test message"
        }"#;

        let event: HookEvent = serde_json::from_str(json).unwrap();
        match event.kind {
            HookEventKind::Notification { message } => {
                assert_eq!(message, "Test message");
            }
            _ => panic!("Wrong event kind"),
        }
    }
}
