use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::app::{AppState, PanelFocus, ViewState};

/// Half-page jump size for Ctrl+D / Ctrl+U
const PAGE_JUMP: usize = 20;

/// Pure navigation state transition function.
/// Takes current state + keyboard event, returns new state.
/// No I/O, no side effects, fully unit testable.
pub fn handle_key(mut state: AppState, key: KeyEvent) -> AppState {
    // Help overlay has priority
    if state.show_help {
        return handle_help_key(state, key);
    }

    // Filter mode has priority over normal navigation
    if state.filter.is_some() {
        return handle_filter_key(state, key);
    }

    // Normal navigation
    match key.code {
        KeyCode::Char('q') => {
            state.should_quit = true;
            state
        }
        KeyCode::Char('1') => {
            state.view = ViewState::Dashboard;
            state
        }
        KeyCode::Char('2') => switch_to_agent_detail(state),
        KeyCode::Char('3') => {
            state.view = ViewState::Sessions;
            let has_sessions = !state.active_sessions.is_empty() || !state.sessions.is_empty();
            if state.selected_session_index.is_none() && has_sessions {
                state.selected_session_index = Some(0);
            }
            state
        }
        KeyCode::Tab | KeyCode::Char('l') => toggle_focus_right(state),
        KeyCode::Char('h') => toggle_focus_left(state),
        KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => scroll_page_down(state),
        KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => scroll_page_up(state),
        KeyCode::Char('j') | KeyCode::Down => scroll_down(state),
        KeyCode::Char('k') | KeyCode::Up => scroll_up(state),
        KeyCode::Enter => drill_down(state),
        KeyCode::Esc => go_back(state),
        KeyCode::Char('/') => start_filter(state),
        KeyCode::Char('?') => toggle_help(state),
        KeyCode::Char(' ') => toggle_auto_scroll(state),
        _ => state,
    }
}

/// Handle key input when help overlay is shown.
/// Any key dismisses help.
fn handle_help_key(mut state: AppState, _key: KeyEvent) -> AppState {
    state.show_help = false;
    state
}

/// Handle key input when filter mode is active.
/// Esc dismisses filter, other keys would modify filter string.
fn handle_filter_key(mut state: AppState, key: KeyEvent) -> AppState {
    match key.code {
        KeyCode::Esc => {
            state.filter = None;
            state
        }
        KeyCode::Enter => {
            // Apply filter and exit filter mode (keep filter string)
            state
        }
        KeyCode::Backspace => {
            if let Some(ref mut filter) = state.filter {
                filter.pop();
            }
            state
        }
        KeyCode::Char(c) => {
            if let Some(ref mut filter) = state.filter {
                filter.push(c);
            }
            state
        }
        _ => state,
    }
}

/// Switch to AgentDetail view. Auto-selects first agent if none selected.
fn switch_to_agent_detail(mut state: AppState) -> AppState {
    state.view = ViewState::AgentDetail;
    if state.selected_agent_index.is_none() && !state.agents.is_empty() {
        state.selected_agent_index = Some(0);
    }
    state
}

/// Toggle focus to the right panel.
fn toggle_focus_right(mut state: AppState) -> AppState {
    state.focus = PanelFocus::Right;
    state
}

/// Toggle focus to the left panel.
fn toggle_focus_left(mut state: AppState) -> AppState {
    state.focus = PanelFocus::Left;
    state
}

/// Scroll down in the currently focused panel.
fn scroll_down(mut state: AppState) -> AppState {
    match state.view {
        ViewState::Dashboard => match state.focus {
            PanelFocus::Left => {
                state.scroll_offsets.task_list = state.scroll_offsets.task_list.saturating_add(1);
            }
            PanelFocus::Right => {
                state.scroll_offsets.event_stream =
                    state.scroll_offsets.event_stream.saturating_add(1);
                state.auto_scroll = false;
            }
        },
        ViewState::AgentDetail => match state.focus {
            PanelFocus::Left => {
                let agent_count = state.agents.len();
                if agent_count > 0 {
                    let current = state.selected_agent_index.unwrap_or(0);
                    let new_idx = (current + 1).min(agent_count - 1);
                    if new_idx != current {
                        state.scroll_offsets.agent_events = 0;
                    }
                    state.selected_agent_index = Some(new_idx);
                }
            }
            PanelFocus::Right => {
                state.scroll_offsets.agent_events =
                    state.scroll_offsets.agent_events.saturating_add(1);
                state.auto_scroll = false;
            }
        },
        ViewState::Sessions => {
            let session_count = state.active_sessions.len() + state.sessions.len();
            if session_count > 0 {
                let current = state.selected_session_index.unwrap_or(0);
                state.selected_session_index = Some((current + 1).min(session_count - 1));
            }
        }
        ViewState::SessionDetail => match state.focus {
            PanelFocus::Left => {
                state.scroll_offsets.session_detail_left =
                    state.scroll_offsets.session_detail_left.saturating_add(1);
            }
            PanelFocus::Right => {
                state.scroll_offsets.session_detail_right =
                    state.scroll_offsets.session_detail_right.saturating_add(1);
            }
        },
    }
    state
}

/// Scroll up in the currently focused panel.
fn scroll_up(mut state: AppState) -> AppState {
    match state.view {
        ViewState::Dashboard => match state.focus {
            PanelFocus::Left => {
                state.scroll_offsets.task_list = state.scroll_offsets.task_list.saturating_sub(1);
            }
            PanelFocus::Right => {
                state.scroll_offsets.event_stream =
                    state.scroll_offsets.event_stream.saturating_sub(1);
                state.auto_scroll = false;
            }
        },
        ViewState::AgentDetail => match state.focus {
            PanelFocus::Left => {
                let current = state.selected_agent_index.unwrap_or(0);
                let new_idx = current.saturating_sub(1);
                if new_idx != current {
                    state.scroll_offsets.agent_events = 0;
                }
                state.selected_agent_index = Some(new_idx);
            }
            PanelFocus::Right => {
                state.scroll_offsets.agent_events =
                    state.scroll_offsets.agent_events.saturating_sub(1);
                state.auto_scroll = false;
            }
        },
        ViewState::Sessions => {
            let current = state.selected_session_index.unwrap_or(0);
            state.selected_session_index = Some(current.saturating_sub(1));
        }
        ViewState::SessionDetail => match state.focus {
            PanelFocus::Left => {
                state.scroll_offsets.session_detail_left =
                    state.scroll_offsets.session_detail_left.saturating_sub(1);
            }
            PanelFocus::Right => {
                state.scroll_offsets.session_detail_right =
                    state.scroll_offsets.session_detail_right.saturating_sub(1);
            }
        },
    }
    state
}

/// Scroll down by PAGE_JUMP lines (Ctrl+D).
fn scroll_page_down(mut state: AppState) -> AppState {
    match state.view {
        ViewState::Dashboard => match state.focus {
            PanelFocus::Left => {
                state.scroll_offsets.task_list = state.scroll_offsets.task_list.saturating_add(PAGE_JUMP);
            }
            PanelFocus::Right => {
                state.scroll_offsets.event_stream = state.scroll_offsets.event_stream.saturating_add(PAGE_JUMP);
                state.auto_scroll = false;
            }
        },
        ViewState::AgentDetail => match state.focus {
            PanelFocus::Left => {
                let agent_count = state.agents.len();
                if agent_count > 0 {
                    let current = state.selected_agent_index.unwrap_or(0);
                    let new_idx = (current + PAGE_JUMP).min(agent_count - 1);
                    if new_idx != current {
                        state.scroll_offsets.agent_events = 0;
                    }
                    state.selected_agent_index = Some(new_idx);
                }
            }
            PanelFocus::Right => {
                state.scroll_offsets.agent_events = state.scroll_offsets.agent_events.saturating_add(PAGE_JUMP);
                state.auto_scroll = false;
            }
        },
        ViewState::Sessions => {
            let session_count = state.active_sessions.len() + state.sessions.len();
            if session_count > 0 {
                let current = state.selected_session_index.unwrap_or(0);
                state.selected_session_index = Some((current + PAGE_JUMP).min(session_count - 1));
            }
        }
        ViewState::SessionDetail => match state.focus {
            PanelFocus::Left => {
                state.scroll_offsets.session_detail_left = state.scroll_offsets.session_detail_left.saturating_add(PAGE_JUMP);
            }
            PanelFocus::Right => {
                state.scroll_offsets.session_detail_right = state.scroll_offsets.session_detail_right.saturating_add(PAGE_JUMP);
            }
        },
    }
    state
}

/// Scroll up by PAGE_JUMP lines (Ctrl+U).
fn scroll_page_up(mut state: AppState) -> AppState {
    match state.view {
        ViewState::Dashboard => match state.focus {
            PanelFocus::Left => {
                state.scroll_offsets.task_list = state.scroll_offsets.task_list.saturating_sub(PAGE_JUMP);
            }
            PanelFocus::Right => {
                state.scroll_offsets.event_stream = state.scroll_offsets.event_stream.saturating_sub(PAGE_JUMP);
                state.auto_scroll = false;
            }
        },
        ViewState::AgentDetail => match state.focus {
            PanelFocus::Left => {
                let current = state.selected_agent_index.unwrap_or(0);
                let new_idx = current.saturating_sub(PAGE_JUMP);
                if new_idx != current {
                    state.scroll_offsets.agent_events = 0;
                }
                state.selected_agent_index = Some(new_idx);
            }
            PanelFocus::Right => {
                state.scroll_offsets.agent_events = state.scroll_offsets.agent_events.saturating_sub(PAGE_JUMP);
                state.auto_scroll = false;
            }
        },
        ViewState::Sessions => {
            let current = state.selected_session_index.unwrap_or(0);
            state.selected_session_index = Some(current.saturating_sub(PAGE_JUMP));
        }
        ViewState::SessionDetail => match state.focus {
            PanelFocus::Left => {
                state.scroll_offsets.session_detail_left = state.scroll_offsets.session_detail_left.saturating_sub(PAGE_JUMP);
            }
            PanelFocus::Right => {
                state.scroll_offsets.session_detail_right = state.scroll_offsets.session_detail_right.saturating_sub(PAGE_JUMP);
            }
        },
    }
    state
}

/// Drill down into selected item.
/// Dashboard: Enter on task with agent -> switch to AgentDetail, select that agent
/// AgentDetail: no drill-down (no-op)
/// Sessions: Enter on session -> SessionDetail (active or archived)
/// SessionDetail: no-op
fn drill_down(mut state: AppState) -> AppState {
    match state.view {
        ViewState::Dashboard => {
            if let Some(task_idx) = state.selected_task_index {
                if let Some(ref task_graph) = state.task_graph {
                    let all_tasks: Vec<_> = task_graph
                        .waves
                        .iter()
                        .flat_map(|w| &w.tasks)
                        .collect();

                    if let Some(task) = all_tasks.get(task_idx) {
                        if let Some(ref agent_id) = task.agent_id {
                            let agent_idx = state
                                .sorted_agent_keys()
                                .iter()
                                .position(|k| k == agent_id);
                            state.selected_agent_index = agent_idx;
                            state.view = ViewState::AgentDetail;
                        }
                    }
                }
            }
        }
        ViewState::AgentDetail => {}
        ViewState::Sessions => {
            if let Some(idx) = state.selected_session_index {
                let active_count = state.active_sessions.len();
                if idx < active_count {
                    // Active session — navigate directly (live data)
                    state.view = ViewState::SessionDetail;
                    state.scroll_offsets.session_detail_left = 0;
                    state.scroll_offsets.session_detail_right = 0;
                    state.focus = PanelFocus::Left;
                } else {
                    let archive_idx = idx - active_count;
                    if let Some(session) = state.sessions.get(archive_idx) {
                        if session.data.is_some() {
                            // Already loaded — navigate directly
                            state.view = ViewState::SessionDetail;
                            state.scroll_offsets.session_detail_left = 0;
                            state.scroll_offsets.session_detail_right = 0;
                            state.focus = PanelFocus::Left;
                        } else {
                            // Not loaded — set loading flag (main loop handles I/O)
                            state.loading_session = Some(archive_idx);
                        }
                    }
                }
            }
        }
        ViewState::SessionDetail => {}
    }
    state
}

/// Navigate back to previous view.
fn go_back(mut state: AppState) -> AppState {
    match state.view {
        ViewState::AgentDetail => {
            state.view = ViewState::Dashboard;
        }
        ViewState::Sessions => {
            state.view = ViewState::Dashboard;
        }
        ViewState::SessionDetail => {
            state.view = ViewState::Sessions;
        }
        ViewState::Dashboard => {}
    }
    state
}

/// Start filter/search mode.
fn start_filter(mut state: AppState) -> AppState {
    state.filter = Some(String::new());
    state
}

/// Toggle help overlay.
fn toggle_help(mut state: AppState) -> AppState {
    state.show_help = !state.show_help;
    state
}

/// Toggle auto-scroll mode for event stream.
fn toggle_auto_scroll(mut state: AppState) -> AppState {
    state.auto_scroll = !state.auto_scroll;
    state
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Agent, ArchivedSession, SessionMeta, Task, TaskGraph, TaskStatus, Wave};
    use std::path::PathBuf;
    use chrono::Utc;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::empty())
    }

    #[test]
    fn quit_key_sets_should_quit() {
        let state = AppState::new();
        let new_state = handle_key(state, key(KeyCode::Char('q')));
        assert!(new_state.should_quit);
    }

    #[test]
    fn key_1_switches_to_dashboard() {
        let mut state = AppState::new();
        state.view = ViewState::Sessions;
        let new_state = handle_key(state, key(KeyCode::Char('1')));
        assert!(matches!(new_state.view, ViewState::Dashboard));
    }

    #[test]
    fn key_3_switches_to_sessions() {
        let state = AppState::new();
        let new_state = handle_key(state, key(KeyCode::Char('3')));
        assert!(matches!(new_state.view, ViewState::Sessions));
    }

    #[test]
    fn key_2_switches_to_agent_detail() {
        let state = AppState::new();
        let new_state = handle_key(state, key(KeyCode::Char('2')));
        assert!(matches!(new_state.view, ViewState::AgentDetail));
    }

    #[test]
    fn key_2_auto_selects_first_agent() {
        let mut state = AppState::new();
        state.agents.insert("a01".into(), Agent::new("a01".into(), Utc::now()));

        let new_state = handle_key(state, key(KeyCode::Char('2')));
        assert!(matches!(new_state.view, ViewState::AgentDetail));
        assert_eq!(new_state.selected_agent_index, Some(0));
    }

    #[test]
    fn key_2_no_auto_select_when_empty() {
        let state = AppState::new();
        let new_state = handle_key(state, key(KeyCode::Char('2')));
        assert_eq!(new_state.selected_agent_index, None);
    }

    #[test]
    fn tab_switches_focus_to_right() {
        let state = AppState::new();
        assert!(matches!(state.focus, PanelFocus::Left));

        let new_state = handle_key(state, key(KeyCode::Tab));
        assert!(matches!(new_state.focus, PanelFocus::Right));
    }

    #[test]
    fn l_switches_focus_to_right() {
        let state = AppState::new();
        let new_state = handle_key(state, key(KeyCode::Char('l')));
        assert!(matches!(new_state.focus, PanelFocus::Right));
    }

    #[test]
    fn h_switches_focus_to_left() {
        let mut state = AppState::new();
        state.focus = PanelFocus::Right;

        let new_state = handle_key(state, key(KeyCode::Char('h')));
        assert!(matches!(new_state.focus, PanelFocus::Left));
    }

    #[test]
    fn j_scrolls_down_left_panel_in_dashboard() {
        let state = AppState::new();
        assert_eq!(state.scroll_offsets.task_list, 0);

        let new_state = handle_key(state, key(KeyCode::Char('j')));
        assert_eq!(new_state.scroll_offsets.task_list, 1);
    }

    #[test]
    fn down_arrow_scrolls_down() {
        let state = AppState::new();
        let new_state = handle_key(state, key(KeyCode::Down));
        assert_eq!(new_state.scroll_offsets.task_list, 1);
    }

    #[test]
    fn k_scrolls_up_left_panel_in_dashboard() {
        let mut state = AppState::new();
        state.scroll_offsets.task_list = 5;

        let new_state = handle_key(state, key(KeyCode::Char('k')));
        assert_eq!(new_state.scroll_offsets.task_list, 4);
    }

    #[test]
    fn up_arrow_scrolls_up() {
        let mut state = AppState::new();
        state.scroll_offsets.task_list = 5;

        let new_state = handle_key(state, key(KeyCode::Up));
        assert_eq!(new_state.scroll_offsets.task_list, 4);
    }

    #[test]
    fn scroll_up_at_zero_stays_at_zero() {
        let state = AppState::new();
        assert_eq!(state.scroll_offsets.task_list, 0);

        let new_state = handle_key(state, key(KeyCode::Char('k')));
        assert_eq!(new_state.scroll_offsets.task_list, 0);
    }

    #[test]
    fn j_scrolls_right_panel_when_focused() {
        let mut state = AppState::new();
        state.focus = PanelFocus::Right;
        assert_eq!(state.scroll_offsets.event_stream, 0);

        let new_state = handle_key(state, key(KeyCode::Char('j')));
        assert_eq!(new_state.scroll_offsets.event_stream, 1);
    }

    #[test]
    fn scroll_disables_auto_scroll_for_event_stream() {
        let mut state = AppState::new();
        state.focus = PanelFocus::Right;
        state.auto_scroll = true;

        let new_state = handle_key(state, key(KeyCode::Char('j')));
        assert!(!new_state.auto_scroll);
    }

    #[test]
    fn j_moves_agent_selection_down() {
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
    fn j_clamps_agent_selection_at_max() {
        let mut state = AppState::new();
        state.view = ViewState::AgentDetail;
        state.focus = PanelFocus::Left;
        state.agents.insert("a01".into(), Agent::new("a01".into(), Utc::now()));
        state.agents.insert("a02".into(), Agent::new("a02".into(), Utc::now()));
        state.selected_agent_index = Some(1);

        let new_state = handle_key(state, key(KeyCode::Char('j')));
        assert_eq!(new_state.selected_agent_index, Some(1));
    }

    #[test]
    fn k_moves_agent_selection_up() {
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
    fn agent_selection_change_resets_event_scroll() {
        let mut state = AppState::new();
        state.view = ViewState::AgentDetail;
        state.focus = PanelFocus::Left;
        state.agents.insert("a01".into(), Agent::new("a01".into(), Utc::now()));
        state.agents.insert("a02".into(), Agent::new("a02".into(), Utc::now()));
        state.selected_agent_index = Some(0);
        state.scroll_offsets.agent_events = 15;

        let new_state = handle_key(state, key(KeyCode::Char('j')));
        assert_eq!(new_state.scroll_offsets.agent_events, 0);
    }

    #[test]
    fn j_scrolls_agent_events_right_panel() {
        let mut state = AppState::new();
        state.view = ViewState::AgentDetail;
        state.focus = PanelFocus::Right;

        let new_state = handle_key(state, key(KeyCode::Char('j')));
        assert_eq!(new_state.scroll_offsets.agent_events, 1);
    }

    #[test]
    fn k_scrolls_agent_events_right_panel() {
        let mut state = AppState::new();
        state.view = ViewState::AgentDetail;
        state.focus = PanelFocus::Right;
        state.scroll_offsets.agent_events = 8;

        let new_state = handle_key(state, key(KeyCode::Char('k')));
        assert_eq!(new_state.scroll_offsets.agent_events, 7);
    }

    #[test]
    fn scroll_in_sessions_view() {
        let mut state = AppState::new();
        state.view = ViewState::Sessions;
        state.sessions = vec![
            ArchivedSession::new(SessionMeta::new("s1".into(), Utc::now(), "/proj".into()), PathBuf::new()),
            ArchivedSession::new(SessionMeta::new("s2".into(), Utc::now(), "/proj".into()), PathBuf::new()),
        ];
        state.selected_session_index = Some(0);

        let new_state = handle_key(state, key(KeyCode::Char('j')));
        assert_eq!(new_state.selected_session_index, Some(1));
    }

    #[test]
    fn enter_on_dashboard_drills_into_agent_detail() {
        let mut state = AppState::new();

        let task = Task {
            id: "T1".to_string(),
            description: "Test".to_string(),
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
    fn enter_on_agent_detail_is_noop() {
        let mut state = AppState::new();
        state.view = ViewState::AgentDetail;

        let new_state = handle_key(state, key(KeyCode::Enter));
        assert!(matches!(new_state.view, ViewState::AgentDetail));
    }

    #[test]
    fn esc_on_agent_detail_goes_back_to_dashboard() {
        let mut state = AppState::new();
        state.view = ViewState::AgentDetail;

        let new_state = handle_key(state, key(KeyCode::Esc));
        assert!(matches!(new_state.view, ViewState::Dashboard));
    }

    #[test]
    fn esc_on_sessions_goes_back_to_dashboard() {
        let mut state = AppState::new();
        state.view = ViewState::Sessions;

        let new_state = handle_key(state, key(KeyCode::Esc));
        assert!(matches!(new_state.view, ViewState::Dashboard));
    }

    #[test]
    fn esc_on_dashboard_is_noop() {
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
        assert_eq!(new_state.filter.unwrap(), "");
    }

    #[test]
    fn question_mark_toggles_help() {
        let state = AppState::new();
        assert!(!state.show_help);

        let new_state = handle_key(state, key(KeyCode::Char('?')));
        assert!(new_state.show_help);

        let new_state2 = handle_key(new_state, key(KeyCode::Char('?')));
        assert!(!new_state2.show_help);
    }

    #[test]
    fn space_toggles_auto_scroll() {
        let state = AppState::new();
        assert!(state.auto_scroll);

        let new_state = handle_key(state, key(KeyCode::Char(' ')));
        assert!(!new_state.auto_scroll);

        let new_state2 = handle_key(new_state, key(KeyCode::Char(' ')));
        assert!(new_state2.auto_scroll);
    }

    #[test]
    fn any_key_dismisses_help_overlay() {
        let mut state = AppState::new();
        state.show_help = true;

        let new_state = handle_key(state, key(KeyCode::Char('a')));
        assert!(!new_state.show_help);
    }

    #[test]
    fn esc_dismisses_filter_mode() {
        let mut state = AppState::new();
        state.filter = Some("test".to_string());

        let new_state = handle_key(state, key(KeyCode::Esc));
        assert!(new_state.filter.is_none());
    }

    #[test]
    fn char_appends_to_filter() {
        let mut state = AppState::new();
        state.filter = Some("te".to_string());

        let new_state = handle_key(state, key(KeyCode::Char('s')));
        assert_eq!(new_state.filter.unwrap(), "tes");
    }

    #[test]
    fn backspace_removes_from_filter() {
        let mut state = AppState::new();
        state.filter = Some("test".to_string());

        let new_state = handle_key(state, key(KeyCode::Backspace));
        assert_eq!(new_state.filter.unwrap(), "tes");
    }

    #[test]
    fn enter_keeps_filter_in_filter_mode() {
        let mut state = AppState::new();
        state.filter = Some("test".to_string());

        let new_state = handle_key(state, key(KeyCode::Enter));
        assert_eq!(new_state.filter.unwrap(), "test");
    }

    fn ctrl(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::CONTROL)
    }

    #[test]
    fn ctrl_d_page_scrolls_down_dashboard_left() {
        let state = AppState::new();
        let new_state = handle_key(state, ctrl(KeyCode::Char('d')));
        assert_eq!(new_state.scroll_offsets.task_list, PAGE_JUMP);
    }

    #[test]
    fn ctrl_u_page_scrolls_up_dashboard_left() {
        let mut state = AppState::new();
        state.scroll_offsets.task_list = 30;
        let new_state = handle_key(state, ctrl(KeyCode::Char('u')));
        assert_eq!(new_state.scroll_offsets.task_list, 30 - PAGE_JUMP);
    }

    #[test]
    fn ctrl_d_page_scrolls_down_dashboard_right() {
        let mut state = AppState::new();
        state.focus = PanelFocus::Right;
        let new_state = handle_key(state, ctrl(KeyCode::Char('d')));
        assert_eq!(new_state.scroll_offsets.event_stream, PAGE_JUMP);
        assert!(!new_state.auto_scroll);
    }

    #[test]
    fn ctrl_u_page_scrolls_up_saturates_at_zero() {
        let state = AppState::new();
        let new_state = handle_key(state, ctrl(KeyCode::Char('u')));
        assert_eq!(new_state.scroll_offsets.task_list, 0);
    }

    #[test]
    fn ctrl_d_page_scrolls_session_detail() {
        let mut state = AppState::new();
        state.view = ViewState::SessionDetail;
        state.focus = PanelFocus::Right;
        let new_state = handle_key(state, ctrl(KeyCode::Char('d')));
        assert_eq!(new_state.scroll_offsets.session_detail_right, PAGE_JUMP);
    }

    #[test]
    fn ctrl_u_page_scrolls_session_detail() {
        let mut state = AppState::new();
        state.view = ViewState::SessionDetail;
        state.focus = PanelFocus::Left;
        state.scroll_offsets.session_detail_left = 25;
        let new_state = handle_key(state, ctrl(KeyCode::Char('u')));
        assert_eq!(new_state.scroll_offsets.session_detail_left, 25 - PAGE_JUMP);
    }

    #[test]
    fn unknown_key_is_noop() {
        let state = AppState::new();
        let new_state = handle_key(state, key(KeyCode::F(1)));
        assert!(matches!(new_state.view, ViewState::Dashboard));
        assert!(!new_state.should_quit);
    }

    #[test]
    fn multiple_tasks_drill_down_selects_correct_agent() {
        let mut state = AppState::new();
        state.agents.insert("a01".into(), Agent::new("a01".into(), Utc::now()));
        state.agents.insert("a02".into(), Agent::new("a02".into(), Utc::now()));

        let tasks = vec![
            Task {
                id: "T1".to_string(),
                description: "Task 1".to_string(),
                agent_id: Some("a01".to_string()),
                status: TaskStatus::Running,
                review_status: Default::default(),
                files_modified: vec![],
                tests_passed: None,
            },
            Task {
                id: "T2".to_string(),
                description: "Task 2".to_string(),
                agent_id: Some("a02".to_string()),
                status: TaskStatus::Running,
                review_status: Default::default(),
                files_modified: vec![],
                tests_passed: None,
            },
        ];
        let wave = Wave::new(1, vec![tasks[0].clone(), tasks[1].clone()]);
        state.task_graph = Some(TaskGraph::new(vec![wave]));
        state.selected_task_index = Some(1);

        let new_state = handle_key(state, key(KeyCode::Enter));
        assert!(matches!(new_state.view, ViewState::AgentDetail));
        // a02 is at index 1 in BTreeMap (sorted keys: a01, a02)
        assert_eq!(new_state.selected_agent_index, Some(1));
    }
}
