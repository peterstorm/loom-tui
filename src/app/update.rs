use chrono::Utc;

use crate::app::{handle_key, AppState};
use crate::event::AppEvent;
use crate::model::Agent;

/// Pure update function following Elm Architecture.
/// Takes current state and event, returns new state.
/// No I/O, no side effects - fully deterministic and unit testable.
pub fn update(mut state: AppState, event: AppEvent) -> AppState {
    match event {
        AppEvent::TaskGraphUpdated(graph) => {
            state.task_graph = Some(graph);
            state
        }

        AppEvent::TranscriptUpdated { agent_id, messages } => {
            state.agents.entry(agent_id).and_modify(|agent| {
                agent.messages = messages;
            });
            state
        }

        AppEvent::HookEventReceived(event) => {
            // Ring buffer eviction: pop oldest if at capacity
            if state.events.len() >= 10_000 {
                state.events.pop_front();
            }
            state.events.push_back(event);
            state
        }

        AppEvent::AgentStarted(agent_id) => {
            let agent = Agent::new(agent_id.clone(), Utc::now());
            state.agents.insert(agent_id, agent);
            state
        }

        AppEvent::AgentStopped(agent_id) => {
            state.agents.entry(agent_id).and_modify(|agent| {
                agent.finished_at = Some(Utc::now());
            });
            state
        }

        AppEvent::Key(key) => {
            // Delegate to navigation handler
            handle_key(state, key)
        }

        AppEvent::Tick => {
            // Elapsed time computed in view from started_at
            // Tick is passive - no state changes needed
            state
        }

        AppEvent::ParseError { source, error } => {
            // Ring buffer eviction for errors: pop oldest if at capacity
            if state.errors.len() >= 100 {
                state.errors.pop_front();
            }
            let error_msg = format!("{}: {}", source, error);
            state.errors.push_back(error_msg);
            state
        }

        AppEvent::SessionLoaded(archive) => {
            state.active_session = Some(archive.meta);
            state.task_graph = archive.task_graph;
            state.agents = archive.agents;
            // Convert Vec to VecDeque for events
            state.events.clear();
            for event in archive.events {
                state.events.push_back(event);
            }
            state
        }

        AppEvent::SessionListRefreshed(sessions) => {
            state.sessions = sessions;
            state
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{
        Agent, AgentMessage, HookEvent, HookEventKind, SessionArchive, SessionMeta, TaskGraph,
        Wave,
    };
    use std::collections::BTreeMap;

    #[test]
    fn update_task_graph() {
        let state = AppState::new();
        let graph = TaskGraph {
            waves: vec![Wave {
                number: 1,
                tasks: vec![],
            }],
            total_tasks: 0,
            completed_tasks: 0,
        };

        let new_state = update(state, AppEvent::TaskGraphUpdated(graph.clone()));

        assert!(new_state.task_graph.is_some());
        assert_eq!(new_state.task_graph.unwrap().waves.len(), 1);
    }

    #[test]
    fn update_transcript_existing_agent() {
        let mut state = AppState::new();
        let agent = Agent::new("a01".into(), Utc::now());
        state.agents.insert("a01".into(), agent);

        let messages = vec![AgentMessage::reasoning(
            Utc::now(),
            "test reasoning".into(),
        )];

        let new_state = update(
            state,
            AppEvent::TranscriptUpdated {
                agent_id: "a01".into(),
                messages: messages.clone(),
            },
        );

        assert_eq!(new_state.agents.get("a01").unwrap().messages.len(), 1);
    }

    #[test]
    fn update_transcript_nonexistent_agent_no_panic() {
        let state = AppState::new();
        let messages = vec![AgentMessage::reasoning(
            Utc::now(),
            "test reasoning".into(),
        )];

        let new_state = update(
            state,
            AppEvent::TranscriptUpdated {
                agent_id: "nonexistent".into(),
                messages,
            },
        );

        // Should not panic, just be a no-op since agent doesn't exist
        assert!(new_state.agents.is_empty());
    }

    #[test]
    fn hook_event_ring_buffer_eviction() {
        let mut state = AppState::new();
        // Fill to capacity
        for i in 0..10_000 {
            let event = HookEvent::new(
                Utc::now(),
                HookEventKind::Notification {
                    message: format!("event {}", i),
                },
            );
            state.events.push_back(event);
        }

        // Add one more - should evict oldest
        let new_event = HookEvent::new(
            Utc::now(),
            HookEventKind::Notification {
                message: "newest".into(),
            },
        );

        let new_state = update(state, AppEvent::HookEventReceived(new_event.clone()));

        assert_eq!(new_state.events.len(), 10_000);
        // Last event should be the newest one
        assert!(matches!(
            &new_state.events.back().unwrap().kind,
            HookEventKind::Notification { message } if message == "newest"
        ));
    }

    #[test]
    fn hook_event_below_capacity_no_eviction() {
        let state = AppState::new();
        let event = HookEvent::new(Utc::now(), HookEventKind::SessionStart);

        let new_state = update(state, AppEvent::HookEventReceived(event));

        assert_eq!(new_state.events.len(), 1);
    }

    #[test]
    fn agent_started_inserts_new_agent() {
        let state = AppState::new();
        let new_state = update(state, AppEvent::AgentStarted("a01".into()));

        assert_eq!(new_state.agents.len(), 1);
        assert!(new_state.agents.contains_key("a01"));
        assert!(new_state.agents.get("a01").unwrap().finished_at.is_none());
    }

    #[test]
    fn agent_stopped_marks_finished() {
        let mut state = AppState::new();
        let agent = Agent::new("a01".into(), Utc::now());
        state.agents.insert("a01".into(), agent);

        let new_state = update(state, AppEvent::AgentStopped("a01".into()));

        assert!(new_state.agents.get("a01").unwrap().finished_at.is_some());
    }

    #[test]
    fn agent_stopped_nonexistent_no_panic() {
        let state = AppState::new();
        let new_state = update(state, AppEvent::AgentStopped("nonexistent".into()));

        // Should not panic
        assert!(new_state.agents.is_empty());
    }

    #[test]
    fn key_event_delegates_to_navigation() {
        let state = AppState::new();
        let key = crossterm::event::KeyEvent::from(crossterm::event::KeyCode::Char('1'));

        let new_state = update(state, AppEvent::Key(key));

        // Key events are delegated to handle_key in navigation module
        // '1' switches to dashboard (which is already the default, so unchanged)
        assert!(matches!(new_state.view, crate::app::ViewState::Dashboard));
    }

    #[test]
    fn tick_event_is_noop() {
        let state = AppState::new();
        let new_state = update(state.clone(), AppEvent::Tick);

        // Tick doesn't change state - elapsed time computed in view
        assert_eq!(new_state.events.len(), state.events.len());
    }

    #[test]
    fn parse_error_ring_buffer_eviction() {
        let mut state = AppState::new();
        // Fill to capacity
        for i in 0..100 {
            state.errors.push_back(format!("error {}", i));
        }

        let new_state = update(
            state,
            AppEvent::ParseError {
                source: "test".into(),
                error: "newest error".into(),
            },
        );

        assert_eq!(new_state.errors.len(), 100);
        assert!(new_state
            .errors
            .back()
            .unwrap()
            .contains("newest error"));
    }

    #[test]
    fn parse_error_below_capacity_no_eviction() {
        let state = AppState::new();
        let new_state = update(
            state,
            AppEvent::ParseError {
                source: "file.json".into(),
                error: "invalid JSON".into(),
            },
        );

        assert_eq!(new_state.errors.len(), 1);
        assert_eq!(
            new_state.errors.front().unwrap(),
            "file.json: invalid JSON"
        );
    }

    #[test]
    fn session_loaded_sets_state() {
        let state = AppState::new();
        let meta = SessionMeta::new("s1".into(), Utc::now(), "/proj".into());
        let graph = TaskGraph {
            waves: vec![],
            total_tasks: 5,
            completed_tasks: 2,
        };

        let mut agents = BTreeMap::new();
        agents.insert("a01".into(), Agent::new("a01".into(), Utc::now()));

        let events = vec![HookEvent::new(Utc::now(), HookEventKind::SessionStart)];

        let archive = SessionArchive::new(meta.clone())
            .with_task_graph(graph.clone())
            .with_agents(agents.clone())
            .with_events(events.clone());

        let new_state = update(state, AppEvent::SessionLoaded(archive));

        assert_eq!(new_state.active_session, Some(meta));
        assert_eq!(new_state.task_graph.unwrap().total_tasks, 5);
        assert_eq!(new_state.agents.len(), 1);
        assert_eq!(new_state.events.len(), 1);
    }

    #[test]
    fn session_loaded_replaces_existing_state() {
        let mut state = AppState::new();
        state.events.push_back(HookEvent::new(
            Utc::now(),
            HookEventKind::Notification {
                message: "old event".into(),
            },
        ));
        state
            .agents
            .insert("old".into(), Agent::new("old".into(), Utc::now()));

        let meta = SessionMeta::new("s1".into(), Utc::now(), "/proj".into());
        let archive = SessionArchive::new(meta.clone());

        let new_state = update(state, AppEvent::SessionLoaded(archive));

        // Old state should be replaced
        assert_eq!(new_state.events.len(), 0);
        assert_eq!(new_state.agents.len(), 0);
        assert_eq!(new_state.active_session, Some(meta));
    }

    #[test]
    fn session_list_refreshed() {
        let state = AppState::new();
        let sessions = vec![
            SessionMeta::new("s1".into(), Utc::now(), "/proj1".into()),
            SessionMeta::new("s2".into(), Utc::now(), "/proj2".into()),
        ];

        let new_state = update(state, AppEvent::SessionListRefreshed(sessions.clone()));

        assert_eq!(new_state.sessions.len(), 2);
        assert_eq!(new_state.sessions, sessions);
    }

    #[test]
    fn ring_buffer_property_never_exceeds_capacity() {
        let mut state = AppState::new();

        // Add 15,000 events (50% over capacity)
        for i in 0..15_000 {
            let event = HookEvent::new(
                Utc::now(),
                HookEventKind::Notification {
                    message: format!("{}", i),
                },
            );
            state = update(state, AppEvent::HookEventReceived(event));
        }

        assert_eq!(state.events.len(), 10_000);
    }

    #[test]
    fn error_ring_buffer_property_never_exceeds_capacity() {
        let mut state = AppState::new();

        // Add 200 errors (100% over capacity)
        for i in 0..200 {
            state = update(
                state,
                AppEvent::ParseError {
                    source: "test".into(),
                    error: format!("error {}", i),
                },
            );
        }

        assert_eq!(state.errors.len(), 100);
    }
}
