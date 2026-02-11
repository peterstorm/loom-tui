use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

use crate::app::state::{AppState, PanelFocus};
use crate::model::Theme;
use crate::view::components::{render_reasoning_panel, render_tool_call_list};

/// Pure rendering function: render agent detail view
pub fn render_agent_detail(frame: &mut Frame, state: &AppState, agent_id: &str) {
    let area = frame.area();

    // Find the agent
    let agent = match state.agents.get(agent_id) {
        Some(a) => a,
        None => {
            render_agent_not_found(frame, area, agent_id);
            return;
        }
    };

    // Layout: [header][main_area][footer]
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // header
            Constraint::Min(0),    // main area
            Constraint::Length(1), // footer
        ])
        .split(area);

    render_agent_header(frame, chunks[0], agent_id, agent, state);

    // Split main area horizontally: [tool_call_list | reasoning_panel]
    let main_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(40), // tool call list
            Constraint::Percentage(60), // reasoning panel
        ])
        .split(chunks[1]);

    let is_left_focused = matches!(state.focus, PanelFocus::Left);
    let is_right_focused = matches!(state.focus, PanelFocus::Right);

    render_tool_call_list(
        frame,
        main_chunks[0],
        agent,
        state.scroll_offsets.tool_calls,
        is_left_focused,
    );

    render_reasoning_panel(
        frame,
        main_chunks[1],
        agent,
        state.scroll_offsets.reasoning,
        is_right_focused,
        state.auto_scroll,
    );

    render_footer(frame, chunks[2], state);
}

/// Pure function: render agent header
fn render_agent_header(
    frame: &mut Frame,
    area: Rect,
    agent_id: &str,
    agent: &crate::model::Agent,
    state: &AppState,
) {
    let status = if agent.finished_at.is_some() {
        ("Finished", Theme::TASK_COMPLETED)
    } else {
        ("Active", Theme::TASK_RUNNING)
    };

    let duration = if let Some(finished) = agent.finished_at {
        let elapsed = finished.signed_duration_since(agent.started_at);
        format!("{}s", elapsed.num_seconds())
    } else {
        let elapsed = state.started_at.elapsed();
        format!("{}s", elapsed.as_secs())
    };

    let task_info = agent
        .task_id
        .as_ref()
        .map(|tid| format!(" | Task: {}", tid))
        .unwrap_or_default();

    let header_line = Line::from(vec![
        Span::raw("Agent: "),
        Span::styled(agent_id, Style::default().add_modifier(Modifier::BOLD)),
        Span::raw(" | Status: "),
        Span::styled(status.0, Style::default().fg(status.1)),
        Span::raw(" | Duration: "),
        Span::styled(duration, Style::default().fg(Theme::INFO)),
        Span::raw(&task_info),
    ]);

    let header = Paragraph::new(header_line)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Theme::PANEL_BORDER)),
        )
        .style(Style::default().fg(Theme::TEXT));

    frame.render_widget(header, area);
}

/// Pure function: render agent not found message
fn render_agent_not_found(frame: &mut Frame, area: Rect, agent_id: &str) {
    let message = format!("Agent not found: {}", agent_id);
    let paragraph = Paragraph::new(message)
        .style(Style::default().fg(Theme::ERROR))
        .block(
            Block::default()
                .title("Error")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Theme::ERROR)),
        );
    frame.render_widget(paragraph, area);
}

/// Pure function: render footer with keybinding hints
fn render_footer(frame: &mut Frame, area: Rect, _state: &AppState) {
    let footer_line = Line::from(vec![
        Span::styled("Tab", Style::default().add_modifier(Modifier::BOLD)),
        Span::raw(":switch | "),
        Span::styled("↑↓", Style::default().add_modifier(Modifier::BOLD)),
        Span::raw(":scroll | "),
        Span::styled("A", Style::default().add_modifier(Modifier::BOLD)),
        Span::raw(":auto-scroll | "),
        Span::styled("Esc", Style::default().add_modifier(Modifier::BOLD)),
        Span::raw(":back | "),
        Span::styled("q", Style::default().add_modifier(Modifier::BOLD)),
        Span::raw(":quit"),
    ]);

    let footer = Paragraph::new(footer_line)
        .style(Style::default().fg(Theme::TEXT).bg(Theme::FOOTER_BG));

    frame.render_widget(footer, area);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Agent, AgentMessage, ToolCall};
    use chrono::Utc;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    #[test]
    fn test_render_agent_not_found() {
        let backend = TestBackend::new(80, 20);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal
            .draw(|frame| {
                let state = AppState::new();
                render_agent_detail(frame, &state, "nonexistent");
            })
            .unwrap();
    }

    #[test]
    fn test_render_agent_detail_with_empty_agent() {
        let backend = TestBackend::new(80, 20);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal
            .draw(|frame| {
                let mut state = AppState::new();
                let agent = Agent::new("a01".into(), Utc::now());
                state.agents.insert("a01".into(), agent);
                render_agent_detail(frame, &state, "a01");
            })
            .unwrap();
    }

    #[test]
    fn test_render_agent_detail_with_active_agent() {
        let backend = TestBackend::new(80, 20);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal
            .draw(|frame| {
                let mut state = AppState::new();
                let agent = Agent::new("a01".into(), Utc::now())
                    .with_task("T1".into())
                    .add_message(AgentMessage::reasoning(
                        Utc::now(),
                        "Thinking...".into(),
                    ))
                    .add_message(AgentMessage::tool(
                        Utc::now(),
                        ToolCall::new("Read".into(), "file.rs".into()),
                    ));
                state.agents.insert("a01".into(), agent);
                render_agent_detail(frame, &state, "a01");
            })
            .unwrap();
    }

    #[test]
    fn test_render_agent_detail_with_finished_agent() {
        let backend = TestBackend::new(80, 20);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal
            .draw(|frame| {
                let mut state = AppState::new();
                let now = Utc::now();
                let later = now + chrono::Duration::seconds(10);
                let agent = Agent::new("a01".into(), now)
                    .with_task("T1".into())
                    .finish(later);
                state.agents.insert("a01".into(), agent);
                render_agent_detail(frame, &state, "a01");
            })
            .unwrap();
    }

    #[test]
    fn test_render_agent_detail_with_focus_left() {
        let backend = TestBackend::new(80, 20);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal
            .draw(|frame| {
                let mut state = AppState::new();
                state.focus = PanelFocus::Left;
                let agent = Agent::new("a01".into(), Utc::now());
                state.agents.insert("a01".into(), agent);
                render_agent_detail(frame, &state, "a01");
            })
            .unwrap();
    }

    #[test]
    fn test_render_agent_detail_with_focus_right() {
        let backend = TestBackend::new(80, 20);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal
            .draw(|frame| {
                let mut state = AppState::new();
                state.focus = PanelFocus::Right;
                let agent = Agent::new("a01".into(), Utc::now());
                state.agents.insert("a01".into(), agent);
                render_agent_detail(frame, &state, "a01");
            })
            .unwrap();
    }

    #[test]
    fn test_render_agent_detail_with_scroll_offsets() {
        let backend = TestBackend::new(80, 20);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal
            .draw(|frame| {
                let mut state = AppState::new();
                state.scroll_offsets.tool_calls = 5;
                state.scroll_offsets.reasoning = 10;
                let agent = Agent::new("a01".into(), Utc::now())
                    .add_message(AgentMessage::reasoning(
                        Utc::now(),
                        "Line 1\nLine 2\nLine 3".into(),
                    ))
                    .add_message(AgentMessage::tool(
                        Utc::now(),
                        ToolCall::new("Read".into(), "file.rs".into()),
                    ));
                state.agents.insert("a01".into(), agent);
                render_agent_detail(frame, &state, "a01");
            })
            .unwrap();
    }

    #[test]
    fn test_render_agent_detail_with_auto_scroll() {
        let backend = TestBackend::new(80, 20);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal
            .draw(|frame| {
                let mut state = AppState::new();
                state.auto_scroll = true;
                let agent = Agent::new("a01".into(), Utc::now())
                    .add_message(AgentMessage::reasoning(
                        Utc::now(),
                        "Lots of text\n".repeat(100),
                    ));
                state.agents.insert("a01".into(), agent);
                render_agent_detail(frame, &state, "a01");
            })
            .unwrap();
    }

    #[test]
    fn test_render_agent_detail_with_agent_without_task() {
        let backend = TestBackend::new(80, 20);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal
            .draw(|frame| {
                let mut state = AppState::new();
                let agent = Agent::new("a01".into(), Utc::now());
                state.agents.insert("a01".into(), agent);
                render_agent_detail(frame, &state, "a01");
            })
            .unwrap();
    }
}
