use chrono::Utc;

use crate::app::{handle_key, AppState, ViewState};
use crate::event::AppEvent;
use crate::model::{Agent, AgentId, AgentMessage, ArchivedSession, HookEventKind, MessageKind, SessionId, SessionMeta, SessionStatus, ToolCall, ToolName};
use crate::session;
use std::time::Duration;

/// Event handler (Elm-inspired loop). Mutates state in place.
pub fn update(state: &mut AppState, event: AppEvent) {
    let mut agents_changed = false;

    match event {
        AppEvent::TaskGraphUpdated(graph) => {
            let total = graph.total_tasks as u32;
            state.domain.task_graph = Some(graph);
            // Update task count on all active sessions (task graph is project-level)
            for meta in state.domain.active_sessions.values_mut() {
                meta.task_count = total;
            }
        }

        AppEvent::TranscriptUpdated { agent_id, messages } => {
            state.domain.agents.entry(agent_id).and_modify(|agent| {
                agent.messages = messages;
            });
        }

        AppEvent::HookEventReceived(event) => {
            let is_assistant_text = matches!(event.kind, HookEventKind::AssistantText { .. });
            let event_session = &event.session_id;

            // Resolve agent attribution using pure function
            let (agent_id, attribution_confident) = resolve_agent_attribution(
                event.agent_id.as_ref(),
                event_session.as_ref(),
                &state.domain.transcript_agent_map,
                &state.domain.agents,
                is_assistant_text,
            );

            match &event.kind {
                HookEventKind::SubagentStart {
                    ref agent_type,
                    ref task_description,
                } => {
                    if let Some(ref id) = event.agent_id {
                        let resolved_type = agent_type
                            .clone()
                            .or_else(|| task_description.clone());
                        let desc = task_description.clone();
                        let is_new = !state.domain.agents.contains_key(id);
                        state
                            .domain
                            .agents
                            .entry(id.clone())
                            .and_modify(|a| {
                                // Clear finished state on restart
                                a.finished_at = None;
                                a.started_at = event.timestamp;
                                if resolved_type.is_some() {
                                    a.agent_type = resolved_type.clone();
                                }
                                // Update task_description on restart (may have changed)
                                if desc.is_some() {
                                    a.task_description = desc.clone();
                                }
                            })
                            .or_insert_with(|| {
                                let mut a = Agent::new(id.clone(), event.timestamp);
                                a.agent_type = resolved_type;
                                a.task_description = desc;
                                a.session_id = event.session_id.clone();
                                a
                            });
                        agents_changed = true;

                        // Populate transcript_agent_map for subagent event attribution.
                        // Multiple agents may share the same session_id (bulk spawns).
                        if let Some(ref sid) = event.session_id {
                            let agents = state.domain.transcript_agent_map
                                .entry(sid.clone())
                                .or_default();
                            if !agents.contains(id) {
                                agents.push(id.clone());
                            }
                        }

                        // Increment per-session agent count
                        if is_new {
                            if let Some(ref sid) = event.session_id {
                                if let Some(meta) = state.domain.active_sessions.get_mut(sid) {
                                    meta.agent_count += 1;
                                }
                            }
                        }
                    }
                }
                HookEventKind::SubagentStop => {
                    if let Some(ref id) = event.agent_id {
                        state.domain.agents.entry(id.clone()).and_modify(|agent| {
                            if agent.finished_at.is_none() {
                                agent.finished_at = Some(event.timestamp);
                            }
                        });
                        agents_changed = true;
                    }
                }
                HookEventKind::PreToolUse {
                    tool_name,
                    input_summary,
                } => {
                    if let Some(ref id) = agent_id {
                        state.domain.agents.entry(id.clone()).and_modify(|agent| {
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
                        state.domain.agents.entry(id.clone()).and_modify(|agent| {
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
                HookEventKind::SessionStart => {
                    let session_id = event.session_id.clone()
                        .unwrap_or_else(|| format!("s{}", event.timestamp.format("%Y%m%d-%H%M%S")).into());

                    // Only create session if not already active (idempotent)
                    // Prevents duplicate SessionStart events from resetting counters
                    if !state.domain.active_sessions.contains_key(&session_id) {
                        let project_path = event.raw.get("cwd")
                            .and_then(|v| v.as_str())
                            .map(String::from)
                            .unwrap_or_else(|| state.meta.project_path.clone());

                        // Transcript path and git branch are pre-computed in the imperative shell (watcher)
                        let transcript_path = event.raw.get("transcript_path")
                            .and_then(|v| v.as_str())
                            .map(String::from);

                        let git_branch = event.raw.get("git_branch")
                            .and_then(|v| v.as_str())
                            .map(String::from);

                        let mut meta = SessionMeta::new(
                            session_id.clone(),
                            event.timestamp,
                            project_path,
                        );
                        meta.transcript_path = transcript_path;
                        meta.git_branch = git_branch;
                        state.domain.active_sessions.insert(session_id, meta);
                    }
                }
                HookEventKind::SessionEnd => {
                    let session_id = event.session_id.clone()
                        .unwrap_or_else(|| SessionId::new(""));

                    // Mark all active agents in this session as finished
                    for agent in state.domain.agents.values_mut() {
                        if agent.finished_at.is_none() && agent.session_id.as_ref() == Some(&session_id) {
                            agent.finished_at = Some(event.timestamp);
                            agents_changed = true;
                        }
                    }

                    if let Some(mut meta) = state.domain.active_sessions.remove(&session_id) {
                        meta.status = SessionStatus::Completed;
                        // agent_count, event_count, task_count already tracked incrementally
                        let dur = (event.timestamp - meta.timestamp)
                            .to_std()
                            .unwrap_or_default();
                        meta.duration = Some(dur);
                        let archive = session::build_archive(&state.domain, &meta);
                        let archived = ArchivedSession::new(meta, std::path::PathBuf::new())
                            .with_data(archive);
                        state.domain.sessions.insert(0, archived);
                    }
                }
                HookEventKind::Stop { .. } => {
                    // Stop events fire when a session/agent stops. Mark attributed agent
                    // as finished, and also mark all active agents in the session.
                    if let Some(ref id) = agent_id {
                        state.domain.agents.entry(id.clone()).and_modify(|agent| {
                            if agent.finished_at.is_none() {
                                agent.finished_at = Some(event.timestamp);
                                agents_changed = true;
                            }
                        });
                    }
                    if let Some(ref sid) = event.session_id {
                        for agent in state.domain.agents.values_mut() {
                            if agent.finished_at.is_none() && agent.session_id.as_ref() == Some(sid) {
                                agent.finished_at = Some(event.timestamp);
                                agents_changed = true;
                            }
                        }
                    }
                }
                _ => {}
            }

            // Only stamp agent_id on stored event when attribution is unambiguous.
            // With concurrent agents sharing a session, tool events lack agent_id;
            // stamping a guess causes them to show under the wrong agent.
            let mut enriched = event;
            if enriched.agent_id.is_none() && attribution_confident {
                enriched.agent_id = agent_id;
            }

            // Increment per-session event count + update last_event_at + backfill project_path
            if let Some(ref sid) = enriched.session_id {
                if let Some(meta) = state.domain.active_sessions.get_mut(sid) {
                    meta.event_count += 1;
                    meta.last_event_at = Some(enriched.timestamp);
                    // Backfill project_path from cwd if still default
                    if meta.project_path == state.meta.project_path || meta.project_path.is_empty() {
                        if let Some(cwd) = enriched.raw.get("cwd").and_then(|v| v.as_str()) {
                            if !cwd.is_empty() {
                                meta.project_path = cwd.to_string();
                            }
                        }
                    }
                }
            }

            // Ring buffer eviction: pop oldest if at capacity
            if state.domain.events.len() >= 10_000 {
                state.domain.events.pop_front();
            }
            state.domain.events.push_back(enriched);
        }

        AppEvent::AgentStarted(agent_id) => {
            let agent = Agent::new(agent_id.clone(), Utc::now());
            state.domain.agents.insert(agent_id, agent);
            agents_changed = true;
        }

        AppEvent::AgentStopped(agent_id) => {
            state.domain.agents.entry(agent_id).and_modify(|agent| {
                agent.finished_at = Some(Utc::now());
            });
            agents_changed = true;
        }

        AppEvent::Key(key) => {
            handle_key(state, key);
        }

        AppEvent::Tick(now) => {
            // Expire stale sessions (no event received in 5 minutes)
            let cutoff = now - chrono::Duration::minutes(5);
            let stale_ids: Vec<SessionId> = state
                .domain
                .active_sessions
                .iter()
                .filter(|(_, meta)| {
                    meta.last_event_at
                        .map(|t| t < cutoff)
                        .unwrap_or(meta.timestamp < cutoff)
                })
                .map(|(id, _)| id.clone())
                .collect();
            for id in stale_ids {
                if let Some(mut meta) = state.domain.active_sessions.remove(&id) {
                    meta.status = SessionStatus::Cancelled;
                    let dur = (now - meta.timestamp).to_std().unwrap_or_default();
                    meta.duration = Some(dur);
                    let archive = session::build_archive(&state.domain, &meta);
                    let archived = ArchivedSession::new(meta, std::path::PathBuf::new())
                        .with_data(archive);
                    state.domain.sessions.insert(0, archived);
                }
            }
        }

        AppEvent::Error { source, error } => {
            if state.meta.errors.len() >= 100 {
                state.meta.errors.pop_front();
            }
            let error_msg = format!("{}: {}", source, error);
            state.meta.errors.push_back(error_msg);
        }

        AppEvent::SessionLoaded(archive) => {
            if let Some(session) = state.domain.sessions.iter_mut().find(|s| s.meta.id == archive.meta.id) {
                session.data = Some(archive);
            }
            state.ui.loading_session = None;
            state.ui.view = ViewState::SessionDetail;
            state.ui.scroll_offsets.session_detail_left = 0;
            state.ui.scroll_offsets.session_detail_right = 0;
            state.ui.focus = crate::app::PanelFocus::Left;
        }

        AppEvent::SessionListRefreshed(archives) => {
            state.domain.sessions = archives
                .into_iter()
                .map(|a| {
                    let meta = a.meta.clone();
                    ArchivedSession::new(meta, std::path::PathBuf::new()).with_data(a)
                })
                .collect();
        }

        AppEvent::SessionMetasLoaded(metas) => {
            state.domain.sessions = metas
                .into_iter()
                .map(|(path, meta)| ArchivedSession::new(meta, path))
                .collect();
        }

        AppEvent::LoadSessionRequested(idx) => {
            state.ui.loading_session = Some(idx);
        }
    }

    if agents_changed {
        state.recompute_sorted_keys();
    }
}

/// Pure function to resolve agent attribution from event data.
///
/// Returns (agent_id, is_confident) where:
/// - Confident = explicit agent_id or single-candidate match
/// - Not confident = multiple active agents (ambiguous, best guess)
///
/// Attribution strategy:
/// 1. Explicit agent_id → confident
/// 2. Single candidate in transcript_agent_map → confident
/// 3. Multiple candidates → pick most recent active, not confident
/// 4. Fall back to session_id match (unless is_assistant_text) → confident if single match
fn resolve_agent_attribution(
    explicit_agent_id: Option<&AgentId>,
    session_id: Option<&SessionId>,
    transcript_agent_map: &std::collections::BTreeMap<SessionId, Vec<AgentId>>,
    agents: &std::collections::BTreeMap<AgentId, Agent>,
    is_assistant_text: bool,
) -> (Option<AgentId>, bool) {
    // Explicit agent_id is always confident
    if let Some(id) = explicit_agent_id {
        return (Some(id.clone()), true);
    }

    // Try transcript_agent_map (session → agents mapping)
    if let Some(sid) = session_id {
        if let Some(candidates) = transcript_agent_map.get(sid) {
            if candidates.len() == 1 {
                return (Some(candidates[0].clone()), true);
            }

            // Multiple agents — best guess: most recent active agent
            let agent_id = candidates
                .iter()
                .filter_map(|aid| agents.get(aid).map(|a| (aid, a)))
                .filter(|(_, a)| a.finished_at.is_none())
                .max_by_key(|(_, a)| a.started_at)
                .map(|(aid, _)| aid.clone())
                .or_else(|| {
                    // No active agents, pick most recent finished
                    candidates
                        .iter()
                        .filter_map(|aid| agents.get(aid).map(|a| (aid, a)))
                        .max_by_key(|(_, a)| a.started_at)
                        .map(|(aid, _)| aid.clone())
                });

            if agent_id.is_some() {
                return (agent_id, false); // Multiple candidates, not confident
            }
        }
    }

    // AssistantText events should not fall back to session_id matching
    // (main transcript shares parent session_id with subagents)
    if is_assistant_text {
        return (None, false);
    }

    // Last resort: match by session_id among active agents
    let mut matches: Vec<_> = agents
        .iter()
        .filter(|(_, a)| a.finished_at.is_none() && a.session_id.as_ref() == session_id)
        .collect();

    match matches.len() {
        0 => (None, false),
        1 => (Some(matches[0].0.clone()), true),
        _ => {
            // Multiple active agents, pick most recent
            matches.sort_by(|a, b| b.1.started_at.cmp(&a.1.started_at));
            (Some(matches[0].0.clone()), false)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{
        Agent, AgentId, AgentMessage, HookEvent, HookEventKind, SessionArchive, SessionId, SessionMeta, TaskGraph,
        TaskId, Wave,
    };
    use std::collections::BTreeMap;

    #[test]
    fn update_task_graph() {
        let mut state = AppState::new();
        let graph = TaskGraph {
            waves: vec![Wave {
                number: 1,
                tasks: vec![],
            }],
            total_tasks: 0,
            completed_tasks: 0,
        };

        update(&mut state, AppEvent::TaskGraphUpdated(graph.clone()));

        assert!(state.domain.task_graph.is_some());
        assert_eq!(state.domain.task_graph.unwrap().waves.len(), 1);
    }

    #[test]
    fn update_transcript_existing_agent() {
        let mut state = AppState::new();
        let agent = Agent::new("a01", Utc::now());
        state.domain.agents.insert("a01".into(), agent);

        let messages = vec![AgentMessage::reasoning(
            Utc::now(),
            "test reasoning".into(),
        )];

        update(
            &mut state,
            AppEvent::TranscriptUpdated {
                agent_id: "a01".into(),
                messages: messages.clone(),
            },
        );

        assert_eq!(state.domain.agents.get(&AgentId::new("a01")).unwrap().messages.len(), 1);
    }

    #[test]
    fn update_transcript_nonexistent_agent_no_panic() {
        let mut state = AppState::new();
        let messages = vec![AgentMessage::reasoning(
            Utc::now(),
            "test reasoning".into(),
        )];

        update(
            &mut state,
            AppEvent::TranscriptUpdated {
                agent_id: "nonexistent".into(),
                messages,
            },
        );

        assert!(state.domain.agents.is_empty());
    }

    #[test]
    fn hook_event_ring_buffer_eviction() {
        let mut state = AppState::new();
        for i in 0..10_000 {
            let event = HookEvent::new(
                Utc::now(),
                HookEventKind::Notification {
                    message: format!("event {}", i),
                },
            );
            state.domain.events.push_back(event);
        }

        let new_event = HookEvent::new(
            Utc::now(),
            HookEventKind::Notification {
                message: "newest".into(),
            },
        );

        update(&mut state, AppEvent::HookEventReceived(new_event));

        assert_eq!(state.domain.events.len(), 10_000);
        assert!(matches!(
            &state.domain.events.back().unwrap().kind,
            HookEventKind::Notification { message } if message == "newest"
        ));
    }

    #[test]
    fn hook_event_below_capacity_no_eviction() {
        let mut state = AppState::new();
        let event = HookEvent::new(Utc::now(), HookEventKind::SessionStart);

        update(&mut state, AppEvent::HookEventReceived(event));

        assert_eq!(state.domain.events.len(), 1);
    }

    #[test]
    fn agent_started_inserts_new_agent() {
        let mut state = AppState::new();
        update(&mut state, AppEvent::AgentStarted("a01".into()));

        assert_eq!(state.domain.agents.len(), 1);
        assert!(state.domain.agents.contains_key(&AgentId::new("a01")));
        assert!(state.domain.agents.get(&AgentId::new("a01")).unwrap().finished_at.is_none());
    }

    #[test]
    fn agent_stopped_marks_finished() {
        let mut state = AppState::new();
        let agent = Agent::new("a01", Utc::now());
        state.domain.agents.insert("a01".into(), agent);

        update(&mut state, AppEvent::AgentStopped("a01".into()));

        assert!(state.domain.agents.get(&AgentId::new("a01")).unwrap().finished_at.is_some());
    }

    #[test]
    fn agent_stopped_nonexistent_no_panic() {
        let mut state = AppState::new();
        update(&mut state, AppEvent::AgentStopped("nonexistent".into()));

        assert!(state.domain.agents.is_empty());
    }

    #[test]
    fn key_event_delegates_to_navigation() {
        let mut state = AppState::new();
        let key = crossterm::event::KeyEvent::from(crossterm::event::KeyCode::Char('1'));

        update(&mut state, AppEvent::Key(key));

        assert!(matches!(state.ui.view, crate::app::ViewState::Dashboard));
    }

    #[test]
    fn tick_event_is_noop() {
        let mut state = AppState::new();
        let initial_len = state.domain.events.len();
        update(&mut state, AppEvent::Tick(Utc::now()));

        assert_eq!(state.domain.events.len(), initial_len);
    }

    #[test]
    fn parse_error_ring_buffer_eviction() {
        let mut state = AppState::new();
        for i in 0..100 {
            state.meta.errors.push_back(format!("error {}", i));
        }

        update(
            &mut state,
            AppEvent::Error {
                source: "test".into(),
                error: crate::error::WatcherError::Parse(
                    crate::error::ParseError::Json("newest error".into())
                ).into(),
            },
        );

        assert_eq!(state.meta.errors.len(), 100);
        assert!(state.meta.errors.back().unwrap().contains("newest error"));
    }

    #[test]
    fn parse_error_below_capacity_no_eviction() {
        let mut state = AppState::new();
        update(
            &mut state,
            AppEvent::Error {
                source: "file.json".into(),
                error: crate::error::WatcherError::Parse(
                    crate::error::ParseError::Json("invalid JSON".into())
                ).into(),
            },
        );

        assert_eq!(state.meta.errors.len(), 1);
        assert_eq!(state.meta.errors.front().unwrap(), "file.json: parse: JSON parse: invalid JSON");
    }

    #[test]
    fn session_loaded_populates_data() {
        let mut state = AppState::new();
        let meta = SessionMeta::new("s1", Utc::now(), "/proj".to_string());
        let graph = TaskGraph {
            waves: vec![],
            total_tasks: 5,
            completed_tasks: 2,
        };

        let mut agents = BTreeMap::new();
        agents.insert(AgentId::new("a01"), Agent::new("a01", Utc::now()));

        let events = vec![HookEvent::new(Utc::now(), HookEventKind::SessionStart)];

        state.domain.sessions.push(crate::model::ArchivedSession::new(
            meta.clone(),
            std::path::PathBuf::new(),
        ));
        state.ui.loading_session = Some(0);

        let archive = SessionArchive::new(meta.clone())
            .with_task_graph(graph)
            .with_agents(agents)
            .with_events(events);

        update(&mut state, AppEvent::SessionLoaded(archive));

        let data = state.domain.sessions[0].data.as_ref().unwrap();
        assert_eq!(data.task_graph.as_ref().unwrap().total_tasks, 5);
        assert_eq!(data.agents.len(), 1);
        assert_eq!(data.events.len(), 1);
        assert!(state.ui.loading_session.is_none());
        assert!(matches!(state.ui.view, ViewState::SessionDetail));
    }

    #[test]
    fn session_loaded_clears_loading_and_navigates() {
        let mut state = AppState::new();
        let meta = SessionMeta::new("s1", Utc::now(), "/proj".to_string());
        state.domain.sessions.push(crate::model::ArchivedSession::new(
            meta.clone(),
            std::path::PathBuf::new(),
        ));
        state.ui.loading_session = Some(0);

        let archive = SessionArchive::new(meta);
        update(&mut state, AppEvent::SessionLoaded(archive));

        assert!(state.ui.loading_session.is_none());
        assert!(matches!(state.ui.view, ViewState::SessionDetail));
    }

    #[test]
    fn session_list_refreshed() {
        let mut state = AppState::new();
        let sessions = vec![
            SessionArchive::new(SessionMeta::new("s1", Utc::now(), "/proj1".to_string())),
            SessionArchive::new(SessionMeta::new("s2", Utc::now(), "/proj2".to_string())),
        ];

        update(&mut state, AppEvent::SessionListRefreshed(sessions));

        assert_eq!(state.domain.sessions.len(), 2);
        assert_eq!(state.domain.sessions[0].meta.id.as_str(), "s1");
        assert_eq!(state.domain.sessions[1].meta.id.as_str(), "s2");
    }

    #[test]
    fn ring_buffer_property_never_exceeds_capacity() {
        let mut state = AppState::new();

        for i in 0..15_000 {
            let event = HookEvent::new(
                Utc::now(),
                HookEventKind::Notification {
                    message: format!("{}", i),
                },
            );
            update(&mut state, AppEvent::HookEventReceived(event));
        }

        assert_eq!(state.domain.events.len(), 10_000);
    }

    #[test]
    fn error_ring_buffer_property_never_exceeds_capacity() {
        let mut state = AppState::new();

        for i in 0..200 {
            update(
                &mut state,
                AppEvent::Error {
                    source: "test".into(),
                    error: crate::error::WatcherError::Parse(
                        crate::error::ParseError::Json(format!("error {}", i))
                    ).into(),
                },
            );
        }

        assert_eq!(state.meta.errors.len(), 100);
    }

    #[test]
    fn hook_subagent_start_creates_agent() {
        let mut state = AppState::new();
        let event = HookEvent::new(Utc::now(), HookEventKind::subagent_start(None))
            .with_agent("a01");

        update(&mut state, AppEvent::HookEventReceived(event));

        assert_eq!(state.domain.agents.len(), 1);
        assert!(state.domain.agents.get(&AgentId::new("a01")).unwrap().finished_at.is_none());
    }

    #[test]
    fn hook_subagent_stop_finishes_agent() {
        let mut state = AppState::new();
        let start = HookEvent::new(Utc::now(), HookEventKind::subagent_start(None))
            .with_agent("a01");
        let stop =
            HookEvent::new(Utc::now(), HookEventKind::subagent_stop()).with_agent("a01");

        update(&mut state, AppEvent::HookEventReceived(start));
        update(&mut state, AppEvent::HookEventReceived(stop));

        assert!(state.domain.agents.get(&AgentId::new("a01")).unwrap().finished_at.is_some());
    }

    #[test]
    fn hook_subagent_start_idempotent() {
        let mut state = AppState::new();
        let ts = Utc::now();
        let e1 = HookEvent::new(ts, HookEventKind::subagent_start(None))
            .with_agent("a01");
        let e2 = HookEvent::new(ts, HookEventKind::subagent_start(None))
            .with_agent("a01");

        update(&mut state, AppEvent::HookEventReceived(e1));
        update(&mut state, AppEvent::HookEventReceived(e2));

        assert_eq!(state.domain.agents.len(), 1);
    }

    #[test]
    fn hook_pre_tool_use_with_agent_id() {
        let mut state = AppState::new();
        let start = HookEvent::new(Utc::now(), HookEventKind::subagent_start(None))
            .with_agent("a01");
        let tool = HookEvent::new(
            Utc::now(),
            HookEventKind::pre_tool_use("Read", "file.rs".to_string()),
        )
        .with_agent("a01");

        update(&mut state, AppEvent::HookEventReceived(start));
        update(&mut state, AppEvent::HookEventReceived(tool));

        assert_eq!(state.domain.agents.get(&AgentId::new("a01")).unwrap().messages.len(), 1);
        match &state.domain.agents.get(&AgentId::new("a01")).unwrap().messages[0].kind {
            MessageKind::Tool(tc) => {
                assert_eq!(tc.tool_name.as_str(), "Read");
                assert_eq!(tc.input_summary, "file.rs");
                assert!(tc.success.is_none());
            }
            _ => panic!("Expected Tool message"),
        }
    }

    #[test]
    fn hook_post_tool_use_updates_pending() {
        let mut state = AppState::new();
        let start = HookEvent::new(Utc::now(), HookEventKind::subagent_start(None))
            .with_agent("a01");
        let pre = HookEvent::new(
            Utc::now(),
            HookEventKind::pre_tool_use("Read", "file.rs".to_string()),
        )
        .with_agent("a01");
        let post = HookEvent::new(
            Utc::now(),
            HookEventKind::post_tool_use("Read", "ok".to_string(), Some(250)),
        )
        .with_agent("a01");

        update(&mut state, AppEvent::HookEventReceived(start));
        update(&mut state, AppEvent::HookEventReceived(pre));
        update(&mut state, AppEvent::HookEventReceived(post));

        let msg = &state.domain.agents.get(&AgentId::new("a01")).unwrap().messages[0];
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
        let mut state = AppState::new();
        let start = HookEvent::new(Utc::now(), HookEventKind::subagent_start(None))
            .with_agent("a01");
        let tool = HookEvent::new(
            Utc::now(),
            HookEventKind::pre_tool_use("Bash", "cargo test".to_string()),
        );

        update(&mut state, AppEvent::HookEventReceived(start));
        update(&mut state, AppEvent::HookEventReceived(tool));

        assert_eq!(state.domain.agents.get(&AgentId::new("a01")).unwrap().messages.len(), 1);
    }

    #[test]
    fn hook_tool_use_attributed_to_most_recent_with_multiple_agents() {
        // When multiple agents share a session and tool event has no agent_id,
        // attribute to the most recently started agent (best-effort).
        // Transcript-sourced events with explicit agent_id correct attribution.
        let mut state = AppState::new();
        let t1 = Utc::now();
        let t2 = t1 + chrono::Duration::seconds(1);
        let s1 = HookEvent::new(t1, HookEventKind::subagent_start(None))
            .with_agent("a01");
        let s2 = HookEvent::new(t2, HookEventKind::subagent_start(None))
            .with_agent("a02");
        let tool = HookEvent::new(
            Utc::now(),
            HookEventKind::pre_tool_use("Bash", "cargo test".to_string()),
        );

        update(&mut state, AppEvent::HookEventReceived(s1));
        update(&mut state, AppEvent::HookEventReceived(s2));
        update(&mut state, AppEvent::HookEventReceived(tool));

        // Most recently started agent (a02) gets the tool event
        assert_eq!(state.domain.agents.get(&AgentId::new("a01")).unwrap().messages.len(), 0);
        assert_eq!(state.domain.agents.get(&AgentId::new("a02")).unwrap().messages.len(), 1);
    }

    #[test]
    fn hook_tool_use_with_explicit_agent_id_attributed_correctly() {
        // Transcript-sourced events carry explicit agent_id, bypassing fallback
        let mut state = AppState::new();
        let s1 = HookEvent::new(Utc::now(), HookEventKind::subagent_start(None))
            .with_agent("a01");
        let s2 = HookEvent::new(Utc::now(), HookEventKind::subagent_start(None))
            .with_agent("a02");
        let tool = HookEvent::new(
            Utc::now(),
            HookEventKind::pre_tool_use("Read", "file.rs".to_string()),
        )
        .with_agent("a02"); // explicit from transcript

        update(&mut state, AppEvent::HookEventReceived(s1));
        update(&mut state, AppEvent::HookEventReceived(s2));
        update(&mut state, AppEvent::HookEventReceived(tool));

        assert_eq!(state.domain.agents.get(&AgentId::new("a01")).unwrap().messages.len(), 0);
        assert_eq!(state.domain.agents.get(&AgentId::new("a02")).unwrap().messages.len(), 1);
    }

    #[test]
    fn concurrent_session_starts_both_tracked() {
        let mut state = AppState::new();
        let e1 = HookEvent::new(Utc::now(), HookEventKind::SessionStart)
            .with_session("s1");
        let e2 = HookEvent::new(Utc::now(), HookEventKind::SessionStart)
            .with_session("s2");

        update(&mut state, AppEvent::HookEventReceived(e1));
        update(&mut state, AppEvent::HookEventReceived(e2));

        assert_eq!(state.domain.active_sessions.len(), 2);
        assert!(state.domain.active_sessions.contains_key(&SessionId::new("s1")));
        assert!(state.domain.active_sessions.contains_key(&SessionId::new("s2")));
    }

    #[test]
    fn session_end_removes_correct_session() {
        let mut state = AppState::new();
        let e1 = HookEvent::new(Utc::now(), HookEventKind::SessionStart)
            .with_session("s1");
        let e2 = HookEvent::new(Utc::now(), HookEventKind::SessionStart)
            .with_session("s2");
        let end = HookEvent::new(Utc::now(), HookEventKind::SessionEnd)
            .with_session("s1");

        update(&mut state, AppEvent::HookEventReceived(e1));
        update(&mut state, AppEvent::HookEventReceived(e2));
        update(&mut state, AppEvent::HookEventReceived(end));

        assert_eq!(state.domain.active_sessions.len(), 1);
        assert!(!state.domain.active_sessions.contains_key(&SessionId::new("s1")));
        assert!(state.domain.active_sessions.contains_key(&SessionId::new("s2")));
        assert_eq!(state.domain.sessions.len(), 1);
        assert_eq!(state.domain.sessions[0].meta.id.as_str(), "s1");
    }

    #[test]
    fn session_start_does_not_clear_live_state() {
        let mut state = AppState::new();
        state.domain.agents.insert(AgentId::new("a01"), Agent::new("a01", Utc::now()));
        state.domain.events.push_back(HookEvent::new(
            Utc::now(),
            HookEventKind::Notification { message: "existing".into() },
        ));

        let e = HookEvent::new(Utc::now(), HookEventKind::SessionStart)
            .with_session("s1");
        update(&mut state, AppEvent::HookEventReceived(e));

        assert_eq!(state.domain.agents.len(), 1);
        assert_eq!(state.domain.events.len(), 2);
    }

    #[test]
    fn duplicate_session_start_is_idempotent() {
        let mut state = AppState::new();
        let ts = Utc::now();

        // First SessionStart creates the session
        let e1 = HookEvent::new(ts, HookEventKind::SessionStart)
            .with_session("s1");
        update(&mut state, AppEvent::HookEventReceived(e1));

        // Simulate some activity that increments counters
        let agent = HookEvent::new(ts, HookEventKind::subagent_start(None))
            .with_agent("a01")
            .with_session("s1");
        update(&mut state, AppEvent::HookEventReceived(agent));

        let tool = HookEvent::new(ts, HookEventKind::pre_tool_use("Read", "file.rs".into()))
            .with_session("s1");
        update(&mut state, AppEvent::HookEventReceived(tool));

        // Verify counters are set
        assert_eq!(state.domain.active_sessions[&SessionId::new("s1")].agent_count, 1);
        assert_eq!(state.domain.active_sessions[&SessionId::new("s1")].event_count, 3); // SessionStart + SubagentStart + PreToolUse

        // Duplicate SessionStart should be ignored (not reset counters)
        let e2 = HookEvent::new(ts, HookEventKind::SessionStart)
            .with_session("s1");
        update(&mut state, AppEvent::HookEventReceived(e2));

        // Counters should remain unchanged (duplicate event still added to ring buffer and counted)
        assert_eq!(state.domain.active_sessions[&SessionId::new("s1")].agent_count, 1);
        assert_eq!(state.domain.active_sessions[&SessionId::new("s1")].event_count, 4); // +1 for duplicate event
        assert_eq!(state.domain.active_sessions.len(), 1); // Still only 1 session
    }

    #[test]
    fn subagent_start_increments_per_session_agent_count() {
        let mut state = AppState::new();
        let s1 = HookEvent::new(Utc::now(), HookEventKind::SessionStart)
            .with_session("s1");
        let s2 = HookEvent::new(Utc::now(), HookEventKind::SessionStart)
            .with_session("s2");

        update(&mut state, AppEvent::HookEventReceived(s1));
        update(&mut state, AppEvent::HookEventReceived(s2));

        // 2 agents in session s1
        let a1 = HookEvent::new(Utc::now(), HookEventKind::subagent_start(None))
            .with_agent("a01")
            .with_session("s1");
        let a2 = HookEvent::new(Utc::now(), HookEventKind::subagent_start(None))
            .with_agent("a02")
            .with_session("s1");
        // 1 agent in session s2
        let a3 = HookEvent::new(Utc::now(), HookEventKind::subagent_start(None))
            .with_agent("a03")
            .with_session("s2");

        update(&mut state, AppEvent::HookEventReceived(a1));
        update(&mut state, AppEvent::HookEventReceived(a2));
        update(&mut state, AppEvent::HookEventReceived(a3));

        assert_eq!(state.domain.active_sessions[&SessionId::new("s1")].agent_count, 2);
        assert_eq!(state.domain.active_sessions[&SessionId::new("s2")].agent_count, 1);
    }

    #[test]
    fn subagent_start_idempotent_does_not_double_count() {
        let mut state = AppState::new();
        let s = HookEvent::new(Utc::now(), HookEventKind::SessionStart)
            .with_session("s1");
        update(&mut state, AppEvent::HookEventReceived(s));

        let a1 = HookEvent::new(Utc::now(), HookEventKind::subagent_start(None))
            .with_agent("a01")
            .with_session("s1");
        let a1_dup = HookEvent::new(Utc::now(), HookEventKind::subagent_start(None))
            .with_agent("a01")
            .with_session("s1");

        update(&mut state, AppEvent::HookEventReceived(a1));
        update(&mut state, AppEvent::HookEventReceived(a1_dup));

        assert_eq!(state.domain.active_sessions[&SessionId::new("s1")].agent_count, 1);
    }

    #[test]
    fn session_end_preserves_per_session_agent_count() {
        let mut state = AppState::new();
        let s = HookEvent::new(Utc::now(), HookEventKind::SessionStart)
            .with_session("s1");
        let a1 = HookEvent::new(Utc::now(), HookEventKind::subagent_start(None))
            .with_agent("a01")
            .with_session("s1");
        let a2 = HookEvent::new(Utc::now(), HookEventKind::subagent_start(None))
            .with_agent("a02")
            .with_session("s1");
        let end = HookEvent::new(Utc::now(), HookEventKind::SessionEnd)
            .with_session("s1");

        update(&mut state, AppEvent::HookEventReceived(s));
        update(&mut state, AppEvent::HookEventReceived(a1));
        update(&mut state, AppEvent::HookEventReceived(a2));
        update(&mut state, AppEvent::HookEventReceived(end));

        assert_eq!(state.domain.sessions[0].meta.agent_count, 2);
    }

    #[test]
    fn event_count_tracked_per_session() {
        let mut state = AppState::new();
        let s1 = HookEvent::new(Utc::now(), HookEventKind::SessionStart)
            .with_session("s1");
        let s2 = HookEvent::new(Utc::now(), HookEventKind::SessionStart)
            .with_session("s2");
        update(&mut state, AppEvent::HookEventReceived(s1));
        update(&mut state, AppEvent::HookEventReceived(s2));

        // 3 events for s1, 1 for s2
        for _ in 0..3 {
            let e = HookEvent::new(Utc::now(), HookEventKind::notification("msg".to_string()))
                .with_session("s1");
            update(&mut state, AppEvent::HookEventReceived(e));
        }
        let e = HookEvent::new(Utc::now(), HookEventKind::notification("msg".to_string()))
            .with_session("s2");
        update(&mut state, AppEvent::HookEventReceived(e));

        // +1 each for their own SessionStart
        assert_eq!(state.domain.active_sessions[&SessionId::new("s1")].event_count, 4);
        assert_eq!(state.domain.active_sessions[&SessionId::new("s2")].event_count, 2);
    }

    #[test]
    fn task_graph_updated_sets_per_session_task_count() {
        let mut state = AppState::new();
        let s = HookEvent::new(Utc::now(), HookEventKind::SessionStart)
            .with_session("s1");
        update(&mut state, AppEvent::HookEventReceived(s));

        let graph = crate::model::TaskGraph {
            waves: vec![],
            total_tasks: 7,
            completed_tasks: 2,
        };
        update(&mut state, AppEvent::TaskGraphUpdated(graph));

        assert_eq!(state.domain.active_sessions[&SessionId::new("s1")].task_count, 7);
    }

    #[test]
    fn subagent_start_sets_session_id_on_agent() {
        let mut state = AppState::new();
        let s = HookEvent::new(Utc::now(), HookEventKind::SessionStart)
            .with_session("s1");
        let a = HookEvent::new(Utc::now(), HookEventKind::subagent_start(None))
            .with_agent("a01")
            .with_session("s1");
        update(&mut state, AppEvent::HookEventReceived(s));
        update(&mut state, AppEvent::HookEventReceived(a));

        assert_eq!(state.domain.agents[&AgentId::new("a01")].session_id.as_ref(), Some(&SessionId::new("s1")));
    }

    #[test]
    fn hook_tool_use_not_attributed_when_no_agents() {
        let mut state = AppState::new();
        let tool = HookEvent::new(
            Utc::now(),
            HookEventKind::pre_tool_use("Read", "file.rs".to_string()),
        );

        update(&mut state, AppEvent::HookEventReceived(tool));

        assert!(state.domain.agents.is_empty());
    }

    // ============================================================================
    // Stale Session Expiration Tests
    // ============================================================================

    #[test]
    fn stale_session_expired_after_5_minutes() {
        let mut state = AppState::new();
        let now = Utc::now();

        // Create session that's 6 minutes old
        let old_time = now - chrono::Duration::minutes(6);
        let mut meta = SessionMeta::new("s1", old_time, "/proj".to_string());
        meta.last_event_at = Some(old_time);
        state.domain.active_sessions.insert(SessionId::new("s1"), meta);

        // Tick should expire the stale session
        update(&mut state, AppEvent::Tick(now));

        assert!(state.domain.active_sessions.is_empty());
        assert_eq!(state.domain.sessions.len(), 1);
        assert_eq!(state.domain.sessions[0].meta.status, SessionStatus::Cancelled);
    }

    #[test]
    fn active_session_with_recent_events_not_expired() {
        let mut state = AppState::new();
        let now = Utc::now();

        // Create session with recent event (2 minutes ago)
        let recent_time = now - chrono::Duration::minutes(2);
        let mut meta = SessionMeta::new("s1", now - chrono::Duration::minutes(10), "/proj".to_string());
        meta.last_event_at = Some(recent_time);
        state.domain.active_sessions.insert(SessionId::new("s1"), meta);

        // Tick should NOT expire the session
        update(&mut state, AppEvent::Tick(now));

        assert_eq!(state.domain.active_sessions.len(), 1);
        assert!(state.domain.sessions.is_empty());
    }

    #[test]
    fn session_expiration_falls_back_to_timestamp_when_no_last_event() {
        let mut state = AppState::new();
        let now = Utc::now();

        // Create session with old timestamp, no last_event_at
        let old_time = now - chrono::Duration::minutes(6);
        let meta = SessionMeta::new("s1", old_time, "/proj".to_string());
        // last_event_at is None — should fall back to timestamp
        state.domain.active_sessions.insert(SessionId::new("s1"), meta);

        // Tick should expire based on timestamp
        update(&mut state, AppEvent::Tick(now));

        assert!(state.domain.active_sessions.is_empty());
        assert_eq!(state.domain.sessions.len(), 1);
        assert_eq!(state.domain.sessions[0].meta.status, SessionStatus::Cancelled);
    }

    #[test]
    fn expired_session_added_to_archived_list() {
        let mut state = AppState::new();
        let now = Utc::now();

        // Pre-populate archived sessions
        let old_archived = ArchivedSession::new(
            SessionMeta::new("s_old", now - chrono::Duration::hours(1), "/proj".to_string()),
            std::path::PathBuf::new(),
        );
        state.domain.sessions.push(old_archived);

        // Create stale session
        let stale_time = now - chrono::Duration::minutes(6);
        let mut meta = SessionMeta::new("s_stale", stale_time, "/proj".to_string());
        meta.last_event_at = Some(stale_time);
        state.domain.active_sessions.insert(SessionId::new("s_stale"), meta);

        // Tick expires stale session
        update(&mut state, AppEvent::Tick(now));

        // Archived list should have 2 sessions (stale inserted at front)
        assert_eq!(state.domain.sessions.len(), 2);
        assert_eq!(state.domain.sessions[0].meta.id.as_str(), "s_stale");
        assert_eq!(state.domain.sessions[1].meta.id.as_str(), "s_old");
    }

    // ============================================================================
    // Session End Marks Agents Finished
    // ============================================================================

    #[test]
    fn session_end_marks_active_agents_finished() {
        let mut state = AppState::new();
        let ts = Utc::now();
        let s = HookEvent::new(ts, HookEventKind::SessionStart).with_session("s1");
        let a1 = HookEvent::new(ts, HookEventKind::subagent_start(None))
            .with_agent("a01").with_session("s1");
        let a2 = HookEvent::new(ts, HookEventKind::subagent_start(None))
            .with_agent("a02").with_session("s1");
        update(&mut state, AppEvent::HookEventReceived(s));
        update(&mut state, AppEvent::HookEventReceived(a1));
        update(&mut state, AppEvent::HookEventReceived(a2));

        // Both agents active
        assert!(state.domain.agents[&AgentId::new("a01")].finished_at.is_none());
        assert!(state.domain.agents[&AgentId::new("a02")].finished_at.is_none());

        let end = HookEvent::new(ts, HookEventKind::SessionEnd).with_session("s1");
        update(&mut state, AppEvent::HookEventReceived(end));

        // Both agents marked finished
        assert!(state.domain.agents[&AgentId::new("a01")].finished_at.is_some());
        assert!(state.domain.agents[&AgentId::new("a02")].finished_at.is_some());
    }

    #[test]
    fn session_end_does_not_affect_other_session_agents() {
        let mut state = AppState::new();
        let ts = Utc::now();
        let s1 = HookEvent::new(ts, HookEventKind::SessionStart).with_session("s1");
        let s2 = HookEvent::new(ts, HookEventKind::SessionStart).with_session("s2");
        let a1 = HookEvent::new(ts, HookEventKind::subagent_start(None))
            .with_agent("a01").with_session("s1");
        let a2 = HookEvent::new(ts, HookEventKind::subagent_start(None))
            .with_agent("a02").with_session("s2");
        update(&mut state, AppEvent::HookEventReceived(s1));
        update(&mut state, AppEvent::HookEventReceived(s2));
        update(&mut state, AppEvent::HookEventReceived(a1));
        update(&mut state, AppEvent::HookEventReceived(a2));

        // End only s1
        let end = HookEvent::new(ts, HookEventKind::SessionEnd).with_session("s1");
        update(&mut state, AppEvent::HookEventReceived(end));

        assert!(state.domain.agents[&AgentId::new("a01")].finished_at.is_some());
        assert!(state.domain.agents[&AgentId::new("a02")].finished_at.is_none());
    }

    #[test]
    fn session_end_skips_already_finished_agents() {
        let mut state = AppState::new();
        let ts = Utc::now();
        let later = ts + chrono::Duration::seconds(5);
        let s = HookEvent::new(ts, HookEventKind::SessionStart).with_session("s1");
        let a = HookEvent::new(ts, HookEventKind::subagent_start(None))
            .with_agent("a01").with_session("s1");
        let stop = HookEvent::new(ts, HookEventKind::subagent_stop()).with_agent("a01");
        update(&mut state, AppEvent::HookEventReceived(s));
        update(&mut state, AppEvent::HookEventReceived(a));
        update(&mut state, AppEvent::HookEventReceived(stop));

        let original_finish = state.domain.agents[&AgentId::new("a01")].finished_at;

        // SessionEnd should not overwrite earlier finish time
        let end = HookEvent::new(later, HookEventKind::SessionEnd).with_session("s1");
        update(&mut state, AppEvent::HookEventReceived(end));

        assert_eq!(state.domain.agents[&AgentId::new("a01")].finished_at, original_finish);
    }

    // ============================================================================
    // Stop Event Marks Agents Finished
    // ============================================================================

    #[test]
    fn stop_event_marks_session_agents_finished() {
        let mut state = AppState::new();
        let ts = Utc::now();
        let s = HookEvent::new(ts, HookEventKind::SessionStart).with_session("s1");
        let a1 = HookEvent::new(ts, HookEventKind::subagent_start(None))
            .with_agent("a01").with_session("s1");
        let a2 = HookEvent::new(ts, HookEventKind::subagent_start(None))
            .with_agent("a02").with_session("s1");
        update(&mut state, AppEvent::HookEventReceived(s));
        update(&mut state, AppEvent::HookEventReceived(a1));
        update(&mut state, AppEvent::HookEventReceived(a2));

        let stop = HookEvent::new(ts, HookEventKind::stop(Some("done".to_string())))
            .with_session("s1");
        update(&mut state, AppEvent::HookEventReceived(stop));

        assert!(state.domain.agents[&AgentId::new("a01")].finished_at.is_some());
        assert!(state.domain.agents[&AgentId::new("a02")].finished_at.is_some());
    }

    // ============================================================================
    // Multi-Agent Attribution
    // ============================================================================

    #[test]
    fn transcript_agent_map_stores_all_agents() {
        let mut state = AppState::new();
        let ts = Utc::now();
        let a1 = HookEvent::new(ts, HookEventKind::subagent_start(None))
            .with_agent("a01").with_session("s1");
        let a2 = HookEvent::new(ts, HookEventKind::subagent_start(None))
            .with_agent("a02").with_session("s1");

        update(&mut state, AppEvent::HookEventReceived(a1));
        update(&mut state, AppEvent::HookEventReceived(a2));

        let agents = state.domain.transcript_agent_map.get(&SessionId::new("s1")).unwrap();
        assert_eq!(agents.len(), 2);
        assert!(agents.contains(&AgentId::new("a01")));
        assert!(agents.contains(&AgentId::new("a02")));
    }

    #[test]
    fn tool_events_not_enriched_when_multiple_agents_active() {
        let mut state = AppState::new();
        let ts = Utc::now();
        // Two agents on same session
        let a1 = HookEvent::new(ts, HookEventKind::subagent_start(None))
            .with_agent("a01").with_session("s1");
        let a2 = HookEvent::new(ts, HookEventKind::subagent_start(None))
            .with_agent("a02").with_session("s1");
        update(&mut state, AppEvent::HookEventReceived(a1));
        update(&mut state, AppEvent::HookEventReceived(a2));

        // Tool event without agent_id (like real Claude Code tool events)
        let tool = HookEvent::new(ts, HookEventKind::pre_tool_use("Read", "file.rs".to_string()))
            .with_session("s1");
        update(&mut state, AppEvent::HookEventReceived(tool));

        // Event should remain unattributed (agent_id = None)
        let last = state.domain.events.back().unwrap();
        assert!(last.agent_id.is_none(), "ambiguous tool events should not be force-attributed");
    }

    #[test]
    fn tool_events_enriched_when_single_agent_active() {
        let mut state = AppState::new();
        let ts = Utc::now();
        // Single agent on session
        let a1 = HookEvent::new(ts, HookEventKind::subagent_start(None))
            .with_agent("a01").with_session("s1");
        update(&mut state, AppEvent::HookEventReceived(a1));

        let tool = HookEvent::new(ts, HookEventKind::pre_tool_use("Read", "file.rs".to_string()))
            .with_session("s1");
        update(&mut state, AppEvent::HookEventReceived(tool));

        let last = state.domain.events.back().unwrap();
        assert_eq!(last.agent_id, Some(AgentId::new("a01")));
    }

    // --- resolve_agent_attribution unit tests ---

    fn make_agent(id: &str, session: &str, finished: bool) -> (AgentId, Agent) {
        let mut a = Agent::default();
        a.id = AgentId::new(id);
        a.session_id = Some(SessionId::new(session));
        a.started_at = Utc::now();
        if finished {
            a.finished_at = Some(Utc::now());
        }
        (AgentId::new(id), a)
    }

    #[test]
    fn attribution_explicit_id_is_confident() {
        let id = AgentId::new("a01");
        let (resolved, confident) = resolve_agent_attribution(
            Some(&id), None, &BTreeMap::new(), &BTreeMap::new(), false,
        );
        assert_eq!(resolved, Some(AgentId::new("a01")));
        assert!(confident);
    }

    #[test]
    fn attribution_single_transcript_candidate_is_confident() {
        let mut map = BTreeMap::new();
        map.insert(SessionId::new("s1"), vec![AgentId::new("a01")]);
        let agents = BTreeMap::new();
        let sid = SessionId::new("s1");
        let (resolved, confident) = resolve_agent_attribution(
            None, Some(&sid), &map, &agents, false,
        );
        assert_eq!(resolved, Some(AgentId::new("a01")));
        assert!(confident);
    }

    #[test]
    fn attribution_multiple_candidates_not_confident() {
        let mut map = BTreeMap::new();
        map.insert(SessionId::new("s1"), vec![AgentId::new("a01"), AgentId::new("a02")]);
        let agents: BTreeMap<_, _> = [make_agent("a01", "s1", false), make_agent("a02", "s1", false)].into();
        let sid = SessionId::new("s1");
        let (resolved, confident) = resolve_agent_attribution(
            None, Some(&sid), &map, &agents, false,
        );
        assert!(resolved.is_some());
        assert!(!confident);
    }

    #[test]
    fn attribution_assistant_text_no_session_fallback() {
        let agents: BTreeMap<_, _> = [make_agent("a01", "s1", false)].into();
        let sid = SessionId::new("s1");
        let (resolved, confident) = resolve_agent_attribution(
            None, Some(&sid), &BTreeMap::new(), &agents, true,
        );
        assert_eq!(resolved, None);
        assert!(!confident);
    }

    #[test]
    fn attribution_session_fallback_single_match() {
        let agents: BTreeMap<_, _> = [make_agent("a01", "s1", false)].into();
        let sid = SessionId::new("s1");
        let (resolved, confident) = resolve_agent_attribution(
            None, Some(&sid), &BTreeMap::new(), &agents, false,
        );
        assert_eq!(resolved, Some(AgentId::new("a01")));
        assert!(confident);
    }

    #[test]
    fn attribution_no_match_returns_none() {
        let (resolved, confident) = resolve_agent_attribution(
            None, None, &BTreeMap::new(), &BTreeMap::new(), false,
        );
        assert_eq!(resolved, None);
        assert!(!confident);
    }
}
