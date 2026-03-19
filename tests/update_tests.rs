use chrono::Utc;
use loom_tui::app::{update, AppState, ViewState};
use loom_tui::event::AppEvent;
use loom_tui::model::{
    Agent, AgentId, ArchivedSession, SessionArchive, SessionId, SessionMeta,
    SessionStatus, Task, TaskGraph, TaskStatus, TranscriptEvent, TranscriptEventKind, Wave,
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

/// Replacement for deprecated transcript_updated tests — now events go to the ring buffer.
#[test]
fn transcript_event_received_pushes_to_ring_buffer() {
    let mut state = AppState::new();
    let now = Utc::now();

    let event = TranscriptEvent::new(now, TranscriptEventKind::UserMessage);
    update(&mut state, AppEvent::TranscriptEventReceived(event));

    assert_eq!(state.domain.events.len(), 1);
    assert_eq!(state.domain.events[0].kind, TranscriptEventKind::UserMessage);
}

#[test]
fn transcript_event_received_updates_session_last_event() {
    let mut state = AppState::new();
    let sid = SessionId::new("sess-1");
    let now = Utc::now();
    let meta = SessionMeta::new(sid.clone(), now, "/proj".to_string());
    state.domain.active_sessions.insert(sid.clone(), meta);

    let later = now + chrono::Duration::seconds(5);
    let event = TranscriptEvent::new(later, TranscriptEventKind::UserMessage)
        .with_session(sid.clone());
    update(&mut state, AppEvent::TranscriptEventReceived(event));

    assert_eq!(state.domain.active_sessions[&sid].last_event_at, Some(later));
    assert_eq!(state.domain.active_sessions[&sid].event_count, 1);
}

/// Replacement for hook_event_received_appends_to_buffer.
#[test]
fn transcript_event_received_appends_to_buffer() {
    let mut state = AppState::new();
    let now = Utc::now();
    let event1 = TranscriptEvent::new(now, TranscriptEventKind::UserMessage);
    let event2 = TranscriptEvent::new(
        now,
        TranscriptEventKind::AssistantMessage { content: "test".into() },
    );

    update(&mut state, AppEvent::TranscriptEventReceived(event1));
    update(&mut state, AppEvent::TranscriptEventReceived(event2));

    assert_eq!(state.domain.events.len(), 2);
}

/// Replacement for hook_event_evicts_oldest_at_capacity.
#[test]
fn transcript_event_evicts_oldest_at_capacity() {
    let mut state = AppState::new();
    let now = Utc::now();

    // Fill to exactly 10,000
    for i in 0..10_000usize {
        let event = TranscriptEvent::new(
            now,
            TranscriptEventKind::AssistantMessage { content: format!("event {i}") },
        );
        state.domain.events.push_back(event);
    }

    // Front should be event 0
    assert!(matches!(
        &state.domain.events.front().unwrap().kind,
        TranscriptEventKind::AssistantMessage { content } if content.contains("event 0")
    ));

    // Add one more via update
    let new_event = TranscriptEvent::new(now, TranscriptEventKind::UserMessage);
    update(&mut state, AppEvent::TranscriptEventReceived(new_event));

    // Should still be 10,000
    assert_eq!(state.domain.events.len(), 10_000);

    // Front should now be event 1 (event 0 evicted)
    assert!(matches!(
        &state.domain.events.front().unwrap().kind,
        TranscriptEventKind::AssistantMessage { content } if content.contains("event 1")
    ));

    // Back should be UserMessage
    assert_eq!(state.domain.events.back().unwrap().kind, TranscriptEventKind::UserMessage);
}

/// AgentStarted/AgentStopped removed — agents now discovered via AgentMetadataUpdated.
/// Test equivalent: AgentMetadataUpdated creates a new agent entry.
#[test]
fn agent_metadata_updated_creates_new_agent() {
    use loom_tui::watcher::TranscriptMetadata;

    let mut state = AppState::new();
    let aid = AgentId::new("a01");

    update(&mut state, AppEvent::AgentMetadataUpdated {
        agent_id: aid.clone(),
        metadata: TranscriptMetadata {
            model: Some("claude-sonnet".to_string()),
            ..Default::default()
        },
    });

    assert_eq!(state.domain.agents.len(), 1);
    let agent = state.domain.agents.get(&aid).unwrap();
    assert_eq!(agent.id.as_str(), "a01");
    assert_eq!(agent.model.as_deref(), Some("claude-sonnet"));
}

/// AgentMetadataUpdated updates model/tokens on existing agent.
#[test]
fn agent_metadata_updated_updates_existing_agent() {
    use loom_tui::watcher::TranscriptMetadata;

    let mut state = AppState::new();
    let now = Utc::now();
    state.domain.agents.insert(AgentId::new("a01"), Agent::new("a01", now));

    update(&mut state, AppEvent::AgentMetadataUpdated {
        agent_id: AgentId::new("a01"),
        metadata: TranscriptMetadata {
            model: Some("claude-opus".to_string()),
            ..Default::default()
        },
    });

    assert_eq!(state.domain.agents.len(), 1);
    let agent = state.domain.agents.get(&AgentId::new("a01")).unwrap();
    assert_eq!(agent.model.as_deref(), Some("claude-opus"));
}

#[test]
fn key_event_q_triggers_quit() {
    let mut state = AppState::new();

    let key = crossterm::event::KeyEvent::from(crossterm::event::KeyCode::Char('q'));
    update(&mut state, AppEvent::Key(key));

    assert!(state.meta.should_quit);
    assert_eq!(state.domain.agents.len(), 0);
}

#[test]
fn tick_event_is_passive() {
    let mut state = AppState::new();
    // replay_complete must be false (default) so Tick is a no-op
    state.domain.events.push_back(TranscriptEvent::new(
        Utc::now(),
        TranscriptEventKind::UserMessage,
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
        TranscriptEvent::new(Utc::now(), TranscriptEventKind::UserMessage),
        TranscriptEvent::new(Utc::now(), TranscriptEventKind::UserMessage),
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

/// SessionListRefreshed removed — sessions now discovered via SessionMetasLoaded/SessionDiscovered.
/// Equivalent: SessionMetasLoaded populates sessions list.
#[test]
fn session_metas_loaded_updates_sessions() {
    let mut state = AppState::new();
    let metas = vec![
        (PathBuf::from("/sessions/s1.json"), SessionMeta::new("s1", Utc::now(), "/proj1".to_string())),
        (PathBuf::from("/sessions/s2.json"), SessionMeta::new("s2", Utc::now(), "/proj2".to_string())),
        (PathBuf::from("/sessions/s3.json"), SessionMeta::new("s3", Utc::now(), "/proj3".to_string())),
    ];

    update(&mut state, AppEvent::SessionMetasLoaded(metas));

    assert_eq!(state.domain.sessions.len(), 3);
    assert_eq!(state.domain.sessions[0].meta.id.as_str(), "s1");
    assert_eq!(state.domain.sessions[2].meta.id.as_str(), "s3");
}

#[test]
fn session_metas_loaded_replaces_existing_list() {
    let mut state = AppState::new();
    state
        .domain.sessions
        .push(ArchivedSession::new(SessionMeta::new("old", Utc::now(), "/old".to_string()), PathBuf::new()));

    let new_metas = vec![
        (PathBuf::from("/sessions/new.json"), SessionMeta::new("new", Utc::now(), "/new".to_string()))
    ];
    update(&mut state, AppEvent::SessionMetasLoaded(new_metas));

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

/// Multiple updates compose: AgentMetadataUpdated + TranscriptEventReceived.
#[test]
fn multiple_updates_compose_correctly() {
    use loom_tui::watcher::TranscriptMetadata;

    let mut state = AppState::new();

    // Discover agent via metadata
    update(&mut state, AppEvent::AgentMetadataUpdated {
        agent_id: AgentId::new("a01"),
        metadata: TranscriptMetadata {
            model: Some("claude-sonnet".to_string()),
            ..Default::default()
        },
    });
    assert_eq!(state.domain.agents.len(), 1);

    // Add transcript event to ring buffer
    let now = Utc::now();
    let event = TranscriptEvent::new(now, TranscriptEventKind::UserMessage);
    update(&mut state, AppEvent::TranscriptEventReceived(event));
    assert_eq!(state.domain.events.len(), 1);

    // All state should be preserved
    assert_eq!(state.domain.agents.len(), 1);
    assert_eq!(state.domain.events.len(), 1);
}

#[test]
fn update_preserves_unmodified_fields() {
    let mut state = AppState::new();
    state.ui.view = ViewState::Sessions;
    state.ui.auto_scroll = false;
    state.ui.show_help = true;

    let event = TranscriptEvent::new(Utc::now(), TranscriptEventKind::UserMessage);
    update(&mut state, AppEvent::TranscriptEventReceived(event));

    // These fields should be preserved
    assert_eq!(state.ui.view, ViewState::Sessions);
    assert_eq!(state.ui.auto_scroll, false);
    assert_eq!(state.ui.show_help, true);
}

#[test]
fn property_event_buffer_never_exceeds_10000() {
    let mut state = AppState::new();
    let now = Utc::now();

    // Stress test: add way more than capacity
    for i in 0..20_000usize {
        let event = TranscriptEvent::new(
            now,
            TranscriptEventKind::AssistantMessage { content: format!("{i}") },
        );
        update(&mut state, AppEvent::TranscriptEventReceived(event));
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
    use loom_tui::watcher::TranscriptMetadata;

    let mut state = AppState::new();

    let events: Vec<AppEvent> = vec![
        AppEvent::AgentMetadataUpdated {
            agent_id: AgentId::new("a01"),
            metadata: TranscriptMetadata::default(),
        },
        AppEvent::TranscriptEventReceived(
            TranscriptEvent::new(Utc::now(), TranscriptEventKind::UserMessage)
        ),
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
        AppEvent::SessionMetasLoaded(vec![]),
        AppEvent::LoadSessionRequested(SessionId::new("test")),
    ];

    for event in events {
        update(&mut state, event); // Should never panic
    }
}
