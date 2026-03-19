use chrono::{DateTime, Utc};
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;

use super::ids::{AgentId, SessionId, ToolName};

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct TranscriptEvent {
    pub timestamp: DateTime<Utc>,
    #[serde(flatten)]
    pub kind: TranscriptEventKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<SessionId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<AgentId>,
}

impl TranscriptEvent {
    pub fn new(timestamp: DateTime<Utc>, kind: TranscriptEventKind) -> Self {
        Self {
            timestamp,
            kind,
            session_id: None,
            agent_id: None,
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
}

/// Custom Deserialize for TranscriptEvent.
///
/// `#[serde(flatten)]` combined with `#[serde(tag = "event")]` (internally-tagged enum)
/// is a known serde limitation — the internally-tagged enum consumes the map during tag
/// lookup and can silently drop sibling fields like `session_id`/`agent_id`.
///
/// We fix this by deserializing into a raw `serde_json::Value::Object`, pulling out
/// the known scalar fields first, then deserializing `TranscriptEventKind` from the
/// remaining map (which still contains the `"event"` discriminant).
impl<'de> Deserialize<'de> for TranscriptEvent {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let mut map = match Value::deserialize(deserializer)? {
            Value::Object(m) => m,
            other => {
                return Err(serde::de::Error::custom(format!(
                    "expected object, got {other:?}"
                )))
            }
        };

        let timestamp: DateTime<Utc> = map
            .remove("timestamp")
            .ok_or_else(|| serde::de::Error::missing_field("timestamp"))
            .and_then(|v| {
                DateTime::parse_from_rfc3339(
                    v.as_str()
                        .ok_or_else(|| serde::de::Error::custom("timestamp must be a string"))?,
                )
                .map(|dt| dt.with_timezone(&Utc))
                .map_err(|e| serde::de::Error::custom(format!("invalid timestamp: {e}")))
            })?;

        let session_id: Option<SessionId> = map
            .remove("session_id")
            .map(|v| serde_json::from_value(v).map_err(serde::de::Error::custom))
            .transpose()?;

        let agent_id: Option<AgentId> = map
            .remove("agent_id")
            .map(|v| serde_json::from_value(v).map_err(serde::de::Error::custom))
            .transpose()?;

        // Remaining map contains "event" discriminant + variant fields — feed to
        // TranscriptEventKind's derived Deserialize (internally tagged).
        let kind: TranscriptEventKind =
            serde_json::from_value(Value::Object(map)).map_err(serde::de::Error::custom)?;

        Ok(TranscriptEvent {
            timestamp,
            kind,
            session_id,
            agent_id,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum TranscriptEventKind {
    UserMessage,
    AssistantMessage { content: String },
    ToolUse {
        tool_name: ToolName,
        input_summary: String,
    },
    ToolResult {
        tool_name: ToolName,
        result_summary: String,
        #[serde(default)]
        duration_ms: Option<u64>,
    },
    /// Catch-all for forward compatibility
    Unknown { entry_type: String },
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ts() -> DateTime<Utc> {
        "2026-03-18T10:00:00Z".parse().unwrap()
    }

    // --- round-trip tests ---

    #[test]
    fn user_message_round_trip() {
        let event = TranscriptEvent::new(ts(), TranscriptEventKind::UserMessage);
        let json = serde_json::to_string(&event).unwrap();
        let back: TranscriptEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(event, back);
    }

    #[test]
    fn assistant_message_round_trip() {
        let event = TranscriptEvent::new(
            ts(),
            TranscriptEventKind::AssistantMessage {
                content: "hello".to_string(),
            },
        );
        let json = serde_json::to_string(&event).unwrap();
        let back: TranscriptEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(event, back);
    }

    #[test]
    fn tool_use_round_trip() {
        let event = TranscriptEvent::new(
            ts(),
            TranscriptEventKind::ToolUse {
                tool_name: ToolName::new("Read"),
                input_summary: "src/main.rs".to_string(),
            },
        );
        let json = serde_json::to_string(&event).unwrap();
        let back: TranscriptEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(event, back);
    }

    #[test]
    fn tool_result_round_trip() {
        let event = TranscriptEvent::new(
            ts(),
            TranscriptEventKind::ToolResult {
                tool_name: ToolName::new("Bash"),
                result_summary: "ok".to_string(),
                duration_ms: Some(42),
            },
        );
        let json = serde_json::to_string(&event).unwrap();
        let back: TranscriptEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(event, back);
    }

    #[test]
    fn unknown_round_trip() {
        let event = TranscriptEvent::new(
            ts(),
            TranscriptEventKind::Unknown {
                entry_type: "future_type".to_string(),
            },
        );
        let json = serde_json::to_string(&event).unwrap();
        let back: TranscriptEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(event, back);
    }

    // --- serde shape tests ---

    #[test]
    fn user_message_has_event_tag() {
        let event = TranscriptEvent::new(ts(), TranscriptEventKind::UserMessage);
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains(r#""event":"user_message""#), "json={json}");
    }

    #[test]
    fn assistant_message_has_event_tag_and_content() {
        let event = TranscriptEvent::new(
            ts(),
            TranscriptEventKind::AssistantMessage {
                content: "world".to_string(),
            },
        );
        let v = serde_json::to_value(&event).unwrap();
        assert_eq!(v["event"], "assistant_message");
        assert_eq!(v["content"], "world");
    }

    #[test]
    fn tool_use_serializes_fields() {
        let event = TranscriptEvent::new(
            ts(),
            TranscriptEventKind::ToolUse {
                tool_name: ToolName::new("Write"),
                input_summary: "foo.rs".to_string(),
            },
        );
        let v = serde_json::to_value(&event).unwrap();
        assert_eq!(v["event"], "tool_use");
        assert_eq!(v["tool_name"], "Write");
        assert_eq!(v["input_summary"], "foo.rs");
    }

    #[test]
    fn tool_result_duration_ms_optional() {
        // omit duration_ms in JSON -- should default to None
        let json = r#"{
            "timestamp": "2026-03-18T10:00:00Z",
            "event": "tool_result",
            "tool_name": "Bash",
            "result_summary": "ok"
        }"#;
        let event: TranscriptEvent = serde_json::from_str(json).unwrap();
        match event.kind {
            TranscriptEventKind::ToolResult { duration_ms, .. } => {
                assert_eq!(duration_ms, None);
            }
            _ => panic!("wrong variant"),
        }
    }

    // --- unknown fields ignored (FR-007 / SC-007) ---

    #[test]
    fn unknown_fields_ignored_on_user_message() {
        let json = r#"{
            "timestamp": "2026-03-18T10:00:00Z",
            "event": "user_message",
            "future_field": "some_value",
            "another_unknown": 42
        }"#;
        let event: TranscriptEvent = serde_json::from_str(json).unwrap();
        assert_eq!(event.kind, TranscriptEventKind::UserMessage);
    }

    #[test]
    fn unknown_fields_ignored_on_assistant_message() {
        let json = r#"{
            "timestamp": "2026-03-18T10:00:00Z",
            "event": "assistant_message",
            "content": "hi",
            "extra": true
        }"#;
        let event: TranscriptEvent = serde_json::from_str(json).unwrap();
        match &event.kind {
            TranscriptEventKind::AssistantMessage { content } => {
                assert_eq!(content, "hi");
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn unknown_fields_ignored_on_tool_use() {
        let json = r#"{
            "timestamp": "2026-03-18T10:00:00Z",
            "event": "tool_use",
            "tool_name": "Glob",
            "input_summary": "**/*.rs",
            "unexpected_field": {"nested": true}
        }"#;
        let event: TranscriptEvent = serde_json::from_str(json).unwrap();
        match &event.kind {
            TranscriptEventKind::ToolUse { tool_name, .. } => {
                assert_eq!(tool_name.as_str(), "Glob");
            }
            _ => panic!("wrong variant"),
        }
    }

    // --- session/agent attribution (FR-008) ---

    #[test]
    fn with_session_sets_session_id() {
        let event = TranscriptEvent::new(ts(), TranscriptEventKind::UserMessage)
            .with_session("sess-1");
        assert_eq!(event.session_id, Some(SessionId::new("sess-1")));
    }

    #[test]
    fn with_agent_sets_agent_id() {
        let event = TranscriptEvent::new(ts(), TranscriptEventKind::UserMessage)
            .with_agent("agent-1");
        assert_eq!(event.agent_id, Some(AgentId::new("agent-1")));
    }

    #[test]
    fn session_and_agent_round_trip() {
        let event = TranscriptEvent::new(ts(), TranscriptEventKind::UserMessage)
            .with_session("sess-abc")
            .with_agent("agent-xyz");
        let json = serde_json::to_string(&event).unwrap();
        let back: TranscriptEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(event, back);
        assert_eq!(back.session_id, Some(SessionId::new("sess-abc")));
        assert_eq!(back.agent_id, Some(AgentId::new("agent-xyz")));
    }

    // --- session_id and agent_id deserialize correctly alongside event fields ---

    #[test]
    fn session_id_deserializes_from_flat_json() {
        let json = r#"{
            "timestamp": "2026-03-18T10:00:00Z",
            "event": "user_message",
            "session_id": "sess-42"
        }"#;
        let event: TranscriptEvent = serde_json::from_str(json).unwrap();
        assert_eq!(event.session_id, Some(SessionId::new("sess-42")));
        assert_eq!(event.kind, TranscriptEventKind::UserMessage);
    }

    #[test]
    fn agent_id_deserializes_from_flat_json() {
        let json = r#"{
            "timestamp": "2026-03-18T10:00:00Z",
            "event": "user_message",
            "agent_id": "agent-7"
        }"#;
        let event: TranscriptEvent = serde_json::from_str(json).unwrap();
        assert_eq!(event.agent_id, Some(AgentId::new("agent-7")));
    }

    #[test]
    fn session_and_agent_alongside_tool_use() {
        let json = r#"{
            "timestamp": "2026-03-18T10:00:00Z",
            "event": "tool_use",
            "tool_name": "Bash",
            "input_summary": "ls",
            "session_id": "s1",
            "agent_id": "a1"
        }"#;
        let event: TranscriptEvent = serde_json::from_str(json).unwrap();
        assert_eq!(event.session_id, Some(SessionId::new("s1")));
        assert_eq!(event.agent_id, Some(AgentId::new("a1")));
        match &event.kind {
            TranscriptEventKind::ToolUse { tool_name, input_summary } => {
                assert_eq!(tool_name.as_str(), "Bash");
                assert_eq!(input_summary, "ls");
            }
            _ => panic!("wrong variant"),
        }
    }

    // --- unknown entry type maps to Unknown variant ---

    #[test]
    fn unknown_event_type_deserializes_to_unknown_variant() {
        let json = r#"{
            "timestamp": "2026-03-18T10:00:00Z",
            "event": "unknown",
            "entry_type": "some_future_entry"
        }"#;
        let event: TranscriptEvent = serde_json::from_str(json).unwrap();
        match &event.kind {
            TranscriptEventKind::Unknown { entry_type } => {
                assert_eq!(entry_type, "some_future_entry");
            }
            _ => panic!("wrong variant"),
        }
    }
}
