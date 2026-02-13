use chrono::Utc;
use loom_tui::app::{update, AppState, ViewState};
use loom_tui::event::AppEvent;
use loom_tui::model::{
    Agent, AgentMessage, ArchivedSession, HookEvent, HookEventKind, SessionArchive, SessionMeta,
    SessionStatus, Task, TaskGraph, TaskStatus, ToolCall, Wave,
};
use std::collections::BTreeMap;
use std::path::PathBuf;

#[test]
fn task_graph_updated_sets_graph() {
    let mut state = AppState::new();
    let graph = TaskGraph {
        waves: vec![
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
        ],
        total_tasks: 1,
        completed_tasks: 0,
    };

    update(&mut state, AppEvent::TaskGraphUpdated(graph.clone()));

    assert!(state.task_graph.is_some());
    let stored_graph = state.task_graph.unwrap();
    assert_eq!(stored_graph.waves.len(), 2);
    assert_eq!(stored_graph.total_tasks, 1);
    assert_eq!(stored_graph.waves[0].tasks[0].id, "T1");
}

#[test]
fn task_graph_updated_replaces_existing() {
    let mut state = AppState::new();
    state.task_graph = Some(TaskGraph {
        waves: vec![],
        total_tasks: 0,
        completed_tasks: 0,
    });

    let new_graph = TaskGraph {
        waves: vec![Wave {
            number: 1,
            tasks: vec![],
        }],
        total_tasks: 5,
        completed_tasks: 2,
    };

    update(&mut state, AppEvent::TaskGraphUpdated(new_graph));

    assert_eq!(state.task_graph.unwrap().total_tasks, 5);
}

#[test]
fn transcript_updated_adds_messages_to_existing_agent() {
    let mut state = AppState::new();
    let agent = Agent::new("a01".into(), Utc::now());
    state.agents.insert("a01".into(), agent);

    let messages = vec![
        AgentMessage::reasoning(Utc::now(), "analyzing problem".into()),
        AgentMessage::tool(
            Utc::now(),
            ToolCall::new("Read".into(), "file.rs".into()),
        ),
    ];

    update(
        &mut state,
        AppEvent::TranscriptUpdated {
            agent_id: "a01".into(),
            messages: messages.clone(),
        },
    );

    let agent = state.agents.get("a01").unwrap();
    assert_eq!(agent.messages.len(), 2);
    assert!(matches!(
        agent.messages[0].kind,
        loom_tui::model::MessageKind::Reasoning { .. }
    ));
}

#[test]
fn transcript_updated_replaces_existing_messages() {
    let mut state = AppState::new();
    let mut agent = Agent::new("a01".into(), Utc::now());
    agent.messages = vec![AgentMessage::reasoning(Utc::now(), "old".into())];
    state.agents.insert("a01".into(), agent);

    let new_messages = vec![AgentMessage::reasoning(Utc::now(), "new".into())];

    update(
        &mut state,
        AppEvent::TranscriptUpdated {
            agent_id: "a01".into(),
            messages: new_messages,
        },
    );

    let agent = state.agents.get("a01").unwrap();
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

    assert_eq!(state.events.len(), 2);
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
        state.events.push_back(event);
    }

    // Front should be event 0
    assert!(matches!(
        &state.events.front().unwrap().kind,
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
    assert_eq!(state.events.len(), 10_000);

    // Front should now be event 1 (event 0 evicted)
    assert!(matches!(
        &state.events.front().unwrap().kind,
        HookEventKind::Notification { message } if message.contains("event 1")
    ));

    // Back should be event 10000
    assert!(matches!(
        &state.events.back().unwrap().kind,
        HookEventKind::Notification { message } if message.contains("event 10000")
    ));
}

#[test]
fn agent_started_creates_new_agent() {
    let mut state = AppState::new();
    update(&mut state, AppEvent::AgentStarted("a01".into()));

    assert_eq!(state.agents.len(), 1);
    let agent = state.agents.get("a01").unwrap();
    assert_eq!(agent.id, "a01");
    assert!(agent.finished_at.is_none());
    assert!(agent.messages.is_empty());
}

#[test]
fn agent_started_uses_current_timestamp() {
    let before = Utc::now();
    let mut state = AppState::new();
    update(&mut state, AppEvent::AgentStarted("a01".into()));
    let after = Utc::now();

    let agent = state.agents.get("a01").unwrap();
    assert!(agent.started_at >= before);
    assert!(agent.started_at <= after);
}

#[test]
fn agent_stopped_sets_finished_timestamp() {
    let mut state = AppState::new();
    state
        .agents
        .insert("a01".into(), Agent::new("a01".into(), Utc::now()));

    let before = Utc::now();
    update(&mut state, AppEvent::AgentStopped("a01".into()));
    let after = Utc::now();

    let agent = state.agents.get("a01").unwrap();
    assert!(agent.finished_at.is_some());
    let finished = agent.finished_at.unwrap();
    assert!(finished >= before);
    assert!(finished <= after);
}

#[test]
fn key_event_no_op_until_t6() {
    let mut state = AppState::new();
    let original_view = state.view.clone();

    let key = crossterm::event::KeyEvent::from(crossterm::event::KeyCode::Char('q'));
    update(&mut state, AppEvent::Key(key));

    // State should be unchanged
    assert_eq!(state.view, original_view);
    assert_eq!(state.agents.len(), 0);
}

#[test]
fn tick_event_is_passive() {
    let mut state = AppState::new();
    state.events.push_back(HookEvent::new(
        Utc::now(),
        HookEventKind::Notification {
            message: "test".into(),
        },
    ));

    let original_len = state.events.len();
    update(&mut state, AppEvent::Tick(Utc::now()));

    assert_eq!(state.events.len(), original_len);
}

#[test]
fn parse_error_adds_formatted_message() {
    let mut state = AppState::new();
    update(
        &mut state,
        AppEvent::ParseError {
            source: "task_graph.json".into(),
            error: "unexpected token at line 5".into(),
        },
    );

    assert_eq!(state.errors.len(), 1);
    assert_eq!(
        state.errors.front().unwrap(),
        "task_graph.json: unexpected token at line 5"
    );
}

#[test]
fn parse_error_evicts_oldest_at_capacity() {
    let mut state = AppState::new();

    // Fill to exactly 100
    for i in 0..100 {
        state.errors.push_back(format!("error {}", i));
    }

    assert_eq!(state.errors.front().unwrap(), "error 0");

    // Add one more
    update(
        &mut state,
        AppEvent::ParseError {
            source: "file".into(),
            error: "error 100".into(),
        },
    );

    assert_eq!(state.errors.len(), 100);
    assert_eq!(state.errors.front().unwrap(), "error 1");
    assert!(state.errors.back().unwrap().contains("error 100"));
}

#[test]
fn session_loaded_populates_data_and_navigates() {
    let mut state = AppState::new();
    let meta = SessionMeta::new("session-123".into(), Utc::now(), "/home/user/proj".into())
        .with_status(SessionStatus::Completed);

    // Pre-populate with meta-only archived session
    state.sessions.push(ArchivedSession::new(meta.clone(), PathBuf::new()));
    state.loading_session = Some(0);

    let archive = SessionArchive::new(meta);
    update(&mut state, AppEvent::SessionLoaded(archive));

    // Data should be populated
    assert!(state.sessions[0].data.is_some());
    assert_eq!(state.sessions[0].meta.id, "session-123");
    // Loading cleared and navigated to detail
    assert!(state.loading_session.is_none());
    assert!(matches!(state.view, ViewState::SessionDetail));
}

#[test]
fn session_loaded_sets_task_graph_in_archive() {
    let mut state = AppState::new();
    let meta = SessionMeta::new("s1".into(), Utc::now(), "/proj".into());
    let graph = TaskGraph {
        waves: vec![],
        total_tasks: 10,
        completed_tasks: 5,
    };

    state.sessions.push(ArchivedSession::new(meta.clone(), PathBuf::new()));

    let archive = SessionArchive::new(meta).with_task_graph(graph);
    update(&mut state, AppEvent::SessionLoaded(archive));

    let data = state.sessions[0].data.as_ref().unwrap();
    assert!(data.task_graph.is_some());
    assert_eq!(data.task_graph.as_ref().unwrap().total_tasks, 10);
}

#[test]
fn session_loaded_sets_agents_in_archive() {
    let mut state = AppState::new();
    let meta = SessionMeta::new("s1".into(), Utc::now(), "/proj".into());

    let mut agents = BTreeMap::new();
    agents.insert("a01".into(), Agent::new("a01".into(), Utc::now()));
    agents.insert("a02".into(), Agent::new("a02".into(), Utc::now()));

    state.sessions.push(ArchivedSession::new(meta.clone(), PathBuf::new()));

    let archive = SessionArchive::new(meta).with_agents(agents);
    update(&mut state, AppEvent::SessionLoaded(archive));

    let data = state.sessions[0].data.as_ref().unwrap();
    assert_eq!(data.agents.len(), 2);
    assert!(data.agents.contains_key("a01"));
    assert!(data.agents.contains_key("a02"));
}

#[test]
fn session_loaded_stores_events_in_archive() {
    let mut state = AppState::new();
    let meta = SessionMeta::new("s1".into(), Utc::now(), "/proj".into());

    let events = vec![
        HookEvent::new(Utc::now(), HookEventKind::SessionStart),
        HookEvent::new(Utc::now(), HookEventKind::SessionEnd),
    ];

    state.sessions.push(ArchivedSession::new(meta.clone(), PathBuf::new()));

    let archive = SessionArchive::new(meta).with_events(events);
    update(&mut state, AppEvent::SessionLoaded(archive));

    let data = state.sessions[0].data.as_ref().unwrap();
    assert_eq!(data.events.len(), 2);
}

#[test]
fn session_loaded_clears_loading_flag() {
    let mut state = AppState::new();
    let meta = SessionMeta::new("s1".into(), Utc::now(), "/proj".into());
    state.sessions.push(ArchivedSession::new(meta.clone(), PathBuf::new()));
    state.loading_session = Some(0);

    let archive = SessionArchive::new(meta);
    update(&mut state, AppEvent::SessionLoaded(archive));

    assert!(state.loading_session.is_none());
}

#[test]
fn session_list_refreshed_updates_sessions() {
    let mut state = AppState::new();
    let sessions = vec![
        SessionArchive::new(SessionMeta::new("s1".into(), Utc::now(), "/proj1".into())),
        SessionArchive::new(SessionMeta::new("s2".into(), Utc::now(), "/proj2".into())),
        SessionArchive::new(SessionMeta::new("s3".into(), Utc::now(), "/proj3".into())),
    ];

    update(&mut state, AppEvent::SessionListRefreshed(sessions));

    assert_eq!(state.sessions.len(), 3);
    assert_eq!(state.sessions[0].meta.id, "s1");
    assert_eq!(state.sessions[2].meta.id, "s3");
}

#[test]
fn session_list_refreshed_replaces_existing_list() {
    let mut state = AppState::new();
    state
        .sessions
        .push(ArchivedSession::new(SessionMeta::new("old".into(), Utc::now(), "/old".into()), PathBuf::new()));

    let new_sessions = vec![SessionArchive::new(SessionMeta::new("new".into(), Utc::now(), "/new".into()))];

    update(&mut state, AppEvent::SessionListRefreshed(new_sessions));

    assert_eq!(state.sessions.len(), 1);
    assert_eq!(state.sessions[0].meta.id, "new");
}

#[test]
fn session_metas_loaded_creates_archived_sessions() {
    let mut state = AppState::new();
    let metas = vec![
        (PathBuf::from("/sessions/s1.json"), SessionMeta::new("s1".into(), Utc::now(), "/proj1".into())),
        (PathBuf::from("/sessions/s2.json"), SessionMeta::new("s2".into(), Utc::now(), "/proj2".into())),
    ];

    update(&mut state, AppEvent::SessionMetasLoaded(metas));

    assert_eq!(state.sessions.len(), 2);
    assert_eq!(state.sessions[0].meta.id, "s1");
    assert_eq!(state.sessions[0].path, PathBuf::from("/sessions/s1.json"));
    assert!(state.sessions[0].data.is_none()); // Not loaded yet
    assert_eq!(state.sessions[1].meta.id, "s2");
}

#[test]
fn load_session_requested_sets_loading_flag() {
    let mut state = AppState::new();
    state.sessions.push(ArchivedSession::new(
        SessionMeta::new("s1".into(), Utc::now(), "/proj".into()),
        PathBuf::from("/sessions/s1.json"),
    ));

    update(&mut state, AppEvent::LoadSessionRequested(0));
    assert_eq!(state.loading_session, Some(0));
}

#[test]
fn multiple_updates_compose_correctly() {
    let mut state = AppState::new();

    // Start agent
    update(&mut state, AppEvent::AgentStarted("a01".into()));
    assert_eq!(state.agents.len(), 1);

    // Update transcript
    let messages = vec![AgentMessage::reasoning(Utc::now(), "thinking".into())];
    update(
        &mut state,
        AppEvent::TranscriptUpdated {
            agent_id: "a01".into(),
            messages,
        },
    );
    assert_eq!(state.agents.get("a01").unwrap().messages.len(), 1);

    // Add event (use Notification, not SessionStart which resets state)
    let event = HookEvent::new(Utc::now(), HookEventKind::Notification { message: "test".into() });
    update(&mut state, AppEvent::HookEventReceived(event));
    assert_eq!(state.events.len(), 1);

    // Stop agent
    update(&mut state, AppEvent::AgentStopped("a01".into()));
    assert!(state.agents.get("a01").unwrap().finished_at.is_some());

    // All state should be preserved
    assert_eq!(state.agents.len(), 1);
    assert_eq!(state.events.len(), 1);
    assert_eq!(state.agents.get("a01").unwrap().messages.len(), 1);
}

#[test]
fn update_preserves_unmodified_fields() {
    let mut state = AppState::new();
    state.view = ViewState::Sessions;
    state.auto_scroll = false;
    state.show_help = true;

    let event = HookEvent::new(Utc::now(), HookEventKind::SessionStart);
    update(&mut state, AppEvent::HookEventReceived(event));

    // These fields should be preserved
    assert_eq!(state.view, ViewState::Sessions);
    assert_eq!(state.auto_scroll, false);
    assert_eq!(state.show_help, true);
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
        assert!(state.events.len() <= 10_000);
    }

    assert_eq!(state.events.len(), 10_000);
}

#[test]
fn property_error_buffer_never_exceeds_100() {
    let mut state = AppState::new();

    for i in 0..500 {
        update(
            &mut state,
            AppEvent::ParseError {
                source: "test".into(),
                error: format!("error {}", i),
            },
        );
        assert!(state.errors.len() <= 100);
    }

    assert_eq!(state.errors.len(), 100);
}

#[test]
fn property_update_never_panics() {
    let mut state = AppState::new();

    // Throw a variety of events at it
    let events: Vec<AppEvent> = vec![
        AppEvent::AgentStarted("a01".into()),
        AppEvent::AgentStopped("nonexistent".into()),
        AppEvent::TranscriptUpdated {
            agent_id: "nonexistent".into(),
            messages: vec![],
        },
        AppEvent::HookEventReceived(HookEvent::new(Utc::now(), HookEventKind::SessionStart)),
        AppEvent::ParseError {
            source: "test".into(),
            error: "error".into(),
        },
        AppEvent::Tick(Utc::now()),
        AppEvent::Key(crossterm::event::KeyEvent::from(
            crossterm::event::KeyCode::Char('x'),
        )),
        AppEvent::SessionListRefreshed(vec![]),
        AppEvent::SessionMetasLoaded(vec![]),
        AppEvent::LoadSessionRequested(0),
    ];

    for event in events {
        update(&mut state, event); // Should never panic
    }
}
