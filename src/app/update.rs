use std::path::PathBuf;

use crate::app::{handle_key, AppState, ViewState};
use crate::event::AppEvent;
use crate::model::{ArchivedSession, SessionId, SessionMeta, SessionStatus, TranscriptEventKind};
use crate::session;

/// Event handler (Elm-inspired loop). Mutates state in place.
pub fn update(state: &mut AppState, event: AppEvent) {
    let mut agents_changed = false;

    match event {
        AppEvent::TaskGraphUpdated(graph) => {
            let total = graph.total_tasks() as u32;
            state.domain.task_graph = Some(graph);
            // Update task count on all active sessions (task graph is project-level)
            for meta in state.domain.active_sessions.values_mut() {
                meta.task_count = total;
            }
        }

        AppEvent::TranscriptEventReceived(event) => {
            // Attribute to agent if agent_id set
            if let Some(ref agent_id) = event.agent_id {
                // Track tool use on agent
                if let TranscriptEventKind::ToolUse { .. } = &event.kind {
                    state.increment_tool_count(agent_id);
                }
            }

            // Update session metadata for the session this event belongs to
            if let Some(ref sid) = event.session_id {
                if let Some(meta) = state.domain.active_sessions.get_mut(sid) {
                    meta.event_count += 1;
                    meta.last_event_at = Some(event.timestamp);
                    // Confirm session on UserMessage (real user prompt received)
                    if matches!(event.kind, TranscriptEventKind::UserMessage) {
                        meta.confirmed = true;
                    }
                    // Auto-confirm sessions that persist beyond phantom timeout
                    if !meta.confirmed
                        && (event.timestamp - meta.timestamp) > chrono::Duration::seconds(30)
                    {
                        meta.confirmed = true;
                    }
                }
            }

            // Push to ring buffer (evict oldest if at capacity)
            if state.domain.events.len() >= 10_000 {
                state.domain.events.pop_front();
            }
            state.domain.events.push_back(event);
        }

        AppEvent::SessionDiscovered { session_id, transcript_path } => {
            // Only create session if not already tracked (idempotent)
            if !state.domain.active_sessions.contains_key(&session_id) {
                let now = chrono::Utc::now();
                let mut meta = SessionMeta::new(
                    session_id.clone(),
                    now,
                    state.meta.project_path.clone(),
                );
                meta.transcript_path = Some(transcript_path.display().to_string());
                state.domain.active_sessions.insert(session_id, meta);
            }
        }

        AppEvent::SessionCompleted { session_id } => {
            if let Some(mut meta) = state.domain.active_sessions.remove(&session_id) {
                meta.status = SessionStatus::Completed;
                let now = chrono::Utc::now();
                let dur = (now - meta.timestamp).to_std().unwrap_or_default();
                meta.duration = Some(dur);

                let archive = session::build_archive(
                    state.domain.task_graph.as_ref(),
                    &state.domain.events,
                    &state.domain.agents,
                    &meta,
                );
                let archived = ArchivedSession::new(meta, PathBuf::new()).with_data(archive);
                state.domain.sessions.insert(0, archived);
            }
        }

        AppEvent::SessionReactivated { session_id } => {
            // Move from archived back to active, or create fresh entry
            // First check if we have it in sessions list to restore meta
            let archived_meta = state.domain.sessions.iter()
                .find(|s| s.meta.id == session_id)
                .map(|s| s.meta.clone());

            if !state.domain.active_sessions.contains_key(&session_id) {
                let meta = if let Some(mut m) = archived_meta {
                    m.status = SessionStatus::Active;
                    m.last_event_at = Some(chrono::Utc::now());
                    m
                } else {
                    SessionMeta::new(
                        session_id.clone(),
                        chrono::Utc::now(),
                        state.meta.project_path.clone(),
                    )
                };
                state.domain.active_sessions.insert(session_id, meta);
            }
        }

        AppEvent::Key(key) => {
            handle_key(state, key);
        }

        AppEvent::ReplayComplete => {
            state.meta.replay_complete = true;
        }

        AppEvent::Tick(now) => {
            // Skip stale cleanup until initial event replay is done.
            // During replay, historical timestamps would cause all sessions to expire
            // because Tick uses real-time `now` but events have old timestamps.
            if state.meta.replay_complete {
                // Expire stale sessions:
                // - Confirmed sessions: 10 minute timeout (FR-010)
                // - Unconfirmed sessions: 30 second timeout (FR-013)
                let confirmed_cutoff = now - chrono::Duration::minutes(10);
                let unconfirmed_cutoff = now - chrono::Duration::seconds(30);
                let stale_ids: Vec<(SessionId, bool)> = state
                    .domain
                    .active_sessions
                    .iter()
                    .filter(|(_, meta)| {
                        let cutoff = if meta.confirmed { confirmed_cutoff } else { unconfirmed_cutoff };
                        meta.last_event_at
                            .map(|t| t < cutoff)
                            .unwrap_or(meta.timestamp < cutoff)
                    })
                    .map(|(id, meta)| (id.clone(), meta.confirmed))
                    .collect();

                for (id, was_confirmed) in stale_ids {
                    if let Some(mut meta) = state.domain.active_sessions.remove(&id) {
                        // Only archive confirmed sessions; drop phantom sessions silently (FR-013)
                        if was_confirmed {
                            meta.status = SessionStatus::Cancelled;
                            let dur = (now - meta.timestamp).to_std().unwrap_or_default();
                            meta.duration = Some(dur);
                            let archive = session::build_archive(
                                state.domain.task_graph.as_ref(),
                                &state.domain.events,
                                &state.domain.agents,
                                &meta,
                            );
                            let archived = ArchivedSession::new(meta, PathBuf::new()).with_data(archive);
                            state.domain.sessions.insert(0, archived);
                        }
                    }
                }
            }
        }

        AppEvent::Error { source, error } => {
            if state.meta.errors.len() >= 100 {
                state.meta.errors.pop_front();
            }
            // Clear loading state if this error is from a session load
            if let Some(ref sid) = state.ui.loading_session {
                if source.contains(sid.as_str()) {
                    state.ui.loading_session = None;
                }
            }
            let error_msg = format!("{}: {}", source, error);
            state.meta.errors.push_back(error_msg);
        }

        AppEvent::SessionLoaded(archive) => {
            state.ui.loading_session = None;
            if let Some(session) = state.domain.sessions.iter_mut().find(|s| s.meta.id == archive.meta.id) {
                session.data = Some(archive);
                state.ui.view = ViewState::SessionDetail;
                state.ui.scroll_offsets.session_detail_left = 0;
                state.ui.scroll_offsets.session_detail_right = 0;
                state.ui.focus = crate::app::PanelFocus::Left;
            } else {
                state.meta.errors.push_back(format!("session {} not found after load", archive.meta.id));
            }
        }

        AppEvent::SessionMetasLoaded(metas) => {
            state.domain.sessions = metas
                .into_iter()
                .map(|(path, meta)| ArchivedSession::new(meta, path))
                .collect();
        }

        AppEvent::LoadSessionRequested(sid) => {
            state.ui.loading_session = Some(sid);
        }

        AppEvent::AgentMetadataUpdated { agent_id, metadata } => {
            use crate::model::Agent;
            // Ensure agent entry exists (create if metadata arrives before discovery)
            let now = chrono::Utc::now();
            let len_before = state.domain.agents.len();
            let agent = state.domain.agents
                .entry(agent_id.clone())
                .or_insert_with(|| Agent::new(agent_id.clone(), now));

            // SET semantics — watcher sends absolute totals from full file parse.
            if let Some(ref m) = metadata.model {
                agent.model = Some(m.clone());
            }
            agent.token_usage = metadata.token_usage.clone();
            agent.skills = metadata.skills.clone();
            if metadata.task_description.is_some() {
                agent.task_description = metadata.task_description.clone();
            }

            if state.domain.agents.len() > len_before {
                agents_changed = true;
            }
        }
    }

    if agents_changed {
        state.recompute_sorted_keys();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use crate::app::AppState;
    use crate::event::AppEvent;
    use crate::model::{
        Agent, AgentId, SessionId, SessionMeta, TaskGraph, TranscriptEvent, TranscriptEventKind,
        Wave,
    };
    use std::path::PathBuf;

    // -------------------------------------------------------------------------
    // TaskGraphUpdated
    // -------------------------------------------------------------------------

    #[test]
    fn update_task_graph() {
        let mut state = AppState::new();
        let graph = TaskGraph::new(vec![Wave {
            number: 1,
            tasks: vec![],
        }]);

        update(&mut state, AppEvent::TaskGraphUpdated(graph.clone()));

        assert!(state.domain.task_graph.is_some());
        assert_eq!(state.domain.task_graph.unwrap().waves.len(), 1);
    }

    #[test]
    fn task_graph_updated_propagates_task_count_to_active_sessions() {
        use crate::model::{Task, TaskStatus};

        let mut state = AppState::new();
        let sid = SessionId::new("sess-1");
        let meta = SessionMeta::new(sid.clone(), Utc::now(), "/proj".to_string());
        state.domain.active_sessions.insert(sid, meta);

        let graph = TaskGraph::new(vec![Wave::new(
            1,
            vec![
                Task::new("T1", "Task 1".to_string(), TaskStatus::Pending),
                Task::new("T2", "Task 2".to_string(), TaskStatus::Running),
            ],
        )]);

        update(&mut state, AppEvent::TaskGraphUpdated(graph));

        let meta = state.domain.active_sessions.values().next().unwrap();
        assert_eq!(meta.task_count, 2);
    }

    // -------------------------------------------------------------------------
    // TranscriptEventReceived
    // -------------------------------------------------------------------------

    #[test]
    fn transcript_event_received_pushes_to_ring_buffer() {
        let mut state = AppState::new();
        let ts = Utc::now();
        let event = TranscriptEvent::new(ts, TranscriptEventKind::UserMessage)
            .with_session("sess-1");

        update(&mut state, AppEvent::TranscriptEventReceived(event));

        assert_eq!(state.domain.events.len(), 1);
        assert_eq!(state.domain.events[0].kind, TranscriptEventKind::UserMessage);
    }

    #[test]
    fn transcript_event_received_updates_session_event_count() {
        let mut state = AppState::new();
        let sid = SessionId::new("sess-1");
        let now = Utc::now();
        let meta = SessionMeta::new(sid.clone(), now, "/proj".to_string());
        state.domain.active_sessions.insert(sid.clone(), meta);

        let event = TranscriptEvent::new(now, TranscriptEventKind::UserMessage)
            .with_session(sid.clone());
        update(&mut state, AppEvent::TranscriptEventReceived(event));

        let meta = state.domain.active_sessions.get(&sid).unwrap();
        assert_eq!(meta.event_count, 1);
        assert_eq!(meta.last_event_at, Some(now));
    }

    #[test]
    fn transcript_event_user_message_confirms_session() {
        let mut state = AppState::new();
        let sid = SessionId::new("sess-1");
        let now = Utc::now();
        let meta = SessionMeta::new(sid.clone(), now, "/proj".to_string());
        assert!(!meta.confirmed);
        state.domain.active_sessions.insert(sid.clone(), meta);

        let event = TranscriptEvent::new(now, TranscriptEventKind::UserMessage)
            .with_session(sid.clone());
        update(&mut state, AppEvent::TranscriptEventReceived(event));

        assert!(state.domain.active_sessions[&sid].confirmed);
    }

    #[test]
    fn transcript_event_tool_use_increments_tool_count() {
        let mut state = AppState::new();
        let aid = AgentId::new("agent-1");
        let now = Utc::now();
        state.domain.agents.insert(aid.clone(), Agent::new(aid.clone(), now));

        let event = TranscriptEvent::new(
            now,
            TranscriptEventKind::ToolUse {
                tool_name: "Read".into(),
                input_summary: "src/main.rs".to_string(),
            },
        )
        .with_agent(aid.clone());

        update(&mut state, AppEvent::TranscriptEventReceived(event));

        assert_eq!(state.agent_tool_count(&aid), 1);
    }

    #[test]
    fn transcript_event_ring_buffer_evicts_oldest_at_capacity() {
        let mut state = AppState::new();
        let now = Utc::now();

        // Fill to capacity
        for i in 0..10_000usize {
            let content = format!("msg-{i}");
            let event = TranscriptEvent::new(
                now,
                TranscriptEventKind::AssistantMessage { content },
            );
            state.domain.events.push_back(event);
        }
        assert_eq!(state.domain.events.len(), 10_000);

        // First event content is "msg-0"
        assert!(matches!(
            &state.domain.events[0].kind,
            TranscriptEventKind::AssistantMessage { content } if content == "msg-0"
        ));

        // Push one more via update
        let new_event = TranscriptEvent::new(now, TranscriptEventKind::UserMessage);
        update(&mut state, AppEvent::TranscriptEventReceived(new_event));

        // Still at 10_000
        assert_eq!(state.domain.events.len(), 10_000);
        // Oldest (msg-0) evicted; first is now msg-1
        assert!(matches!(
            &state.domain.events[0].kind,
            TranscriptEventKind::AssistantMessage { content } if content == "msg-1"
        ));
        // Last is UserMessage
        assert_eq!(state.domain.events.back().unwrap().kind, TranscriptEventKind::UserMessage);
    }

    #[test]
    fn transcript_event_auto_confirms_session_after_30s() {
        let mut state = AppState::new();
        let sid = SessionId::new("sess-old");
        let started_at = Utc::now() - chrono::Duration::seconds(60);
        let meta = SessionMeta::new(sid.clone(), started_at, "/proj".to_string());
        assert!(!meta.confirmed);
        state.domain.active_sessions.insert(sid.clone(), meta);

        // Event timestamp is 60s after session start → exceeds 30s threshold
        let event_ts = started_at + chrono::Duration::seconds(60);
        let event = TranscriptEvent::new(
            event_ts,
            TranscriptEventKind::AssistantMessage { content: "hello".to_string() },
        )
        .with_session(sid.clone());
        update(&mut state, AppEvent::TranscriptEventReceived(event));

        assert!(state.domain.active_sessions[&sid].confirmed);
    }

    // -------------------------------------------------------------------------
    // SessionDiscovered (FR-009)
    // -------------------------------------------------------------------------

    #[test]
    fn session_discovered_creates_active_session() {
        let mut state = AppState::new();
        let sid = SessionId::new("sess-new");
        let path = PathBuf::from("/home/user/.claude/projects/abc/sess-new.jsonl");

        update(&mut state, AppEvent::SessionDiscovered {
            session_id: sid.clone(),
            transcript_path: path.clone(),
        });

        assert!(state.domain.active_sessions.contains_key(&sid));
        let meta = &state.domain.active_sessions[&sid];
        assert_eq!(meta.transcript_path, Some(path.display().to_string()));
        assert_eq!(meta.status, SessionStatus::Active);
    }

    #[test]
    fn session_discovered_is_idempotent() {
        let mut state = AppState::new();
        let sid = SessionId::new("sess-idempotent");
        let path = PathBuf::from("/tmp/sess.jsonl");

        update(&mut state, AppEvent::SessionDiscovered {
            session_id: sid.clone(),
            transcript_path: path.clone(),
        });
        // Manually set event_count to verify it's not reset
        state.domain.active_sessions.get_mut(&sid).unwrap().event_count = 42;

        update(&mut state, AppEvent::SessionDiscovered {
            session_id: sid.clone(),
            transcript_path: path,
        });

        // event_count unchanged — session not re-created
        assert_eq!(state.domain.active_sessions[&sid].event_count, 42);
    }

    // -------------------------------------------------------------------------
    // SessionCompleted (FR-010)
    // -------------------------------------------------------------------------

    #[test]
    fn session_completed_moves_to_archived() {
        let mut state = AppState::new();
        let sid = SessionId::new("sess-done");
        let now = Utc::now();
        let meta = SessionMeta::new(sid.clone(), now, "/proj".to_string());
        state.domain.active_sessions.insert(sid.clone(), meta);

        update(&mut state, AppEvent::SessionCompleted { session_id: sid.clone() });

        assert!(!state.domain.active_sessions.contains_key(&sid));
        assert_eq!(state.domain.sessions.len(), 1);
        assert_eq!(state.domain.sessions[0].meta.id, sid);
        assert_eq!(state.domain.sessions[0].meta.status, SessionStatus::Completed);
    }

    #[test]
    fn session_completed_unknown_session_is_noop() {
        let mut state = AppState::new();
        update(&mut state, AppEvent::SessionCompleted {
            session_id: SessionId::new("nonexistent"),
        });
        assert!(state.domain.sessions.is_empty());
        assert!(state.domain.active_sessions.is_empty());
    }

    // -------------------------------------------------------------------------
    // SessionReactivated (FR-011)
    // -------------------------------------------------------------------------

    #[test]
    fn session_reactivated_creates_active_entry() {
        let mut state = AppState::new();
        let sid = SessionId::new("sess-reactivated");

        update(&mut state, AppEvent::SessionReactivated { session_id: sid.clone() });

        assert!(state.domain.active_sessions.contains_key(&sid));
        assert_eq!(state.domain.active_sessions[&sid].status, SessionStatus::Active);
    }

    #[test]
    fn session_reactivated_does_not_duplicate_active_session() {
        let mut state = AppState::new();
        let sid = SessionId::new("sess-already-active");
        let now = Utc::now();
        let meta = SessionMeta::new(sid.clone(), now, "/proj".to_string());
        state.domain.active_sessions.insert(sid.clone(), meta);

        update(&mut state, AppEvent::SessionReactivated { session_id: sid.clone() });

        assert_eq!(state.domain.active_sessions.len(), 1);
    }

    #[test]
    fn session_reactivated_restores_meta_from_archive() {
        let mut state = AppState::new();
        let sid = SessionId::new("sess-restore");
        let now = Utc::now();
        let mut meta = SessionMeta::new(sid.clone(), now, "/original-proj".to_string());
        meta.git_branch = Some("main".to_string());
        meta.status = SessionStatus::Completed;

        // Put in archived sessions
        let archived = ArchivedSession::new(meta, PathBuf::new());
        state.domain.sessions.push(archived);

        update(&mut state, AppEvent::SessionReactivated { session_id: sid.clone() });

        assert!(state.domain.active_sessions.contains_key(&sid));
        let active_meta = &state.domain.active_sessions[&sid];
        assert_eq!(active_meta.project_path, "/original-proj");
        assert_eq!(active_meta.git_branch, Some("main".to_string()));
        assert_eq!(active_meta.status, SessionStatus::Active);
    }

    // -------------------------------------------------------------------------
    // Tick timeout logic (FR-010, FR-013)
    // -------------------------------------------------------------------------

    #[test]
    fn tick_does_not_expire_sessions_before_replay_complete() {
        let mut state = AppState::new();
        state.meta.replay_complete = false;

        let sid = SessionId::new("sess-tick");
        // Session with very old last_event_at
        let old_ts = Utc::now() - chrono::Duration::hours(1);
        let mut meta = SessionMeta::new(sid.clone(), old_ts, "/proj".to_string());
        meta.last_event_at = Some(old_ts);
        meta.confirmed = true;
        state.domain.active_sessions.insert(sid.clone(), meta);

        update(&mut state, AppEvent::Tick(Utc::now()));

        // Still active — replay not complete
        assert!(state.domain.active_sessions.contains_key(&sid));
    }

    #[test]
    fn tick_expires_confirmed_session_after_10_minutes() {
        let mut state = AppState::new();
        state.meta.replay_complete = true;

        let sid = SessionId::new("sess-confirmed-expire");
        let old_ts = Utc::now() - chrono::Duration::minutes(15);
        let mut meta = SessionMeta::new(sid.clone(), old_ts, "/proj".to_string());
        meta.confirmed = true;
        meta.last_event_at = Some(old_ts);
        state.domain.active_sessions.insert(sid.clone(), meta);

        update(&mut state, AppEvent::Tick(Utc::now()));

        assert!(!state.domain.active_sessions.contains_key(&sid));
        // Confirmed session archived (Cancelled status)
        assert_eq!(state.domain.sessions.len(), 1);
        assert_eq!(state.domain.sessions[0].meta.status, SessionStatus::Cancelled);
    }

    #[test]
    fn tick_drops_unconfirmed_session_after_30_seconds_without_archiving() {
        let mut state = AppState::new();
        state.meta.replay_complete = true;

        let sid = SessionId::new("sess-phantom");
        let old_ts = Utc::now() - chrono::Duration::seconds(60);
        let mut meta = SessionMeta::new(sid.clone(), old_ts, "/proj".to_string());
        meta.confirmed = false;
        meta.last_event_at = Some(old_ts);
        state.domain.active_sessions.insert(sid.clone(), meta);

        update(&mut state, AppEvent::Tick(Utc::now()));

        assert!(!state.domain.active_sessions.contains_key(&sid));
        // Phantom session NOT archived
        assert!(state.domain.sessions.is_empty());
    }

    #[test]
    fn tick_keeps_recently_active_confirmed_session() {
        let mut state = AppState::new();
        state.meta.replay_complete = true;

        let sid = SessionId::new("sess-active");
        let recent_ts = Utc::now() - chrono::Duration::minutes(2);
        let mut meta = SessionMeta::new(sid.clone(), recent_ts, "/proj".to_string());
        meta.confirmed = true;
        meta.last_event_at = Some(recent_ts);
        state.domain.active_sessions.insert(sid.clone(), meta);

        update(&mut state, AppEvent::Tick(Utc::now()));

        assert!(state.domain.active_sessions.contains_key(&sid));
    }

    // -------------------------------------------------------------------------
    // Error handling
    // -------------------------------------------------------------------------

    #[test]
    fn error_event_pushes_to_error_buffer() {
        use crate::error::{LoomError, WatcherError};

        let mut state = AppState::new();
        update(&mut state, AppEvent::Error {
            source: "watcher".to_string(),
            error: LoomError::Watcher(WatcherError::Io("disk error".to_string())),
        });

        assert_eq!(state.meta.errors.len(), 1);
        assert!(state.meta.errors[0].contains("watcher"));
    }

    #[test]
    fn error_event_evicts_oldest_at_100() {
        use crate::error::{LoomError, WatcherError};

        let mut state = AppState::new();

        // Fill error buffer
        for i in 0..100usize {
            state.meta.errors.push_back(format!("err-{i}"));
        }

        update(&mut state, AppEvent::Error {
            source: "test".to_string(),
            error: LoomError::Watcher(WatcherError::Io("x".to_string())),
        });

        assert_eq!(state.meta.errors.len(), 100);
        // First error (err-0) was evicted
        assert!(!state.meta.errors[0].contains("err-0"));
    }

    // -------------------------------------------------------------------------
    // ReplayComplete
    // -------------------------------------------------------------------------

    #[test]
    fn replay_complete_sets_flag() {
        let mut state = AppState::new();
        assert!(!state.meta.replay_complete);

        update(&mut state, AppEvent::ReplayComplete);

        assert!(state.meta.replay_complete);
    }

    // -------------------------------------------------------------------------
    // SessionLoaded
    // -------------------------------------------------------------------------

    #[test]
    fn session_loaded_updates_archive_and_navigates() {
        use crate::model::SessionArchive;

        let mut state = AppState::new();
        let sid = SessionId::new("sess-load");
        let now = Utc::now();
        let meta = SessionMeta::new(sid.clone(), now, "/proj".to_string());
        let archived = ArchivedSession::new(meta.clone(), PathBuf::from("/tmp/sess-load.json"));
        state.domain.sessions.push(archived);

        let archive = SessionArchive::new(meta);
        update(&mut state, AppEvent::SessionLoaded(archive));

        // Navigation updated
        assert!(matches!(state.ui.view, ViewState::SessionDetail));
        assert!(state.ui.loading_session.is_none());
        // Data populated
        assert!(state.domain.sessions[0].data.is_some());
    }

    // -------------------------------------------------------------------------
    // SessionMetasLoaded
    // -------------------------------------------------------------------------

    #[test]
    fn session_metas_loaded_populates_sessions_list() {
        let mut state = AppState::new();
        let now = Utc::now();
        let metas = vec![
            (PathBuf::from("/tmp/s1.json"), SessionMeta::new("s1", now, "/proj".to_string())),
            (PathBuf::from("/tmp/s2.json"), SessionMeta::new("s2", now, "/proj".to_string())),
        ];

        update(&mut state, AppEvent::SessionMetasLoaded(metas));

        assert_eq!(state.domain.sessions.len(), 2);
    }

    // -------------------------------------------------------------------------
    // AgentMetadataUpdated
    // -------------------------------------------------------------------------

    #[test]
    fn agent_metadata_updated_sets_model_and_tokens() {
        use crate::watcher::TranscriptMetadata;
        use crate::model::TokenUsage;

        let mut state = AppState::new();
        let aid = AgentId::new("agent-meta");
        let now = Utc::now();
        state.domain.agents.insert(aid.clone(), Agent::new(aid.clone(), now));

        let metadata = TranscriptMetadata {
            model: Some("claude-3-opus".to_string()),
            token_usage: TokenUsage {
                input_tokens: 100,
                output_tokens: 50,
                cache_creation_input_tokens: 0,
                cache_read_input_tokens: 0,
            },
            ..Default::default()
        };

        update(&mut state, AppEvent::AgentMetadataUpdated { agent_id: aid.clone(), metadata });

        let agent = &state.domain.agents[&aid];
        assert_eq!(agent.model, Some("claude-3-opus".to_string()));
        assert_eq!(agent.token_usage.input_tokens, 100);
    }

    #[test]
    fn agent_metadata_updated_sets_agents_changed_for_new_agent() {
        use crate::watcher::TranscriptMetadata;

        // New agent → agents_changed must trigger recompute_sorted_keys.
        // We verify indirectly: sorted_agent_keys is updated after update().
        let mut state = AppState::new();
        let aid = AgentId::new("agent-new-sc");

        let metadata = TranscriptMetadata {
            model: Some("claude-3".to_string()),
            ..Default::default()
        };

        // Before: no agents, sorted keys empty
        assert!(state.domain.agents.is_empty());

        update(&mut state, AppEvent::AgentMetadataUpdated { agent_id: aid.clone(), metadata });

        // Agent created
        assert!(state.domain.agents.contains_key(&aid));
        // sorted_agent_keys populated (recompute_sorted_keys was called)
        assert!(!state.sorted_agent_keys().is_empty());
    }

    #[test]
    fn agent_metadata_updated_creates_agent_if_not_exists() {
        use crate::watcher::TranscriptMetadata;

        let mut state = AppState::new();
        let aid = AgentId::new("agent-new");

        let metadata = TranscriptMetadata {
            model: Some("claude-3".to_string()),
            ..Default::default()
        };

        update(&mut state, AppEvent::AgentMetadataUpdated { agent_id: aid.clone(), metadata });

        assert!(state.domain.agents.contains_key(&aid));
        assert_eq!(state.domain.agents[&aid].model, Some("claude-3".to_string()));
    }

    // -------------------------------------------------------------------------
    // SessionLoaded — guard against missing session
    // -------------------------------------------------------------------------

    #[test]
    fn session_loaded_unknown_session_pushes_error_and_no_navigation() {
        use crate::model::SessionArchive;

        let mut state = AppState::new();
        // sessions list is empty — no matching session
        let sid = SessionId::new("ghost");
        let now = Utc::now();
        let meta = SessionMeta::new(sid.clone(), now, "/proj".to_string());
        let archive = SessionArchive::new(meta);

        update(&mut state, AppEvent::SessionLoaded(archive));

        // Should NOT navigate to SessionDetail
        assert!(!matches!(state.ui.view, ViewState::SessionDetail));
        // Should push an error
        assert_eq!(state.meta.errors.len(), 1);
        assert!(state.meta.errors[0].contains("ghost"));
        // loading_session cleared
        assert!(state.ui.loading_session.is_none());
    }

    // -------------------------------------------------------------------------
    // Error — loading_session cleared on matching source
    // -------------------------------------------------------------------------

    #[test]
    fn error_clears_loading_session_when_source_matches() {
        use crate::error::{LoomError, WatcherError};

        let mut state = AppState::new();
        let sid = SessionId::new("sess-failing");
        state.ui.loading_session = Some(sid.clone());

        update(&mut state, AppEvent::Error {
            source: format!("load:{}", sid.as_str()),
            error: LoomError::Watcher(WatcherError::Io("not found".to_string())),
        });

        assert!(state.ui.loading_session.is_none());
    }

    #[test]
    fn error_preserves_loading_session_when_source_unrelated() {
        use crate::error::{LoomError, WatcherError};

        let mut state = AppState::new();
        let sid = SessionId::new("sess-waiting");
        state.ui.loading_session = Some(sid.clone());

        update(&mut state, AppEvent::Error {
            source: "watcher:some_other_file".to_string(),
            error: LoomError::Watcher(WatcherError::Io("unrelated".to_string())),
        });

        // loading_session untouched — source doesn't match session id
        assert!(state.ui.loading_session.is_some());
    }

    // -------------------------------------------------------------------------
    // AgentMetadataUpdated creates new agent
    // -------------------------------------------------------------------------

    #[test]
    fn agent_metadata_creates_agent_and_updates_sorted_keys() {
        use crate::watcher::TranscriptMetadata;

        let mut state = AppState::new();
        let aid = AgentId::new("brand-new-agent");
        assert!(state.domain.agents.is_empty());

        let metadata = TranscriptMetadata {
            model: Some("claude-sonnet".to_string()),
            ..Default::default()
        };

        update(&mut state, AppEvent::AgentMetadataUpdated { agent_id: aid.clone(), metadata });

        assert!(state.domain.agents.contains_key(&aid));
        assert!(!state.sorted_agent_keys().is_empty());
    }
}
