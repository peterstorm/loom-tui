use std::collections::{BTreeMap, VecDeque};
use std::time::Instant;

use crate::model::{Agent, AgentId, ArchivedSession, HookEvent, SessionId, SessionMeta, TaskGraph};

/// UI state: view mode, focus, scrolling, selections, display flags
#[derive(Debug, Clone)]
pub struct UiState {
    /// Current view mode
    pub view: ViewState,

    /// Task view mode (wave vs kanban) for Dashboard
    pub task_view_mode: TaskViewMode,

    /// Current panel focus
    pub focus: PanelFocus,

    /// Show help overlay
    pub show_help: bool,

    /// Show agent popup overlay (agent ID if active)
    pub show_agent_popup: Option<AgentId>,

    /// Active filter string (None if no filter)
    pub filter: Option<String>,

    /// Auto-scroll mode for event stream
    pub auto_scroll: bool,

    /// Scroll offsets for each panel
    pub scroll_offsets: ScrollState,

    /// Index of selected task in current view's task list
    pub selected_task_index: Option<usize>,

    /// Index of selected agent in agent detail view
    pub selected_agent_index: Option<usize>,

    /// Index of selected session in sessions view
    pub selected_session_index: Option<usize>,

    /// Index of session currently being loaded from disk (shows loading indicator)
    pub loading_session: Option<usize>,
}

/// Domain state: agents, events, sessions, task graph
#[derive(Debug, Clone)]
pub struct DomainState {
    /// Active agents keyed by agent ID
    pub agents: BTreeMap<AgentId, Agent>,

    /// Ring buffer of hook events (max 10,000)
    pub events: VecDeque<HookEvent>,

    /// List of archived sessions (meta always available, full data loaded on demand)
    pub sessions: Vec<ArchivedSession>,

    /// Currently active sessions keyed by session ID (supports concurrent sessions)
    pub active_sessions: BTreeMap<SessionId, SessionMeta>,

    /// Current task graph (None if not yet loaded)
    pub task_graph: Option<TaskGraph>,

    /// Maps session_id → agent_ids for subagent event attribution.
    /// Multiple agents can share the same parent session_id when spawned in bulk.
    pub transcript_agent_map: BTreeMap<SessionId, Vec<AgentId>>,
}

/// Application metadata: lifecycle, errors, configuration
#[derive(Debug, Clone)]
pub struct AppMeta {
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

    /// Initial event replay complete — stale cleanup deferred until true
    pub replay_complete: bool,
}

/// Cache state (private): sorted keys, dirty flags, agent tool counts
#[derive(Debug, Clone)]
struct CacheState {
    /// Cached sorted agent keys (recomputed when dirty)
    sorted_keys: Vec<AgentId>,

    /// Whether agent keys need re-sorting
    dirty: bool,

    /// Cached tool counts per agent (incremented on PostToolUse events)
    agent_tool_counts: BTreeMap<AgentId, usize>,
}

/// Main application state.
/// Updated via `update(&mut state, event)` function.
/// Decomposed into sub-states for better organization.
#[derive(Debug, Clone)]
pub struct AppState {
    pub ui: UiState,
    pub domain: DomainState,
    pub meta: AppMeta,
    cache: CacheState,
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

/// Task view mode for Dashboard
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskViewMode {
    /// Wave-based grouping (default)
    Wave,

    /// Kanban board (status-based columns)
    Kanban,
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

impl DomainState {
    /// Iterator over active sessions confirmed by a UserPromptSubmit event.
    /// Filters out phantom subagent sessions from display and navigation.
    pub fn confirmed_active_sessions(&self) -> impl Iterator<Item = (&SessionId, &SessionMeta)> {
        self.active_sessions.iter().filter(|(_, m)| m.confirmed)
    }

    /// Count of confirmed active sessions.
    pub fn confirmed_active_count(&self) -> usize {
        self.active_sessions.values().filter(|m| m.confirmed).count()
    }
}

impl Default for UiState {
    fn default() -> Self {
        Self {
            view: ViewState::Dashboard,
            task_view_mode: TaskViewMode::Wave,
            focus: PanelFocus::Left,
            show_help: false,
            show_agent_popup: None,
            filter: None,
            auto_scroll: true,
            scroll_offsets: ScrollState::default(),
            selected_task_index: None,
            selected_agent_index: None,
            selected_session_index: None,
            loading_session: None,
        }
    }
}

impl Default for DomainState {
    fn default() -> Self {
        Self {
            agents: BTreeMap::new(),
            events: VecDeque::with_capacity(10_000),
            sessions: Vec::new(),
            active_sessions: BTreeMap::new(),
            task_graph: None,
            transcript_agent_map: BTreeMap::new(),
        }
    }
}

impl Default for AppMeta {
    fn default() -> Self {
        Self {
            hook_status: HookStatus::Unknown,
            errors: VecDeque::with_capacity(100),
            started_at: Instant::now(),
            project_path: String::new(),
            should_quit: false,
            replay_complete: false,
        }
    }
}

impl Default for CacheState {
    fn default() -> Self {
        Self {
            sorted_keys: Vec::new(),
            dirty: true,
            agent_tool_counts: BTreeMap::new(),
        }
    }
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
            ui: UiState::default(),
            domain: DomainState::default(),
            meta: AppMeta::default(),
            cache: CacheState::default(),
        }
    }

    /// Create new state with custom view
    pub fn with_view(view: ViewState) -> Self {
        Self {
            ui: UiState {
                view,
                ..UiState::default()
            },
            ..Self::new()
        }
    }

    /// Create new state with custom hook status
    pub fn with_hook_status(status: HookStatus) -> Self {
        Self {
            meta: AppMeta {
                hook_status: status,
                ..AppMeta::default()
            },
            ..Self::new()
        }
    }

    /// Set project path for session metadata
    pub fn with_project_path(mut self, path: String) -> Self {
        self.meta.project_path = path;
        self
    }

    /// Agent keys sorted: active first (by started_at desc), then finished (by started_at desc).
    /// Returns cached result — call `recompute_sorted_keys()` after modifying agents.
    pub fn sorted_agent_keys(&self) -> &[AgentId] {
        &self.cache.sorted_keys
    }

    /// Recompute cached sorted agent keys. Call after any agent mutation.
    pub fn recompute_sorted_keys(&mut self) {
        let mut keys: Vec<_> = self.domain.agents.keys().cloned().collect();
        keys.sort_by(|a, b| {
            let aa = &self.domain.agents[a];
            let bb = &self.domain.agents[b];
            let a_active = aa.finished_at.is_none();
            let b_active = bb.finished_at.is_none();
            b_active
                .cmp(&a_active)
                .then(bb.started_at.cmp(&aa.started_at))
        });
        self.cache.sorted_keys = keys;
        self.cache.dirty = false;
    }

    /// Check if cache is dirty
    pub fn is_cache_dirty(&self) -> bool {
        self.cache.dirty
    }

    /// Mark cache as dirty
    pub fn mark_cache_dirty(&mut self) {
        self.cache.dirty = true;
    }

    /// Get cached tool count for an agent.
    /// Returns 0 if agent has no tool events.
    pub fn agent_tool_count(&self, id: &AgentId) -> usize {
        self.cache.agent_tool_counts.get(id).copied().unwrap_or(0)
    }

    /// Increment tool count for an agent (called on PostToolUse events).
    pub(crate) fn increment_tool_count(&mut self, id: &AgentId) {
        *self.cache.agent_tool_counts.entry(id.clone()).or_insert(0) += 1;
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
        assert!(matches!(state.ui.view, ViewState::Dashboard));
        assert!(state.domain.task_graph.is_none());
        assert!(state.domain.agents.is_empty());
        assert!(state.domain.events.is_empty());
        assert!(state.domain.sessions.is_empty());
        assert!(state.domain.active_sessions.is_empty());
        assert!(matches!(state.ui.focus, PanelFocus::Left));
        assert!(state.ui.auto_scroll);
        assert!(state.ui.filter.is_none());
        assert!(!state.ui.show_help);
        assert!(matches!(state.meta.hook_status, HookStatus::Unknown));
        assert!(state.meta.errors.is_empty());
        assert!(!state.meta.should_quit);
        assert!(state.ui.selected_task_index.is_none());
    }

    #[test]
    fn test_app_state_new() {
        let state = AppState::new();
        assert!(matches!(state.ui.view, ViewState::Dashboard));
        assert!(state.domain.task_graph.is_none());
    }

    #[test]
    fn test_app_state_with_view() {
        let state = AppState::with_view(ViewState::Sessions);
        assert!(matches!(state.ui.view, ViewState::Sessions));
        assert!(matches!(state.meta.hook_status, HookStatus::Unknown));
    }

    #[test]
    fn test_app_state_with_view_agent_detail() {
        let state = AppState::with_view(ViewState::AgentDetail);
        assert!(matches!(state.ui.view, ViewState::AgentDetail));
    }

    #[test]
    fn test_app_state_with_hook_status() {
        let state = AppState::with_hook_status(HookStatus::Installed);
        assert!(matches!(state.meta.hook_status, HookStatus::Installed));
    }

    #[test]
    fn test_app_state_with_hook_status_failed() {
        let error_msg = "Permission denied".to_string();
        let state =
            AppState::with_hook_status(HookStatus::InstallFailed(error_msg.clone()));
        assert!(matches!(&state.meta.hook_status, HookStatus::InstallFailed(msg) if msg == &error_msg));
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
        assert!(matches!(cloned.ui.view, ViewState::Dashboard));
        assert!(matches!(cloned.meta.hook_status, HookStatus::Unknown));
    }

    #[test]
    fn test_events_capacity() {
        let state = AppState::new();
        assert_eq!(state.domain.events.capacity(), 10_000);
    }

    #[test]
    fn test_errors_capacity() {
        let state = AppState::new();
        assert_eq!(state.meta.errors.capacity(), 100);
    }

    #[test]
    fn test_recompute_sorted_keys_empty_state() {
        let mut state = AppState::new();
        state.recompute_sorted_keys();
        assert!(state.sorted_agent_keys().is_empty());
        assert!(!state.is_cache_dirty());
    }

    #[test]
    fn test_recompute_sorted_keys_active_first_ordering() {
        use chrono::Utc;

        let mut state = AppState::new();

        // Create agents with different statuses and timestamps
        let now = Utc::now();

        // Active agent (newest)
        let mut a1 = Agent::new("a01", now);
        a1.finished_at = None;
        state.domain.agents.insert("a01".into(), a1);

        // Finished agent
        let mut a2 = Agent::new("a02", now - chrono::Duration::seconds(10));
        a2.finished_at = Some(now);
        state.domain.agents.insert("a02".into(), a2);

        // Active agent (older)
        let mut a3 = Agent::new("a03", now - chrono::Duration::seconds(20));
        a3.finished_at = None;
        state.domain.agents.insert("a03".into(), a3);

        state.recompute_sorted_keys();

        let keys = state.sorted_agent_keys();
        assert_eq!(keys.len(), 3);

        // Active agents first (newest to oldest), then finished
        assert_eq!(keys[0].as_str(), "a01"); // active, newest
        assert_eq!(keys[1].as_str(), "a03"); // active, older
        assert_eq!(keys[2].as_str(), "a02"); // finished
    }

    #[test]
    fn test_recompute_sorted_keys_all_finished() {
        use chrono::Utc;

        let mut state = AppState::new();
        let now = Utc::now();

        // All finished agents, sorted by started_at desc
        let mut a1 = Agent::new("a01", now);
        a1.finished_at = Some(now + chrono::Duration::seconds(10));
        state.domain.agents.insert("a01".into(), a1);

        let mut a2 = Agent::new("a02", now - chrono::Duration::seconds(10));
        a2.finished_at = Some(now);
        state.domain.agents.insert("a02".into(), a2);

        state.recompute_sorted_keys();

        let keys = state.sorted_agent_keys();
        assert_eq!(keys.len(), 2);
        assert_eq!(keys[0].as_str(), "a01"); // newer started_at
        assert_eq!(keys[1].as_str(), "a02");
    }

    #[test]
    fn test_recompute_sorted_keys_all_active() {
        use chrono::Utc;

        let mut state = AppState::new();
        let now = Utc::now();

        // All active agents, sorted by started_at desc
        let a1 = Agent::new("a01", now);
        state.domain.agents.insert("a01".into(), a1);

        let a2 = Agent::new("a02", now - chrono::Duration::seconds(10));
        state.domain.agents.insert("a02".into(), a2);

        let a3 = Agent::new("a03", now - chrono::Duration::seconds(20));
        state.domain.agents.insert("a03".into(), a3);

        state.recompute_sorted_keys();

        let keys = state.sorted_agent_keys();
        assert_eq!(keys.len(), 3);
        assert_eq!(keys[0].as_str(), "a01"); // newest
        assert_eq!(keys[1].as_str(), "a02");
        assert_eq!(keys[2].as_str(), "a03"); // oldest
    }

    #[test]
    fn test_recompute_sorted_keys_mixed_statuses() {
        use chrono::Utc;

        let mut state = AppState::new();
        let now = Utc::now();

        // Mix of active and finished
        let mut a1 = Agent::new("a01", now - chrono::Duration::seconds(30));
        a1.finished_at = Some(now);
        state.domain.agents.insert("a01".into(), a1);

        let a2 = Agent::new("a02", now - chrono::Duration::seconds(10));
        state.domain.agents.insert("a02".into(), a2);

        let mut a3 = Agent::new("a03", now - chrono::Duration::seconds(40));
        a3.finished_at = Some(now - chrono::Duration::seconds(5));
        state.domain.agents.insert("a03".into(), a3);

        let a4 = Agent::new("a04", now - chrono::Duration::seconds(20));
        state.domain.agents.insert("a04".into(), a4);

        state.recompute_sorted_keys();

        let keys = state.sorted_agent_keys();
        assert_eq!(keys.len(), 4);

        // All active first (a02, a04 by started_at desc), then finished (a01, a03 by started_at desc)
        assert_eq!(keys[0].as_str(), "a02"); // active, newest
        assert_eq!(keys[1].as_str(), "a04"); // active, older
        assert_eq!(keys[2].as_str(), "a01"); // finished, newer started_at
        assert_eq!(keys[3].as_str(), "a03"); // finished, oldest started_at
    }

    #[test]
    fn test_recompute_sorted_keys_marks_cache_clean() {
        let mut state = AppState::new();
        state.mark_cache_dirty();
        assert!(state.is_cache_dirty());

        state.recompute_sorted_keys();
        assert!(!state.is_cache_dirty());
    }
}
