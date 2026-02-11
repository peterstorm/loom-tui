use crossterm::event::{KeyCode, KeyEvent};

use crate::app::{AppState, PanelFocus, ViewState};

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
            state
        }
        KeyCode::Tab | KeyCode::Char('l') => toggle_focus_right(state),
        KeyCode::Char('h') => toggle_focus_left(state),
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

/// Switch to AgentDetail view if an agent is selected.
/// If no task is selected or task has no agent, no-op.
fn switch_to_agent_detail(mut state: AppState) -> AppState {
    // Get selected task from current view
    if let Some(task_idx) = state.selected_task_index {
        if let Some(ref task_graph) = state.task_graph {
            // Flatten all tasks across waves to find selected task
            let all_tasks: Vec<_> = task_graph
                .waves
                .iter()
                .flat_map(|w| &w.tasks)
                .collect();

            if let Some(task) = all_tasks.get(task_idx) {
                if let Some(ref agent_id) = task.agent_id {
                    state.view = ViewState::AgentDetail {
                        agent_id: agent_id.clone(),
                    };
                }
            }
        }
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
                // Disable auto-scroll when manually scrolling
                state.auto_scroll = false;
            }
        },
        ViewState::AgentDetail { .. } => match state.focus {
            PanelFocus::Left => {
                state.scroll_offsets.tool_calls =
                    state.scroll_offsets.tool_calls.saturating_add(1);
            }
            PanelFocus::Right => {
                state.scroll_offsets.reasoning =
                    state.scroll_offsets.reasoning.saturating_add(1);
            }
        },
        ViewState::Sessions => {
            state.scroll_offsets.sessions = state.scroll_offsets.sessions.saturating_add(1);
        }
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
                // Disable auto-scroll when manually scrolling
                state.auto_scroll = false;
            }
        },
        ViewState::AgentDetail { .. } => match state.focus {
            PanelFocus::Left => {
                state.scroll_offsets.tool_calls =
                    state.scroll_offsets.tool_calls.saturating_sub(1);
            }
            PanelFocus::Right => {
                state.scroll_offsets.reasoning =
                    state.scroll_offsets.reasoning.saturating_sub(1);
            }
        },
        ViewState::Sessions => {
            state.scroll_offsets.sessions = state.scroll_offsets.sessions.saturating_sub(1);
        }
    }
    state
}

/// Drill down into selected item.
/// Dashboard: Enter on task with agent -> switch to AgentDetail
/// AgentDetail: no drill-down (no-op)
/// Sessions: Enter on session -> load session
fn drill_down(mut state: AppState) -> AppState {
    match state.view {
        ViewState::Dashboard => {
            // Enter on a task with agent -> switch to agent detail
            if let Some(task_idx) = state.selected_task_index {
                if let Some(ref task_graph) = state.task_graph {
                    let all_tasks: Vec<_> = task_graph
                        .waves
                        .iter()
                        .flat_map(|w| &w.tasks)
                        .collect();

                    if let Some(task) = all_tasks.get(task_idx) {
                        if let Some(ref agent_id) = task.agent_id {
                            state.view = ViewState::AgentDetail {
                                agent_id: agent_id.clone(),
                            };
                        }
                    }
                }
            }
        }
        ViewState::AgentDetail { .. } => {
            // No drill-down in agent detail view
        }
        ViewState::Sessions => {
            // Enter on session would trigger session load
            // This is handled by side effects in main loop, not here
        }
    }
    state
}

/// Navigate back to previous view.
/// AgentDetail -> Dashboard
/// Sessions -> Dashboard
/// Dashboard -> no-op
fn go_back(mut state: AppState) -> AppState {
    match state.view {
        ViewState::AgentDetail { .. } => {
            state.view = ViewState::Dashboard;
        }
        ViewState::Sessions => {
            state.view = ViewState::Dashboard;
        }
        ViewState::Dashboard => {
            // Already at top level, no-op
        }
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
    use crate::model::{Task, TaskGraph, TaskStatus, Wave};
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
    fn key_2_switches_to_agent_detail_if_task_selected() {
        let mut state = AppState::new();

        // Create task graph with agent
        let task = Task {
            id: "T1".to_string(),
            description: "Test task".to_string(),
            agent_id: Some("a04".to_string()),
            status: TaskStatus::Running,
            review_status: Default::default(),
            files_modified: vec![],
            tests_passed: None,
        };
        let wave = Wave::new(1, vec![task]);
        state.task_graph = Some(TaskGraph::new(vec![wave]));
        state.selected_task_index = Some(0);

        let new_state = handle_key(state, key(KeyCode::Char('2')));
        assert!(matches!(
            new_state.view,
            ViewState::AgentDetail { agent_id } if agent_id == "a04"
        ));
    }

    #[test]
    fn key_2_no_op_if_no_task_selected() {
        let mut state = AppState::new();
        state.task_graph = Some(TaskGraph::empty());
        state.selected_task_index = None;

        let new_state = handle_key(state, key(KeyCode::Char('2')));
        assert!(matches!(new_state.view, ViewState::Dashboard));
    }

    #[test]
    fn key_2_no_op_if_task_has_no_agent() {
        let mut state = AppState::new();

        let task = Task::new("T1".to_string(), "Test".to_string(), TaskStatus::Pending);
        let wave = Wave::new(1, vec![task]);
        state.task_graph = Some(TaskGraph::new(vec![wave]));
        state.selected_task_index = Some(0);

        let new_state = handle_key(state, key(KeyCode::Char('2')));
        assert!(matches!(new_state.view, ViewState::Dashboard));
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
    fn scroll_in_agent_detail_tool_calls() {
        let mut state = AppState::new();
        state.view = ViewState::AgentDetail {
            agent_id: "a04".to_string(),
        };
        state.focus = PanelFocus::Left;

        let new_state = handle_key(state, key(KeyCode::Char('j')));
        assert_eq!(new_state.scroll_offsets.tool_calls, 1);
    }

    #[test]
    fn scroll_in_agent_detail_reasoning() {
        let mut state = AppState::new();
        state.view = ViewState::AgentDetail {
            agent_id: "a04".to_string(),
        };
        state.focus = PanelFocus::Right;

        let new_state = handle_key(state, key(KeyCode::Char('j')));
        assert_eq!(new_state.scroll_offsets.reasoning, 1);
    }

    #[test]
    fn scroll_in_sessions_view() {
        let mut state = AppState::new();
        state.view = ViewState::Sessions;

        let new_state = handle_key(state, key(KeyCode::Char('j')));
        assert_eq!(new_state.scroll_offsets.sessions, 1);
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

        let new_state = handle_key(state, key(KeyCode::Enter));
        assert!(matches!(
            new_state.view,
            ViewState::AgentDetail { agent_id } if agent_id == "a04"
        ));
    }

    #[test]
    fn enter_on_agent_detail_is_noop() {
        let mut state = AppState::new();
        state.view = ViewState::AgentDetail {
            agent_id: "a04".to_string(),
        };

        let new_state = handle_key(state, key(KeyCode::Enter));
        assert!(matches!(
            new_state.view,
            ViewState::AgentDetail { agent_id } if agent_id == "a04"
        ));
    }

    #[test]
    fn esc_on_agent_detail_goes_back_to_dashboard() {
        let mut state = AppState::new();
        state.view = ViewState::AgentDetail {
            agent_id: "a04".to_string(),
        };

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

    #[test]
    fn unknown_key_is_noop() {
        let state = AppState::new();
        let new_state = handle_key(state, key(KeyCode::F(1)));
        assert!(matches!(new_state.view, ViewState::Dashboard));
        assert!(!new_state.should_quit);
    }

    #[test]
    fn multiple_tasks_drill_down_correct_task() {
        let mut state = AppState::new();

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
        assert!(matches!(
            new_state.view,
            ViewState::AgentDetail { agent_id } if agent_id == "a02"
        ));
    }
}
