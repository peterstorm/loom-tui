use std::collections::{BTreeMap, VecDeque};
use std::time::Instant;

use crate::model::{Agent, ArchivedSession, HookEvent, SessionMeta, TaskGraph};

/// Main application state.
/// Updated via pure `update(state, event) -> state` function.
/// All fields are immutable from external perspective.
#[derive(Debug, Clone)]
pub struct AppState {
    /// Current view mode
    pub view: ViewState,

    /// Current task graph (None if not yet loaded)
    pub task_graph: Option<TaskGraph>,

    /// Active agents keyed by agent ID
    pub agents: BTreeMap<String, Agent>,

    /// Ring buffer of hook events (max 10,000 per NFR-005)
    pub events: VecDeque<HookEvent>,

    /// List of archived sessions (meta always available, full data loaded on demand)
    pub sessions: Vec<ArchivedSession>,

    /// Currently active sessions keyed by session ID (supports concurrent sessions)
    pub active_sessions: BTreeMap<String, SessionMeta>,

    /// Current panel focus
    pub focus: PanelFocus,

    /// Scroll offsets for each panel
    pub scroll_offsets: ScrollState,

    /// Auto-scroll mode for event stream
    pub auto_scroll: bool,

    /// Active filter string (None if no filter)
    pub filter: Option<String>,

    /// Show help overlay
    pub show_help: bool,

    /// Hook installation status
    pub hook_status: HookStatus,

    /// Error message ring buffer (for status bar display)
    pub errors: VecDeque<String>,

    /// Application start time (for elapsed time display)
    pub started_at: Instant,

    /// Project root path (for session metadata)
    pub project_path: String,

    /// Signal to quit the application
    pub should_quit: bool,

    /// Index of selected task in current view's task list
    pub selected_task_index: Option<usize>,

    /// Index of selected agent in agent detail view
    pub selected_agent_index: Option<usize>,

    /// Index of selected session in sessions view
    pub selected_session_index: Option<usize>,

    /// Index of session currently being loaded from disk (shows loading indicator)
    pub loading_session: Option<usize>,

    /// Cached sorted agent keys (recomputed when agent_keys_dirty)
    pub cached_sorted_keys: Vec<String>,

    /// Whether agent keys need re-sorting
    pub agent_keys_dirty: bool,

    /// Maps transcript session_id → agent_id for subagent transcript attribution.
    /// Subagent transcripts have their own session_id (different from the parent
    /// session_id stored on Agent). This map links them.
    pub transcript_agent_map: BTreeMap<String, String>,
}

/// View state variants
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ViewState {
    /// Main dashboard view
    Dashboard,

    /// Agent detail view with selectable agent list
    AgentDetail,

    /// Sessions archive view
    Sessions,

    /// Session detail view (inspecting a single session)
    SessionDetail,
}

/// Panel focus for two-panel layouts
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PanelFocus {
    Left,
    Right,
}

/// Hook installation status
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HookStatus {
    /// Status unknown (not yet checked)
    Unknown,

    /// Hooks properly installed
    Installed,

    /// Hooks missing or not installed
    Missing,

    /// Hook installation failed with error
    InstallFailed(String),
}

/// Scroll state for each scrollable panel
#[derive(Debug, Clone, Default)]
pub struct ScrollState {
    /// Scroll offset for task list
    pub task_list: usize,

    /// Scroll offset for event stream
    pub event_stream: usize,

    /// Scroll offset for agent list (agent detail left)
    pub agent_list: usize,

    /// Scroll offset for agent events (agent detail right)
    pub agent_events: usize,

    /// Scroll offset for sessions table
    pub sessions: usize,

    /// Scroll offset for session detail left panel (agent list)
    pub session_detail_left: usize,

    /// Scroll offset for session detail right panel (events)
    pub session_detail_right: usize,
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}

impl AppState {
    /// Create new default application state
    pub fn new() -> Self {
        Self {
            view: ViewState::Dashboard,
            task_graph: None,
            agents: BTreeMap::new(),
            events: VecDeque::with_capacity(10_000),
            sessions: Vec::new(),
            active_sessions: BTreeMap::new(),
            focus: PanelFocus::Left,
            scroll_offsets: ScrollState::default(),
            auto_scroll: true,
            filter: None,
            show_help: false,
            hook_status: HookStatus::Unknown,
            errors: VecDeque::with_capacity(100),
            started_at: Instant::now(),
            project_path: String::new(),
            should_quit: false,
            selected_task_index: None,
            selected_agent_index: None,
            selected_session_index: None,
            loading_session: None,
            cached_sorted_keys: Vec::new(),
            agent_keys_dirty: true,
            transcript_agent_map: BTreeMap::new(),
        }
    }

    /// Create new state with custom view
    pub fn with_view(view: ViewState) -> Self {
        Self {
            view,
            ..Self::new()
        }
    }

    /// Create new state with custom hook status
    pub fn with_hook_status(status: HookStatus) -> Self {
        Self {
            hook_status: status,
            ..Self::new()
        }
    }

    /// Set project path for session metadata
    pub fn with_project_path(mut self, path: String) -> Self {
        self.project_path = path;
        self
    }

    /// Agent keys sorted: active first (by started_at desc), then finished (by started_at desc).
    /// Returns cached result — call `recompute_sorted_keys()` after modifying agents.
    pub fn sorted_agent_keys(&self) -> &[String] {
        &self.cached_sorted_keys
    }

    /// Recompute cached sorted agent keys. Call after any agent mutation.
    pub fn recompute_sorted_keys(&mut self) {
        let mut keys: Vec<_> = self.agents.keys().cloned().collect();
        keys.sort_by(|a, b| {
            let aa = &self.agents[a];
            let bb = &self.agents[b];
            let a_active = aa.finished_at.is_none();
            let b_active = bb.finished_at.is_none();
            b_active
                .cmp(&a_active)
                .then(bb.started_at.cmp(&aa.started_at))
        });
        self.cached_sorted_keys = keys;
        self.agent_keys_dirty = false;
    }
}

impl ScrollState {
    /// Create new scroll state with all offsets at zero
    pub fn new() -> Self {
        Self::default()
    }

    /// Reset all scroll offsets to zero
    pub fn reset(&mut self) {
        self.task_list = 0;
        self.event_stream = 0;
        self.agent_list = 0;
        self.agent_events = 0;
        self.sessions = 0;
        self.session_detail_left = 0;
        self.session_detail_right = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_app_state_default() {
        let state = AppState::default();
        assert!(matches!(state.view, ViewState::Dashboard));
        assert!(state.task_graph.is_none());
        assert!(state.agents.is_empty());
        assert!(state.events.is_empty());
        assert!(state.sessions.is_empty());
        assert!(state.active_sessions.is_empty());
        assert!(matches!(state.focus, PanelFocus::Left));
        assert!(state.auto_scroll);
        assert!(state.filter.is_none());
        assert!(!state.show_help);
        assert!(matches!(state.hook_status, HookStatus::Unknown));
        assert!(state.errors.is_empty());
        assert!(!state.should_quit);
        assert!(state.selected_task_index.is_none());
    }

    #[test]
    fn test_app_state_new() {
        let state = AppState::new();
        assert!(matches!(state.view, ViewState::Dashboard));
        assert!(state.task_graph.is_none());
    }

    #[test]
    fn test_app_state_with_view() {
        let state = AppState::with_view(ViewState::Sessions);
        assert!(matches!(state.view, ViewState::Sessions));
        assert!(matches!(state.hook_status, HookStatus::Unknown));
    }

    #[test]
    fn test_app_state_with_view_agent_detail() {
        let state = AppState::with_view(ViewState::AgentDetail);
        assert!(matches!(state.view, ViewState::AgentDetail));
    }

    #[test]
    fn test_app_state_with_hook_status() {
        let state = AppState::with_hook_status(HookStatus::Installed);
        assert!(matches!(state.hook_status, HookStatus::Installed));
    }

    #[test]
    fn test_app_state_with_hook_status_failed() {
        let error_msg = "Permission denied".to_string();
        let state =
            AppState::with_hook_status(HookStatus::InstallFailed(error_msg.clone()));
        assert!(matches!(&state.hook_status, HookStatus::InstallFailed(msg) if msg == &error_msg));
    }

    #[test]
    fn test_scroll_state_default() {
        let scroll = ScrollState::default();
        assert_eq!(scroll.task_list, 0);
        assert_eq!(scroll.event_stream, 0);
        assert_eq!(scroll.agent_list, 0);
        assert_eq!(scroll.agent_events, 0);
        assert_eq!(scroll.sessions, 0);
    }

    #[test]
    fn test_scroll_state_reset() {
        let mut scroll = ScrollState::default();
        scroll.task_list = 10;
        scroll.event_stream = 20;
        scroll.agent_list = 5;
        scroll.agent_events = 15;
        scroll.sessions = 3;

        scroll.reset();

        assert_eq!(scroll.task_list, 0);
        assert_eq!(scroll.event_stream, 0);
        assert_eq!(scroll.agent_list, 0);
        assert_eq!(scroll.agent_events, 0);
        assert_eq!(scroll.sessions, 0);
    }

    #[test]
    fn test_view_state_equality() {
        assert_eq!(ViewState::Dashboard, ViewState::Dashboard);
        assert_eq!(ViewState::Sessions, ViewState::Sessions);
        assert_eq!(ViewState::AgentDetail, ViewState::AgentDetail);
        assert_ne!(ViewState::Dashboard, ViewState::Sessions);
    }

    #[test]
    fn test_panel_focus_equality() {
        assert_eq!(PanelFocus::Left, PanelFocus::Left);
        assert_eq!(PanelFocus::Right, PanelFocus::Right);
        assert_ne!(PanelFocus::Left, PanelFocus::Right);
    }

    #[test]
    fn test_hook_status_equality() {
        assert_eq!(HookStatus::Unknown, HookStatus::Unknown);
        assert_eq!(HookStatus::Installed, HookStatus::Installed);
        assert_eq!(HookStatus::Missing, HookStatus::Missing);
        assert_eq!(
            HookStatus::InstallFailed("error".to_string()),
            HookStatus::InstallFailed("error".to_string())
        );
        assert_ne!(HookStatus::Unknown, HookStatus::Installed);
    }

    #[test]
    fn test_app_state_clone() {
        let state = AppState::new();
        let cloned = state.clone();
        assert!(matches!(cloned.view, ViewState::Dashboard));
        assert!(matches!(cloned.hook_status, HookStatus::Unknown));
    }

    #[test]
    fn test_events_capacity() {
        let state = AppState::new();
        assert_eq!(state.events.capacity(), 10_000);
    }

    #[test]
    fn test_errors_capacity() {
        let state = AppState::new();
        assert_eq!(state.errors.capacity(), 100);
    }
}
