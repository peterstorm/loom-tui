use std::collections::{BTreeMap, VecDeque};
use std::time::Instant;

use crate::model::{Agent, HookEvent, SessionMeta, TaskGraph};

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

    /// List of archived sessions
    pub sessions: Vec<SessionMeta>,

    /// Currently loaded session (if viewing archived session)
    pub active_session: Option<SessionMeta>,

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

    /// Signal to quit the application
    pub should_quit: bool,

    /// Index of selected task in current view's task list
    pub selected_task_index: Option<usize>,

    /// Index of selected agent in agent detail view
    pub selected_agent_index: Option<usize>,
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
            active_session: None,
            focus: PanelFocus::Left,
            scroll_offsets: ScrollState::default(),
            auto_scroll: true,
            filter: None,
            show_help: false,
            hook_status: HookStatus::Unknown,
            errors: VecDeque::with_capacity(100),
            started_at: Instant::now(),
            should_quit: false,
            selected_task_index: None,
            selected_agent_index: None,
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
        assert!(state.active_session.is_none());
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
