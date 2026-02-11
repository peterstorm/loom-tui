use ratatui::{
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem},
    Frame,
};

use crate::model::{Agent, AgentMessage, MessageKind, Theme};

/// Pure rendering function: transform agent's tool calls into widget
pub fn render_tool_call_list(
    frame: &mut Frame,
    area: Rect,
    agent: &Agent,
    scroll_offset: usize,
    is_focused: bool,
) {
    let items = build_tool_call_items(agent, scroll_offset, area.height as usize);
    let border_color = if is_focused {
        Theme::ACTIVE_BORDER
    } else {
        Theme::PANEL_BORDER
    };

    let list = List::new(items)
        .block(
            Block::default()
                .title("Tool Calls")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(border_color)),
        )
        .style(Style::default().fg(Theme::TEXT));

    frame.render_widget(list, area);
}

/// Pure function: build list items from tool calls
fn build_tool_call_items(agent: &Agent, scroll_offset: usize, visible_height: usize) -> Vec<ListItem<'static>> {
    let tool_calls: Vec<&AgentMessage> = agent
        .messages
        .iter()
        .filter(|msg| matches!(msg.kind, MessageKind::Tool(_)))
        .collect();

    // Account for borders (2 lines) and title (1 line)
    let content_height = visible_height.saturating_sub(3);

    tool_calls
        .iter()
        .skip(scroll_offset)
        .take(content_height)
        .map(|msg| {
            if let MessageKind::Tool(ref call) = msg.kind {
                let status_icon = match call.success {
                    Some(true) => ("✓", Theme::SUCCESS),
                    Some(false) => ("✗", Theme::ERROR),
                    None => ("⋯", Theme::WARNING),
                };

                let duration_text = call
                    .duration
                    .map(|d| format!(" {}ms", d.as_millis()))
                    .unwrap_or_default();

                let line = Line::from(vec![
                    Span::styled(status_icon.0.to_string(), Style::default().fg(status_icon.1)),
                    Span::raw(" "),
                    Span::styled(
                        call.tool_name.clone(),
                        Style::default().fg(Theme::tool_color(&call.tool_name)),
                    ),
                    Span::styled(duration_text, Style::default().fg(Theme::MUTED_TEXT)),
                ]);

                ListItem::new(line)
            } else {
                ListItem::new(Line::from(Span::raw("")))
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Agent, AgentMessage, ToolCall};
    use chrono::Utc;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;
    use std::time::Duration;

    #[test]
    fn test_build_tool_call_items_empty_agent() {
        let agent = Agent::new("a01".into(), Utc::now());
        let items = build_tool_call_items(&agent, 0, 10);
        assert_eq!(items.len(), 0);
    }

    #[test]
    fn test_build_tool_call_items_with_tool_calls() {
        let agent = Agent::new("a01".into(), Utc::now())
            .add_message(AgentMessage::tool(
                Utc::now(),
                ToolCall::new("Read".into(), "file.rs".into())
                    .with_result("ok".into(), true)
                    .with_duration(Duration::from_millis(100)),
            ))
            .add_message(AgentMessage::tool(
                Utc::now(),
                ToolCall::new("Bash".into(), "cargo test".into())
                    .with_result("failed".into(), false)
                    .with_duration(Duration::from_millis(500)),
            ));

        let items = build_tool_call_items(&agent, 0, 10);
        assert_eq!(items.len(), 2);
    }

    #[test]
    fn test_build_tool_call_items_with_scroll() {
        let agent = Agent::new("a01".into(), Utc::now())
            .add_message(AgentMessage::tool(
                Utc::now(),
                ToolCall::new("Read".into(), "1".into()),
            ))
            .add_message(AgentMessage::tool(
                Utc::now(),
                ToolCall::new("Bash".into(), "2".into()),
            ))
            .add_message(AgentMessage::tool(
                Utc::now(),
                ToolCall::new("Edit".into(), "3".into()),
            ));

        let items = build_tool_call_items(&agent, 1, 10);
        assert_eq!(items.len(), 2);
    }

    #[test]
    fn test_build_tool_call_items_with_height_limit() {
        let agent = Agent::new("a01".into(), Utc::now())
            .add_message(AgentMessage::tool(
                Utc::now(),
                ToolCall::new("Read".into(), "1".into()),
            ))
            .add_message(AgentMessage::tool(
                Utc::now(),
                ToolCall::new("Bash".into(), "2".into()),
            ))
            .add_message(AgentMessage::tool(
                Utc::now(),
                ToolCall::new("Edit".into(), "3".into()),
            ));

        // visible_height 5 - 3 (borders+title) = 2 content lines
        let items = build_tool_call_items(&agent, 0, 5);
        assert_eq!(items.len(), 2);
    }

    #[test]
    fn test_build_tool_call_items_filters_reasoning() {
        let agent = Agent::new("a01".into(), Utc::now())
            .add_message(AgentMessage::reasoning(Utc::now(), "thinking...".into()))
            .add_message(AgentMessage::tool(
                Utc::now(),
                ToolCall::new("Read".into(), "file.rs".into()),
            ))
            .add_message(AgentMessage::reasoning(Utc::now(), "more thinking...".into()));

        let items = build_tool_call_items(&agent, 0, 10);
        assert_eq!(items.len(), 1);
    }

    #[test]
    fn test_render_does_not_panic_with_empty_agent() {
        let backend = TestBackend::new(40, 10);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal
            .draw(|frame| {
                let agent = Agent::new("a01".into(), Utc::now());
                render_tool_call_list(frame, frame.area(), &agent, 0, false);
            })
            .unwrap();
    }

    #[test]
    fn test_render_does_not_panic_with_tool_calls() {
        let backend = TestBackend::new(40, 10);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal
            .draw(|frame| {
                let agent = Agent::new("a01".into(), Utc::now())
                    .add_message(AgentMessage::tool(
                        Utc::now(),
                        ToolCall::new("Read".into(), "file.rs".into())
                            .with_result("ok".into(), true),
                    ));
                render_tool_call_list(frame, frame.area(), &agent, 0, true);
            })
            .unwrap();
    }
}
