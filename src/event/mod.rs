use std::path::PathBuf;

use chrono::{DateTime, Utc};
use crossterm::event::KeyEvent;

use crate::error::LoomError;
use crate::model::{AgentId, SessionArchive, SessionId, SessionMeta, TaskGraph};
use crate::model::TranscriptEvent;
use crate::watcher::TranscriptMetadata;

/// All events that can occur in the application.
/// Sourced from file watchers, keyboard input, timers, and internal operations.
#[derive(Debug, Clone)]
pub enum AppEvent {
    /// Task graph file updated with new graph state
    TaskGraphUpdated(TaskGraph),

    /// Transcript event received from JSONL stream
    TranscriptEventReceived(TranscriptEvent),

    /// New session discovered on disk (transcript path found)
    SessionDiscovered { session_id: SessionId, transcript_path: PathBuf },

    /// Session completed (no more activity expected)
    SessionCompleted { session_id: SessionId },

    /// Session reactivated (new activity after completion)
    SessionReactivated { session_id: SessionId },

    /// Agent metadata extracted from subagent transcript (model, tokens, skills)
    AgentMetadataUpdated {
        agent_id: AgentId,
        metadata: TranscriptMetadata,
    },

    /// Agent transcript finished (result entry seen or idle timeout)
    AgentFinished { agent_id: AgentId },

    /// Keyboard input event
    Key(KeyEvent),

    /// Timer tick (for elapsed time updates, animations)
    Tick(DateTime<Utc>),

    /// Error occurred (non-fatal - parse, I/O, watcher, session)
    Error { source: String, error: LoomError },

    /// Session loaded from archive
    SessionLoaded(SessionArchive),

    /// Lightweight session metas loaded at startup
    SessionMetasLoaded(Vec<(PathBuf, SessionMeta)>),

    /// Request to load a full session archive by session ID
    LoadSessionRequested(SessionId),

    /// Initial event file replay is complete — safe to run stale session cleanup
    ReplayComplete,
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use crate::model::TranscriptEventKind;

    fn session_id() -> SessionId {
        SessionId::new("sess-test")
    }

    fn agent_id() -> AgentId {
        AgentId::new("agent-test")
    }

    #[test]
    fn transcript_event_received_constructs() {
        let ts = Utc::now();
        let event = crate::model::TranscriptEvent::new(ts, TranscriptEventKind::UserMessage)
            .with_session("sess-1");
        let app_event = AppEvent::TranscriptEventReceived(event.clone());
        match app_event {
            AppEvent::TranscriptEventReceived(e) => {
                assert_eq!(e.session_id, Some(SessionId::new("sess-1")));
                assert_eq!(e.kind, TranscriptEventKind::UserMessage);
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn session_discovered_constructs() {
        let path = PathBuf::from("/tmp/session/transcript.jsonl");
        let app_event = AppEvent::SessionDiscovered {
            session_id: session_id(),
            transcript_path: path.clone(),
        };
        match app_event {
            AppEvent::SessionDiscovered { session_id, transcript_path } => {
                assert_eq!(session_id, SessionId::new("sess-test"));
                assert_eq!(transcript_path, path);
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn session_completed_constructs() {
        let app_event = AppEvent::SessionCompleted { session_id: session_id() };
        match app_event {
            AppEvent::SessionCompleted { session_id } => {
                assert_eq!(session_id, SessionId::new("sess-test"));
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn session_reactivated_constructs() {
        let app_event = AppEvent::SessionReactivated { session_id: session_id() };
        match app_event {
            AppEvent::SessionReactivated { session_id } => {
                assert_eq!(session_id, SessionId::new("sess-test"));
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn transcript_event_received_with_agent_id() {
        let ts = Utc::now();
        let event = crate::model::TranscriptEvent::new(
            ts,
            TranscriptEventKind::AssistantMessage { content: "hello".to_string() },
        )
        .with_agent("agent-42");
        let app_event = AppEvent::TranscriptEventReceived(event);
        match app_event {
            AppEvent::TranscriptEventReceived(e) => {
                assert_eq!(e.agent_id, Some(AgentId::new("agent-42")));
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn agent_metadata_updated_constructs() {
        use crate::watcher::TranscriptMetadata;
        let meta = TranscriptMetadata {
            model: Some("claude-3".to_string()),
            ..Default::default()
        };
        let app_event = AppEvent::AgentMetadataUpdated {
            agent_id: agent_id(),
            metadata: meta,
        };
        match app_event {
            AppEvent::AgentMetadataUpdated { agent_id, metadata } => {
                assert_eq!(agent_id, AgentId::new("agent-test"));
                assert_eq!(metadata.model, Some("claude-3".to_string()));
            }
            _ => panic!("wrong variant"),
        }
    }
}
