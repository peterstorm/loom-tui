use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use loom_tui::app::{handle_key, AppState, PanelFocus, ViewState};
use loom_tui::model::{Agent, AgentId, ArchivedSession, SessionArchive, SessionMeta, Task, TaskId, TaskGraph, TaskStatus, Wave};
use std::path::PathBuf;
use chrono::Utc;

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::empty())
}

#[test]
fn quit_signal_set_by_q_key() {
    let mut state = AppState::new();
    assert!(!state.meta.should_quit);
    handle_key(&mut state, key(KeyCode::Char('q')));
    assert!(state.meta.should_quit);
}

#[test]
fn number_key_1_switches_to_dashboard() {
    let mut state = AppState::new();
    state.ui.view = ViewState::Sessions;
    handle_key(&mut state, key(KeyCode::Char('1')));
    assert!(matches!(state.ui.view, ViewState::Dashboard));
}

#[test]
fn number_key_1_from_agent_detail_to_dashboard() {
    let mut state = AppState::new();
    state.ui.view = ViewState::AgentDetail;
    handle_key(&mut state, key(KeyCode::Char('1')));
    assert!(matches!(state.ui.view, ViewState::Dashboard));
}

#[test]
fn number_key_3_switches_to_sessions() {
    let mut state = AppState::new();
    handle_key(&mut state, key(KeyCode::Char('3')));
    assert!(matches!(state.ui.view, ViewState::Sessions));
}

#[test]
fn number_key_3_from_agent_detail_to_sessions() {
    let mut state = AppState::new();
    state.ui.view = ViewState::AgentDetail;
    handle_key(&mut state, key(KeyCode::Char('3')));
    assert!(matches!(state.ui.view, ViewState::Sessions));
}

#[test]
fn number_key_2_switches_to_agent_detail() {
    let mut state = AppState::new();
    handle_key(&mut state, key(KeyCode::Char('2')));
    assert!(matches!(state.ui.view, ViewState::AgentDetail));
}

#[test]
fn number_key_2_auto_selects_first_agent() {
    let mut state = AppState::new();
    state.domain.agents.insert(AgentId::new("a01"), Agent::new("a01", Utc::now()));
    handle_key(&mut state, key(KeyCode::Char('2')));
    assert!(matches!(state.ui.view, ViewState::AgentDetail));
    assert_eq!(state.ui.selected_agent_index, Some(0));
}

#[test]
fn number_key_2_no_agents_no_selection() {
    let mut state = AppState::new();
    handle_key(&mut state, key(KeyCode::Char('2')));
    assert_eq!(state.ui.selected_agent_index, None);
}

#[test]
fn tab_switches_focus_from_left_to_right() {
    let mut state = AppState::new();
    state.ui.focus = PanelFocus::Left;
    handle_key(&mut state, key(KeyCode::Tab));
    assert!(matches!(state.ui.focus, PanelFocus::Right));
}

#[test]
fn tab_toggles_focus_back_to_left() {
    let mut state = AppState::new();
    state.ui.focus = PanelFocus::Right;
    handle_key(&mut state, key(KeyCode::Tab));
    assert!(matches!(state.ui.focus, PanelFocus::Left));
}

#[test]
fn l_key_switches_focus_to_right() {
    let mut state = AppState::new();
    state.ui.focus = PanelFocus::Left;
    handle_key(&mut state, key(KeyCode::Char('l')));
    assert!(matches!(state.ui.focus, PanelFocus::Right));
}

#[test]
fn h_key_switches_focus_to_left() {
    let mut state = AppState::new();
    state.ui.focus = PanelFocus::Right;
    handle_key(&mut state, key(KeyCode::Char('h')));
    assert!(matches!(state.ui.focus, PanelFocus::Left));
}

#[test]
fn h_key_sets_focus_to_left_even_if_already_left() {
    let mut state = AppState::new();
    state.ui.focus = PanelFocus::Left;
    handle_key(&mut state, key(KeyCode::Char('h')));
    assert!(matches!(state.ui.focus, PanelFocus::Left));
}

#[test]
fn j_key_scrolls_down_left_panel_in_dashboard() {
    let mut state = AppState::new();
    state.ui.view = ViewState::Dashboard;
    state.ui.focus = PanelFocus::Left;
    state.ui.scroll_offsets.task_list = 5;
    handle_key(&mut state, key(KeyCode::Char('j')));
    assert_eq!(state.ui.scroll_offsets.task_list, 6);
}

#[test]
fn down_arrow_scrolls_down_left_panel_in_dashboard() {
    let mut state = AppState::new();
    state.ui.view = ViewState::Dashboard;
    state.ui.focus = PanelFocus::Left;
    state.ui.scroll_offsets.task_list = 5;
    handle_key(&mut state, key(KeyCode::Down));
    assert_eq!(state.ui.scroll_offsets.task_list, 6);
}

#[test]
fn k_key_scrolls_up_left_panel_in_dashboard() {
    let mut state = AppState::new();
    state.ui.view = ViewState::Dashboard;
    state.ui.focus = PanelFocus::Left;
    state.ui.scroll_offsets.task_list = 5;
    handle_key(&mut state, key(KeyCode::Char('k')));
    assert_eq!(state.ui.scroll_offsets.task_list, 4);
}

#[test]
fn up_arrow_scrolls_up_left_panel_in_dashboard() {
    let mut state = AppState::new();
    state.ui.view = ViewState::Dashboard;
    state.ui.focus = PanelFocus::Left;
    state.ui.scroll_offsets.task_list = 5;
    handle_key(&mut state, key(KeyCode::Up));
    assert_eq!(state.ui.scroll_offsets.task_list, 4);
}

#[test]
fn scroll_up_saturates_at_zero() {
    let mut state = AppState::new();
    state.ui.view = ViewState::Dashboard;
    state.ui.focus = PanelFocus::Left;
    state.ui.scroll_offsets.task_list = 0;
    handle_key(&mut state, key(KeyCode::Char('k')));
    assert_eq!(state.ui.scroll_offsets.task_list, 0);
}

#[test]
fn j_key_scrolls_right_panel_when_focused_in_dashboard() {
    let mut state = AppState::new();
    state.ui.view = ViewState::Dashboard;
    state.ui.focus = PanelFocus::Right;
    state.ui.scroll_offsets.event_stream = 10;
    handle_key(&mut state, key(KeyCode::Char('j')));
    assert_eq!(state.ui.scroll_offsets.event_stream, 11);
}

#[test]
fn k_key_scrolls_right_panel_when_focused_in_dashboard() {
    let mut state = AppState::new();
    state.ui.view = ViewState::Dashboard;
    state.ui.focus = PanelFocus::Right;
    state.ui.scroll_offsets.event_stream = 10;
    handle_key(&mut state, key(KeyCode::Char('k')));
    assert_eq!(state.ui.scroll_offsets.event_stream, 9);
}

#[test]
fn scroll_event_stream_disables_auto_scroll() {
    let mut state = AppState::new();
    state.ui.view = ViewState::Dashboard;
    state.ui.focus = PanelFocus::Right;
    state.ui.auto_scroll = true;
    handle_key(&mut state, key(KeyCode::Char('j')));
    assert!(!state.ui.auto_scroll);
}

#[test]
fn scroll_task_list_does_not_affect_auto_scroll() {
    let mut state = AppState::new();
    state.ui.view = ViewState::Dashboard;
    state.ui.focus = PanelFocus::Left;
    state.ui.auto_scroll = true;
    handle_key(&mut state, key(KeyCode::Char('j')));
    assert!(state.ui.auto_scroll);
}

#[test]
fn j_key_moves_agent_selection_down() {
    let mut state = AppState::new();
    state.ui.view = ViewState::AgentDetail;
    state.ui.focus = PanelFocus::Left;
    state.domain.agents.insert(AgentId::new("a01"), Agent::new("a01", Utc::now()));
    state.domain.agents.insert(AgentId::new("a02"), Agent::new("a02", Utc::now()));
    state.ui.selected_agent_index = Some(0);
    handle_key(&mut state, key(KeyCode::Char('j')));
    assert_eq!(state.ui.selected_agent_index, Some(1));
}

#[test]
fn k_key_moves_agent_selection_up() {
    let mut state = AppState::new();
    state.ui.view = ViewState::AgentDetail;
    state.ui.focus = PanelFocus::Left;
    state.domain.agents.insert(AgentId::new("a01"), Agent::new("a01", Utc::now()));
    state.domain.agents.insert(AgentId::new("a02"), Agent::new("a02", Utc::now()));
    state.ui.selected_agent_index = Some(1);
    handle_key(&mut state, key(KeyCode::Char('k')));
    assert_eq!(state.ui.selected_agent_index, Some(0));
}

#[test]
fn j_key_scrolls_agent_events_in_right_panel() {
    let mut state = AppState::new();
    state.ui.view = ViewState::AgentDetail;
    state.ui.focus = PanelFocus::Right;
    state.ui.scroll_offsets.agent_events = 8;
    handle_key(&mut state, key(KeyCode::Char('j')));
    assert_eq!(state.ui.scroll_offsets.agent_events, 9);
}

#[test]
fn k_key_scrolls_agent_events_in_right_panel() {
    let mut state = AppState::new();
    state.ui.view = ViewState::AgentDetail;
    state.ui.focus = PanelFocus::Right;
    state.ui.scroll_offsets.agent_events = 8;
    handle_key(&mut state, key(KeyCode::Char('k')));
    assert_eq!(state.ui.scroll_offsets.agent_events, 7);
}

#[test]
fn j_key_scrolls_sessions_table() {
    let mut state = AppState::new();
    state.ui.view = ViewState::Sessions;
    state.domain.sessions = vec![
        ArchivedSession::new(SessionMeta::new("s1", Utc::now(), "/proj".to_string()), PathBuf::new()),
        ArchivedSession::new(SessionMeta::new("s2", Utc::now(), "/proj".to_string()), PathBuf::new()),
    ];
    state.ui.selected_session_index = Some(0);
    handle_key(&mut state, key(KeyCode::Char('j')));
    assert_eq!(state.ui.selected_session_index, Some(1));
}

#[test]
fn k_key_scrolls_sessions_table() {
    let mut state = AppState::new();
    state.ui.view = ViewState::Sessions;
    state.domain.sessions = vec![
        ArchivedSession::new(SessionMeta::new("s1", Utc::now(), "/proj".to_string()), PathBuf::new()),
        ArchivedSession::new(SessionMeta::new("s2", Utc::now(), "/proj".to_string()), PathBuf::new()),
    ];
    state.ui.selected_session_index = Some(1);
    handle_key(&mut state, key(KeyCode::Char('k')));
    assert_eq!(state.ui.selected_session_index, Some(0));
}

#[test]
fn enter_on_dashboard_drills_into_agent_detail() {
    let mut state = AppState::new();
    state.ui.view = ViewState::Dashboard;

    let task = Task {
        id: TaskId::new("T1"),
        description: "Implement feature".to_string(),
        agent_id: Some(AgentId::new("a04")),
        status: TaskStatus::Running,
        review_status: Default::default(),
        files_modified: vec![],
        tests_passed: None,
    };
    let wave = Wave::new(1, vec![task]);
    state.domain.task_graph = Some(TaskGraph::new(vec![wave]));
    state.ui.selected_task_index = Some(0);
    state.domain.agents.insert(AgentId::new("a04"), Agent::new("a04", Utc::now()));
    state.recompute_sorted_keys();

    handle_key(&mut state, key(KeyCode::Enter));
    assert!(matches!(state.ui.view, ViewState::AgentDetail));
    assert_eq!(state.ui.selected_agent_index, Some(0));
}

#[test]
fn enter_on_dashboard_noop_if_no_agent() {
    let mut state = AppState::new();
    state.ui.view = ViewState::Dashboard;

    let task = Task::new("T1".to_string(), "Test".to_string(), TaskStatus::Pending);
    let wave = Wave::new(1, vec![task]);
    state.domain.task_graph = Some(TaskGraph::new(vec![wave]));
    state.ui.selected_task_index = Some(0);

    handle_key(&mut state, key(KeyCode::Enter));
    assert!(matches!(state.ui.view, ViewState::Dashboard));
}

#[test]
fn enter_on_dashboard_noop_if_no_selection() {
    let mut state = AppState::new();
    state.ui.view = ViewState::Dashboard;
    state.ui.selected_task_index = None;

    handle_key(&mut state, key(KeyCode::Enter));
    assert!(matches!(state.ui.view, ViewState::Dashboard));
}

#[test]
fn enter_on_agent_detail_is_noop() {
    let mut state = AppState::new();
    state.ui.view = ViewState::AgentDetail;
    handle_key(&mut state, key(KeyCode::Enter));
    assert!(matches!(state.ui.view, ViewState::AgentDetail));
}

#[test]
fn enter_on_sessions_no_selection_is_noop() {
    let mut state = AppState::new();
    state.ui.view = ViewState::Sessions;
    handle_key(&mut state, key(KeyCode::Enter));
    assert!(matches!(state.ui.view, ViewState::Sessions));
}

#[test]
fn enter_on_sessions_with_loaded_data_opens_detail() {
    let mut state = AppState::new();
    state.ui.view = ViewState::Sessions;
    let meta = SessionMeta::new("s1", Utc::now(), "/proj".to_string());
    state.domain.sessions = vec![
        ArchivedSession::new(meta.clone(), PathBuf::new())
            .with_data(SessionArchive::new(meta)),
    ];
    state.ui.selected_session_index = Some(0);
    handle_key(&mut state, key(KeyCode::Enter));
    assert!(matches!(state.ui.view, ViewState::SessionDetail));
}

#[test]
fn enter_on_sessions_unloaded_sets_loading() {
    let mut state = AppState::new();
    state.ui.view = ViewState::Sessions;
    state.domain.sessions = vec![
        ArchivedSession::new(SessionMeta::new("s1", Utc::now(), "/proj".to_string()), PathBuf::new()),
    ];
    state.ui.selected_session_index = Some(0);
    handle_key(&mut state, key(KeyCode::Enter));
    assert!(matches!(state.ui.view, ViewState::Sessions));
    assert_eq!(state.ui.loading_session, Some(0));
}

#[test]
fn esc_from_session_detail_returns_to_sessions() {
    let mut state = AppState::new();
    state.ui.view = ViewState::SessionDetail;
    handle_key(&mut state, key(KeyCode::Esc));
    assert!(matches!(state.ui.view, ViewState::Sessions));
}

#[test]
fn esc_from_agent_detail_returns_to_dashboard() {
    let mut state = AppState::new();
    state.ui.view = ViewState::AgentDetail;
    handle_key(&mut state, key(KeyCode::Esc));
    assert!(matches!(state.ui.view, ViewState::Dashboard));
}

#[test]
fn esc_from_sessions_returns_to_dashboard() {
    let mut state = AppState::new();
    state.ui.view = ViewState::Sessions;
    handle_key(&mut state, key(KeyCode::Esc));
    assert!(matches!(state.ui.view, ViewState::Dashboard));
}

#[test]
fn esc_from_dashboard_is_noop() {
    let mut state = AppState::new();
    handle_key(&mut state, key(KeyCode::Esc));
    assert!(matches!(state.ui.view, ViewState::Dashboard));
}

#[test]
fn slash_starts_filter_mode() {
    let mut state = AppState::new();
    assert!(state.ui.filter.is_none());
    handle_key(&mut state, key(KeyCode::Char('/')));
    assert!(state.ui.filter.is_some());
    assert_eq!(state.ui.filter.as_deref(), Some(""));
}

#[test]
fn question_mark_toggles_help_on() {
    let mut state = AppState::new();
    assert!(!state.ui.show_help);
    handle_key(&mut state, key(KeyCode::Char('?')));
    assert!(state.ui.show_help);
}

#[test]
fn question_mark_toggles_help_off() {
    let mut state = AppState::new();
    state.ui.show_help = true;
    handle_key(&mut state, key(KeyCode::Char('?')));
    assert!(!state.ui.show_help);
}

#[test]
fn space_toggles_auto_scroll_off() {
    let mut state = AppState::new();
    state.ui.auto_scroll = true;
    handle_key(&mut state, key(KeyCode::Char(' ')));
    assert!(!state.ui.auto_scroll);
}

#[test]
fn space_toggles_auto_scroll_on() {
    let mut state = AppState::new();
    state.ui.auto_scroll = false;
    handle_key(&mut state, key(KeyCode::Char(' ')));
    assert!(state.ui.auto_scroll);
}

#[test]
fn help_overlay_dismisses_on_any_key() {
    let mut state = AppState::new();
    state.ui.show_help = true;
    handle_key(&mut state, key(KeyCode::Char('x')));
    assert!(!state.ui.show_help);
}

#[test]
fn help_overlay_dismisses_on_escape() {
    let mut state = AppState::new();
    state.ui.show_help = true;
    handle_key(&mut state, key(KeyCode::Esc));
    assert!(!state.ui.show_help);
}

#[test]
fn filter_mode_escape_dismisses_filter() {
    let mut state = AppState::new();
    state.ui.filter = Some("test".to_string());
    handle_key(&mut state, key(KeyCode::Esc));
    assert!(state.ui.filter.is_none());
}

#[test]
fn filter_mode_char_appends_to_filter() {
    let mut state = AppState::new();
    state.ui.filter = Some("te".to_string());
    handle_key(&mut state, key(KeyCode::Char('s')));
    assert_eq!(state.ui.filter.as_deref(), Some("tes"));
}

#[test]
fn filter_mode_backspace_removes_char() {
    let mut state = AppState::new();
    state.ui.filter = Some("test".to_string());
    handle_key(&mut state, key(KeyCode::Backspace));
    assert_eq!(state.ui.filter.as_deref(), Some("tes"));
}

#[test]
fn filter_mode_backspace_on_empty_filter() {
    let mut state = AppState::new();
    state.ui.filter = Some("".to_string());
    handle_key(&mut state, key(KeyCode::Backspace));
    assert_eq!(state.ui.filter.as_deref(), Some(""));
}

#[test]
fn filter_mode_enter_keeps_filter() {
    let mut state = AppState::new();
    state.ui.filter = Some("test".to_string());
    handle_key(&mut state, key(KeyCode::Enter));
    assert_eq!(state.ui.filter.as_deref(), Some("test"));
}

#[test]
fn unknown_key_in_normal_mode_is_noop() {
    let mut state = AppState::new();
    handle_key(&mut state, key(KeyCode::F(1)));
    assert!(matches!(state.ui.view, ViewState::Dashboard));
    assert!(!state.meta.should_quit);
    assert_eq!(0, state.ui.scroll_offsets.task_list);
}

#[test]
fn multiple_waves_drill_down_correct_task() {
    let now = Utc::now();
    let mut state = AppState::new();
    // Deterministic timestamps: a01 oldest, a02 middle, a03 newest
    // sorted_agent_keys order (active, started_at desc): [a03, a02, a01]
    state.domain.agents.insert(
        "a01".into(),
        Agent::new("a01", now - chrono::Duration::seconds(20)),
    );
    state.domain.agents.insert(
        "a02".into(),
        Agent::new("a02", now - chrono::Duration::seconds(10)),
    );
    state.domain.agents.insert(AgentId::new("a03"), Agent::new("a03", now));
    state.recompute_sorted_keys();

    let task1 = Task {
        id: TaskId::new("T1"),
        description: "Task 1".to_string(),
        agent_id: Some(AgentId::new("a01")),
        status: TaskStatus::Running,
        review_status: Default::default(),
        files_modified: vec![],
        tests_passed: None,
    };
    let task2 = Task {
        id: TaskId::new("T2"),
        description: "Task 2".to_string(),
        agent_id: Some(AgentId::new("a02")),
        status: TaskStatus::Running,
        review_status: Default::default(),
        files_modified: vec![],
        tests_passed: None,
    };
    let task3 = Task {
        id: TaskId::new("T3"),
        description: "Task 3".to_string(),
        agent_id: Some(AgentId::new("a03")),
        status: TaskStatus::Pending,
        review_status: Default::default(),
        files_modified: vec![],
        tests_passed: None,
    };

    let wave1 = Wave::new(1, vec![task1, task2]);
    let wave2 = Wave::new(2, vec![task3]);
    state.domain.task_graph = Some(TaskGraph::new(vec![wave1, wave2]));
    state.ui.selected_task_index = Some(2); // Third task → agent a03

    handle_key(&mut state, key(KeyCode::Enter));
    assert!(matches!(state.ui.view, ViewState::AgentDetail));
    // sorted_agent_keys: [a03, a02, a01] → a03 is at index 0
    assert_eq!(state.ui.selected_agent_index, Some(0));
}

#[test]
fn out_of_bounds_task_index_is_noop() {
    let mut state = AppState::new();

    let task = Task::new("T1".to_string(), "Task 1".to_string(), TaskStatus::Pending);
    let wave = Wave::new(1, vec![task]);
    state.domain.task_graph = Some(TaskGraph::new(vec![wave]));
    state.ui.selected_task_index = Some(999); // Out of bounds

    handle_key(&mut state, key(KeyCode::Enter));
    assert!(matches!(state.ui.view, ViewState::Dashboard));
}

#[test]
fn navigation_preserves_unrelated_state() {
    let mut state = AppState::new();
    state.ui.scroll_offsets.sessions = 5;
    state.meta.errors.push_back("Test error".to_string());
    handle_key(&mut state, key(KeyCode::Char('3')));
    assert_eq!(state.ui.scroll_offsets.sessions, 5);
    assert_eq!(state.meta.errors.len(), 1);
    assert!(matches!(state.ui.view, ViewState::Sessions));
}

#[test]
fn help_overlay_prevents_navigation() {
    let mut state = AppState::new();
    state.ui.show_help = true;
    handle_key(&mut state, key(KeyCode::Char('3')));
    assert!(!state.ui.show_help);
    assert!(matches!(state.ui.view, ViewState::Dashboard));
}

#[test]
fn filter_mode_prevents_navigation() {
    let mut state = AppState::new();
    state.ui.filter = Some("test".to_string());
    handle_key(&mut state, key(KeyCode::Char('3')));
    assert_eq!(state.ui.filter.as_deref(), Some("test3"));
    assert!(matches!(state.ui.view, ViewState::Dashboard));
}
