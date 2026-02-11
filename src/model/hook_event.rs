use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HookEvent {
    pub timestamp: DateTime<Utc>,
    #[serde(flatten)]
    pub kind: HookEventKind,
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub agent_id: Option<String>,
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

    pub fn with_session(mut self, session_id: String) -> Self {
        self.session_id = Some(session_id);
        self
    }

    pub fn with_agent(mut self, agent_id: String) -> Self {
        self.agent_id = Some(agent_id);
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
        task_description: Option<String>,
    },
    SubagentStop,
    PreToolUse {
        tool_name: String,
        input_summary: String,
    },
    PostToolUse {
        tool_name: String,
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
}

impl HookEventKind {
    pub fn session_start() -> Self {
        Self::SessionStart
    }

    pub fn session_end() -> Self {
        Self::SessionEnd
    }

    pub fn subagent_start(task_description: Option<String>) -> Self {
        Self::SubagentStart { task_description }
    }

    pub fn subagent_stop() -> Self {
        Self::SubagentStop
    }

    pub fn pre_tool_use(tool_name: String, input_summary: String) -> Self {
        Self::PreToolUse {
            tool_name,
            input_summary,
        }
    }

    pub fn post_tool_use(
        tool_name: String,
        result_summary: String,
        duration_ms: Option<u64>,
    ) -> Self {
        Self::PostToolUse {
            tool_name,
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
        let kind = HookEventKind::pre_tool_use("Read".into(), "file.rs".into());
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
