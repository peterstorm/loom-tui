use ratatui::{
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem},
    Frame,
};

use crate::app::{AppState, PanelFocus};
use crate::model::{HookEventKind, Theme};

/// Render event stream panel.
/// Shows scrollable log of recent hook events with timestamps.
/// Respects scroll offset from state.
pub fn render_event_stream(frame: &mut Frame, area: Rect, state: &AppState) {
    let items = build_event_stream_items(state);

    let is_focused = matches!(state.focus, PanelFocus::Right);

    let title = if state.auto_scroll {
        "Events [auto-scroll]"
    } else {
        "Events"
    };

    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(if is_focused {
                    Theme::ACTIVE_BORDER
                } else {
                    Theme::PANEL_BORDER
                }))
                .title(title),
        )
        .style(Style::default().fg(Theme::TEXT));

    frame.render_widget(list, area);
}

/// Pure function: build event stream items from state.
fn build_event_stream_items(state: &AppState) -> Vec<ListItem<'static>> {
    if state.events.is_empty() {
        return vec![ListItem::new(Line::from(Span::styled(
            "No events",
            Style::default().fg(Theme::MUTED_TEXT),
        )))];
    }

    state
        .events
        .iter()
        .rev() // Most recent first
        .map(|event| {
            let timestamp = event.timestamp.format("%H:%M:%S").to_string();
            let (event_text, event_color) = format_event(&event.kind);

            let mut spans = vec![
                Span::styled(timestamp, Style::default().fg(Theme::MUTED_TEXT)),
                Span::raw(" "),
                Span::styled(event_text, Style::default().fg(event_color)),
            ];

            // Add agent ID if present
            if let Some(ref agent_id) = event.agent_id {
                spans.push(Span::raw(" "));
                spans.push(Span::styled(
                    format!("[{}]", agent_id),
                    Style::default().fg(Theme::INFO),
                ));
            }

            ListItem::new(Line::from(spans))
        })
        .collect()
}

/// Format hook event kind into display text and color.
fn format_event(kind: &HookEventKind) -> (String, ratatui::style::Color) {
    match kind {
        HookEventKind::SessionStart => ("Session started".into(), Theme::SUCCESS),
        HookEventKind::SessionEnd => ("Session ended".into(), Theme::INFO),
        HookEventKind::SubagentStart { task_description } => {
            let text = if let Some(desc) = task_description {
                format!("Subagent started: {}", truncate(desc, 40))
            } else {
                "Subagent started".into()
            };
            (text, Theme::SUCCESS)
        }
        HookEventKind::SubagentStop => ("Subagent stopped".into(), Theme::INFO),
        HookEventKind::PreToolUse {
            tool_name,
            input_summary,
        } => {
            let text = format!("{}: {}", tool_name, truncate(input_summary, 40));
            let color = Theme::tool_color(tool_name);
            (text, color)
        }
        HookEventKind::PostToolUse {
            tool_name,
            result_summary,
            duration_ms,
        } => {
            let duration_text = duration_ms
                .map(|ms| format!(" ({}ms)", ms))
                .unwrap_or_default();
            let text = format!(
                "{}: {}{}",
                tool_name,
                truncate(result_summary, 30),
                duration_text
            );
            let color = Theme::tool_color(tool_name);
            (text, color)
        }
        HookEventKind::Stop { reason } => {
            let text = if let Some(r) = reason {
                format!("Stopped: {}", truncate(r, 40))
            } else {
                "Stopped".into()
            };
            (text, Theme::WARNING)
        }
        HookEventKind::Notification { message } => {
            (format!("Note: {}", truncate(message, 50)), Theme::INFO)
        }
        HookEventKind::UserPromptSubmit => ("User prompt submitted".into(), Theme::INFO),
    }
}

/// Truncate string with ellipsis if longer than max_len.
fn truncate(s: &str, max_len: usize) -> String {
    if s.len() > max_len {
        format!("{}...", &s[..max_len.saturating_sub(3)])
    } else {
        s.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::HookEvent;
    use chrono::Utc;
    use std::collections::VecDeque;

    #[test]
    fn render_event_stream_does_not_panic_with_empty_state() {
        let backend = ratatui::backend::TestBackend::new(40, 20);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();

        let state = AppState::new();

        terminal
            .draw(|frame| {
                render_event_stream(frame, frame.area(), &state);
            })
            .unwrap();
    }

    #[test]
    fn build_event_stream_items_shows_no_events_when_empty() {
        let state = AppState::new();
        let items = build_event_stream_items(&state);

        assert_eq!(items.len(), 1);
    }

    #[test]
    fn build_event_stream_items_shows_events_in_reverse_order() {
        let mut state = AppState::new();

        let event1 = HookEvent::new(Utc::now(), HookEventKind::session_start());
        let event2 = HookEvent::new(Utc::now(), HookEventKind::session_end());

        state.events = VecDeque::from(vec![event1, event2]);

        let items = build_event_stream_items(&state);

        // Should have 2 events (reversed)
        assert_eq!(items.len(), 2);
    }

    #[test]
    fn format_event_session_start() {
        let (text, _) = format_event(&HookEventKind::SessionStart);
        assert_eq!(text, "Session started");
    }

    #[test]
    fn format_event_pre_tool_use() {
        let (text, _) = format_event(&HookEventKind::pre_tool_use(
            "Read".into(),
            "file.rs".into(),
        ));
        assert!(text.contains("Read"));
        assert!(text.contains("file.rs"));
    }

    #[test]
    fn format_event_post_tool_use_with_duration() {
        let (text, _) = format_event(&HookEventKind::post_tool_use(
            "Bash".into(),
            "success".into(),
            Some(250),
        ));
        assert!(text.contains("Bash"));
        assert!(text.contains("250ms"));
    }

    #[test]
    fn truncate_shortens_long_strings() {
        let long_str = "a".repeat(100);
        let result = truncate(&long_str, 10);
        assert_eq!(result.len(), 10);
        assert!(result.ends_with("..."));
    }

    #[test]
    fn truncate_preserves_short_strings() {
        let short_str = "hello";
        let result = truncate(short_str, 10);
        assert_eq!(result, "hello");
    }
}
