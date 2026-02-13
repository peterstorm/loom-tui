use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::app::{AppState, PanelFocus, ViewState};

/// Half-page jump size for Ctrl+D / Ctrl+U
const PAGE_JUMP: usize = 20;

/// Navigation state transition function.
/// Mutates state in place based on keyboard event.
pub fn handle_key(state: &mut AppState, key: KeyEvent) {
    // Help overlay has priority
    if state.ui.show_help {
        handle_help_key(state, key);
        return;
    }

    // Filter mode has priority over normal navigation
    if state.ui.filter.is_some() {
        handle_filter_key(state, key);
        return;
    }

    // Normal navigation
    match key.code {
        KeyCode::Char('q') => {
            state.meta.should_quit = true;
        }
        KeyCode::Char('1') => {
            state.ui.view = ViewState::Dashboard;
        }
        KeyCode::Char('2') => switch_to_agent_detail(state),
        KeyCode::Char('3') => {
            state.ui.view = ViewState::Sessions;
            let has_sessions = !state.domain.active_sessions.is_empty() || !state.domain.sessions.is_empty();
            if state.ui.selected_session_index.is_none() && has_sessions {
                state.ui.selected_session_index = Some(0);
            }
        }
        KeyCode::Tab => toggle_focus(state),
        KeyCode::Char('l') => toggle_focus_right(state),
        KeyCode::Char('h') => toggle_focus_left(state),
        KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => scroll_page_down(state),
        KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => scroll_page_up(state),
        KeyCode::Char('j') | KeyCode::Down => scroll_down(state),
        KeyCode::Char('k') | KeyCode::Up => scroll_up(state),
        KeyCode::Char('g') => jump_to_top(state),
        KeyCode::Char('G') => jump_to_bottom(state),
        KeyCode::Enter => drill_down(state),
        KeyCode::Esc => go_back(state),
        KeyCode::Char('/') => start_filter(state),
        KeyCode::Char('?') => toggle_help(state),
        KeyCode::Char(' ') => toggle_auto_scroll(state),
        _ => {}
    }
}

fn handle_help_key(state: &mut AppState, _key: KeyEvent) {
    state.ui.show_help = false;
}

fn handle_filter_key(state: &mut AppState, key: KeyEvent) {
    match key.code {
        KeyCode::Esc => {
            state.ui.filter = None;
        }
        KeyCode::Enter => {}
        KeyCode::Backspace => {
            if let Some(ref mut filter) = state.ui.filter {
                filter.pop();
            }
        }
        KeyCode::Char(c) => {
            if let Some(ref mut filter) = state.ui.filter {
                filter.push(c);
            }
        }
        _ => {}
    }
}

fn switch_to_agent_detail(state: &mut AppState) {
    state.ui.view = ViewState::AgentDetail;
    if state.ui.selected_agent_index.is_none() && !state.domain.agents.is_empty() {
        state.ui.selected_agent_index = Some(0);
    }
}

/// Total flat task count across all waves.
fn task_count(state: &AppState) -> usize {
    state.domain.task_graph.as_ref().map(|g| g.total_tasks).unwrap_or(0)
}

fn toggle_focus(state: &mut AppState) {
    state.ui.focus = match state.ui.focus {
        PanelFocus::Left => PanelFocus::Right,
        PanelFocus::Right => PanelFocus::Left,
    };
}

fn toggle_focus_right(state: &mut AppState) {
    state.ui.focus = PanelFocus::Right;
}

fn toggle_focus_left(state: &mut AppState) {
    state.ui.focus = PanelFocus::Left;
}

fn scroll_down(state: &mut AppState) {
    match state.ui.view {
        ViewState::Dashboard => match state.ui.focus {
            PanelFocus::Left => {
                state.ui.scroll_offsets.task_list = state.ui.scroll_offsets.task_list.saturating_add(1);
                let max = task_count(state).saturating_sub(1);
                let current = state.ui.selected_task_index.unwrap_or(0);
                state.ui.selected_task_index = Some(current.saturating_add(1).min(max));
            }
            PanelFocus::Right => {
                state.ui.scroll_offsets.event_stream =
                    state.ui.scroll_offsets.event_stream.saturating_add(1);
                state.ui.auto_scroll = false;
            }
        },
        ViewState::AgentDetail => match state.ui.focus {
            PanelFocus::Left => {
                let agent_count = state.domain.agents.len();
                if agent_count > 0 {
                    let current = state.ui.selected_agent_index.unwrap_or(0);
                    let new_idx = (current + 1).min(agent_count - 1);
                    if new_idx != current {
                        state.ui.scroll_offsets.agent_events = 0;
                    }
                    state.ui.selected_agent_index = Some(new_idx);
                }
            }
            PanelFocus::Right => {
                state.ui.scroll_offsets.agent_events =
                    state.ui.scroll_offsets.agent_events.saturating_add(1);
                state.ui.auto_scroll = false;
            }
        },
        ViewState::Sessions => {
            let session_count = state.domain.active_sessions.len() + state.domain.sessions.len();
            if session_count > 0 {
                let current = state.ui.selected_session_index.unwrap_or(0);
                state.ui.selected_session_index = Some((current + 1).min(session_count - 1));
            }
        }
        ViewState::SessionDetail => match state.ui.focus {
            PanelFocus::Left => {
                state.ui.scroll_offsets.session_detail_left =
                    state.ui.scroll_offsets.session_detail_left.saturating_add(1);
            }
            PanelFocus::Right => {
                state.ui.scroll_offsets.session_detail_right =
                    state.ui.scroll_offsets.session_detail_right.saturating_add(1);
            }
        },
    }
}

fn scroll_up(state: &mut AppState) {
    match state.ui.view {
        ViewState::Dashboard => match state.ui.focus {
            PanelFocus::Left => {
                state.ui.scroll_offsets.task_list = state.ui.scroll_offsets.task_list.saturating_sub(1);
                let current = state.ui.selected_task_index.unwrap_or(0);
                state.ui.selected_task_index = Some(current.saturating_sub(1));
            }
            PanelFocus::Right => {
                state.ui.scroll_offsets.event_stream =
                    state.ui.scroll_offsets.event_stream.saturating_sub(1);
                state.ui.auto_scroll = false;
            }
        },
        ViewState::AgentDetail => match state.ui.focus {
            PanelFocus::Left => {
                let current = state.ui.selected_agent_index.unwrap_or(0);
                let new_idx = current.saturating_sub(1);
                if new_idx != current {
                    state.ui.scroll_offsets.agent_events = 0;
                }
                state.ui.selected_agent_index = Some(new_idx);
            }
            PanelFocus::Right => {
                state.ui.scroll_offsets.agent_events =
                    state.ui.scroll_offsets.agent_events.saturating_sub(1);
                state.ui.auto_scroll = false;
            }
        },
        ViewState::Sessions => {
            let current = state.ui.selected_session_index.unwrap_or(0);
            state.ui.selected_session_index = Some(current.saturating_sub(1));
        }
        ViewState::SessionDetail => match state.ui.focus {
            PanelFocus::Left => {
                state.ui.scroll_offsets.session_detail_left =
                    state.ui.scroll_offsets.session_detail_left.saturating_sub(1);
            }
            PanelFocus::Right => {
                state.ui.scroll_offsets.session_detail_right =
                    state.ui.scroll_offsets.session_detail_right.saturating_sub(1);
            }
        },
    }
}

fn scroll_page_down(state: &mut AppState) {
    match state.ui.view {
        ViewState::Dashboard => match state.ui.focus {
            PanelFocus::Left => {
                state.ui.scroll_offsets.task_list = state.ui.scroll_offsets.task_list.saturating_add(PAGE_JUMP);
            }
            PanelFocus::Right => {
                state.ui.scroll_offsets.event_stream = state.ui.scroll_offsets.event_stream.saturating_add(PAGE_JUMP);
                state.ui.auto_scroll = false;
            }
        },
        ViewState::AgentDetail => match state.ui.focus {
            PanelFocus::Left => {
                let agent_count = state.domain.agents.len();
                if agent_count > 0 {
                    let current = state.ui.selected_agent_index.unwrap_or(0);
                    let new_idx = (current + PAGE_JUMP).min(agent_count - 1);
                    if new_idx != current {
                        state.ui.scroll_offsets.agent_events = 0;
                    }
                    state.ui.selected_agent_index = Some(new_idx);
                }
            }
            PanelFocus::Right => {
                state.ui.scroll_offsets.agent_events = state.ui.scroll_offsets.agent_events.saturating_add(PAGE_JUMP);
                state.ui.auto_scroll = false;
            }
        },
        ViewState::Sessions => {
            let session_count = state.domain.active_sessions.len() + state.domain.sessions.len();
            if session_count > 0 {
                let current = state.ui.selected_session_index.unwrap_or(0);
                state.ui.selected_session_index = Some((current + PAGE_JUMP).min(session_count - 1));
            }
        }
        ViewState::SessionDetail => match state.ui.focus {
            PanelFocus::Left => {
                state.ui.scroll_offsets.session_detail_left = state.ui.scroll_offsets.session_detail_left.saturating_add(PAGE_JUMP);
            }
            PanelFocus::Right => {
                state.ui.scroll_offsets.session_detail_right = state.ui.scroll_offsets.session_detail_right.saturating_add(PAGE_JUMP);
            }
        },
    }
}

fn scroll_page_up(state: &mut AppState) {
    match state.ui.view {
        ViewState::Dashboard => match state.ui.focus {
            PanelFocus::Left => {
                state.ui.scroll_offsets.task_list = state.ui.scroll_offsets.task_list.saturating_sub(PAGE_JUMP);
            }
            PanelFocus::Right => {
                state.ui.scroll_offsets.event_stream = state.ui.scroll_offsets.event_stream.saturating_sub(PAGE_JUMP);
                state.ui.auto_scroll = false;
            }
        },
        ViewState::AgentDetail => match state.ui.focus {
            PanelFocus::Left => {
                let current = state.ui.selected_agent_index.unwrap_or(0);
                let new_idx = current.saturating_sub(PAGE_JUMP);
                if new_idx != current {
                    state.ui.scroll_offsets.agent_events = 0;
                }
                state.ui.selected_agent_index = Some(new_idx);
            }
            PanelFocus::Right => {
                state.ui.scroll_offsets.agent_events = state.ui.scroll_offsets.agent_events.saturating_sub(PAGE_JUMP);
                state.ui.auto_scroll = false;
            }
        },
        ViewState::Sessions => {
            let current = state.ui.selected_session_index.unwrap_or(0);
            state.ui.selected_session_index = Some(current.saturating_sub(PAGE_JUMP));
        }
        ViewState::SessionDetail => match state.ui.focus {
            PanelFocus::Left => {
                state.ui.scroll_offsets.session_detail_left = state.ui.scroll_offsets.session_detail_left.saturating_sub(PAGE_JUMP);
            }
            PanelFocus::Right => {
                state.ui.scroll_offsets.session_detail_right = state.ui.scroll_offsets.session_detail_right.saturating_sub(PAGE_JUMP);
            }
        },
    }
}

fn jump_to_top(state: &mut AppState) {
    match state.ui.view {
        ViewState::Dashboard => match state.ui.focus {
            PanelFocus::Left => {
                state.ui.scroll_offsets.task_list = 0;
                state.ui.selected_task_index = Some(0);
            }
            PanelFocus::Right => {
                state.ui.scroll_offsets.event_stream = 0;
            }
        },
        ViewState::AgentDetail => match state.ui.focus {
            PanelFocus::Left => {
                if !state.domain.agents.is_empty() {
                    state.ui.selected_agent_index = Some(0);
                    state.ui.scroll_offsets.agent_events = 0;
                }
            }
            PanelFocus::Right => {
                state.ui.scroll_offsets.agent_events = 0;
            }
        },
        ViewState::Sessions => {
            state.ui.selected_session_index = Some(0);
        }
        ViewState::SessionDetail => match state.ui.focus {
            PanelFocus::Left => {
                state.ui.scroll_offsets.session_detail_left = 0;
            }
            PanelFocus::Right => {
                state.ui.scroll_offsets.session_detail_right = 0;
            }
        },
    }
}

fn jump_to_bottom(state: &mut AppState) {
    match state.ui.view {
        ViewState::Dashboard => match state.ui.focus {
            PanelFocus::Left => {
                state.ui.scroll_offsets.task_list = usize::MAX / 2;
            }
            PanelFocus::Right => {
                state.ui.scroll_offsets.event_stream = usize::MAX / 2;
            }
        },
        ViewState::AgentDetail => match state.ui.focus {
            PanelFocus::Left => {
                let count = state.domain.agents.len();
                if count > 0 {
                    state.ui.selected_agent_index = Some(count - 1);
                    state.ui.scroll_offsets.agent_events = 0;
                }
            }
            PanelFocus::Right => {
                state.ui.scroll_offsets.agent_events = usize::MAX / 2;
            }
        },
        ViewState::Sessions => {
            let count = state.domain.active_sessions.len() + state.domain.sessions.len();
            if count > 0 {
                state.ui.selected_session_index = Some(count - 1);
            }
        }
        ViewState::SessionDetail => match state.ui.focus {
            PanelFocus::Left => {
                state.ui.scroll_offsets.session_detail_left = usize::MAX / 2;
            }
            PanelFocus::Right => {
                state.ui.scroll_offsets.session_detail_right = usize::MAX / 2;
            }
        },
    }
}

fn drill_down(state: &mut AppState) {
    match state.ui.view {
        ViewState::Dashboard => {
            if let Some(task_idx) = state.ui.selected_task_index {
                if let Some(ref task_graph) = state.domain.task_graph {
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
                            state.ui.selected_agent_index = agent_idx;
                            state.ui.view = ViewState::AgentDetail;
                        }
                    }
                }
            }
        }
        ViewState::AgentDetail => {}
        ViewState::Sessions => {
            if let Some(idx) = state.ui.selected_session_index {
                let active_count = state.domain.active_sessions.len();
                if idx < active_count {
                    state.ui.view = ViewState::SessionDetail;
                    state.ui.scroll_offsets.session_detail_left = 0;
                    state.ui.scroll_offsets.session_detail_right = 0;
                    state.ui.focus = PanelFocus::Left;
                } else {
                    let archive_idx = idx - active_count;
                    if let Some(session) = state.domain.sessions.get(archive_idx) {
                        if session.data.is_some() {
                            state.ui.view = ViewState::SessionDetail;
                            state.ui.scroll_offsets.session_detail_left = 0;
                            state.ui.scroll_offsets.session_detail_right = 0;
                            state.ui.focus = PanelFocus::Left;
                        } else {
                            state.ui.loading_session = Some(archive_idx);
                        }
                    }
                }
            }
        }
        ViewState::SessionDetail => {}
    }
}

fn go_back(state: &mut AppState) {
    match state.ui.view {
        ViewState::AgentDetail => {
            state.ui.view = ViewState::Dashboard;
        }
        ViewState::Sessions => {
            state.ui.view = ViewState::Dashboard;
        }
        ViewState::SessionDetail => {
            state.ui.view = ViewState::Sessions;
        }
        ViewState::Dashboard => {}
    }
}

fn start_filter(state: &mut AppState) {
    state.ui.filter = Some(String::new());
}

fn toggle_help(state: &mut AppState) {
    state.ui.show_help = !state.ui.show_help;
}

fn toggle_auto_scroll(state: &mut AppState) {
    state.ui.auto_scroll = !state.ui.auto_scroll;
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
        let mut state = AppState::new();
        handle_key(&mut state, key(KeyCode::Char('q')));
        assert!(state.meta.should_quit);
    }

    #[test]
    fn key_1_switches_to_dashboard() {
        let mut state = AppState::new();
        state.ui.view = ViewState::Sessions;
        handle_key(&mut state, key(KeyCode::Char('1')));
        assert!(matches!(state.ui.view, ViewState::Dashboard));
    }

    #[test]
    fn key_3_switches_to_sessions() {
        let mut state = AppState::new();
        handle_key(&mut state, key(KeyCode::Char('3')));
        assert!(matches!(state.ui.view, ViewState::Sessions));
    }

    #[test]
    fn key_2_switches_to_agent_detail() {
        let mut state = AppState::new();
        handle_key(&mut state, key(KeyCode::Char('2')));
        assert!(matches!(state.ui.view, ViewState::AgentDetail));
    }

    #[test]
    fn key_2_auto_selects_first_agent() {
        let mut state = AppState::new();
        state.domain.agents.insert("a01".into(), Agent::new("a01".into(), Utc::now()));

        handle_key(&mut state, key(KeyCode::Char('2')));
        assert!(matches!(state.ui.view, ViewState::AgentDetail));
        assert_eq!(state.ui.selected_agent_index, Some(0));
    }

    #[test]
    fn key_2_no_auto_select_when_empty() {
        let mut state = AppState::new();
        handle_key(&mut state, key(KeyCode::Char('2')));
        assert_eq!(state.ui.selected_agent_index, None);
    }

    #[test]
    fn tab_toggles_focus() {
        let mut state = AppState::new();
        assert!(matches!(state.ui.focus, PanelFocus::Left));
        handle_key(&mut state, key(KeyCode::Tab));
        assert!(matches!(state.ui.focus, PanelFocus::Right));
        handle_key(&mut state, key(KeyCode::Tab));
        assert!(matches!(state.ui.focus, PanelFocus::Left));
    }

    #[test]
    fn l_switches_focus_to_right() {
        let mut state = AppState::new();
        handle_key(&mut state, key(KeyCode::Char('l')));
        assert!(matches!(state.ui.focus, PanelFocus::Right));
    }

    #[test]
    fn h_switches_focus_to_left() {
        let mut state = AppState::new();
        state.ui.focus = PanelFocus::Right;
        handle_key(&mut state, key(KeyCode::Char('h')));
        assert!(matches!(state.ui.focus, PanelFocus::Left));
    }

    #[test]
    fn j_scrolls_down_left_panel_in_dashboard() {
        let mut state = AppState::new();
        assert_eq!(state.ui.scroll_offsets.task_list, 0);
        handle_key(&mut state, key(KeyCode::Char('j')));
        assert_eq!(state.ui.scroll_offsets.task_list, 1);
    }

    #[test]
    fn down_arrow_scrolls_down() {
        let mut state = AppState::new();
        handle_key(&mut state, key(KeyCode::Down));
        assert_eq!(state.ui.scroll_offsets.task_list, 1);
    }

    #[test]
    fn k_scrolls_up_left_panel_in_dashboard() {
        let mut state = AppState::new();
        state.ui.scroll_offsets.task_list = 5;
        handle_key(&mut state, key(KeyCode::Char('k')));
        assert_eq!(state.ui.scroll_offsets.task_list, 4);
    }

    #[test]
    fn up_arrow_scrolls_up() {
        let mut state = AppState::new();
        state.ui.scroll_offsets.task_list = 5;
        handle_key(&mut state, key(KeyCode::Up));
        assert_eq!(state.ui.scroll_offsets.task_list, 4);
    }

    #[test]
    fn scroll_up_at_zero_stays_at_zero() {
        let mut state = AppState::new();
        assert_eq!(state.ui.scroll_offsets.task_list, 0);
        handle_key(&mut state, key(KeyCode::Char('k')));
        assert_eq!(state.ui.scroll_offsets.task_list, 0);
    }

    #[test]
    fn j_scrolls_right_panel_when_focused() {
        let mut state = AppState::new();
        state.ui.focus = PanelFocus::Right;
        assert_eq!(state.ui.scroll_offsets.event_stream, 0);
        handle_key(&mut state, key(KeyCode::Char('j')));
        assert_eq!(state.ui.scroll_offsets.event_stream, 1);
    }

    #[test]
    fn scroll_disables_auto_scroll_for_event_stream() {
        let mut state = AppState::new();
        state.ui.focus = PanelFocus::Right;
        state.ui.auto_scroll = true;
        handle_key(&mut state, key(KeyCode::Char('j')));
        assert!(!state.ui.auto_scroll);
    }

    #[test]
    fn j_moves_agent_selection_down() {
        let mut state = AppState::new();
        state.ui.view = ViewState::AgentDetail;
        state.ui.focus = PanelFocus::Left;
        state.domain.agents.insert("a01".into(), Agent::new("a01".into(), Utc::now()));
        state.domain.agents.insert("a02".into(), Agent::new("a02".into(), Utc::now()));
        state.ui.selected_agent_index = Some(0);
        handle_key(&mut state, key(KeyCode::Char('j')));
        assert_eq!(state.ui.selected_agent_index, Some(1));
    }

    #[test]
    fn j_clamps_agent_selection_at_max() {
        let mut state = AppState::new();
        state.ui.view = ViewState::AgentDetail;
        state.ui.focus = PanelFocus::Left;
        state.domain.agents.insert("a01".into(), Agent::new("a01".into(), Utc::now()));
        state.domain.agents.insert("a02".into(), Agent::new("a02".into(), Utc::now()));
        state.ui.selected_agent_index = Some(1);
        handle_key(&mut state, key(KeyCode::Char('j')));
        assert_eq!(state.ui.selected_agent_index, Some(1));
    }

    #[test]
    fn k_moves_agent_selection_up() {
        let mut state = AppState::new();
        state.ui.view = ViewState::AgentDetail;
        state.ui.focus = PanelFocus::Left;
        state.domain.agents.insert("a01".into(), Agent::new("a01".into(), Utc::now()));
        state.domain.agents.insert("a02".into(), Agent::new("a02".into(), Utc::now()));
        state.ui.selected_agent_index = Some(1);
        handle_key(&mut state, key(KeyCode::Char('k')));
        assert_eq!(state.ui.selected_agent_index, Some(0));
    }

    #[test]
    fn agent_selection_change_resets_event_scroll() {
        let mut state = AppState::new();
        state.ui.view = ViewState::AgentDetail;
        state.ui.focus = PanelFocus::Left;
        state.domain.agents.insert("a01".into(), Agent::new("a01".into(), Utc::now()));
        state.domain.agents.insert("a02".into(), Agent::new("a02".into(), Utc::now()));
        state.ui.selected_agent_index = Some(0);
        state.ui.scroll_offsets.agent_events = 15;
        handle_key(&mut state, key(KeyCode::Char('j')));
        assert_eq!(state.ui.scroll_offsets.agent_events, 0);
    }

    #[test]
    fn j_scrolls_agent_events_right_panel() {
        let mut state = AppState::new();
        state.ui.view = ViewState::AgentDetail;
        state.ui.focus = PanelFocus::Right;
        handle_key(&mut state, key(KeyCode::Char('j')));
        assert_eq!(state.ui.scroll_offsets.agent_events, 1);
    }

    #[test]
    fn k_scrolls_agent_events_right_panel() {
        let mut state = AppState::new();
        state.ui.view = ViewState::AgentDetail;
        state.ui.focus = PanelFocus::Right;
        state.ui.scroll_offsets.agent_events = 8;
        handle_key(&mut state, key(KeyCode::Char('k')));
        assert_eq!(state.ui.scroll_offsets.agent_events, 7);
    }

    #[test]
    fn scroll_in_sessions_view() {
        let mut state = AppState::new();
        state.ui.view = ViewState::Sessions;
        state.domain.sessions = vec![
            ArchivedSession::new(SessionMeta::new("s1".into(), Utc::now(), "/proj".into()), PathBuf::new()),
            ArchivedSession::new(SessionMeta::new("s2".into(), Utc::now(), "/proj".into()), PathBuf::new()),
        ];
        state.ui.selected_session_index = Some(0);
        handle_key(&mut state, key(KeyCode::Char('j')));
        assert_eq!(state.ui.selected_session_index, Some(1));
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
        state.domain.task_graph = Some(TaskGraph::new(vec![wave]));
        state.ui.selected_task_index = Some(0);
        state.domain.agents.insert("a04".into(), Agent::new("a04".into(), Utc::now()));
        state.recompute_sorted_keys();

        handle_key(&mut state, key(KeyCode::Enter));
        assert!(matches!(state.ui.view, ViewState::AgentDetail));
        assert_eq!(state.ui.selected_agent_index, Some(0));
    }

    #[test]
    fn enter_on_agent_detail_is_noop() {
        let mut state = AppState::new();
        state.ui.view = ViewState::AgentDetail;
        handle_key(&mut state, key(KeyCode::Enter));
        assert!(matches!(state.ui.view, ViewState::AgentDetail));
    }

    #[test]
    fn esc_on_agent_detail_goes_back_to_dashboard() {
        let mut state = AppState::new();
        state.ui.view = ViewState::AgentDetail;
        handle_key(&mut state, key(KeyCode::Esc));
        assert!(matches!(state.ui.view, ViewState::Dashboard));
    }

    #[test]
    fn esc_on_sessions_goes_back_to_dashboard() {
        let mut state = AppState::new();
        state.ui.view = ViewState::Sessions;
        handle_key(&mut state, key(KeyCode::Esc));
        assert!(matches!(state.ui.view, ViewState::Dashboard));
    }

    #[test]
    fn esc_on_dashboard_is_noop() {
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
        assert_eq!(state.ui.filter.unwrap(), "");
    }

    #[test]
    fn question_mark_toggles_help() {
        let mut state = AppState::new();
        assert!(!state.ui.show_help);
        handle_key(&mut state, key(KeyCode::Char('?')));
        assert!(state.ui.show_help);
        handle_key(&mut state, key(KeyCode::Char('?')));
        assert!(!state.ui.show_help);
    }

    #[test]
    fn space_toggles_auto_scroll() {
        let mut state = AppState::new();
        assert!(state.ui.auto_scroll);
        handle_key(&mut state, key(KeyCode::Char(' ')));
        assert!(!state.ui.auto_scroll);
        handle_key(&mut state, key(KeyCode::Char(' ')));
        assert!(state.ui.auto_scroll);
    }

    #[test]
    fn any_key_dismisses_help_overlay() {
        let mut state = AppState::new();
        state.ui.show_help = true;
        handle_key(&mut state, key(KeyCode::Char('a')));
        assert!(!state.ui.show_help);
    }

    #[test]
    fn esc_dismisses_filter_mode() {
        let mut state = AppState::new();
        state.ui.filter = Some("test".to_string());
        handle_key(&mut state, key(KeyCode::Esc));
        assert!(state.ui.filter.is_none());
    }

    #[test]
    fn char_appends_to_filter() {
        let mut state = AppState::new();
        state.ui.filter = Some("te".to_string());
        handle_key(&mut state, key(KeyCode::Char('s')));
        assert_eq!(state.ui.filter.unwrap(), "tes");
    }

    #[test]
    fn backspace_removes_from_filter() {
        let mut state = AppState::new();
        state.ui.filter = Some("test".to_string());
        handle_key(&mut state, key(KeyCode::Backspace));
        assert_eq!(state.ui.filter.unwrap(), "tes");
    }

    #[test]
    fn enter_keeps_filter_in_filter_mode() {
        let mut state = AppState::new();
        state.ui.filter = Some("test".to_string());
        handle_key(&mut state, key(KeyCode::Enter));
        assert_eq!(state.ui.filter.unwrap(), "test");
    }

    fn ctrl(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::CONTROL)
    }

    #[test]
    fn ctrl_d_page_scrolls_down_dashboard_left() {
        let mut state = AppState::new();
        handle_key(&mut state, ctrl(KeyCode::Char('d')));
        assert_eq!(state.ui.scroll_offsets.task_list, PAGE_JUMP);
    }

    #[test]
    fn ctrl_u_page_scrolls_up_dashboard_left() {
        let mut state = AppState::new();
        state.ui.scroll_offsets.task_list = 30;
        handle_key(&mut state, ctrl(KeyCode::Char('u')));
        assert_eq!(state.ui.scroll_offsets.task_list, 30 - PAGE_JUMP);
    }

    #[test]
    fn ctrl_d_page_scrolls_down_dashboard_right() {
        let mut state = AppState::new();
        state.ui.focus = PanelFocus::Right;
        handle_key(&mut state, ctrl(KeyCode::Char('d')));
        assert_eq!(state.ui.scroll_offsets.event_stream, PAGE_JUMP);
        assert!(!state.ui.auto_scroll);
    }

    #[test]
    fn ctrl_u_page_scrolls_up_saturates_at_zero() {
        let mut state = AppState::new();
        handle_key(&mut state, ctrl(KeyCode::Char('u')));
        assert_eq!(state.ui.scroll_offsets.task_list, 0);
    }

    #[test]
    fn ctrl_d_page_scrolls_session_detail() {
        let mut state = AppState::new();
        state.ui.view = ViewState::SessionDetail;
        state.ui.focus = PanelFocus::Right;
        handle_key(&mut state, ctrl(KeyCode::Char('d')));
        assert_eq!(state.ui.scroll_offsets.session_detail_right, PAGE_JUMP);
    }

    #[test]
    fn ctrl_u_page_scrolls_session_detail() {
        let mut state = AppState::new();
        state.ui.view = ViewState::SessionDetail;
        state.ui.focus = PanelFocus::Left;
        state.ui.scroll_offsets.session_detail_left = 25;
        handle_key(&mut state, ctrl(KeyCode::Char('u')));
        assert_eq!(state.ui.scroll_offsets.session_detail_left, 25 - PAGE_JUMP);
    }

    #[test]
    fn unknown_key_is_noop() {
        let mut state = AppState::new();
        handle_key(&mut state, key(KeyCode::F(1)));
        assert!(matches!(state.ui.view, ViewState::Dashboard));
        assert!(!state.meta.should_quit);
    }

    #[test]
    fn g_jumps_to_top_dashboard_left() {
        let mut state = AppState::new();
        state.ui.scroll_offsets.task_list = 50;
        handle_key(&mut state, key(KeyCode::Char('g')));
        assert_eq!(state.ui.scroll_offsets.task_list, 0);
        assert_eq!(state.ui.selected_task_index, Some(0));
    }

    #[test]
    fn g_uppercase_jumps_to_bottom_dashboard_left() {
        let mut state = AppState::new();
        handle_key(&mut state, key(KeyCode::Char('G')));
        assert!(state.ui.scroll_offsets.task_list > 1000);
    }

    #[test]
    fn g_jumps_to_top_sessions() {
        let mut state = AppState::new();
        state.ui.view = ViewState::Sessions;
        state.ui.selected_session_index = Some(5);
        handle_key(&mut state, key(KeyCode::Char('g')));
        assert_eq!(state.ui.selected_session_index, Some(0));
    }

    #[test]
    fn g_uppercase_jumps_to_bottom_sessions() {
        let mut state = AppState::new();
        state.ui.view = ViewState::Sessions;
        state.domain.sessions = vec![
            ArchivedSession::new(SessionMeta::new("s1".into(), Utc::now(), "/p".into()), PathBuf::new()),
            ArchivedSession::new(SessionMeta::new("s2".into(), Utc::now(), "/p".into()), PathBuf::new()),
            ArchivedSession::new(SessionMeta::new("s3".into(), Utc::now(), "/p".into()), PathBuf::new()),
        ];
        handle_key(&mut state, key(KeyCode::Char('G')));
        assert_eq!(state.ui.selected_session_index, Some(2));
    }

    #[test]
    fn g_jumps_to_top_agent_detail() {
        let mut state = AppState::new();
        state.ui.view = ViewState::AgentDetail;
        state.domain.agents.insert("a01".into(), Agent::new("a01".into(), Utc::now()));
        state.domain.agents.insert("a02".into(), Agent::new("a02".into(), Utc::now()));
        state.ui.selected_agent_index = Some(1);
        state.ui.scroll_offsets.agent_events = 20;
        handle_key(&mut state, key(KeyCode::Char('g')));
        assert_eq!(state.ui.selected_agent_index, Some(0));
        assert_eq!(state.ui.scroll_offsets.agent_events, 0);
    }

    #[test]
    fn multiple_tasks_drill_down_selects_correct_agent() {
        let now = Utc::now();
        let mut state = AppState::new();
        state.domain.agents.insert(
            "a01".into(),
            Agent::new("a01".into(), now - chrono::Duration::seconds(10)),
        );
        state.domain.agents.insert("a02".into(), Agent::new("a02".into(), now));
        state.recompute_sorted_keys();

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
        state.domain.task_graph = Some(TaskGraph::new(vec![wave]));
        state.ui.selected_task_index = Some(1); // Task 2 → agent a02

        handle_key(&mut state, key(KeyCode::Enter));
        assert!(matches!(state.ui.view, ViewState::AgentDetail));
        // sorted_agent_keys: [a02 (newest), a01 (oldest)] → a02 is at index 0
        assert_eq!(state.ui.selected_agent_index, Some(0));
    }
}
