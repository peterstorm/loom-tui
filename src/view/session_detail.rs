use std::collections::{BTreeMap, VecDeque};

use chrono::Utc;
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
    Frame,
};

use crate::app::state::{AppState, PanelFocus};
use crate::model::{Agent, AgentId, HookEvent, SessionMeta, SessionStatus, TaskGraph, Theme};
use super::components::agent_list::render_agent_list_with_main;
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
    Deque(&'a VecDeque<HookEvent>),
    Vec(&'a Vec<HookEvent>),
    Owned(Vec<HookEvent>),
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

    pub fn iter(&'a self) -> Box<dyn Iterator<Item = &'a HookEvent> + 'a> {
        match self {
            EventsRef::Deque(d) => Box::new(d.iter()),
            EventsRef::Vec(v) => Box::new(v.iter()),
            EventsRef::Owned(v) => Box::new(v.iter()),
        }
    }

    pub fn iter_rev(&'a self) -> Box<dyn Iterator<Item = &'a HookEvent> + 'a> {
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
        let filtered_events: Vec<HookEvent> = state.domain.events.iter()
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

    // Right: per-agent filtered events
    // Index 0 = Main (show agent_id=None events), index n>=1 = sorted_agents[n-1]
    let event_filter = match state.ui.selected_session_agent_index {
        Some(0) => EventFilter::Main,
        Some(n) => match sorted_agents.get(n - 1) {
            Some(agent) => EventFilter::Agent(&agent.id),
            None => EventFilter::All,
        },
        None => EventFilter::All,
    };
    let selected_agent = match state.ui.selected_session_agent_index {
        Some(n) if n >= 1 => sorted_agents.get(n - 1).copied(),
        _ => None,
    };
    render_right_panel(frame, main_chunks[1], &data, &event_filter, state.ui.scroll_offsets.session_detail_right, !is_left_focused);

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
    // Split vertically: [info block ~6 lines] [agent list rest]
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(7), Constraint::Min(0)])
        .split(area);

    render_session_info(frame, chunks[0], data, is_focused);
    render_agent_list_with_main(
        frame,
        chunks[1],
        sorted_agents,
        state.ui.selected_session_agent_index,
        is_focused,
        data.meta,
    );
}

fn render_session_info(frame: &mut Frame, area: Rect, data: &SessionViewData<'_>, is_focused: bool) {
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

/// Which events to show in the right panel.
enum EventFilter<'a> {
    /// Main orchestrator: events with no agent_id
    Main,
    /// Specific agent: strict match on agent_id
    Agent(&'a AgentId),
    /// All events (no filter)
    All,
}

fn render_right_panel(
    frame: &mut Frame,
    area: Rect,
    data: &SessionViewData<'_>,
    filter: &EventFilter<'_>,
    scroll_offset: usize,
    is_focused: bool,
) {
    render_events_list(frame, area, data, filter, scroll_offset, is_focused);
}

fn render_events_list(
    frame: &mut Frame,
    area: Rect,
    data: &SessionViewData<'_>,
    filter: &EventFilter<'_>,
    scroll_offset: usize,
    is_focused: bool,
) {
    let events: Vec<&HookEvent> = data.events.iter_rev()
        .filter(|e| match filter {
            EventFilter::Main => e.agent_id.is_none(),
            EventFilter::Agent(aid) => e.agent_id.as_ref() == Some(*aid),
            EventFilter::All => true,
        })
        .collect();

    if events.is_empty() {
        let p = Paragraph::new("No events")
            .style(Style::default().fg(Theme::MUTED_TEXT))
            .block(
                Block::default()
                    .title(" Events ")
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(if is_focused {
                        Theme::ACTIVE_BORDER
                    } else {
                        Theme::PANEL_BORDER
                    })),
            );
        frame.render_widget(p, area);
        return;
    }

    let mut lines = Vec::new();
    let mut first = true;

    for event in &events {
        if !first {
            lines.push(Line::from(Span::styled(
                "────────────────────────────────",
                Style::default().fg(Theme::SEPARATOR),
            )));
        }
        first = false;

        let timestamp = event.timestamp.format("%H:%M:%S").to_string();
        let (icon, header, detail, event_color, tool_name) =
            crate::view::components::event_stream::format_event_lines(&event.kind);

        let agent_label = event.agent_id.as_ref().map(|aid| {
            data.agents
                .get(aid)
                .map(|a| a.display_name().to_string())
                .unwrap_or_else(|| short_id(aid.as_str()))
        });

        let mut spans = vec![
            Span::styled(format!("{} ", timestamp), Style::default().fg(Theme::MUTED_TEXT)),
            Span::styled(format!("{} ", icon), Style::default().fg(event_color)),
            Span::styled(header, Style::default().fg(event_color)),
        ];

        if let Some(ref label) = agent_label {
            spans.push(Span::styled(
                format!("  {}", label),
                Style::default().fg(Theme::AGENT_LABEL),
            ));
        }

        lines.push(Line::from(spans));

        // Detail with markdown + syntax highlighting (shared with dashboard event stream)
        if let Some(detail_text) = detail {
            let clean = crate::view::components::event_stream::clean_detail(&detail_text);
            if !clean.is_empty() {
                let ext_hint = tool_name
                    .as_ref()
                    .filter(|t| matches!(t.as_str(), "Read" | "Edit" | "Write" | "Grep" | "Glob"))
                    .and_then(|_| {
                        // Strip offset prefix before scanning for extension
                        let (_, text_for_ext) = crate::view::components::event_stream::extract_line_offset(&clean);
                        text_for_ext.lines()
                            .take(5)
                            .find_map(crate::view::components::syntax::detect_extension)
                    });
                lines.extend(crate::view::components::event_stream::render_detail_lines(
                    &clean,
                    ext_hint.as_deref(),
                ));
            }
        }
    }

    // Clamp scroll_offset to u16::MAX to prevent silent truncation overflow
    // Additionally clamp to a reasonable maximum to avoid ratatui internal panics
    let scroll = scroll_offset
        .min(u16::MAX as usize)
        .min(10000) as u16;

    let p = Paragraph::new(lines)
        .block(
            Block::default()
                .title(" Events ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(if is_focused {
                    Theme::ACTIVE_BORDER
                } else {
                    Theme::PANEL_BORDER
                })),
        )
        .wrap(Wrap { trim: false })
        .scroll((scroll, 0));

    frame.render_widget(p, area);
}

fn short_id(id: &str) -> String {
    if id.chars().count() > 7 { id.chars().take(7).collect() } else { id.to_string() }
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
    use crate::model::{Agent, AgentId, ArchivedSession, HookEvent, HookEventKind, SessionArchive, SessionId, SessionMeta, SessionStatus};
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
            HookEvent::new(Utc::now(), HookEventKind::SessionStart),
            HookEvent::new(Utc::now(), HookEventKind::post_tool_use("Read", "ok".to_string(), Some(100))),
            HookEvent::new(Utc::now(), HookEventKind::post_tool_use("Read", "ok".to_string(), Some(200))),
            HookEvent::new(Utc::now(), HookEventKind::post_tool_use("Bash", "ok".to_string(), Some(500))),
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
