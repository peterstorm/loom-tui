use chrono::Utc;

use crate::app::{handle_key, AppState};
use crate::event::AppEvent;
use crate::model::{Agent, AgentMessage, HookEventKind, MessageKind, ToolCall};
use std::time::Duration;

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
            // Derive agent lifecycle and tool calls from hook events
            let agent_id = event.agent_id.clone().or_else(|| {
                // Heuristic: attribute to single active agent
                let active: Vec<_> = state
                    .agents
                    .iter()
                    .filter(|(_, a)| a.finished_at.is_none())
                    .map(|(id, _)| id.clone())
                    .collect();
                if active.len() == 1 {
                    Some(active[0].clone())
                } else {
                    None
                }
            });

            match &event.kind {
                HookEventKind::SubagentStart { ref task_description } => {
                    if let Some(ref id) = event.agent_id {
                        let agent_type = task_description.clone();
                        state
                            .agents
                            .entry(id.clone())
                            .and_modify(|a| {
                                if a.agent_type.is_none() {
                                    a.agent_type = agent_type.clone();
                                }
                            })
                            .or_insert_with(|| {
                                let mut a = Agent::new(id.clone(), event.timestamp);
                                a.agent_type = agent_type;
                                a
                            });
                    }
                }
                HookEventKind::SubagentStop => {
                    if let Some(ref id) = event.agent_id {
                        state.agents.entry(id.clone()).and_modify(|agent| {
                            if agent.finished_at.is_none() {
                                agent.finished_at = Some(event.timestamp);
                            }
                        });
                    }
                }
                HookEventKind::PreToolUse {
                    tool_name,
                    input_summary,
                } => {
                    if let Some(ref id) = agent_id {
                        state.agents.entry(id.clone()).and_modify(|agent| {
                            agent.messages.push(AgentMessage::tool(
                                event.timestamp,
                                ToolCall::new(tool_name.clone(), input_summary.clone()),
                            ));
                        });
                    }
                }
                HookEventKind::PostToolUse {
                    tool_name,
                    result_summary,
                    duration_ms,
                } => {
                    if let Some(ref id) = agent_id {
                        state.agents.entry(id.clone()).and_modify(|agent| {
                            // Update last matching pending tool call
                            if let Some(msg) = agent.messages.iter_mut().rev().find(|m| {
                                matches!(&m.kind, MessageKind::Tool(tc) if tc.tool_name == *tool_name && tc.success.is_none())
                            }) {
                                if let MessageKind::Tool(ref mut tc) = msg.kind {
                                    tc.result_summary = Some(result_summary.clone());
                                    tc.success = Some(true);
                                    if let Some(ms) = duration_ms {
                                        tc.duration = Some(Duration::from_millis(*ms));
                                    }
                                }
                            }
                        });
                    }
                }
                _ => {}
            }

            // Enrich event with attributed agent_id before storing
            let mut enriched = event;
            if enriched.agent_id.is_none() {
                enriched.agent_id = agent_id;
            }

            // Ring buffer eviction: pop oldest if at capacity
            if state.events.len() >= 10_000 {
                state.events.pop_front();
            }
            state.events.push_back(enriched);
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

    #[test]
    fn hook_subagent_start_creates_agent() {
        let state = AppState::new();
        let event = HookEvent::new(Utc::now(), HookEventKind::subagent_start(None))
            .with_agent("a01".into());

        let new_state = update(state, AppEvent::HookEventReceived(event));

        assert_eq!(new_state.agents.len(), 1);
        assert!(new_state.agents.get("a01").unwrap().finished_at.is_none());
    }

    #[test]
    fn hook_subagent_stop_finishes_agent() {
        let state = AppState::new();
        let start = HookEvent::new(Utc::now(), HookEventKind::subagent_start(None))
            .with_agent("a01".into());
        let stop =
            HookEvent::new(Utc::now(), HookEventKind::subagent_stop()).with_agent("a01".into());

        let state = update(state, AppEvent::HookEventReceived(start));
        let state = update(state, AppEvent::HookEventReceived(stop));

        assert!(state.agents.get("a01").unwrap().finished_at.is_some());
    }

    #[test]
    fn hook_subagent_start_idempotent() {
        let state = AppState::new();
        let ts = Utc::now();
        let e1 = HookEvent::new(ts, HookEventKind::subagent_start(None))
            .with_agent("a01".into());
        let e2 = HookEvent::new(ts, HookEventKind::subagent_start(None))
            .with_agent("a01".into());

        let state = update(state, AppEvent::HookEventReceived(e1));
        let state = update(state, AppEvent::HookEventReceived(e2));

        // Should still be 1 agent, not replaced
        assert_eq!(state.agents.len(), 1);
    }

    #[test]
    fn hook_pre_tool_use_with_agent_id() {
        let state = AppState::new();
        let start = HookEvent::new(Utc::now(), HookEventKind::subagent_start(None))
            .with_agent("a01".into());
        let tool = HookEvent::new(
            Utc::now(),
            HookEventKind::pre_tool_use("Read".into(), "file.rs".into()),
        )
        .with_agent("a01".into());

        let state = update(state, AppEvent::HookEventReceived(start));
        let state = update(state, AppEvent::HookEventReceived(tool));

        assert_eq!(state.agents.get("a01").unwrap().messages.len(), 1);
        match &state.agents.get("a01").unwrap().messages[0].kind {
            MessageKind::Tool(tc) => {
                assert_eq!(tc.tool_name, "Read");
                assert_eq!(tc.input_summary, "file.rs");
                assert!(tc.success.is_none()); // pending
            }
            _ => panic!("Expected Tool message"),
        }
    }

    #[test]
    fn hook_post_tool_use_updates_pending() {
        let state = AppState::new();
        let start = HookEvent::new(Utc::now(), HookEventKind::subagent_start(None))
            .with_agent("a01".into());
        let pre = HookEvent::new(
            Utc::now(),
            HookEventKind::pre_tool_use("Read".into(), "file.rs".into()),
        )
        .with_agent("a01".into());
        let post = HookEvent::new(
            Utc::now(),
            HookEventKind::post_tool_use("Read".into(), "ok".into(), Some(250)),
        )
        .with_agent("a01".into());

        let state = update(state, AppEvent::HookEventReceived(start));
        let state = update(state, AppEvent::HookEventReceived(pre));
        let state = update(state, AppEvent::HookEventReceived(post));

        let msg = &state.agents.get("a01").unwrap().messages[0];
        match &msg.kind {
            MessageKind::Tool(tc) => {
                assert_eq!(tc.success, Some(true));
                assert_eq!(
                    tc.duration,
                    Some(std::time::Duration::from_millis(250))
                );
                assert_eq!(tc.result_summary, Some("ok".into()));
            }
            _ => panic!("Expected Tool message"),
        }
    }

    #[test]
    fn hook_tool_use_attributed_to_single_active_agent() {
        let state = AppState::new();
        // Start an agent
        let start = HookEvent::new(Utc::now(), HookEventKind::subagent_start(None))
            .with_agent("a01".into());
        // Tool use WITHOUT agent_id (like real Claude Code hook events)
        let tool = HookEvent::new(
            Utc::now(),
            HookEventKind::pre_tool_use("Bash".into(), "cargo test".into()),
        );

        let state = update(state, AppEvent::HookEventReceived(start));
        let state = update(state, AppEvent::HookEventReceived(tool));

        // Should be attributed to a01 (only active agent)
        assert_eq!(state.agents.get("a01").unwrap().messages.len(), 1);
    }

    #[test]
    fn hook_tool_use_not_attributed_with_multiple_active_agents() {
        let state = AppState::new();
        let s1 = HookEvent::new(Utc::now(), HookEventKind::subagent_start(None))
            .with_agent("a01".into());
        let s2 = HookEvent::new(Utc::now(), HookEventKind::subagent_start(None))
            .with_agent("a02".into());
        // Tool use without agent_id
        let tool = HookEvent::new(
            Utc::now(),
            HookEventKind::pre_tool_use("Bash".into(), "cargo test".into()),
        );

        let state = update(state, AppEvent::HookEventReceived(s1));
        let state = update(state, AppEvent::HookEventReceived(s2));
        let state = update(state, AppEvent::HookEventReceived(tool));

        // Can't attribute — both agents should have 0 messages
        assert_eq!(state.agents.get("a01").unwrap().messages.len(), 0);
        assert_eq!(state.agents.get("a02").unwrap().messages.len(), 0);
    }

    #[test]
    fn hook_tool_use_not_attributed_when_no_agents() {
        let state = AppState::new();
        let tool = HookEvent::new(
            Utc::now(),
            HookEventKind::pre_tool_use("Read".into(), "file.rs".into()),
        );

        let state = update(state, AppEvent::HookEventReceived(tool));

        // No agents, no attribution, no panic
        assert!(state.agents.is_empty());
    }
}
