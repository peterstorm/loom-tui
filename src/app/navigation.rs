use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::app::{AppState, PanelFocus, TaskViewMode, ViewState};

/// Jump size for Ctrl+D / Ctrl+U (fixed at 20 lines).
const PAGE_JUMP: usize = 20;

/// Key event handler. Mutates state in place.
pub fn handle_key(state: &mut AppState, key: KeyEvent) {
    // Help overlay has priority
    if state.ui.show_help {
        handle_help_key(state, key);
        return;
    }

    // Agent popup has second priority
    if state.ui.show_agent_popup.is_some() {
        handle_popup_key(state, key);
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
        KeyCode::Char('p') => show_agent_popup(state),
        KeyCode::Char('v') => toggle_task_view_mode(state),
        KeyCode::Char('?') => toggle_help(state),
        KeyCode::Char(' ') => toggle_auto_scroll(state),
        _ => {}
    }
}

fn handle_help_key(state: &mut AppState, _key: KeyEvent) {
    state.ui.show_help = false;
}

fn handle_popup_key(state: &mut AppState, key: KeyEvent) {
    match key.code {
        KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('p') => {
            state.ui.show_agent_popup = None;
        }
        _ => {}
    }
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
    state.domain.task_graph.as_ref().map(|g| g.total_tasks()).unwrap_or(0)
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

/// Returns mutable reference to the active scroll offset for current view+focus.
fn active_scroll_offset_mut(state: &mut AppState) -> &mut usize {
    match (&state.ui.view, &state.ui.focus) {
        (ViewState::Dashboard, PanelFocus::Left) => &mut state.ui.scroll_offsets.task_list,
        (ViewState::Dashboard, PanelFocus::Right) => &mut state.ui.scroll_offsets.event_stream,
        (ViewState::AgentDetail, _) => &mut state.ui.scroll_offsets.agent_events,
        (ViewState::Sessions, _) => &mut state.ui.scroll_offsets.task_list, // unused, Sessions uses selected_session_index
        (ViewState::SessionDetail, PanelFocus::Left) => &mut state.ui.scroll_offsets.session_detail_left,
        (ViewState::SessionDetail, PanelFocus::Right) => &mut state.ui.scroll_offsets.session_detail_right,
    }
}

/// Returns item count for current view+focus (for bounds checking).
fn item_count(state: &AppState) -> Option<usize> {
    match (&state.ui.view, &state.ui.focus) {
        (ViewState::Dashboard, PanelFocus::Left) => Some(task_count(state)),
        (ViewState::AgentDetail, PanelFocus::Left) => Some(state.domain.agents.len()),
        (ViewState::Sessions, _) => Some(state.domain.active_sessions.len() + state.domain.sessions.len()),
        _ => None,
    }
}

/// Returns true if scrolling in current view+focus should disable auto_scroll.
fn disables_auto_scroll(state: &AppState) -> bool {
    matches!(
        (&state.ui.view, &state.ui.focus),
        (ViewState::Dashboard, PanelFocus::Right) | (ViewState::AgentDetail, PanelFocus::Right)
    )
}

fn scroll_down(state: &mut AppState) {
    match (&state.ui.view, &state.ui.focus) {
        (ViewState::Dashboard, PanelFocus::Left) => {
            *active_scroll_offset_mut(state) = active_scroll_offset_mut(state).saturating_add(1);
            let max = task_count(state).saturating_sub(1);
            let current = state.ui.selected_task_index.unwrap_or(0);
            state.ui.selected_task_index = Some(current.saturating_add(1).min(max));
        }
        (ViewState::AgentDetail, PanelFocus::Left) => {
            if let Some(count) = item_count(state) {
                if count > 0 {
                    let current = state.ui.selected_agent_index.unwrap_or(0);
                    let new_idx = (current + 1).min(count - 1);
                    if new_idx != current {
                        state.ui.scroll_offsets.agent_events = 0;
                    }
                    state.ui.selected_agent_index = Some(new_idx);
                }
            }
        }
        (ViewState::Sessions, _) => {
            if let Some(count) = item_count(state) {
                if count > 0 {
                    let current = state.ui.selected_session_index.unwrap_or(0);
                    state.ui.selected_session_index = Some((current + 1).min(count - 1));
                }
            }
        }
        _ => {
            *active_scroll_offset_mut(state) = active_scroll_offset_mut(state).saturating_add(1);
        }
    }
    if disables_auto_scroll(state) {
        state.ui.auto_scroll = false;
    }
}

fn scroll_up(state: &mut AppState) {
    match (&state.ui.view, &state.ui.focus) {
        (ViewState::Dashboard, PanelFocus::Left) => {
            *active_scroll_offset_mut(state) = active_scroll_offset_mut(state).saturating_sub(1);
            let current = state.ui.selected_task_index.unwrap_or(0);
            state.ui.selected_task_index = Some(current.saturating_sub(1));
        }
        (ViewState::AgentDetail, PanelFocus::Left) => {
            let current = state.ui.selected_agent_index.unwrap_or(0);
            let new_idx = current.saturating_sub(1);
            if new_idx != current {
                state.ui.scroll_offsets.agent_events = 0;
            }
            state.ui.selected_agent_index = Some(new_idx);
        }
        (ViewState::Sessions, _) => {
            let current = state.ui.selected_session_index.unwrap_or(0);
            state.ui.selected_session_index = Some(current.saturating_sub(1));
        }
        _ => {
            *active_scroll_offset_mut(state) = active_scroll_offset_mut(state).saturating_sub(1);
        }
    }
    if disables_auto_scroll(state) {
        state.ui.auto_scroll = false;
    }
}

fn scroll_page_down(state: &mut AppState) {
    match (&state.ui.view, &state.ui.focus) {
        (ViewState::Dashboard, PanelFocus::Left) => {
            *active_scroll_offset_mut(state) = active_scroll_offset_mut(state).saturating_add(PAGE_JUMP);
        }
        (ViewState::AgentDetail, PanelFocus::Left) => {
            if let Some(count) = item_count(state) {
                if count > 0 {
                    let current = state.ui.selected_agent_index.unwrap_or(0);
                    let new_idx = (current + PAGE_JUMP).min(count - 1);
                    if new_idx != current {
                        state.ui.scroll_offsets.agent_events = 0;
                    }
                    state.ui.selected_agent_index = Some(new_idx);
                }
            }
        }
        (ViewState::Sessions, _) => {
            if let Some(count) = item_count(state) {
                if count > 0 {
                    let current = state.ui.selected_session_index.unwrap_or(0);
                    state.ui.selected_session_index = Some((current + PAGE_JUMP).min(count - 1));
                }
            }
        }
        _ => {
            *active_scroll_offset_mut(state) = active_scroll_offset_mut(state).saturating_add(PAGE_JUMP);
        }
    }
    if disables_auto_scroll(state) {
        state.ui.auto_scroll = false;
    }
}

fn scroll_page_up(state: &mut AppState) {
    match (&state.ui.view, &state.ui.focus) {
        (ViewState::Dashboard, PanelFocus::Left) => {
            *active_scroll_offset_mut(state) = active_scroll_offset_mut(state).saturating_sub(PAGE_JUMP);
        }
        (ViewState::AgentDetail, PanelFocus::Left) => {
            let current = state.ui.selected_agent_index.unwrap_or(0);
            let new_idx = current.saturating_sub(PAGE_JUMP);
            if new_idx != current {
                state.ui.scroll_offsets.agent_events = 0;
            }
            state.ui.selected_agent_index = Some(new_idx);
        }
        (ViewState::Sessions, _) => {
            let current = state.ui.selected_session_index.unwrap_or(0);
            state.ui.selected_session_index = Some(current.saturating_sub(PAGE_JUMP));
        }
        _ => {
            *active_scroll_offset_mut(state) = active_scroll_offset_mut(state).saturating_sub(PAGE_JUMP);
        }
    }
    if disables_auto_scroll(state) {
        state.ui.auto_scroll = false;
    }
}

fn jump_to_top(state: &mut AppState) {
    match (&state.ui.view, &state.ui.focus) {
        (ViewState::Dashboard, PanelFocus::Left) => {
            *active_scroll_offset_mut(state) = 0;
            state.ui.selected_task_index = Some(0);
        }
        (ViewState::AgentDetail, PanelFocus::Left) => {
            if !state.domain.agents.is_empty() {
                state.ui.selected_agent_index = Some(0);
                state.ui.scroll_offsets.agent_events = 0;
            }
        }
        (ViewState::Sessions, _) => {
            state.ui.selected_session_index = Some(0);
        }
        _ => {
            *active_scroll_offset_mut(state) = 0;
        }
    }
}

fn jump_to_bottom(state: &mut AppState) {
    match (&state.ui.view, &state.ui.focus) {
        (ViewState::Dashboard, PanelFocus::Left) => {
            *active_scroll_offset_mut(state) = usize::MAX / 2;
        }
        (ViewState::AgentDetail, PanelFocus::Left) => {
            if let Some(count) = item_count(state) {
                if count > 0 {
                    state.ui.selected_agent_index = Some(count - 1);
                    state.ui.scroll_offsets.agent_events = 0;
                }
            }
        }
        (ViewState::Sessions, _) => {
            if let Some(count) = item_count(state) {
                if count > 0 {
                    state.ui.selected_session_index = Some(count - 1);
                }
            }
        }
        _ => {
            *active_scroll_offset_mut(state) = usize::MAX / 2;
        }
    }
}

fn drill_down(state: &mut AppState) {
    match state.ui.view {
        ViewState::Dashboard => {
            if let Some(task_idx) = state.ui.selected_task_index {
                if let Some(ref task_graph) = state.domain.task_graph {
                    let all_tasks: Vec<_> = task_graph.flat_tasks().collect();

                    if let Some(task) = all_tasks.get(task_idx) {
                        if let Some(ref agent_id) = task.agent_id {
                            let agent_idx = state
                                .sorted_agent_keys()
                                .iter()
                                .position(|k| *k == *agent_id);
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

fn show_agent_popup(state: &mut AppState) {
    // Show popup for selected task's agent (Dashboard only)
    if !matches!(state.ui.view, ViewState::Dashboard) {
        return;
    }

    if let Some(task_idx) = state.ui.selected_task_index {
        if let Some(ref task_graph) = state.domain.task_graph {
            let all_tasks: Vec<_> = task_graph.flat_tasks().collect();

            if let Some(task) = all_tasks.get(task_idx) {
                if let Some(ref agent_id) = task.agent_id {
                    state.ui.show_agent_popup = Some(agent_id.clone());
                }
            }
        }
    }
}

fn toggle_task_view_mode(state: &mut AppState) {
    // Only toggle in Dashboard view
    if !matches!(state.ui.view, ViewState::Dashboard) {
        return;
    }

    state.ui.task_view_mode = match state.ui.task_view_mode {
        TaskViewMode::Wave => TaskViewMode::Kanban,
        TaskViewMode::Kanban => TaskViewMode::Wave,
    };

    // Reset task selection when switching modes
    state.ui.selected_task_index = Some(0);
    state.ui.scroll_offsets.task_list = 0;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Agent, AgentId, ArchivedSession, SessionMeta, Task, TaskId, TaskGraph, TaskStatus, Wave};
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
        state.domain.agents.insert(AgentId::new("a01"), Agent::new("a01", Utc::now()));

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
        state.domain.agents.insert(AgentId::new("a01"), Agent::new("a01", Utc::now()));
        state.domain.agents.insert(AgentId::new("a02"), Agent::new("a02", Utc::now()));
        state.ui.selected_agent_index = Some(0);
        handle_key(&mut state, key(KeyCode::Char('j')));
        assert_eq!(state.ui.selected_agent_index, Some(1));
    }

    #[test]
    fn j_clamps_agent_selection_at_max() {
        let mut state = AppState::new();
        state.ui.view = ViewState::AgentDetail;
        state.ui.focus = PanelFocus::Left;
        state.domain.agents.insert(AgentId::new("a01"), Agent::new("a01", Utc::now()));
        state.domain.agents.insert(AgentId::new("a02"), Agent::new("a02", Utc::now()));
        state.ui.selected_agent_index = Some(1);
        handle_key(&mut state, key(KeyCode::Char('j')));
        assert_eq!(state.ui.selected_agent_index, Some(1));
    }

    #[test]
    fn k_moves_agent_selection_up() {
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
    fn agent_selection_change_resets_event_scroll() {
        let mut state = AppState::new();
        state.ui.view = ViewState::AgentDetail;
        state.ui.focus = PanelFocus::Left;
        state.domain.agents.insert(AgentId::new("a01"), Agent::new("a01", Utc::now()));
        state.domain.agents.insert(AgentId::new("a02"), Agent::new("a02", Utc::now()));
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
            ArchivedSession::new(SessionMeta::new("s1", Utc::now(), "/proj".to_string()), PathBuf::new()),
            ArchivedSession::new(SessionMeta::new("s2", Utc::now(), "/proj".to_string()), PathBuf::new()),
        ];
        state.ui.selected_session_index = Some(0);
        handle_key(&mut state, key(KeyCode::Char('j')));
        assert_eq!(state.ui.selected_session_index, Some(1));
    }

    #[test]
    fn enter_on_dashboard_drills_into_agent_detail() {
        let mut state = AppState::new();
        let task = Task {
            id: TaskId::new("T1"),
            description: "Test".to_string(),
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
            ArchivedSession::new(SessionMeta::new("s1", Utc::now(), "/p".to_string()), PathBuf::new()),
            ArchivedSession::new(SessionMeta::new("s2", Utc::now(), "/p".to_string()), PathBuf::new()),
            ArchivedSession::new(SessionMeta::new("s3", Utc::now(), "/p".to_string()), PathBuf::new()),
        ];
        handle_key(&mut state, key(KeyCode::Char('G')));
        assert_eq!(state.ui.selected_session_index, Some(2));
    }

    #[test]
    fn g_jumps_to_top_agent_detail() {
        let mut state = AppState::new();
        state.ui.view = ViewState::AgentDetail;
        state.domain.agents.insert(AgentId::new("a01"), Agent::new("a01", Utc::now()));
        state.domain.agents.insert(AgentId::new("a02"), Agent::new("a02", Utc::now()));
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
            Agent::new("a01", now - chrono::Duration::seconds(10)),
        );
        state.domain.agents.insert(AgentId::new("a02"), Agent::new("a02", now));
        state.recompute_sorted_keys();

        let tasks = vec![
            Task {
                id: TaskId::new("T1"),
                description: "Task 1".to_string(),
                agent_id: Some(AgentId::new("a01")),
                status: TaskStatus::Running,
                review_status: Default::default(),
                files_modified: vec![],
                tests_passed: None,
            },
            Task {
                id: TaskId::new("T2"),
                description: "Task 2".to_string(),
                agent_id: Some(AgentId::new("a02")),
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

    #[test]
    fn show_agent_popup_with_no_agent_id() {
        let mut state = AppState::new();
        state.ui.view = ViewState::Dashboard;

        let task = Task {
            id: TaskId::new("T1"),
            description: "Unassigned task".to_string(),
            agent_id: None, // No agent assigned
            status: TaskStatus::Pending,
            review_status: Default::default(),
            files_modified: vec![],
            tests_passed: None,
        };
        state.domain.task_graph = Some(TaskGraph::new(vec![Wave::new(1, vec![task])]));
        state.ui.selected_task_index = Some(0);

        handle_key(&mut state, key(KeyCode::Char('p')));
        // Should be no-op since task has no agent_id
        assert_eq!(state.ui.show_agent_popup, None);
    }

    #[test]
    fn show_agent_popup_task_index_out_of_bounds() {
        let mut state = AppState::new();
        state.ui.view = ViewState::Dashboard;
        state.domain.task_graph = Some(TaskGraph::new(vec![Wave::new(1, vec![])]));
        state.ui.selected_task_index = Some(99); // Out of bounds

        handle_key(&mut state, key(KeyCode::Char('p')));
        assert_eq!(state.ui.show_agent_popup, None);
    }

    #[test]
    fn show_agent_popup_only_works_in_dashboard() {
        let mut state = AppState::new();
        state.ui.view = ViewState::Sessions; // Not Dashboard

        let mut task = Task::new("T1", "Task".into(), TaskStatus::Running);
        task.agent_id = Some(AgentId::new("a01"));
        state.domain.task_graph = Some(TaskGraph::new(vec![Wave::new(1, vec![task])]));
        state.ui.selected_task_index = Some(0);

        handle_key(&mut state, key(KeyCode::Char('p')));
        // Should be no-op since not in Dashboard
        assert_eq!(state.ui.show_agent_popup, None);
    }

    #[test]
    fn show_agent_popup_no_task_graph() {
        let mut state = AppState::new();
        state.ui.view = ViewState::Dashboard;
        state.domain.task_graph = None;
        state.ui.selected_task_index = Some(0);

        handle_key(&mut state, key(KeyCode::Char('p')));
        assert_eq!(state.ui.show_agent_popup, None);
    }

    #[test]
    fn toggle_task_view_mode_resets_selection() {
        let mut state = AppState::new();
        state.ui.view = ViewState::Dashboard;
        state.ui.selected_task_index = Some(5);
        state.ui.task_view_mode = crate::app::TaskViewMode::Wave;

        handle_key(&mut state, key(KeyCode::Char('v')));
        assert_eq!(state.ui.selected_task_index, Some(0)); // Reset to 0
    }

    #[test]
    fn toggle_task_view_mode_resets_scroll() {
        let mut state = AppState::new();
        state.ui.view = ViewState::Dashboard;
        state.ui.scroll_offsets.task_list = 10;
        state.ui.task_view_mode = crate::app::TaskViewMode::Wave;

        handle_key(&mut state, key(KeyCode::Char('v')));
        assert_eq!(state.ui.scroll_offsets.task_list, 0); // Reset to 0
    }

    #[test]
    fn toggle_task_view_mode_only_in_dashboard() {
        let mut state = AppState::new();
        state.ui.view = ViewState::Sessions;
        state.ui.task_view_mode = crate::app::TaskViewMode::Wave;

        handle_key(&mut state, key(KeyCode::Char('v')));
        // Should still be Wave (no toggle happened)
        assert_eq!(state.ui.task_view_mode, crate::app::TaskViewMode::Wave);
    }

    #[test]
    fn handle_popup_key_escape_dismisses() {
        let mut state = AppState::new();
        state.ui.show_agent_popup = Some(AgentId::new("a01"));

        handle_key(&mut state, key(KeyCode::Esc));
        assert_eq!(state.ui.show_agent_popup, None);
    }

    #[test]
    fn handle_popup_key_q_dismisses() {
        let mut state = AppState::new();
        state.ui.show_agent_popup = Some(AgentId::new("a01"));

        handle_key(&mut state, key(KeyCode::Char('q')));
        assert_eq!(state.ui.show_agent_popup, None);
    }

    #[test]
    fn handle_popup_key_p_dismisses() {
        let mut state = AppState::new();
        state.ui.show_agent_popup = Some(AgentId::new("a01"));

        handle_key(&mut state, key(KeyCode::Char('p')));
        assert_eq!(state.ui.show_agent_popup, None);
    }

    #[test]
    fn handle_popup_key_other_keys_ignored() {
        let mut state = AppState::new();
        state.ui.show_agent_popup = Some(AgentId::new("a01"));

        handle_key(&mut state, key(KeyCode::Char('x')));
        // Popup should still be open
        assert_eq!(state.ui.show_agent_popup, Some(AgentId::new("a01")));
    }
}
