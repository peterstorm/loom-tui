use chrono::Utc;
use loom_tui::{
    app::{update, AppState, HookStatus, ViewState},
    event::AppEvent,
    model::AgentId,
};
use std::time::Duration;

#[test]
fn event_loop_processes_quit_signal() {
    let mut state = AppState::new();
    assert!(!state.meta.should_quit);

    let key = crossterm::event::KeyEvent::from(crossterm::event::KeyCode::Char('q'));
    update(&mut state, AppEvent::Key(key));

    assert!(state.meta.should_quit);
}

#[test]
fn event_loop_processes_multiple_events_in_sequence() {
    let mut state = AppState::new();

    let key = crossterm::event::KeyEvent::from(crossterm::event::KeyCode::Char('3'));
    update(&mut state, AppEvent::Key(key));
    assert!(matches!(state.ui.view, ViewState::Sessions));

    let key = crossterm::event::KeyEvent::from(crossterm::event::KeyCode::Char('1'));
    update(&mut state, AppEvent::Key(key));
    assert!(matches!(state.ui.view, ViewState::Dashboard));

    let key = crossterm::event::KeyEvent::from(crossterm::event::KeyCode::Char('q'));
    update(&mut state, AppEvent::Key(key));
    assert!(state.meta.should_quit);
}

#[test]
fn event_loop_tick_is_passive() {
    let mut state = AppState::new();
    let initial_events_len = state.domain.events.len();

    update(&mut state, AppEvent::Tick(Utc::now()));

    assert_eq!(state.domain.events.len(), initial_events_len);
    assert!(!state.meta.should_quit);
}

#[test]
fn event_loop_hook_status_transitions() {
    let mut state = AppState::new();
    assert!(matches!(state.meta.hook_status, HookStatus::Unknown));

    state.meta.hook_status = HookStatus::Missing;
    assert!(matches!(state.meta.hook_status, HookStatus::Missing));

    state.meta.hook_status = HookStatus::Installed;
    assert!(matches!(state.meta.hook_status, HookStatus::Installed));
}

#[test]
fn event_loop_handles_watcher_events() {
    use chrono::Utc;
    use loom_tui::model::{HookEvent, HookEventKind, TaskGraph, Wave};

    let mut state = AppState::new();

    let graph = TaskGraph::new(vec![Wave::new(1, vec![])]);
    update(&mut state, AppEvent::TaskGraphUpdated(graph));
    assert!(state.domain.task_graph.is_some());

    let timestamp = Utc::now();
    update(&mut state, AppEvent::AgentStarted {
        agent_id: AgentId::new("a01"),
        timestamp,
    });
    assert!(state.domain.agents.contains_key(&AgentId::new("a01")));

    let hook_event = HookEvent::new(Utc::now(), HookEventKind::SessionStart);
    update(&mut state, AppEvent::HookEventReceived(hook_event));
    assert_eq!(state.domain.events.len(), 1);
}

#[test]
fn tick_rate_configuration() {
    let tick_rate = Duration::from_millis(250);
    assert_eq!(tick_rate.as_millis(), 250);

    assert!(tick_rate >= Duration::from_millis(100));
    assert!(tick_rate <= Duration::from_millis(1000));
}

#[test]
fn event_loop_drains_multiple_watcher_events() {
    let mut state = AppState::new();

    let timestamp = Utc::now();
    update(&mut state, AppEvent::AgentStarted {
        agent_id: AgentId::new("a01"),
        timestamp,
    });
    update(&mut state, AppEvent::AgentStarted {
        agent_id: AgentId::new("a02"),
        timestamp,
    });
    update(&mut state, AppEvent::AgentStarted {
        agent_id: AgentId::new("a03"),
        timestamp,
    });

    assert_eq!(state.domain.agents.len(), 3);
    assert!(state.domain.agents.contains_key(&AgentId::new("a01")));
    assert!(state.domain.agents.contains_key(&AgentId::new("a02")));
    assert!(state.domain.agents.contains_key(&AgentId::new("a03")));
}

#[test]
fn event_loop_preserves_state_between_updates() {
    use loom_tui::model::TaskGraph;

    let mut state = AppState::new();

    let graph = TaskGraph::empty();
    update(&mut state, AppEvent::TaskGraphUpdated(graph));

    update(&mut state, AppEvent::Tick(Utc::now()));

    assert!(state.domain.task_graph.is_some());
}

#[test]
fn event_loop_keyboard_navigation_changes_view() {
    let mut state = AppState::new();
    assert!(matches!(state.ui.view, ViewState::Dashboard));

    let key = crossterm::event::KeyEvent::from(crossterm::event::KeyCode::Char('3'));
    update(&mut state, AppEvent::Key(key));
    assert!(matches!(state.ui.view, ViewState::Sessions));

    let key = crossterm::event::KeyEvent::from(crossterm::event::KeyCode::Esc);
    update(&mut state, AppEvent::Key(key));
    assert!(matches!(state.ui.view, ViewState::Dashboard));
}
