use ratatui::{
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
    Frame,
};

use crate::model::{Agent, MessageKind, Theme};

/// Pure rendering function: transform agent's reasoning messages into widget
pub fn render_reasoning_panel(
    frame: &mut Frame,
    area: Rect,
    agent: &Agent,
    scroll_offset: usize,
    is_focused: bool,
    auto_scroll: bool,
) {
    let lines = build_reasoning_lines(agent, scroll_offset, area.height as usize, auto_scroll);
    let border_color = if is_focused {
        Theme::ACTIVE_BORDER
    } else {
        Theme::PANEL_BORDER
    };

    let paragraph = Paragraph::new(lines)
        .block(
            Block::default()
                .title("Reasoning / Transcript")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(border_color)),
        )
        .style(Style::default().fg(Theme::TEXT))
        .wrap(Wrap { trim: false });

    frame.render_widget(paragraph, area);
}

/// Pure function: build lines from reasoning messages
fn build_reasoning_lines(
    agent: &Agent,
    scroll_offset: usize,
    visible_height: usize,
    auto_scroll: bool,
) -> Vec<Line<'static>> {
    let reasoning_messages: Vec<String> = agent
        .messages
        .iter()
        .filter_map(|msg| {
            if let MessageKind::Reasoning { content } = &msg.kind {
                Some(content.clone())
            } else {
                None
            }
        })
        .collect();

    // Account for borders (2 lines) and title (1 line)
    let content_height = visible_height.saturating_sub(3);

    // Combine all reasoning content with newlines
    let full_text = reasoning_messages.join("\n\n");
    let all_lines: Vec<String> = full_text.lines().map(|s| s.to_string()).collect();

    // If auto_scroll is enabled, show the last N lines
    let displayed_lines = if auto_scroll && all_lines.len() > content_height {
        all_lines
            .iter()
            .skip(all_lines.len().saturating_sub(content_height))
            .take(content_height)
    } else {
        // Otherwise apply scroll_offset
        all_lines.iter().skip(scroll_offset).take(content_height)
    };

    displayed_lines
        .map(|line| Line::from(Span::raw(line.to_string())))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Agent, AgentMessage, ToolCall};
    use chrono::Utc;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    #[test]
    fn test_build_reasoning_lines_empty_agent() {
        let agent = Agent::new("a01".into(), Utc::now());
        let lines = build_reasoning_lines(&agent, 0, 10, false);
        assert_eq!(lines.len(), 0);
    }

    #[test]
    fn test_build_reasoning_lines_with_reasoning() {
        let agent = Agent::new("a01".into(), Utc::now())
            .add_message(AgentMessage::reasoning(Utc::now(), "First thought".into()))
            .add_message(AgentMessage::reasoning(
                Utc::now(),
                "Second thought".into(),
            ));

        let lines = build_reasoning_lines(&agent, 0, 20, false);
        // "First thought\n\nSecond thought" = 3 lines
        assert_eq!(lines.len(), 3);
    }

    #[test]
    fn test_build_reasoning_lines_filters_tool_calls() {
        let agent = Agent::new("a01".into(), Utc::now())
            .add_message(AgentMessage::reasoning(Utc::now(), "Thinking...".into()))
            .add_message(AgentMessage::tool(
                Utc::now(),
                ToolCall::new("Read".into(), "file.rs".into()),
            ))
            .add_message(AgentMessage::reasoning(
                Utc::now(),
                "Done thinking".into(),
            ));

        let lines = build_reasoning_lines(&agent, 0, 20, false);
        // "Thinking...\n\nDone thinking" = 3 lines
        assert_eq!(lines.len(), 3);
    }

    #[test]
    fn test_build_reasoning_lines_with_scroll() {
        let agent = Agent::new("a01".into(), Utc::now())
            .add_message(AgentMessage::reasoning(Utc::now(), "Line 1".into()))
            .add_message(AgentMessage::reasoning(Utc::now(), "Line 2".into()))
            .add_message(AgentMessage::reasoning(Utc::now(), "Line 3".into()));

        // "Line 1\n\nLine 2\n\nLine 3" = 5 lines
        // Skip first 2 lines
        let lines = build_reasoning_lines(&agent, 2, 20, false);
        assert_eq!(lines.len(), 3);
    }

    #[test]
    fn test_build_reasoning_lines_with_height_limit() {
        let agent = Agent::new("a01".into(), Utc::now())
            .add_message(AgentMessage::reasoning(Utc::now(), "Line 1".into()))
            .add_message(AgentMessage::reasoning(Utc::now(), "Line 2".into()))
            .add_message(AgentMessage::reasoning(Utc::now(), "Line 3".into()));

        // visible_height 5 - 3 (borders+title) = 2 content lines
        let lines = build_reasoning_lines(&agent, 0, 5, false);
        assert_eq!(lines.len(), 2);
    }

    #[test]
    fn test_build_reasoning_lines_auto_scroll() {
        let agent = Agent::new("a01".into(), Utc::now())
            .add_message(AgentMessage::reasoning(Utc::now(), "Line 1".into()))
            .add_message(AgentMessage::reasoning(Utc::now(), "Line 2".into()))
            .add_message(AgentMessage::reasoning(Utc::now(), "Line 3".into()))
            .add_message(AgentMessage::reasoning(Utc::now(), "Line 4".into()));

        // visible_height 5 - 3 = 2 content lines
        // With auto_scroll, should show last 2 lines of content
        let lines = build_reasoning_lines(&agent, 0, 5, true);
        assert_eq!(lines.len(), 2);
        // Should be the last 2 lines from the joined content
    }

    #[test]
    fn test_build_reasoning_lines_multiline_reasoning() {
        let agent = Agent::new("a01".into(), Utc::now()).add_message(AgentMessage::reasoning(
            Utc::now(),
            "Line 1\nLine 2\nLine 3".into(),
        ));

        let lines = build_reasoning_lines(&agent, 0, 20, false);
        // 3 lines from the content
        assert_eq!(lines.len(), 3);
    }

    #[test]
    fn test_render_does_not_panic_with_empty_agent() {
        let backend = TestBackend::new(40, 10);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal
            .draw(|frame| {
                let agent = Agent::new("a01".into(), Utc::now());
                render_reasoning_panel(frame, frame.area(), &agent, 0, false, false);
            })
            .unwrap();
    }

    #[test]
    fn test_render_does_not_panic_with_reasoning() {
        let backend = TestBackend::new(40, 10);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal
            .draw(|frame| {
                let agent = Agent::new("a01".into(), Utc::now())
                    .add_message(AgentMessage::reasoning(
                        Utc::now(),
                        "Some reasoning text".into(),
                    ));
                render_reasoning_panel(frame, frame.area(), &agent, 0, true, false);
            })
            .unwrap();
    }

    #[test]
    fn test_render_does_not_panic_with_auto_scroll() {
        let backend = TestBackend::new(40, 10);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal
            .draw(|frame| {
                let agent = Agent::new("a01".into(), Utc::now())
                    .add_message(AgentMessage::reasoning(
                        Utc::now(),
                        "Line 1\nLine 2\nLine 3\nLine 4\nLine 5".into(),
                    ));
                render_reasoning_panel(frame, frame.area(), &agent, 0, false, true);
            })
            .unwrap();
    }
}
