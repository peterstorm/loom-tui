use std::collections::{BTreeMap, VecDeque};

use chrono::Utc;
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    prelude::Stylize,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Row, Table, Wrap},
    Frame,
};

use crate::app::state::{AppState, PanelFocus};
use crate::model::{Agent, AgentId, HookEvent, HookEventKind, SessionMeta, SessionStatus, TaskGraph, Theme};
use super::components::format::format_duration;

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

/// Resolve the session data for the currently selected session index.
/// Returns None if no valid session is selected.
pub fn get_selected_session_data(state: &AppState) -> Option<SessionViewData<'_>> {
    let idx = state.ui.selected_session_index?;
    let active_count = state.domain.active_sessions.len();

    if idx < active_count {
        // Active session — filter agents and events by session_id
        let meta = state.domain.active_sessions.values().nth(idx)?;
        let sid = &meta.id;
        let filtered_agents: BTreeMap<AgentId, &Agent> = state.domain.agents.iter()
            .filter(|(_, a)| a.session_id.as_ref() == Some(sid))
            .map(|(k, v)| (k.clone(), v))
            .collect();
        let filtered_events: Vec<HookEvent> = state.domain.events.iter()
            .filter(|e| e.session_id.as_ref() == Some(sid))
            .cloned()
            .collect();
        Some(SessionViewData {
            meta,
            agents: AgentsRef::Filtered(filtered_agents),
            events: EventsRef::Owned(filtered_events),
            task_graph: state.domain.task_graph.as_ref(),
        })
    } else {
        // Archived session — requires loaded data
        let archive_idx = idx - active_count;
        let session = state.domain.sessions.get(archive_idx)?;
        let archive = session.data.as_ref()?;
        Some(SessionViewData {
            meta: &session.meta,
            agents: AgentsRef::Borrowed(&archive.agents),
            events: EventsRef::Vec(&archive.events),
            task_graph: archive.task_graph.as_ref(),
        })
    }
}

// ============================================================================
// Pure stat functions
// ============================================================================

/// Tool usage statistics computed from PostToolUse events.
#[derive(Debug, Clone)]
pub struct ToolStat {
    pub tool_name: String,
    pub count: usize,
    pub avg_duration_ms: Option<u64>,
}

/// Agent summary for display in the session detail agent table.
#[derive(Debug, Clone)]
pub struct AgentSummary {
    pub id: String,
    pub agent_type: String,
    pub finished: bool,
    pub duration_secs: Option<i64>,
    pub tool_count: usize,
}

/// Compute tool usage stats from events. Groups PostToolUse by tool_name.
pub fn compute_tool_stats(events: &EventsRef<'_>) -> Vec<ToolStat> {
    let mut counts: BTreeMap<String, (usize, Vec<u64>)> = BTreeMap::new();

    for event in events.iter() {
        if let HookEventKind::PostToolUse {
            ref tool_name,
            duration_ms,
            ..
        } = event.kind
        {
            let entry = counts.entry(tool_name.to_string()).or_insert((0, Vec::new()));
            entry.0 += 1;
            if let Some(ms) = duration_ms {
                entry.1.push(ms);
            }
        }
    }

    let mut stats: Vec<ToolStat> = counts
        .into_iter()
        .map(|(name, (count, durations))| {
            let avg = if durations.is_empty() {
                None
            } else {
                Some(durations.iter().sum::<u64>() / durations.len() as u64)
            };
            ToolStat {
                tool_name: name,
                count,
                avg_duration_ms: avg,
            }
        })
        .collect();

    // Sort by count descending
    stats.sort_by(|a, b| b.count.cmp(&a.count));
    stats
}

/// Compute agent summaries from agent map.
pub fn compute_agent_summary(agents: &AgentsRef<'_>) -> Vec<AgentSummary> {
    agents
        .values()
        .into_iter()
        .map(|agent| {
            let tool_count = agent
                .messages
                .iter()
                .filter(|m| matches!(m.kind, crate::model::MessageKind::Tool(_)))
                .count();

            let duration_secs = agent.finished_at.map(|f| {
                (f - agent.started_at).num_seconds()
            });

            AgentSummary {
                id: agent.id.to_string(),
                agent_type: agent.display_name().to_string(),
                finished: agent.finished_at.is_some(),
                duration_secs,
                tool_count,
            }
        })
        .collect()
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

    // Split main: [left 35% | right 65%]
    let main_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(35), Constraint::Percentage(65)])
        .split(chunks[1]);

    let is_left_focused = matches!(state.ui.focus, PanelFocus::Left);
    render_left_panel(frame, main_chunks[0], &data, state.ui.scroll_offsets.session_detail_left, is_left_focused);
    render_right_panel(frame, main_chunks[1], &data, state.ui.scroll_offsets.session_detail_right, !is_left_focused);

    render_session_detail_footer(frame, chunks[2]);
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
    scroll_offset: usize,
    is_focused: bool,
) {
    // Split vertically: [info block ~6 lines] [agent table rest]
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(7), Constraint::Min(0)])
        .split(area);

    render_session_info(frame, chunks[0], data, is_focused);
    render_agent_table(frame, chunks[1], data, scroll_offset, is_focused);
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

fn render_agent_table(
    frame: &mut Frame,
    area: Rect,
    data: &SessionViewData<'_>,
    scroll_offset: usize,
    is_focused: bool,
) {
    let summaries = compute_agent_summary(&data.agents);

    if summaries.is_empty() {
        let p = Paragraph::new("No agents")
            .style(Style::default().fg(Theme::MUTED_TEXT))
            .block(
                Block::default()
                    .title(" Agents ")
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

    let header = Row::new(vec!["Type", "Status", "Dur", "Tools"])
        .style(Style::default().fg(Theme::INFO).add_modifier(Modifier::BOLD));

    let rows: Vec<Row> = summaries
        .iter()
        .skip(scroll_offset)
        .map(|s| {
            let status = if s.finished { "Done" } else { "Active" };
            let dur = s.duration_secs.map(|d| format!("{}s", d)).unwrap_or_else(|| "—".into());
            Row::new(vec![
                s.agent_type.clone(),
                status.to_string(),
                dur,
                s.tool_count.to_string(),
            ])
            .fg(if s.finished { Theme::TASK_COMPLETED } else { Theme::TASK_RUNNING })
        })
        .collect();

    let widths = [
        Constraint::Min(10),
        Constraint::Length(6),
        Constraint::Length(6),
        Constraint::Length(5),
    ];

    let table = Table::new(rows, widths)
        .header(header)
        .block(
            Block::default()
                .title(" Agents ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(if is_focused {
                    Theme::ACTIVE_BORDER
                } else {
                    Theme::PANEL_BORDER
                })),
        );

    frame.render_widget(table, area);
}

fn render_right_panel(
    frame: &mut Frame,
    area: Rect,
    data: &SessionViewData<'_>,
    scroll_offset: usize,
    is_focused: bool,
) {
    // Split: [tool stats ~10 lines] [events rest]
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(10), Constraint::Min(0)])
        .split(area);

    render_tool_stats(frame, chunks[0], data, is_focused);
    render_events_list(frame, chunks[1], data, scroll_offset, is_focused);
}

fn render_tool_stats(frame: &mut Frame, area: Rect, data: &SessionViewData<'_>, is_focused: bool) {
    let stats = compute_tool_stats(&data.events);

    if stats.is_empty() {
        let p = Paragraph::new("No tool usage")
            .style(Style::default().fg(Theme::MUTED_TEXT))
            .block(
                Block::default()
                    .title(" Tool Usage ")
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

    let header = Row::new(vec!["Tool", "Count", "Avg (ms)"])
        .style(Style::default().fg(Theme::INFO).add_modifier(Modifier::BOLD));

    let rows: Vec<Row> = stats
        .iter()
        .take(8) // max 8 rows
        .map(|s| {
            let avg = s.avg_duration_ms.map(|ms| ms.to_string()).unwrap_or_else(|| "—".into());
            Row::new(vec![s.tool_name.clone(), s.count.to_string(), avg])
                .fg(Theme::tool_color(&s.tool_name))
        })
        .collect();

    let widths = [
        Constraint::Min(12),
        Constraint::Length(6),
        Constraint::Length(10),
    ];

    let table = Table::new(rows, widths)
        .header(header)
        .block(
            Block::default()
                .title(" Tool Usage ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(if is_focused {
                    Theme::ACTIVE_BORDER
                } else {
                    Theme::PANEL_BORDER
                })),
        );

    frame.render_widget(table, area);
}

fn render_events_list(
    frame: &mut Frame,
    area: Rect,
    data: &SessionViewData<'_>,
    scroll_offset: usize,
    is_focused: bool,
) {
    let events: Vec<&HookEvent> = data.events.iter_rev().collect();

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
                        // Check first few lines for file path/extension
                        clean.lines()
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
        Span::raw(":scroll | "),
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
        let meta = SessionMeta::new("s1", Utc::now(), "/proj".to_string());
        state.domain.active_sessions.insert(SessionId::new("s1"), meta);
        state.ui.selected_session_index = Some(0);
        state.ui.view = crate::app::state::ViewState::SessionDetail;
        let mut a = Agent::new("a01", Utc::now());
        a.session_id = Some(SessionId::new("s1"));
        state.domain.agents.insert("a01".into(), a);

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
    fn compute_tool_stats_empty() {
        let events = EventsRef::Vec(&vec![]);
        let stats = compute_tool_stats(&events);
        assert!(stats.is_empty());
    }

    #[test]
    fn compute_tool_stats_groups_by_tool() {
        let events = vec![
            HookEvent::new(Utc::now(), HookEventKind::post_tool_use("Read", "ok".to_string(), Some(100))),
            HookEvent::new(Utc::now(), HookEventKind::post_tool_use("Read", "ok".to_string(), Some(200))),
            HookEvent::new(Utc::now(), HookEventKind::post_tool_use("Bash", "ok".to_string(), Some(500))),
            HookEvent::new(Utc::now(), HookEventKind::SessionStart), // non-tool event
        ];
        let events_ref = EventsRef::Vec(&events);
        let stats = compute_tool_stats(&events_ref);

        assert_eq!(stats.len(), 2);
        // Sorted by count desc: Read=2, Bash=1
        assert_eq!(stats[0].tool_name, "Read");
        assert_eq!(stats[0].count, 2);
        assert_eq!(stats[0].avg_duration_ms, Some(150));
        assert_eq!(stats[1].tool_name, "Bash");
        assert_eq!(stats[1].count, 1);
        assert_eq!(stats[1].avg_duration_ms, Some(500));
    }

    #[test]
    fn compute_agent_summary_empty() {
        let agents = BTreeMap::new();
        let agents_ref = AgentsRef::Borrowed(&agents);
        let summaries = compute_agent_summary(&agents_ref);
        assert!(summaries.is_empty());
    }

    #[test]
    fn compute_agent_summary_counts_tools() {
        let now = Utc::now();
        let mut agents = BTreeMap::new();
        let agent = Agent::new("a01", now)
            .with_agent_type("Explore".into())
            .add_message(crate::model::AgentMessage::tool(
                now,
                crate::model::ToolCall::new("Read", "file.rs".to_string()),
            ))
            .add_message(crate::model::AgentMessage::reasoning(now, "thinking".into()))
            .finish(now + chrono::Duration::seconds(10));

        agents.insert("a01".into(), agent);

        let agents_ref = AgentsRef::Borrowed(&agents);
        let summaries = compute_agent_summary(&agents_ref);
        assert_eq!(summaries.len(), 1);
        assert_eq!(summaries[0].agent_type, "Explore");
        assert!(summaries[0].finished);
        assert_eq!(summaries[0].tool_count, 1);
        assert_eq!(summaries[0].duration_secs, Some(10));
    }

    #[test]
    fn get_selected_session_data_none_when_no_selection() {
        let state = AppState::new();
        assert!(get_selected_session_data(&state).is_none());
    }

    #[test]
    fn get_selected_session_data_active_session() {
        let mut state = AppState::new();
        state.domain.active_sessions.insert(SessionId::new("s1"), SessionMeta::new("s1", Utc::now(), "/proj".to_string()));
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
        state.domain.active_sessions.insert(SessionId::new("active"), SessionMeta::new("active", Utc::now(), "/proj".to_string()));

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
