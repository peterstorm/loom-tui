use loom_tui::{
    app::{update, AppState, HookStatus, ViewState},
    event::AppEvent,
};
use std::time::Duration;

/// Integration tests for main event loop logic.
/// These test the event processing without terminal I/O.

#[test]
fn event_loop_processes_quit_signal() {
    let mut state = AppState::new();
    assert!(!state.should_quit);

    // Simulate 'q' key press
    let key = crossterm::event::KeyEvent::from(crossterm::event::KeyCode::Char('q'));
    state = update(state, AppEvent::Key(key));

    // State should have quit flag set
    assert!(state.should_quit);
}

#[test]
fn event_loop_processes_multiple_events_in_sequence() {
    let mut state = AppState::new();

    // Press '3' to go to sessions view
    let key = crossterm::event::KeyEvent::from(crossterm::event::KeyCode::Char('3'));
    state = update(state, AppEvent::Key(key));
    assert!(matches!(state.view, ViewState::Sessions));

    // Press '1' to go back to dashboard
    let key = crossterm::event::KeyEvent::from(crossterm::event::KeyCode::Char('1'));
    state = update(state, AppEvent::Key(key));
    assert!(matches!(state.view, ViewState::Dashboard));

    // Press 'q' to quit
    let key = crossterm::event::KeyEvent::from(crossterm::event::KeyCode::Char('q'));
    state = update(state, AppEvent::Key(key));
    assert!(state.should_quit);
}

#[test]
fn event_loop_tick_is_passive() {
    let state = AppState::new();
    let initial_events_len = state.events.len();

    // Process tick event
    let new_state = update(state, AppEvent::Tick);

    // State should be unchanged (tick is passive)
    assert_eq!(new_state.events.len(), initial_events_len);
    assert!(!new_state.should_quit);
}

#[test]
fn event_loop_hook_status_transitions() {
    // Start with unknown status
    let mut state = AppState::new();
    assert!(matches!(state.hook_status, HookStatus::Unknown));

    // Simulate hook detection result
    state.hook_status = HookStatus::Missing;
    assert!(matches!(state.hook_status, HookStatus::Missing));

    // Simulate successful installation
    state.hook_status = HookStatus::Installed;
    assert!(matches!(state.hook_status, HookStatus::Installed));
}

#[test]
fn event_loop_handles_watcher_events() {
    use chrono::Utc;
    use loom_tui::model::{HookEvent, HookEventKind, TaskGraph, Wave};

    let mut state = AppState::new();

    // Simulate task graph update from file watcher
    let graph = TaskGraph::new(vec![Wave::new(1, vec![])]);
    state = update(state, AppEvent::TaskGraphUpdated(graph));
    assert!(state.task_graph.is_some());

    // Simulate agent start from file watcher
    state = update(state, AppEvent::AgentStarted("a01".to_string()));
    assert!(state.agents.contains_key("a01"));

    // Simulate hook event from file watcher
    let hook_event = HookEvent::new(Utc::now(), HookEventKind::SessionStart);
    state = update(state, AppEvent::HookEventReceived(hook_event));
    assert_eq!(state.events.len(), 1);
}

#[test]
fn tick_rate_configuration() {
    let tick_rate = Duration::from_millis(250);
    assert_eq!(tick_rate.as_millis(), 250);

    // Verify tick rate is not too fast (performance) or too slow (responsiveness)
    assert!(tick_rate >= Duration::from_millis(100));
    assert!(tick_rate <= Duration::from_millis(1000));
}

#[test]
fn event_loop_drains_multiple_watcher_events() {
    let mut state = AppState::new();

    // Simulate multiple events from watcher channel
    state = update(state, AppEvent::AgentStarted("a01".to_string()));
    state = update(state, AppEvent::AgentStarted("a02".to_string()));
    state = update(state, AppEvent::AgentStarted("a03".to_string()));

    assert_eq!(state.agents.len(), 3);
    assert!(state.agents.contains_key("a01"));
    assert!(state.agents.contains_key("a02"));
    assert!(state.agents.contains_key("a03"));
}

#[test]
fn event_loop_preserves_state_between_updates() {
    use loom_tui::model::TaskGraph;

    let mut state = AppState::new();

    // Set some state
    let graph = TaskGraph::empty();
    state = update(state, AppEvent::TaskGraphUpdated(graph));

    // Process unrelated event
    state = update(state, AppEvent::Tick);

    // Task graph should still be present
    assert!(state.task_graph.is_some());
}

#[test]
fn event_loop_keyboard_navigation_changes_view() {
    let mut state = AppState::new();
    assert!(matches!(state.view, ViewState::Dashboard));

    // Navigate to sessions
    let key = crossterm::event::KeyEvent::from(crossterm::event::KeyCode::Char('3'));
    state = update(state, AppEvent::Key(key));
    assert!(matches!(state.view, ViewState::Sessions));

    // Navigate back with Esc
    let key = crossterm::event::KeyEvent::from(crossterm::event::KeyCode::Esc);
    state = update(state, AppEvent::Key(key));
    assert!(matches!(state.view, ViewState::Dashboard));
}
