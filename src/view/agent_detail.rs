use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

use crate::app::state::{AppState, PanelFocus};
use crate::model::Theme;
use crate::view::components::{render_agent_event_stream, render_agent_list, render_prompt_popup};

/// Pure rendering function: render agent detail view.
/// Left panel: selectable agent list. Right panel: filtered events for selected agent.
pub fn render_agent_detail(frame: &mut Frame, state: &AppState, area: Rect) {
    // Layout: [agent_header][main_area][footer]
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // agent header
            Constraint::Min(0),   // main area
            Constraint::Length(1), // footer
        ])
        .split(area);

    // Resolve selected agent via sorted order
    let sorted_keys = state.sorted_agent_keys();
    let selected_agent = state.ui.selected_agent_index.and_then(|idx| {
        sorted_keys.get(idx).and_then(|k| state.domain.agents.get(k))
    });

    render_agent_header(frame, chunks[0], selected_agent, state);

    // Split main area: [agent_list(30%) | agent_events(70%)]
    let main_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(30),
            Constraint::Percentage(70),
        ])
        .split(chunks[1]);

    render_agent_list(frame, main_chunks[0], state);

    // Right panel: filtered events for selected agent
    let is_right_focused = matches!(state.ui.focus, PanelFocus::Right);
    if let Some(agent) = selected_agent {
        render_agent_event_stream(
            frame,
            main_chunks[1],
            state,
            agent.id.as_str(),
            state.ui.scroll_offsets.agent_events,
            is_right_focused,
        );
    } else {
        render_no_agent_selected(frame, main_chunks[1], is_right_focused);
    }

    render_footer(frame, chunks[2], state);

    // Prompt popup overlay (rendered last, on top)
    if state.ui.prompt_popup.is_open() {
        if let Some(agent) = selected_agent {
            let text = agent.task_description.as_deref().unwrap_or("No prompt available");
            render_prompt_popup(
                frame,
                area,
                &agent.display_name(),
                agent.model.as_deref(),
                text,
                &agent.messages,
                &agent.skills,
                &agent.token_usage,
                state.ui.prompt_popup.scroll(),
            );
        }
    }
}

/// Render header showing selected agent info.
fn render_agent_header(
    frame: &mut Frame,
    area: Rect,
    agent: Option<&crate::model::Agent>,
    state: &AppState,
) {
    let header_line = match agent {
        Some(agent) => {
            let status = if agent.finished_at.is_some() {
                ("Finished", Theme::TASK_COMPLETED)
            } else {
                ("Active", Theme::TASK_RUNNING)
            };

            let duration = if let Some(finished) = agent.finished_at {
                let elapsed = finished.signed_duration_since(agent.started_at);
                format!("{}s", elapsed.num_seconds())
            } else {
                let elapsed = state.meta.started_at.elapsed();
                format!("{}s", elapsed.as_secs())
            };

            let task_info = agent.task_description.as_deref()
                .or_else(|| agent.task_id.as_ref().map(|id| id.as_str()))
                .map(|desc| {
                    let truncated = crate::watcher::truncate_str(desc, 60);
                    format!(" | {}", truncated)
                })
                .unwrap_or_default();

            Line::from(vec![
                Span::raw("Agent: "),
                Span::styled(
                    agent.display_name(),
                    Style::default().add_modifier(Modifier::BOLD),
                ),
                Span::raw(" | Status: "),
                Span::styled(status.0, Style::default().fg(status.1)),
                Span::raw(" | Duration: "),
                Span::styled(duration, Style::default().fg(Theme::INFO)),
                Span::styled(task_info, Style::default().fg(Theme::MUTED_TEXT)),
            ])
        }
        None => Line::from(Span::styled(
            "No agent selected",
            Style::default().fg(Theme::MUTED_TEXT),
        )),
    };

    let header = Paragraph::new(header_line)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Theme::PANEL_BORDER)),
        )
        .style(Style::default().fg(Theme::TEXT));

    frame.render_widget(header, area);
}

/// Render placeholder when no agent is selected.
fn render_no_agent_selected(frame: &mut Frame, area: Rect, is_focused: bool) {
    let paragraph = Paragraph::new("Select an agent to view activity")
        .style(Style::default().fg(Theme::MUTED_TEXT))
        .block(
            Block::default()
                .title("Activity")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(if is_focused {
                    Theme::ACTIVE_BORDER
                } else {
                    Theme::PANEL_BORDER
                })),
        );
    frame.render_widget(paragraph, area);
}

/// Render footer with keybinding hints.
fn render_footer(frame: &mut Frame, area: Rect, _state: &AppState) {
    let footer_line = Line::from(vec![
        Span::styled("Tab", Style::default().add_modifier(Modifier::BOLD)),
        Span::raw(":switch | "),
        Span::styled("j/k", Style::default().add_modifier(Modifier::BOLD)),
        Span::raw(":select/scroll | "),
        Span::styled("p", Style::default().add_modifier(Modifier::BOLD)),
        Span::raw(":prompt | "),
        Span::styled("Space", Style::default().add_modifier(Modifier::BOLD)),
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
    use crate::model::{Agent, AgentId};
    use chrono::Utc;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    #[test]
    fn render_no_agents() {
        let backend = TestBackend::new(80, 20);
        let mut terminal = Terminal::new(backend).unwrap();

        let state = AppState::new();
        terminal
            .draw(|frame| render_agent_detail(frame, &state, frame.area()))
            .unwrap();
    }

    #[test]
    fn render_with_selected_agent() {
        let backend = TestBackend::new(80, 20);
        let mut terminal = Terminal::new(backend).unwrap();

        let mut state = AppState::new();
        state.domain.agents.insert(AgentId::new("a01"), Agent::new("a01", Utc::now()));
        state.ui.selected_agent_index = Some(0);

        terminal
            .draw(|frame| render_agent_detail(frame, &state, frame.area()))
            .unwrap();
    }

    #[test]
    fn render_with_multiple_agents() {
        let backend = TestBackend::new(80, 20);
        let mut terminal = Terminal::new(backend).unwrap();

        let mut state = AppState::new();
        let mut a1 = Agent::new("a01", Utc::now());
        a1.agent_type = Some("Explore".into());
        state.domain.agents.insert("a01".into(), a1);
        state.domain.agents.insert(AgentId::new("a02"), Agent::new("a02", Utc::now()));
        state.ui.selected_agent_index = Some(1);

        terminal
            .draw(|frame| render_agent_detail(frame, &state, frame.area()))
            .unwrap();
    }

    #[test]
    fn render_with_finished_agent() {
        let backend = TestBackend::new(80, 20);
        let mut terminal = Terminal::new(backend).unwrap();

        let mut state = AppState::new();
        let now = Utc::now();
        let agent = Agent::new("a01", now).finish(now + chrono::Duration::seconds(10));
        state.domain.agents.insert("a01".into(), agent);
        state.ui.selected_agent_index = Some(0);

        terminal
            .draw(|frame| render_agent_detail(frame, &state, frame.area()))
            .unwrap();
    }

    #[test]
    fn render_with_focus_right() {
        let backend = TestBackend::new(80, 20);
        let mut terminal = Terminal::new(backend).unwrap();

        let mut state = AppState::new();
        state.ui.focus = PanelFocus::Right;
        state.domain.agents.insert(AgentId::new("a01"), Agent::new("a01", Utc::now()));
        state.ui.selected_agent_index = Some(0);

        terminal
            .draw(|frame| render_agent_detail(frame, &state, frame.area()))
            .unwrap();
    }
}
