use std::path::PathBuf;

use crossterm::event::KeyEvent;

use crate::model::{AgentMessage, HookEvent, SessionArchive, SessionMeta, TaskGraph};

/// All events that can occur in the application.
/// Sourced from file watchers, keyboard input, timers, and internal operations.
#[derive(Debug, Clone)]
pub enum AppEvent {
    /// Task graph file updated with new graph state
    TaskGraphUpdated(TaskGraph),

    /// Agent transcript updated with new messages
    TranscriptUpdated {
        agent_id: String,
        messages: Vec<AgentMessage>,
    },

    /// Hook event received from events.jsonl stream
    HookEventReceived(HookEvent),

    /// Agent started (detected from .active file)
    AgentStarted(String),

    /// Agent stopped (detected from .active file removal)
    AgentStopped(String),

    /// Keyboard input event
    Key(KeyEvent),

    /// Timer tick (for elapsed time updates, animations)
    Tick,

    /// Parse error occurred (non-fatal)
    ParseError { source: String, error: String },

    /// Session loaded from archive
    SessionLoaded(SessionArchive),

    /// Session list refreshed from disk (full archives — legacy/session-end path)
    SessionListRefreshed(Vec<SessionArchive>),

    /// Lightweight session metas loaded at startup
    SessionMetasLoaded(Vec<(PathBuf, SessionMeta)>),

    /// Request to load a full session archive by index
    LoadSessionRequested(usize),
}
