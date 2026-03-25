use std::collections::{BTreeMap, VecDeque};

use chrono::Utc;
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

use crate::app::state::{AppState, PanelFocus};
use crate::model::{Agent, AgentId, SessionMeta, SessionStatus, TaskGraph, Theme, TranscriptEvent};
use super::components::agent_list::render_agent_list_with_main;
use super::components::event_stream::{render_agent_event_stream_strict, render_session_main_events};
use super::components::format::format_duration;
use super::components::prompt_popup::render_prompt_popup;

// ============================================================================
// Data access: unifies active session vs archived session
// ============================================================================

/// Borrowed view over session data — either live state or an archive.
pub struct SessionViewData<'a> {
    pub meta: &'a SessionMeta,
    pub agents: AgentsRef<'a>,
    pub events: EventsRef<'a>,
    pub task_graph: Option<&'a TaskGraph>,
}

/// Either a borrowed reference or an owned filtered subset of agents.
pub enum AgentsRef<'a> {
    Borrowed(&'a BTreeMap<AgentId, Agent>),
    Filtered(BTreeMap<AgentId, &'a Agent>),
}

impl<'a> AgentsRef<'a> {
    pub fn len(&self) -> usize {
        match self {
            AgentsRef::Borrowed(m) => m.len(),
            AgentsRef::Filtered(m) => m.len(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn values(&self) -> Vec<&Agent> {
        match self {
            AgentsRef::Borrowed(m) => m.values().collect(),
            AgentsRef::Filtered(m) => m.values().copied().collect(),
        }
    }

    pub fn get(&self, key: &AgentId) -> Option<&Agent> {
        match self {
            AgentsRef::Borrowed(m) => m.get(key),
            AgentsRef::Filtered(m) => m.get(key).copied(),
        }
    }

    pub fn contains_key(&self, key: &AgentId) -> bool {
        match self {
            AgentsRef::Borrowed(m) => m.contains_key(key),
            AgentsRef::Filtered(m) => m.contains_key(key),
        }
    }
}

pub enum EventsRef<'a> {
    Deque(&'a VecDeque<TranscriptEvent>),
    Vec(&'a Vec<TranscriptEvent>),
    Owned(Vec<TranscriptEvent>),
}

impl<'a> EventsRef<'a> {
    pub fn len(&self) -> usize {
        match self {
            EventsRef::Deque(d) => d.len(),
            EventsRef::Vec(v) => v.len(),
            EventsRef::Owned(v) => v.len(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn iter(&'a self) -> Box<dyn Iterator<Item = &'a TranscriptEvent> + 'a> {
        match self {
            EventsRef::Deque(d) => Box::new(d.iter()),
            EventsRef::Vec(v) => Box::new(v.iter()),
            EventsRef::Owned(v) => Box::new(v.iter()),
        }
    }

    pub fn iter_rev(&'a self) -> Box<dyn Iterator<Item = &'a TranscriptEvent> + 'a> {
        match self {
            EventsRef::Deque(d) => Box::new(d.iter().rev()),
            EventsRef::Vec(v) => Box::new(v.iter().rev()),
            EventsRef::Owned(v) => Box::new(v.iter().rev()),
        }
    }
}

/// Resolve the session data for the currently selected session.
/// Uses pinned `selected_session_id` (immune to list reordering) when available,
/// falls back to `selected_session_index` for backwards compatibility.
pub fn get_selected_session_data(state: &AppState) -> Option<SessionViewData<'_>> {
    let sid = state.ui.selected_session_id.as_ref().or_else(|| {
        // Fallback: derive session ID from index (used in Sessions list view)
        let idx = state.ui.selected_session_index?;
        let active_count = state.domain.confirmed_active_count();
        if idx < active_count {
            state.domain.confirmed_active_sessions().nth(idx).map(|(id, _)| id)
        } else {
            let archive_idx = idx - active_count;
            state.domain.sessions.get(archive_idx).map(|s| &s.meta.id)
        }
    })?;

    // Try active sessions first
    if let Some(meta) = state.domain.active_sessions.get(sid).filter(|m| m.confirmed) {
        let filtered_agents: BTreeMap<AgentId, &Agent> = state.domain.agents.iter()
            .filter(|(_, a)| a.session_id.as_ref() == Some(sid))
            .map(|(k, v)| (k.clone(), v))
            .collect();
        let filtered_events: Vec<TranscriptEvent> = state.domain.events.iter()
            .filter(|e| e.session_id.as_ref() == Some(sid))
            .cloned()
            .collect();
        return Some(SessionViewData {
            meta,
            agents: AgentsRef::Filtered(filtered_agents),
            events: EventsRef::Owned(filtered_events),
            task_graph: state.domain.task_graph.as_ref(),
        });
    }

    // Try archived sessions
    let session = state.domain.sessions.iter().find(|s| &s.meta.id == sid)?;
    let archive = session.data.as_ref()?;
    Some(SessionViewData {
        meta: &session.meta,
        agents: AgentsRef::Borrowed(&archive.agents),
        events: EventsRef::Vec(&archive.events),
        task_graph: archive.task_graph.as_ref(),
    })
}

// ============================================================================
// Helper: sorted agent list from session data
// ============================================================================

/// Get sorted agent references from session data (active first, then by started_at desc).
fn sorted_session_agents<'a>(data: &'a SessionViewData<'a>) -> Vec<&'a Agent> {
    let mut agents = data.agents.values();
    agents.sort_by(|a, b| {
        let a_active = a.finished_at.is_none();
        let b_active = b.finished_at.is_none();
        b_active
            .cmp(&a_active)
            .then(b.started_at.cmp(&a.started_at))
    });
    agents
}

// ============================================================================
// Renderer
// ============================================================================

/// Render the session detail view.
pub fn render_session_detail(frame: &mut Frame, state: &AppState, area: Rect) {
    let data = match get_selected_session_data(state) {
        Some(d) => d,
        None => {
            // Distinguish between "no selection" and "loading"
            if state.ui.loading_session.is_some() {
                render_loading_session(frame, area);
            } else {
                render_no_session(frame, area);
            }
            return;
        }
    };

    // Layout: [header 3] [main_area] [footer 1]
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(0),
            Constraint::Length(1),
        ])
        .split(area);

    render_session_header(frame, chunks[0], &data);

    // Split main: [left 30% | right 70%]
    let main_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(30), Constraint::Percentage(70)])
        .split(chunks[1]);

    let is_left_focused = matches!(state.ui.focus, PanelFocus::Left);
    let sorted_agents = sorted_session_agents(&data);

    // Left: session info + interactive agent list
    render_left_panel(frame, main_chunks[0], &data, &sorted_agents, state, is_left_focused);

    // Right: per-agent filtered events — reuse the working agent event stream component
    // Index 0 = Main (show all session events), index n>=1 = sorted_agents[n-1]
    let is_right_focused = !is_left_focused;
    let selected_agent = match state.ui.selected_session_agent_index {
        Some(n) if n >= 1 => sorted_agents.get(n - 1).copied(),
        _ => None,
    };
    match selected_agent {
        Some(agent) => {
            // Reuse event stream component with strict matching (no shared events)
            render_agent_event_stream_strict(
                frame,
                main_chunks[1],
                state,
                agent.id.as_str(),
                state.ui.scroll_offsets.session_detail_right,
                is_right_focused,
            );
        }
        None => {
            // Main: read from global ring buffer filtered by session_id
            render_session_main_events(
                frame,
                main_chunks[1],
                state,
                data.meta.id.as_str(),
                state.ui.scroll_offsets.session_detail_right,
                is_right_focused,
            );
        }
    };

    render_session_detail_footer(frame, chunks[2]);

    // Prompt popup overlay — only for agent selections (not Main)
    if state.ui.prompt_popup.is_open() {
        if let Some(agent) = selected_agent {
            let text = agent.task_description.as_deref().unwrap_or("No prompt available");
            render_prompt_popup(
                frame,
                area,
                &agent.display_name(),
                agent.model.as_deref(),
                agent.agent_type.as_deref(),
                text,
                &agent.messages,
                &agent.skills,
                &agent.token_usage,
                state.ui.prompt_popup.scroll(),
            );
        }
    }
}

fn render_loading_session(frame: &mut Frame, area: Rect) {
    let p = Paragraph::new("Loading session…")
        .style(Style::default().fg(Theme::INFO))
        .block(Block::default().borders(Borders::ALL).border_style(Style::default().fg(Theme::PANEL_BORDER)));
    frame.render_widget(p, area);
}

fn render_no_session(frame: &mut Frame, area: Rect) {
    let p = Paragraph::new("No session selected")
        .style(Style::default().fg(Theme::MUTED_TEXT))
        .block(Block::default().borders(Borders::ALL).border_style(Style::default().fg(Theme::PANEL_BORDER)));
    frame.render_widget(p, area);
}

fn render_session_header(frame: &mut Frame, area: Rect, data: &SessionViewData<'_>) {
    let meta = data.meta;
    let status_str = match meta.status {
        SessionStatus::Active => "Active",
        SessionStatus::Completed => "Done",
        SessionStatus::Failed => "Failed",
        SessionStatus::Cancelled => "Cancelled",
    };
    let status_color = match meta.status {
        SessionStatus::Active => Theme::TASK_RUNNING,
        SessionStatus::Completed => Theme::TASK_COMPLETED,
        SessionStatus::Failed => Theme::TASK_FAILED,
        SessionStatus::Cancelled => Theme::MUTED_TEXT,
    };

    // For active sessions, calculate duration from start to now
    let duration_str = match meta.duration {
        Some(d) => format_duration(Some(d)),
        None if meta.status == SessionStatus::Active => {
            let elapsed = Utc::now().signed_duration_since(meta.timestamp);
            format_duration(elapsed.to_std().ok())
        }
        None => format_duration(None),
    };
    let branch_str = meta.git_branch.as_deref().unwrap_or("—");

    let line = Line::from(vec![
        Span::raw("Session: "),
        Span::styled(meta.id.as_str(), Style::default().add_modifier(Modifier::BOLD)),
        Span::raw(" | "),
        Span::styled(status_str, Style::default().fg(status_color)),
        Span::raw(" | "),
        Span::styled(duration_str, Style::default().fg(Theme::INFO)),
        Span::raw(" | "),
        Span::raw(&meta.project_path),
        Span::raw(" | "),
        Span::styled(format!("branch: {}", branch_str), Style::default().fg(Theme::MUTED_TEXT)),
    ]);

    let header = Paragraph::new(line)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Theme::PANEL_BORDER)),
        )
        .style(Style::default().fg(Theme::TEXT));

    frame.render_widget(header, area);
}

fn render_left_panel(
    frame: &mut Frame,
    area: Rect,
    data: &SessionViewData<'_>,
    sorted_agents: &[&Agent],
    state: &AppState,
    is_focused: bool,
) {
    let git_expanded = state.ui.git_panel_expanded;
    let sid = &data.meta.id;
    let git_result = state.domain.git_cache.get(sid);

    if git_expanded {
        if let Some(Ok(ctx)) = git_result {
            let git_height = (ctx.commits.len() + ctx.file_changes.len() + 4)
                .max(4) as u16;
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(8),
                    Constraint::Length(git_height),
                    Constraint::Min(0),
                ])
                .split(area);

            render_session_info(frame, chunks[0], data, git_result, is_focused);
            render_git_panel(frame, chunks[1], ctx, is_focused);
            render_agent_list_with_main(
                frame,
                chunks[2],
                sorted_agents,
                state.ui.selected_session_agent_index,
                is_focused,
                data.meta,
            );
        } else {
            // Expanded but error or no data — just show info + agents
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Length(8), Constraint::Min(0)])
                .split(area);

            render_session_info(frame, chunks[0], data, git_result, is_focused);
            render_agent_list_with_main(
                frame,
                chunks[1],
                sorted_agents,
                state.ui.selected_session_agent_index,
                is_focused,
                data.meta,
            );
        }
    } else {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(8), Constraint::Min(0)])
            .split(area);

        render_session_info(frame, chunks[0], data, git_result, is_focused);
        render_agent_list_with_main(
            frame,
            chunks[1],
            sorted_agents,
            state.ui.selected_session_agent_index,
            is_focused,
            data.meta,
        );
    }
}

fn render_session_info(
    frame: &mut Frame,
    area: Rect,
    data: &SessionViewData<'_>,
    git_result: Option<&Result<crate::model::GitContext, crate::model::GitError>>,
    is_focused: bool,
) {
    let meta = data.meta;
    let started = meta.timestamp.format("%Y-%m-%d %H:%M:%S").to_string();
    // For active sessions, calculate duration from start to now
    let duration_str = match meta.duration {
        Some(d) => format_duration(Some(d)),
        None if meta.status == SessionStatus::Active => {
            let elapsed = Utc::now().signed_duration_since(meta.timestamp);
            format_duration(elapsed.to_std().ok())
        }
        None => format_duration(None),
    };
    let event_count = data.events.len();
    let agent_count = data.agents.len();

    let git_line = match git_result {
        Some(Ok(ctx)) => {
            let ds = &ctx.diff_stat;
            Line::from(vec![
                Span::styled("Git:      ", Style::default().fg(Theme::MUTED_TEXT)),
                Span::raw(format!("{} files ", ds.files_changed)),
                Span::styled(format!("+{}", ds.insertions), Style::default().fg(Theme::TASK_COMPLETED)),
                Span::raw(" "),
                Span::styled(format!("-{}", ds.deletions), Style::default().fg(Theme::TASK_FAILED)),
            ])
        }
        Some(Err(crate::model::GitError::NotARepo)) => Line::from(vec![
            Span::styled("Git:      ", Style::default().fg(Theme::MUTED_TEXT)),
            Span::styled("not a repo", Style::default().fg(Theme::MUTED_TEXT)),
        ]),
        Some(Err(crate::model::GitError::GitNotFound)) => Line::from(vec![
            Span::styled("Git:      ", Style::default().fg(Theme::MUTED_TEXT)),
            Span::styled("git not found", Style::default().fg(Theme::MUTED_TEXT)),
        ]),
        Some(Err(crate::model::GitError::CommandFailed(_))) => Line::from(vec![
            Span::styled("Git:      ", Style::default().fg(Theme::MUTED_TEXT)),
            Span::styled("error", Style::default().fg(Theme::TASK_FAILED)),
        ]),
        None => Line::from(vec![
            Span::styled("Git:      ", Style::default().fg(Theme::MUTED_TEXT)),
            Span::styled("Ctrl+Shift+g", Style::default().fg(Theme::INFO)),
        ]),
    };

    let lines = vec![
        Line::from(vec![
            Span::styled("Started:  ", Style::default().fg(Theme::MUTED_TEXT)),
            Span::raw(started),
        ]),
        Line::from(vec![
            Span::styled("Duration: ", Style::default().fg(Theme::MUTED_TEXT)),
            Span::raw(duration_str),
        ]),
        Line::from(vec![
            Span::styled("Events:   ", Style::default().fg(Theme::MUTED_TEXT)),
            Span::raw(event_count.to_string()),
        ]),
        Line::from(vec![
            Span::styled("Agents:   ", Style::default().fg(Theme::MUTED_TEXT)),
            Span::raw(agent_count.to_string()),
        ]),
        git_line,
    ];

    let p = Paragraph::new(lines)
        .block(
            Block::default()
                .title(" Session Info ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(if is_focused {
                    Theme::ACTIVE_BORDER
                } else {
                    Theme::PANEL_BORDER
                })),
        )
        .style(Style::default().fg(Theme::TEXT));

    frame.render_widget(p, area);
}

fn render_git_panel(
    frame: &mut Frame,
    area: Rect,
    ctx: &crate::model::GitContext,
    is_focused: bool,
) {
    let mut lines = Vec::new();

    if ctx.commits.is_empty() {
        lines.push(Line::from(Span::styled(
            "  no commits during session",
            Style::default().fg(Theme::MUTED_TEXT),
        )));
    } else {
        lines.push(Line::from(Span::styled(
            "Commits:",
            Style::default().add_modifier(Modifier::BOLD).fg(Theme::TEXT),
        )));
        for commit in &ctx.commits {
            lines.push(Line::from(vec![
                Span::styled(format!("  {} ", commit.hash), Style::default().fg(Theme::INFO)),
                Span::raw(&commit.subject),
            ]));
        }

        if !ctx.file_changes.is_empty() {
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                "Files:",
                Style::default().add_modifier(Modifier::BOLD).fg(Theme::TEXT),
            )));
            for fc in &ctx.file_changes {
                lines.push(Line::from(vec![
                    Span::raw(format!("  {} ", fc.path)),
                    Span::styled(format!("+{}", fc.insertions), Style::default().fg(Theme::TASK_COMPLETED)),
                    Span::raw(" "),
                    Span::styled(format!("-{}", fc.deletions), Style::default().fg(Theme::TASK_FAILED)),
                ]));
            }
        }
    }

    let p = Paragraph::new(lines)
        .block(
            Block::default()
                .title(" Git ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(if is_focused {
                    Theme::ACTIVE_BORDER
                } else {
                    Theme::PANEL_BORDER
                })),
        )
        .style(Style::default().fg(Theme::TEXT));

    frame.render_widget(p, area);
}

fn render_session_detail_footer(frame: &mut Frame, area: Rect) {
    let line = Line::from(vec![
        Span::styled("Esc", Style::default().add_modifier(Modifier::BOLD)),
        Span::raw(":back | "),
        Span::styled("h/l", Style::default().add_modifier(Modifier::BOLD)),
        Span::raw(":focus | "),
        Span::styled("j/k", Style::default().add_modifier(Modifier::BOLD)),
        Span::raw(":select/scroll | "),
        Span::styled("p", Style::default().add_modifier(Modifier::BOLD)),
        Span::raw(":prompt | "),
        Span::styled("^G", Style::default().add_modifier(Modifier::BOLD)),
        Span::raw(":git | "),
        Span::styled("?", Style::default().add_modifier(Modifier::BOLD)),
        Span::raw(":help | "),
        Span::styled("q", Style::default().add_modifier(Modifier::BOLD)),
        Span::raw(":quit"),
    ]);

    let footer = Paragraph::new(line)
        .style(Style::default().fg(Theme::TEXT).bg(Theme::FOOTER_BG));

    frame.render_widget(footer, area);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::state::AppState;
    use crate::model::{Agent, AgentId, ArchivedSession, SessionArchive, SessionId, SessionMeta, SessionStatus, TranscriptEvent, TranscriptEventKind};
    use chrono::Utc;
    use std::time::Duration;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;
    use std::collections::BTreeMap;
    use std::path::PathBuf;

    #[test]
    fn render_session_detail_no_session() {
        let backend = TestBackend::new(100, 30);
        let mut terminal = Terminal::new(backend).unwrap();
        let state = AppState::new();

        terminal
            .draw(|frame| render_session_detail(frame, &state, frame.area()))
            .unwrap();
    }

    #[test]
    fn render_session_detail_active_session() {
        let backend = TestBackend::new(100, 30);
        let mut terminal = Terminal::new(backend).unwrap();

        let mut state = AppState::new();
        let mut meta = SessionMeta::new("s1", Utc::now(), "/proj".to_string());
        meta.confirmed = true;
        state.domain.active_sessions.insert(SessionId::new("s1"), meta);
        state.ui.selected_session_index = Some(0);
        state.ui.view = crate::app::state::ViewState::SessionDetail;
        let mut a = Agent::new("a01", Utc::now());
        a.session_id = Some(SessionId::new("s1"));
        state.domain.agents.insert("a01".into(), a);
        state.ui.selected_session_agent_index = Some(0);

        terminal
            .draw(|frame| render_session_detail(frame, &state, frame.area()))
            .unwrap();
    }

    #[test]
    fn render_session_detail_archived_session() {
        let backend = TestBackend::new(100, 30);
        let mut terminal = Terminal::new(backend).unwrap();

        let mut state = AppState::new();
        let meta = SessionMeta::new("s1", Utc::now(), "/proj".to_string())
            .with_status(SessionStatus::Completed)
            .with_duration(Duration::from_secs(300));

        let mut agents = BTreeMap::new();
        agents.insert(AgentId::new("a01"), Agent::new("a01", Utc::now()));

        let events = vec![
            TranscriptEvent::new(Utc::now(), TranscriptEventKind::UserMessage),
            TranscriptEvent::new(Utc::now(), TranscriptEventKind::ToolResult {
                tool_name: "Read".into(),
                result_summary: "ok".to_string(),
                duration_ms: Some(100),
            }),
            TranscriptEvent::new(Utc::now(), TranscriptEventKind::ToolResult {
                tool_name: "Read".into(),
                result_summary: "ok".to_string(),
                duration_ms: Some(200),
            }),
            TranscriptEvent::new(Utc::now(), TranscriptEventKind::ToolResult {
                tool_name: "Bash".into(),
                result_summary: "ok".to_string(),
                duration_ms: Some(500),
            }),
        ];

        let archive = SessionArchive::new(meta.clone())
            .with_agents(agents)
            .with_events(events);

        state.domain.sessions.push(ArchivedSession::new(meta, PathBuf::new()).with_data(archive));
        state.ui.selected_session_index = Some(0);
        state.ui.view = crate::app::state::ViewState::SessionDetail;

        terminal
            .draw(|frame| render_session_detail(frame, &state, frame.area()))
            .unwrap();
    }

    #[test]
    fn render_session_detail_with_focus_right() {
        let backend = TestBackend::new(100, 30);
        let mut terminal = Terminal::new(backend).unwrap();

        let mut state = AppState::new();
        let meta = SessionMeta::new("s1", Utc::now(), "/proj".to_string());
        state.domain.sessions.push(ArchivedSession::new(meta.clone(), PathBuf::new()).with_data(SessionArchive::new(meta)));
        state.ui.selected_session_index = Some(0);
        state.ui.focus = PanelFocus::Right;

        terminal
            .draw(|frame| render_session_detail(frame, &state, frame.area()))
            .unwrap();
    }

    #[test]
    fn sorted_session_agents_active_first() {
        let now = Utc::now();
        let a1 = Agent::new("a01", now).finish(now + chrono::Duration::seconds(10));
        let a2 = Agent::new("a02", now - chrono::Duration::seconds(5));

        let mut agents = BTreeMap::new();
        agents.insert(AgentId::new("a01"), a1);
        agents.insert(AgentId::new("a02"), a2);

        let meta = SessionMeta::new("s1", now, "/proj".to_string());
        let data = SessionViewData {
            meta: &meta,
            agents: AgentsRef::Borrowed(&agents),
            events: EventsRef::Vec(&vec![]),
            task_graph: None,
        };

        let sorted = sorted_session_agents(&data);
        assert_eq!(sorted.len(), 2);
        assert_eq!(sorted[0].id.as_str(), "a02"); // active first
        assert_eq!(sorted[1].id.as_str(), "a01"); // finished
    }

    #[test]
    fn get_selected_session_data_none_when_no_selection() {
        let state = AppState::new();
        assert!(get_selected_session_data(&state).is_none());
    }

    #[test]
    fn get_selected_session_data_active_session() {
        let mut state = AppState::new();
        let mut meta = SessionMeta::new("s1", Utc::now(), "/proj".to_string());
        meta.confirmed = true;
        state.domain.active_sessions.insert(SessionId::new("s1"), meta);
        state.ui.selected_session_index = Some(0);
        let mut a = Agent::new("a01", Utc::now());
        a.session_id = Some(SessionId::new("s1"));
        state.domain.agents.insert("a01".into(), a);

        let data = get_selected_session_data(&state).unwrap();
        assert_eq!(data.meta.id.as_str(), "s1");
        assert_eq!(data.agents.len(), 1);
    }

    #[test]
    fn get_selected_session_data_archived_session() {
        let mut state = AppState::new();
        let mut active_meta = SessionMeta::new("active", Utc::now(), "/proj".to_string());
        active_meta.confirmed = true;
        state.domain.active_sessions.insert(SessionId::new("active"), active_meta);

        let mut archived_agents = BTreeMap::new();
        archived_agents.insert(AgentId::new("a99"), Agent::new("a99", Utc::now()));
        let meta = SessionMeta::new("archived", Utc::now(), "/proj".to_string());
        let archive = SessionArchive::new(meta.clone()).with_agents(archived_agents);
        state.domain.sessions.push(ArchivedSession::new(meta, PathBuf::new()).with_data(archive));

        state.ui.selected_session_index = Some(1); // idx 0=active, idx 1=archived

        let data = get_selected_session_data(&state).unwrap();
        assert_eq!(data.meta.id.as_str(), "archived");
        assert_eq!(data.agents.len(), 1);
        assert!(data.agents.contains_key(&AgentId::new("a99")));
    }

    #[test]
    fn get_selected_session_data_no_active_archived_at_zero() {
        let mut state = AppState::new();
        let meta = SessionMeta::new("s1", Utc::now(), "/proj".to_string());
        let archive = SessionArchive::new(meta.clone());
        state.domain.sessions.push(ArchivedSession::new(meta, PathBuf::new()).with_data(archive));
        state.ui.selected_session_index = Some(0);

        let data = get_selected_session_data(&state).unwrap();
        assert_eq!(data.meta.id.as_str(), "s1");
    }
}
