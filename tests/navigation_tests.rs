use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use loom_tui::app::{handle_key, AppState, PanelFocus, ViewState};
use loom_tui::model::{Agent, Task, TaskGraph, TaskStatus, Wave};
use chrono::Utc;

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::empty())
}

#[test]
fn quit_signal_set_by_q_key() {
    let state = AppState::new();
    assert!(!state.should_quit);

    let new_state = handle_key(state, key(KeyCode::Char('q')));
    assert!(new_state.should_quit);
}

#[test]
fn number_key_1_switches_to_dashboard() {
    let mut state = AppState::new();
    state.view = ViewState::Sessions;

    let new_state = handle_key(state, key(KeyCode::Char('1')));
    assert!(matches!(new_state.view, ViewState::Dashboard));
}

#[test]
fn number_key_1_from_agent_detail_to_dashboard() {
    let mut state = AppState::new();
    state.view = ViewState::AgentDetail;

    let new_state = handle_key(state, key(KeyCode::Char('1')));
    assert!(matches!(new_state.view, ViewState::Dashboard));
}

#[test]
fn number_key_3_switches_to_sessions() {
    let state = AppState::new();
    let new_state = handle_key(state, key(KeyCode::Char('3')));
    assert!(matches!(new_state.view, ViewState::Sessions));
}

#[test]
fn number_key_3_from_agent_detail_to_sessions() {
    let mut state = AppState::new();
    state.view = ViewState::AgentDetail;

    let new_state = handle_key(state, key(KeyCode::Char('3')));
    assert!(matches!(new_state.view, ViewState::Sessions));
}

#[test]
fn number_key_2_switches_to_agent_detail() {
    let state = AppState::new();
    let new_state = handle_key(state, key(KeyCode::Char('2')));
    assert!(matches!(new_state.view, ViewState::AgentDetail));
}

#[test]
fn number_key_2_auto_selects_first_agent() {
    let mut state = AppState::new();
    state.agents.insert("a01".into(), Agent::new("a01".into(), Utc::now()));

    let new_state = handle_key(state, key(KeyCode::Char('2')));
    assert!(matches!(new_state.view, ViewState::AgentDetail));
    assert_eq!(new_state.selected_agent_index, Some(0));
}

#[test]
fn number_key_2_no_agents_no_selection() {
    let state = AppState::new();
    let new_state = handle_key(state, key(KeyCode::Char('2')));
    assert_eq!(new_state.selected_agent_index, None);
}

#[test]
fn tab_switches_focus_from_left_to_right() {
    let mut state = AppState::new();
    state.focus = PanelFocus::Left;

    let new_state = handle_key(state, key(KeyCode::Tab));
    assert!(matches!(new_state.focus, PanelFocus::Right));
}

#[test]
fn tab_sets_focus_to_right_even_if_already_right() {
    let mut state = AppState::new();
    state.focus = PanelFocus::Right;

    let new_state = handle_key(state, key(KeyCode::Tab));
    assert!(matches!(new_state.focus, PanelFocus::Right));
}

#[test]
fn l_key_switches_focus_to_right() {
    let mut state = AppState::new();
    state.focus = PanelFocus::Left;

    let new_state = handle_key(state, key(KeyCode::Char('l')));
    assert!(matches!(new_state.focus, PanelFocus::Right));
}

#[test]
fn h_key_switches_focus_to_left() {
    let mut state = AppState::new();
    state.focus = PanelFocus::Right;

    let new_state = handle_key(state, key(KeyCode::Char('h')));
    assert!(matches!(new_state.focus, PanelFocus::Left));
}

#[test]
fn h_key_sets_focus_to_left_even_if_already_left() {
    let mut state = AppState::new();
    state.focus = PanelFocus::Left;

    let new_state = handle_key(state, key(KeyCode::Char('h')));
    assert!(matches!(new_state.focus, PanelFocus::Left));
}

#[test]
fn j_key_scrolls_down_left_panel_in_dashboard() {
    let mut state = AppState::new();
    state.view = ViewState::Dashboard;
    state.focus = PanelFocus::Left;
    state.scroll_offsets.task_list = 5;

    let new_state = handle_key(state, key(KeyCode::Char('j')));
    assert_eq!(new_state.scroll_offsets.task_list, 6);
}

#[test]
fn down_arrow_scrolls_down_left_panel_in_dashboard() {
    let mut state = AppState::new();
    state.view = ViewState::Dashboard;
    state.focus = PanelFocus::Left;
    state.scroll_offsets.task_list = 5;

    let new_state = handle_key(state, key(KeyCode::Down));
    assert_eq!(new_state.scroll_offsets.task_list, 6);
}

#[test]
fn k_key_scrolls_up_left_panel_in_dashboard() {
    let mut state = AppState::new();
    state.view = ViewState::Dashboard;
    state.focus = PanelFocus::Left;
    state.scroll_offsets.task_list = 5;

    let new_state = handle_key(state, key(KeyCode::Char('k')));
    assert_eq!(new_state.scroll_offsets.task_list, 4);
}

#[test]
fn up_arrow_scrolls_up_left_panel_in_dashboard() {
    let mut state = AppState::new();
    state.view = ViewState::Dashboard;
    state.focus = PanelFocus::Left;
    state.scroll_offsets.task_list = 5;

    let new_state = handle_key(state, key(KeyCode::Up));
    assert_eq!(new_state.scroll_offsets.task_list, 4);
}

#[test]
fn scroll_up_saturates_at_zero() {
    let mut state = AppState::new();
    state.view = ViewState::Dashboard;
    state.focus = PanelFocus::Left;
    state.scroll_offsets.task_list = 0;

    let new_state = handle_key(state, key(KeyCode::Char('k')));
    assert_eq!(new_state.scroll_offsets.task_list, 0);
}

#[test]
fn j_key_scrolls_right_panel_when_focused_in_dashboard() {
    let mut state = AppState::new();
    state.view = ViewState::Dashboard;
    state.focus = PanelFocus::Right;
    state.scroll_offsets.event_stream = 10;

    let new_state = handle_key(state, key(KeyCode::Char('j')));
    assert_eq!(new_state.scroll_offsets.event_stream, 11);
}

#[test]
fn k_key_scrolls_right_panel_when_focused_in_dashboard() {
    let mut state = AppState::new();
    state.view = ViewState::Dashboard;
    state.focus = PanelFocus::Right;
    state.scroll_offsets.event_stream = 10;

    let new_state = handle_key(state, key(KeyCode::Char('k')));
    assert_eq!(new_state.scroll_offsets.event_stream, 9);
}

#[test]
fn scroll_event_stream_disables_auto_scroll() {
    let mut state = AppState::new();
    state.view = ViewState::Dashboard;
    state.focus = PanelFocus::Right;
    state.auto_scroll = true;

    let new_state = handle_key(state, key(KeyCode::Char('j')));
    assert!(!new_state.auto_scroll);
}

#[test]
fn scroll_task_list_does_not_affect_auto_scroll() {
    let mut state = AppState::new();
    state.view = ViewState::Dashboard;
    state.focus = PanelFocus::Left;
    state.auto_scroll = true;

    let new_state = handle_key(state, key(KeyCode::Char('j')));
    assert!(new_state.auto_scroll);
}

#[test]
fn j_key_moves_agent_selection_down() {
    let mut state = AppState::new();
    state.view = ViewState::AgentDetail;
    state.focus = PanelFocus::Left;
    state.agents.insert("a01".into(), Agent::new("a01".into(), Utc::now()));
    state.agents.insert("a02".into(), Agent::new("a02".into(), Utc::now()));
    state.selected_agent_index = Some(0);

    let new_state = handle_key(state, key(KeyCode::Char('j')));
    assert_eq!(new_state.selected_agent_index, Some(1));
}

#[test]
fn k_key_moves_agent_selection_up() {
    let mut state = AppState::new();
    state.view = ViewState::AgentDetail;
    state.focus = PanelFocus::Left;
    state.agents.insert("a01".into(), Agent::new("a01".into(), Utc::now()));
    state.agents.insert("a02".into(), Agent::new("a02".into(), Utc::now()));
    state.selected_agent_index = Some(1);

    let new_state = handle_key(state, key(KeyCode::Char('k')));
    assert_eq!(new_state.selected_agent_index, Some(0));
}

#[test]
fn j_key_scrolls_agent_events_in_right_panel() {
    let mut state = AppState::new();
    state.view = ViewState::AgentDetail;
    state.focus = PanelFocus::Right;
    state.scroll_offsets.agent_events = 8;

    let new_state = handle_key(state, key(KeyCode::Char('j')));
    assert_eq!(new_state.scroll_offsets.agent_events, 9);
}

#[test]
fn k_key_scrolls_agent_events_in_right_panel() {
    let mut state = AppState::new();
    state.view = ViewState::AgentDetail;
    state.focus = PanelFocus::Right;
    state.scroll_offsets.agent_events = 8;

    let new_state = handle_key(state, key(KeyCode::Char('k')));
    assert_eq!(new_state.scroll_offsets.agent_events, 7);
}

#[test]
fn j_key_scrolls_sessions_table() {
    let mut state = AppState::new();
    state.view = ViewState::Sessions;
    state.scroll_offsets.sessions = 3;

    let new_state = handle_key(state, key(KeyCode::Char('j')));
    assert_eq!(new_state.scroll_offsets.sessions, 4);
}

#[test]
fn k_key_scrolls_sessions_table() {
    let mut state = AppState::new();
    state.view = ViewState::Sessions;
    state.scroll_offsets.sessions = 3;

    let new_state = handle_key(state, key(KeyCode::Char('k')));
    assert_eq!(new_state.scroll_offsets.sessions, 2);
}

#[test]
fn enter_on_dashboard_drills_into_agent_detail() {
    let mut state = AppState::new();
    state.view = ViewState::Dashboard;

    let task = Task {
        id: "T1".to_string(),
        description: "Implement feature".to_string(),
        agent_id: Some("a04".to_string()),
        status: TaskStatus::Running,
        review_status: Default::default(),
        files_modified: vec![],
        tests_passed: None,
    };
    let wave = Wave::new(1, vec![task]);
    state.task_graph = Some(TaskGraph::new(vec![wave]));
    state.selected_task_index = Some(0);
    state.agents.insert("a04".into(), Agent::new("a04".into(), Utc::now()));

    let new_state = handle_key(state, key(KeyCode::Enter));
    assert!(matches!(new_state.view, ViewState::AgentDetail));
    assert_eq!(new_state.selected_agent_index, Some(0));
}

#[test]
fn enter_on_dashboard_noop_if_no_agent() {
    let mut state = AppState::new();
    state.view = ViewState::Dashboard;

    let task = Task::new("T1".to_string(), "Test".to_string(), TaskStatus::Pending);
    let wave = Wave::new(1, vec![task]);
    state.task_graph = Some(TaskGraph::new(vec![wave]));
    state.selected_task_index = Some(0);

    let new_state = handle_key(state, key(KeyCode::Enter));
    assert!(matches!(new_state.view, ViewState::Dashboard));
}

#[test]
fn enter_on_dashboard_noop_if_no_selection() {
    let mut state = AppState::new();
    state.view = ViewState::Dashboard;
    state.selected_task_index = None;

    let new_state = handle_key(state, key(KeyCode::Enter));
    assert!(matches!(new_state.view, ViewState::Dashboard));
}

#[test]
fn enter_on_agent_detail_is_noop() {
    let mut state = AppState::new();
    state.view = ViewState::AgentDetail;

    let new_state = handle_key(state, key(KeyCode::Enter));
    assert!(matches!(new_state.view, ViewState::AgentDetail));
}

#[test]
fn enter_on_sessions_is_noop_for_navigation() {
    let mut state = AppState::new();
    state.view = ViewState::Sessions;

    let new_state = handle_key(state, key(KeyCode::Enter));
    assert!(matches!(new_state.view, ViewState::Sessions));
}

#[test]
fn esc_from_agent_detail_returns_to_dashboard() {
    let mut state = AppState::new();
    state.view = ViewState::AgentDetail;

    let new_state = handle_key(state, key(KeyCode::Esc));
    assert!(matches!(new_state.view, ViewState::Dashboard));
}

#[test]
fn esc_from_sessions_returns_to_dashboard() {
    let mut state = AppState::new();
    state.view = ViewState::Sessions;

    let new_state = handle_key(state, key(KeyCode::Esc));
    assert!(matches!(new_state.view, ViewState::Dashboard));
}

#[test]
fn esc_from_dashboard_is_noop() {
    let state = AppState::new();
    let new_state = handle_key(state, key(KeyCode::Esc));
    assert!(matches!(new_state.view, ViewState::Dashboard));
}

#[test]
fn slash_starts_filter_mode() {
    let state = AppState::new();
    assert!(state.filter.is_none());

    let new_state = handle_key(state, key(KeyCode::Char('/')));
    assert!(new_state.filter.is_some());
    assert_eq!(new_state.filter.as_deref(), Some(""));
}

#[test]
fn question_mark_toggles_help_on() {
    let state = AppState::new();
    assert!(!state.show_help);

    let new_state = handle_key(state, key(KeyCode::Char('?')));
    assert!(new_state.show_help);
}

#[test]
fn question_mark_toggles_help_off() {
    let mut state = AppState::new();
    state.show_help = true;

    let new_state = handle_key(state, key(KeyCode::Char('?')));
    assert!(!new_state.show_help);
}

#[test]
fn space_toggles_auto_scroll_off() {
    let mut state = AppState::new();
    state.auto_scroll = true;

    let new_state = handle_key(state, key(KeyCode::Char(' ')));
    assert!(!new_state.auto_scroll);
}

#[test]
fn space_toggles_auto_scroll_on() {
    let mut state = AppState::new();
    state.auto_scroll = false;

    let new_state = handle_key(state, key(KeyCode::Char(' ')));
    assert!(new_state.auto_scroll);
}

#[test]
fn help_overlay_dismisses_on_any_key() {
    let mut state = AppState::new();
    state.show_help = true;

    let new_state = handle_key(state, key(KeyCode::Char('x')));
    assert!(!new_state.show_help);
}

#[test]
fn help_overlay_dismisses_on_escape() {
    let mut state = AppState::new();
    state.show_help = true;

    let new_state = handle_key(state, key(KeyCode::Esc));
    assert!(!new_state.show_help);
}

#[test]
fn filter_mode_escape_dismisses_filter() {
    let mut state = AppState::new();
    state.filter = Some("test".to_string());

    let new_state = handle_key(state, key(KeyCode::Esc));
    assert!(new_state.filter.is_none());
}

#[test]
fn filter_mode_char_appends_to_filter() {
    let mut state = AppState::new();
    state.filter = Some("te".to_string());

    let new_state = handle_key(state, key(KeyCode::Char('s')));
    assert_eq!(new_state.filter.as_deref(), Some("tes"));
}

#[test]
fn filter_mode_backspace_removes_char() {
    let mut state = AppState::new();
    state.filter = Some("test".to_string());

    let new_state = handle_key(state, key(KeyCode::Backspace));
    assert_eq!(new_state.filter.as_deref(), Some("tes"));
}

#[test]
fn filter_mode_backspace_on_empty_filter() {
    let mut state = AppState::new();
    state.filter = Some("".to_string());

    let new_state = handle_key(state, key(KeyCode::Backspace));
    assert_eq!(new_state.filter.as_deref(), Some(""));
}

#[test]
fn filter_mode_enter_keeps_filter() {
    let mut state = AppState::new();
    state.filter = Some("test".to_string());

    let new_state = handle_key(state, key(KeyCode::Enter));
    assert_eq!(new_state.filter.as_deref(), Some("test"));
}

#[test]
fn unknown_key_in_normal_mode_is_noop() {
    let state = AppState::new();
    let new_state = handle_key(state, key(KeyCode::F(1)));
    assert!(matches!(new_state.view, ViewState::Dashboard));
    assert!(!new_state.should_quit);
    assert_eq!(0, new_state.scroll_offsets.task_list);
}

#[test]
fn multiple_waves_drill_down_correct_task() {
    let mut state = AppState::new();
    state.agents.insert("a01".into(), Agent::new("a01".into(), Utc::now()));
    state.agents.insert("a02".into(), Agent::new("a02".into(), Utc::now()));
    state.agents.insert("a03".into(), Agent::new("a03".into(), Utc::now()));

    let task1 = Task {
        id: "T1".to_string(),
        description: "Task 1".to_string(),
        agent_id: Some("a01".to_string()),
        status: TaskStatus::Running,
        review_status: Default::default(),
        files_modified: vec![],
        tests_passed: None,
    };
    let task2 = Task {
        id: "T2".to_string(),
        description: "Task 2".to_string(),
        agent_id: Some("a02".to_string()),
        status: TaskStatus::Running,
        review_status: Default::default(),
        files_modified: vec![],
        tests_passed: None,
    };
    let task3 = Task {
        id: "T3".to_string(),
        description: "Task 3".to_string(),
        agent_id: Some("a03".to_string()),
        status: TaskStatus::Pending,
        review_status: Default::default(),
        files_modified: vec![],
        tests_passed: None,
    };

    let wave1 = Wave::new(1, vec![task1, task2]);
    let wave2 = Wave::new(2, vec![task3]);
    state.task_graph = Some(TaskGraph::new(vec![wave1, wave2]));
    state.selected_task_index = Some(2); // Third task overall

    let new_state = handle_key(state, key(KeyCode::Enter));
    assert!(matches!(new_state.view, ViewState::AgentDetail));
    assert_eq!(new_state.selected_agent_index, Some(2));
}

#[test]
fn out_of_bounds_task_index_is_noop() {
    let mut state = AppState::new();

    let task = Task::new("T1".to_string(), "Task 1".to_string(), TaskStatus::Pending);
    let wave = Wave::new(1, vec![task]);
    state.task_graph = Some(TaskGraph::new(vec![wave]));
    state.selected_task_index = Some(999); // Out of bounds

    let new_state = handle_key(state, key(KeyCode::Enter));
    assert!(matches!(new_state.view, ViewState::Dashboard));
}

#[test]
fn navigation_preserves_unrelated_state() {
    let mut state = AppState::new();
    state.scroll_offsets.sessions = 5;
    state.errors.push_back("Test error".to_string());

    let new_state = handle_key(state, key(KeyCode::Char('3')));
    assert_eq!(new_state.scroll_offsets.sessions, 5);
    assert_eq!(new_state.errors.len(), 1);
    assert!(matches!(new_state.view, ViewState::Sessions));
}

#[test]
fn help_overlay_prevents_navigation() {
    let mut state = AppState::new();
    state.show_help = true;

    let new_state = handle_key(state, key(KeyCode::Char('3')));
    assert!(!new_state.show_help);
    assert!(matches!(new_state.view, ViewState::Dashboard));
}

#[test]
fn filter_mode_prevents_navigation() {
    let mut state = AppState::new();
    state.filter = Some("test".to_string());

    let new_state = handle_key(state, key(KeyCode::Char('3')));
    assert_eq!(new_state.filter.as_deref(), Some("test3"));
    assert!(matches!(new_state.view, ViewState::Dashboard));
}
