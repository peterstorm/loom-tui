use chrono::Utc;
use loom_tui::app::{update, AppState, ViewState};
use loom_tui::event::AppEvent;
use loom_tui::model::{
    Agent, AgentId, AgentMessage, ArchivedSession, HookEvent, HookEventKind, SessionArchive, SessionId, SessionMeta,
    SessionStatus, Task, TaskGraph, TaskStatus, ToolCall, Wave,
};
use std::collections::BTreeMap;
use std::path::PathBuf;

#[test]
fn task_graph_updated_sets_graph() {
    let mut state = AppState::new();
    let graph = TaskGraph::new(vec![
        Wave {
            number: 1,
            tasks: vec![Task {
                id: "T1".into(),
                description: "Task 1".into(),
                agent_id: Some("a01".into()),
                status: TaskStatus::Running,
                review_status: loom_tui::model::ReviewStatus::Pending,
                files_modified: vec![],
                tests_passed: None,
            }],
        },
        Wave {
            number: 2,
            tasks: vec![],
        },
    ]);

    update(&mut state, AppEvent::TaskGraphUpdated(graph.clone()));

    assert!(state.domain.task_graph.is_some());
    let stored_graph = state.domain.task_graph.unwrap();
    assert_eq!(stored_graph.waves.len(), 2);
    assert_eq!(stored_graph.total_tasks(), 1);
    assert_eq!(stored_graph.waves[0].tasks[0].id.as_str(), "T1");
}

#[test]
fn task_graph_updated_replaces_existing() {
    let mut state = AppState::new();
    state.domain.task_graph = Some(TaskGraph::empty());

    let new_graph = TaskGraph::new(vec![Wave {
        number: 1,
        tasks: vec![],
    }]);

    update(&mut state, AppEvent::TaskGraphUpdated(new_graph));

    assert_eq!(state.domain.task_graph.unwrap().total_tasks(), 0);
}

#[test]
fn transcript_updated_adds_messages_to_existing_agent() {
    let mut state = AppState::new();
    let agent = Agent::new("a01", Utc::now());
    state.domain.agents.insert("a01".into(), agent);

    let messages = vec![
        AgentMessage::reasoning(Utc::now(), "analyzing problem".into()),
        AgentMessage::tool(
            Utc::now(),
            ToolCall::new("Read", "file.rs".to_string()),
        ),
    ];

    update(
        &mut state,
        AppEvent::TranscriptUpdated {
            agent_id: "a01".into(),
            messages: messages.clone(),
        },
    );

    let agent = state.domain.agents.get(&AgentId::new("a01")).unwrap();
    assert_eq!(agent.messages.len(), 2);
    assert!(matches!(
        agent.messages[0].kind,
        loom_tui::model::MessageKind::Reasoning { .. }
    ));
}

#[test]
fn transcript_updated_replaces_existing_messages() {
    let mut state = AppState::new();
    let mut agent = Agent::new("a01", Utc::now());
    agent.messages = vec![AgentMessage::reasoning(Utc::now(), "old".into())];
    state.domain.agents.insert("a01".into(), agent);

    let new_messages = vec![AgentMessage::reasoning(Utc::now(), "new".into())];

    update(
        &mut state,
        AppEvent::TranscriptUpdated {
            agent_id: "a01".into(),
            messages: new_messages,
        },
    );

    let agent = state.domain.agents.get(&AgentId::new("a01")).unwrap();
    assert_eq!(agent.messages.len(), 1);
}

#[test]
fn hook_event_received_appends_to_buffer() {
    let mut state = AppState::new();
    let event1 = HookEvent::new(Utc::now(), HookEventKind::SessionStart);
    let event2 = HookEvent::new(
        Utc::now(),
        HookEventKind::Notification {
            message: "test".into(),
        },
    );

    update(&mut state, AppEvent::HookEventReceived(event1));
    update(&mut state, AppEvent::HookEventReceived(event2));

    assert_eq!(state.domain.events.len(), 2);
}

#[test]
fn hook_event_evicts_oldest_at_capacity() {
    let mut state = AppState::new();

    // Fill to exactly 10,000
    for i in 0..10_000 {
        let event = HookEvent::new(
            Utc::now(),
            HookEventKind::Notification {
                message: format!("event {}", i),
            },
        );
        state.domain.events.push_back(event);
    }

    // Front should be event 0
    assert!(matches!(
        &state.domain.events.front().unwrap().kind,
        HookEventKind::Notification { message } if message.contains("event 0")
    ));

    // Add one more
    let new_event = HookEvent::new(
        Utc::now(),
        HookEventKind::Notification {
            message: "event 10000".into(),
        },
    );
    update(&mut state, AppEvent::HookEventReceived(new_event));

    // Should still be 10,000
    assert_eq!(state.domain.events.len(), 10_000);

    // Front should now be event 1 (event 0 evicted)
    assert!(matches!(
        &state.domain.events.front().unwrap().kind,
        HookEventKind::Notification { message } if message.contains("event 1")
    ));

    // Back should be event 10000
    assert!(matches!(
        &state.domain.events.back().unwrap().kind,
        HookEventKind::Notification { message } if message.contains("event 10000")
    ));
}

#[test]
fn agent_started_creates_new_agent() {
    let mut state = AppState::new();
    let timestamp = Utc::now();
    update(&mut state, AppEvent::AgentStarted {
        agent_id: "a01".into(),
        timestamp,
    });

    assert_eq!(state.domain.agents.len(), 1);
    let agent = state.domain.agents.get(&AgentId::new("a01")).unwrap();
    assert_eq!(agent.id.as_str(), "a01");
    assert!(agent.finished_at.is_none());
    assert!(agent.messages.is_empty());
    assert_eq!(agent.started_at, timestamp);
}

#[test]
fn agent_started_uses_provided_timestamp() {
    let timestamp = Utc::now() - chrono::Duration::minutes(5);
    let mut state = AppState::new();
    update(&mut state, AppEvent::AgentStarted {
        agent_id: "a01".into(),
        timestamp,
    });

    let agent = state.domain.agents.get(&AgentId::new("a01")).unwrap();
    assert_eq!(agent.started_at, timestamp);
}

#[test]
fn agent_stopped_uses_provided_timestamp() {
    let mut state = AppState::new();
    let start_time = Utc::now();
    state
        .domain.agents
        .insert("a01".into(), Agent::new("a01", start_time));

    let stop_time = start_time + chrono::Duration::seconds(30);
    update(&mut state, AppEvent::AgentStopped {
        agent_id: "a01".into(),
        timestamp: stop_time,
    });

    let agent = state.domain.agents.get(&AgentId::new("a01")).unwrap();
    assert_eq!(agent.finished_at, Some(stop_time));
}

#[test]
fn key_event_no_op_until_t6() {
    let mut state = AppState::new();
    let original_view = state.ui.view.clone();

    let key = crossterm::event::KeyEvent::from(crossterm::event::KeyCode::Char('q'));
    update(&mut state, AppEvent::Key(key));

    // State should be unchanged
    assert_eq!(state.ui.view, original_view);
    assert_eq!(state.domain.agents.len(), 0);
}

#[test]
fn tick_event_is_passive() {
    let mut state = AppState::new();
    state.domain.events.push_back(HookEvent::new(
        Utc::now(),
        HookEventKind::Notification {
            message: "test".into(),
        },
    ));

    let original_len = state.domain.events.len();
    update(&mut state, AppEvent::Tick(Utc::now()));

    assert_eq!(state.domain.events.len(), original_len);
}

#[test]
fn parse_error_adds_formatted_message() {
    let mut state = AppState::new();
    update(
        &mut state,
        AppEvent::Error {
            source: "task_graph.json".into(),
            error: loom_tui::error::WatcherError::Parse(
                loom_tui::error::ParseError::Json("unexpected token at line 5".into())
            ).into(),
        },
    );

    assert_eq!(state.meta.errors.len(), 1);
    assert_eq!(
        state.meta.errors.front().unwrap(),
        "task_graph.json: parse: JSON parse: unexpected token at line 5"
    );
}

#[test]
fn parse_error_evicts_oldest_at_capacity() {
    let mut state = AppState::new();

    // Fill to exactly 100
    for i in 0..100 {
        state.meta.errors.push_back(format!("error {}", i));
    }

    assert_eq!(state.meta.errors.front().unwrap(), "error 0");

    // Add one more
    update(
        &mut state,
        AppEvent::Error {
            source: "file".into(),
            error: loom_tui::error::WatcherError::Parse(
                loom_tui::error::ParseError::Json("error 100".into())
            ).into(),
        },
    );

    assert_eq!(state.meta.errors.len(), 100);
    assert_eq!(state.meta.errors.front().unwrap(), "error 1");
    assert!(state.meta.errors.back().unwrap().contains("error 100"));
}

#[test]
fn session_loaded_populates_data_and_navigates() {
    let mut state = AppState::new();
    let meta = SessionMeta::new("session-123", Utc::now(), "/home/user/proj".to_string())
        .with_status(SessionStatus::Completed);

    // Pre-populate with meta-only archived session
    state.domain.sessions.push(ArchivedSession::new(meta.clone(), PathBuf::new()));
    state.ui.loading_session = Some(SessionId::new("session-123"));

    let archive = SessionArchive::new(meta);
    update(&mut state, AppEvent::SessionLoaded(archive));

    // Data should be populated
    assert!(state.domain.sessions[0].data.is_some());
    assert_eq!(state.domain.sessions[0].meta.id.as_str(), "session-123");
    // Loading cleared and navigated to detail
    assert!(state.ui.loading_session.is_none());
    assert!(matches!(state.ui.view, ViewState::SessionDetail));
}

#[test]
fn session_loaded_sets_task_graph_in_archive() {
    let mut state = AppState::new();
    let meta = SessionMeta::new("s1", Utc::now(), "/proj".to_string());
    let graph = TaskGraph::empty();

    state.domain.sessions.push(ArchivedSession::new(meta.clone(), PathBuf::new()));

    let archive = SessionArchive::new(meta).with_task_graph(graph);
    update(&mut state, AppEvent::SessionLoaded(archive));

    let data = state.domain.sessions[0].data.as_ref().unwrap();
    assert!(data.task_graph.is_some());
    assert_eq!(data.task_graph.as_ref().unwrap().total_tasks(), 0);
}

#[test]
fn session_loaded_sets_agents_in_archive() {
    let mut state = AppState::new();
    let meta = SessionMeta::new("s1", Utc::now(), "/proj".to_string());

    let mut agents = BTreeMap::new();
    agents.insert(AgentId::new("a01"), Agent::new("a01", Utc::now()));
    agents.insert(AgentId::new("a02"), Agent::new("a02", Utc::now()));

    state.domain.sessions.push(ArchivedSession::new(meta.clone(), PathBuf::new()));

    let archive = SessionArchive::new(meta).with_agents(agents);
    update(&mut state, AppEvent::SessionLoaded(archive));

    let data = state.domain.sessions[0].data.as_ref().unwrap();
    assert_eq!(data.agents.len(), 2);
    assert!(data.agents.contains_key(&AgentId::new("a01")));
    assert!(data.agents.contains_key(&AgentId::new("a02")));
}

#[test]
fn session_loaded_stores_events_in_archive() {
    let mut state = AppState::new();
    let meta = SessionMeta::new("s1", Utc::now(), "/proj".to_string());

    let events = vec![
        HookEvent::new(Utc::now(), HookEventKind::SessionStart),
        HookEvent::new(Utc::now(), HookEventKind::SessionEnd),
    ];

    state.domain.sessions.push(ArchivedSession::new(meta.clone(), PathBuf::new()));

    let archive = SessionArchive::new(meta).with_events(events);
    update(&mut state, AppEvent::SessionLoaded(archive));

    let data = state.domain.sessions[0].data.as_ref().unwrap();
    assert_eq!(data.events.len(), 2);
}

#[test]
fn session_loaded_clears_loading_flag() {
    let mut state = AppState::new();
    let meta = SessionMeta::new("s1", Utc::now(), "/proj".to_string());
    state.domain.sessions.push(ArchivedSession::new(meta.clone(), PathBuf::new()));
    state.ui.loading_session = Some(SessionId::new("s1"));

    let archive = SessionArchive::new(meta);
    update(&mut state, AppEvent::SessionLoaded(archive));

    assert!(state.ui.loading_session.is_none());
}

#[test]
fn session_list_refreshed_updates_sessions() {
    let mut state = AppState::new();
    let sessions = vec![
        SessionArchive::new(SessionMeta::new("s1", Utc::now(), "/proj1".to_string())),
        SessionArchive::new(SessionMeta::new("s2", Utc::now(), "/proj2".to_string())),
        SessionArchive::new(SessionMeta::new("s3", Utc::now(), "/proj3".to_string())),
    ];

    update(&mut state, AppEvent::SessionListRefreshed(sessions));

    assert_eq!(state.domain.sessions.len(), 3);
    assert_eq!(state.domain.sessions[0].meta.id.as_str(), "s1");
    assert_eq!(state.domain.sessions[2].meta.id.as_str(), "s3");
}

#[test]
fn session_list_refreshed_replaces_existing_list() {
    let mut state = AppState::new();
    state
        .domain.sessions
        .push(ArchivedSession::new(SessionMeta::new("old", Utc::now(), "/old".to_string()), PathBuf::new()));

    let new_sessions = vec![SessionArchive::new(SessionMeta::new("new", Utc::now(), "/new".to_string()))];

    update(&mut state, AppEvent::SessionListRefreshed(new_sessions));

    assert_eq!(state.domain.sessions.len(), 1);
    assert_eq!(state.domain.sessions[0].meta.id.as_str(), "new");
}

#[test]
fn session_metas_loaded_creates_archived_sessions() {
    let mut state = AppState::new();
    let metas = vec![
        (PathBuf::from("/sessions/s1.json"), SessionMeta::new("s1", Utc::now(), "/proj1".to_string())),
        (PathBuf::from("/sessions/s2.json"), SessionMeta::new("s2", Utc::now(), "/proj2".to_string())),
    ];

    update(&mut state, AppEvent::SessionMetasLoaded(metas));

    assert_eq!(state.domain.sessions.len(), 2);
    assert_eq!(state.domain.sessions[0].meta.id.as_str(), "s1");
    assert_eq!(state.domain.sessions[0].path, PathBuf::from("/sessions/s1.json"));
    assert!(state.domain.sessions[0].data.is_none()); // Not loaded yet
    assert_eq!(state.domain.sessions[1].meta.id.as_str(), "s2");
}

#[test]
fn load_session_requested_sets_loading_flag() {
    let mut state = AppState::new();
    state.domain.sessions.push(ArchivedSession::new(
        SessionMeta::new("s1", Utc::now(), "/proj".to_string()),
        PathBuf::from("/sessions/s1.json"),
    ));

    update(&mut state, AppEvent::LoadSessionRequested(SessionId::new("s1")));
    assert_eq!(state.ui.loading_session, Some(SessionId::new("s1")));
}

#[test]
fn multiple_updates_compose_correctly() {
    let mut state = AppState::new();

    // Start agent
    let start_time = Utc::now();
    update(&mut state, AppEvent::AgentStarted {
        agent_id: "a01".into(),
        timestamp: start_time,
    });
    assert_eq!(state.domain.agents.len(), 1);

    // Update transcript
    let messages = vec![AgentMessage::reasoning(Utc::now(), "thinking".into())];
    update(
        &mut state,
        AppEvent::TranscriptUpdated {
            agent_id: "a01".into(),
            messages,
        },
    );
    assert_eq!(state.domain.agents.get(&AgentId::new("a01")).unwrap().messages.len(), 1);

    // Add event (use Notification, not SessionStart which resets state)
    let event = HookEvent::new(Utc::now(), HookEventKind::Notification { message: "test".into() });
    update(&mut state, AppEvent::HookEventReceived(event));
    assert_eq!(state.domain.events.len(), 1);

    // Stop agent
    let stop_time = start_time + chrono::Duration::seconds(10);
    update(&mut state, AppEvent::AgentStopped {
        agent_id: "a01".into(),
        timestamp: stop_time,
    });
    assert!(state.domain.agents.get(&AgentId::new("a01")).unwrap().finished_at.is_some());

    // All state should be preserved
    assert_eq!(state.domain.agents.len(), 1);
    assert_eq!(state.domain.events.len(), 1);
    assert_eq!(state.domain.agents.get(&AgentId::new("a01")).unwrap().messages.len(), 1);
}

#[test]
fn update_preserves_unmodified_fields() {
    let mut state = AppState::new();
    state.ui.view = ViewState::Sessions;
    state.ui.auto_scroll = false;
    state.ui.show_help = true;

    let event = HookEvent::new(Utc::now(), HookEventKind::SessionStart);
    update(&mut state, AppEvent::HookEventReceived(event));

    // These fields should be preserved
    assert_eq!(state.ui.view, ViewState::Sessions);
    assert_eq!(state.ui.auto_scroll, false);
    assert_eq!(state.ui.show_help, true);
}

#[test]
fn property_event_buffer_never_exceeds_10000() {
    let mut state = AppState::new();

    // Stress test: add way more than capacity
    for i in 0..20_000 {
        let event = HookEvent::new(
            Utc::now(),
            HookEventKind::Notification {
                message: format!("{}", i),
            },
        );
        update(&mut state, AppEvent::HookEventReceived(event));
        assert!(state.domain.events.len() <= 10_000);
    }

    assert_eq!(state.domain.events.len(), 10_000);
}

#[test]
fn property_error_buffer_never_exceeds_100() {
    let mut state = AppState::new();

    for i in 0..500 {
        update(
            &mut state,
            AppEvent::Error {
                source: "test".into(),
                error: loom_tui::error::WatcherError::Parse(
                    loom_tui::error::ParseError::Json(format!("error {}", i))
                ).into(),
            },
        );
        assert!(state.meta.errors.len() <= 100);
    }

    assert_eq!(state.meta.errors.len(), 100);
}

#[test]
fn property_update_never_panics() {
    let mut state = AppState::new();

    // Throw a variety of events at it
    let timestamp = Utc::now();
    let events: Vec<AppEvent> = vec![
        AppEvent::AgentStarted {
            agent_id: "a01".into(),
            timestamp,
        },
        AppEvent::AgentStopped {
            agent_id: "nonexistent".into(),
            timestamp,
        },
        AppEvent::TranscriptUpdated {
            agent_id: "nonexistent".into(),
            messages: vec![],
        },
        AppEvent::HookEventReceived(HookEvent::new(Utc::now(), HookEventKind::SessionStart)),
        AppEvent::Error {
            source: "test".into(),
            error: loom_tui::error::WatcherError::Parse(
                loom_tui::error::ParseError::Json("error".into())
            ).into(),
        },
        AppEvent::Tick(Utc::now()),
        AppEvent::Key(crossterm::event::KeyEvent::from(
            crossterm::event::KeyCode::Char('x'),
        )),
        AppEvent::SessionListRefreshed(vec![]),
        AppEvent::SessionMetasLoaded(vec![]),
        AppEvent::LoadSessionRequested(SessionId::new("test")),
    ];

    for event in events {
        update(&mut state, event); // Should never panic
    }
}
