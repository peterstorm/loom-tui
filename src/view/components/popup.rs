use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};

use crate::app::AppState;
use crate::model::Theme;

use super::event_stream::render_agent_event_stream;

/// Render agent detail popup overlay.
/// Renders on top of the current view with a centered modal window.
pub fn render_agent_popup(frame: &mut Frame, state: &AppState, agent_id: &str) {
    let agent = match state.domain.agents.get(&crate::model::AgentId::new(agent_id)) {
        Some(a) => a,
        None => return, // Agent not found - skip rendering
    };

    // Calculate centered popup area (80% width, 80% height)
    let area = centered_rect(80, 80, frame.area());

    // Clear the area behind the popup
    frame.render_widget(Clear, area);

    // Split popup into header and content
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Header
            Constraint::Min(10),   // Event stream
        ])
        .split(area);

    // Render header with agent info
    let header_text = vec![
        Line::from(vec![
            Span::styled("Agent: ", Style::default().fg(Theme::MUTED_TEXT)),
            Span::styled(
                agent.display_name(),
                Style::default().fg(Theme::AGENT_LABEL).add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(vec![
            Span::styled("Status: ", Style::default().fg(Theme::MUTED_TEXT)),
            Span::styled(
                if agent.finished_at.is_some() { "Finished" } else { "Running" },
                Style::default().fg(if agent.finished_at.is_some() {
                    Theme::MUTED_TEXT
                } else {
                    Theme::SUCCESS
                }),
            ),
        ]),
    ];

    let header = Paragraph::new(header_text)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Theme::ACTIVE_BORDER))
                .title(" Agent Details (Esc to close) "),
        )
        .alignment(Alignment::Left);

    frame.render_widget(header, layout[0]);

    // Render agent event stream
    render_agent_event_stream(
        frame,
        layout[1],
        state,
        agent_id,
        0, // scroll offset (fixed at 0 for popup)
        true, // is focused
    );
}

/// Helper to create a centered rect using up certain percentage of the available rect `r`.
fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}
