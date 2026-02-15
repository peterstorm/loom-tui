use std::path::PathBuf;

use chrono::{DateTime, Utc};
use crossterm::event::KeyEvent;

use crate::error::LoomError;
use crate::model::{AgentId, AgentMessage, HookEvent, SessionArchive, SessionId, SessionMeta, TaskGraph};

/// All events that can occur in the application.
/// Sourced from file watchers, keyboard input, timers, and internal operations.
#[derive(Debug, Clone)]
pub enum AppEvent {
    /// Task graph file updated with new graph state
    TaskGraphUpdated(TaskGraph),

    /// Agent transcript updated with new messages
    TranscriptUpdated {
        agent_id: AgentId,
        messages: Vec<AgentMessage>,
    },

    /// Hook event received from events.jsonl stream
    HookEventReceived(HookEvent),

    /// Agent started (detected from .active file)
    AgentStarted { agent_id: AgentId, timestamp: DateTime<Utc> },

    /// Agent stopped (detected from .active file removal)
    AgentStopped { agent_id: AgentId, timestamp: DateTime<Utc> },

    /// Keyboard input event
    Key(KeyEvent),

    /// Timer tick (for elapsed time updates, animations)
    Tick(DateTime<Utc>),

    /// Error occurred (non-fatal - parse, I/O, watcher, session)
    Error { source: String, error: LoomError },

    /// Session loaded from archive
    SessionLoaded(SessionArchive),

    /// Session list refreshed from disk (full archives — legacy/session-end path)
    SessionListRefreshed(Vec<SessionArchive>),

    /// Lightweight session metas loaded at startup
    SessionMetasLoaded(Vec<(PathBuf, SessionMeta)>),

    /// Request to load a full session archive by session ID
    LoadSessionRequested(SessionId),

    /// Request to install hook (side-effect handled in event loop)
    InstallHookRequested,

    /// Initial event file replay is complete — safe to run stale session cleanup
    ReplayComplete,
}
