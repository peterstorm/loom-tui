use chrono::Utc;
use loom_tui::{
    app::{update, AppState, ViewState},
    event::AppEvent,
    model::{AgentId, TranscriptEvent, TranscriptEventKind, TaskGraph, Wave},
    watcher::TranscriptMetadata,
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

/// hook_status removed from AppMeta — replaced with a test verifying error tracking.
#[test]
fn event_loop_error_tracking() {
    use loom_tui::error::{LoomError, WatcherError};

    let mut state = AppState::new();
    assert!(state.meta.errors.is_empty());

    update(&mut state, AppEvent::Error {
        source: "watcher".to_string(),
        error: LoomError::Watcher(WatcherError::Io("disk error".to_string())),
    });

    assert_eq!(state.meta.errors.len(), 1);
    assert!(state.meta.errors[0].contains("watcher"));
}

#[test]
fn event_loop_handles_watcher_events() {
    let mut state = AppState::new();

    let graph = TaskGraph::new(vec![Wave::new(1, vec![])]);
    update(&mut state, AppEvent::TaskGraphUpdated(graph));
    assert!(state.domain.task_graph.is_some());

    update(&mut state, AppEvent::AgentMetadataUpdated {
        agent_id: AgentId::new("a01"),
        metadata: TranscriptMetadata::default(),
    });
    assert!(state.domain.agents.contains_key(&AgentId::new("a01")));

    let transcript_event = TranscriptEvent::new(Utc::now(), TranscriptEventKind::UserMessage);
    update(&mut state, AppEvent::TranscriptEventReceived(transcript_event));
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

    update(&mut state, AppEvent::AgentMetadataUpdated {
        agent_id: AgentId::new("a01"),
        metadata: TranscriptMetadata::default(),
    });
    update(&mut state, AppEvent::AgentMetadataUpdated {
        agent_id: AgentId::new("a02"),
        metadata: TranscriptMetadata::default(),
    });
    update(&mut state, AppEvent::AgentMetadataUpdated {
        agent_id: AgentId::new("a03"),
        metadata: TranscriptMetadata::default(),
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
