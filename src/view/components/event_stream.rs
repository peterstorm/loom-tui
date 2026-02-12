use ratatui::{
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
    Frame,
};

use crate::app::{AppState, PanelFocus};
use crate::model::{HookEventKind, Theme};

/// Render event stream panel.
/// Shows scrollable log of recent hook events with timestamps.
/// Uses Paragraph with word wrap so long lines don't clip.
pub fn render_event_stream(frame: &mut Frame, area: Rect, state: &AppState) {
    let lines = build_filtered_event_lines(state, None);

    let is_focused = matches!(state.focus, PanelFocus::Right);

    let title = if state.auto_scroll {
        "Events [auto-scroll]"
    } else {
        "Events"
    };

    let paragraph = Paragraph::new(lines)
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
        .style(Style::default().fg(Theme::TEXT))
        .wrap(Wrap { trim: false })
        .scroll((state.scroll_offsets.event_stream as u16, 0));

    frame.render_widget(paragraph, area);
}

/// Render filtered event stream for a specific agent.
pub fn render_agent_event_stream(
    frame: &mut Frame,
    area: Rect,
    state: &AppState,
    agent_id: &str,
    scroll_offset: usize,
    is_focused: bool,
) {
    let lines = build_filtered_event_lines(state, Some(agent_id));

    let title = if state.auto_scroll {
        "Activity [auto-scroll]"
    } else {
        "Activity"
    };

    let paragraph = Paragraph::new(lines)
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
        .style(Style::default().fg(Theme::TEXT))
        .wrap(Wrap { trim: false })
        .scroll((scroll_offset as u16, 0));

    frame.render_widget(paragraph, area);
}

/// Pure function: build lines from events, optionally filtered by agent_id.
fn build_filtered_event_lines(state: &AppState, agent_filter: Option<&str>) -> Vec<Line<'static>> {
    let filtered: Vec<_> = state
        .events
        .iter()
        .rev()
        .filter(|e| match agent_filter {
            Some(aid) => e.agent_id.as_deref() == Some(aid),
            None => true,
        })
        .take(500)
        .collect();

    if filtered.is_empty() {
        return vec![Line::from(Span::styled(
            "No events",
            Style::default().fg(Theme::MUTED_TEXT),
        ))];
    }

    let mut lines = Vec::new();
    let mut first = true;

    for event in &filtered {
        // Separator between events (dim line)
        if !first {
            lines.push(Line::from(Span::styled(
                "────────────────────────────────",
                Style::default().fg(Theme::SEPARATOR),
            )));
        }
        first = false;

        let timestamp = event.timestamp.format("%H:%M:%S").to_string();
        let (icon, header, detail, event_color, _tool_name) = format_event_lines(&event.kind);

        // Resolve agent display name
        let agent_label = event.agent_id.as_ref().map(|aid| {
            state
                .agents
                .get(aid)
                .map(|a| a.display_name().to_string())
                .unwrap_or_else(|| short_id(aid))
        });

        // Line 1: timestamp + icon + header
        let mut header_spans = vec![
            Span::styled(
                format!("{} ", timestamp),
                Style::default().fg(Theme::MUTED_TEXT),
            ),
            Span::styled(format!("{} ", icon), Style::default().fg(event_color)),
            Span::styled(header, Style::default().fg(event_color)),
        ];

        // Append agent label if present
        if let Some(ref label) = agent_label {
            header_spans.push(Span::styled(
                format!("  {}", label),
                Style::default().fg(Theme::AGENT_LABEL),
            ));
        }

        lines.push(Line::from(header_spans));

        // Line 2: detail if present (no indent)
        if let Some(detail_text) = detail {
            let clean = clean_detail(&detail_text);
            if !clean.is_empty() {
                // Diff-style coloring: - lines red, + lines green
                let has_diff_lines = clean.contains("\n- ") || clean.contains("\n+ ");
                if has_diff_lines {
                    for line in clean.split('\n') {
                        let color = if line.starts_with("- ") {
                            Theme::ERROR
                        } else if line.starts_with("+ ") {
                            Theme::SUCCESS
                        } else {
                            Theme::MUTED_TEXT
                        };
                        lines.push(Line::from(Span::styled(
                            line.to_string(),
                            Style::default().fg(color),
                        )));
                    }
                } else {
                    lines.push(Line::from(Span::styled(
                        clean,
                        Style::default().fg(Theme::MUTED_TEXT),
                    )));
                }
            }
        }
    }

    lines
}

/// Strip JSON escapes and control chars from detail text for clean display.
/// Preserves real newlines for diff-style content.
pub fn clean_detail(s: &str) -> String {
    s.replace("\\\"", "\"")
        .replace("\\t", " ")
        .replace("\\\\", "\\")
        .replace("\\n", "\n")
        .lines()
        .map(|line| {
            line.chars()
                .filter(|c| !c.is_control() || *c == '\n')
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string()
}

/// Shorten an agent ID to first 7 chars (like git short hash).
fn short_id(id: &str) -> String {
    if id.len() > 7 {
        id[..7].to_string()
    } else {
        id.to_string()
    }
}

/// Format hook event kind into (icon, header, optional detail, color, optional tool_name).
pub fn format_event_lines(kind: &HookEventKind) -> (&'static str, String, Option<String>, ratatui::style::Color, Option<String>) {
    match kind {
        HookEventKind::SessionStart => ("●", "Session started".into(), None, Theme::SUCCESS, None),
        HookEventKind::SessionEnd => ("○", "Session ended".into(), None, Theme::INFO, None),
        HookEventKind::SubagentStart { agent_type, task_description } => {
            let header = agent_type
                .as_ref()
                .map(|t| format!("Agent started ({})", t))
                .unwrap_or_else(|| "Agent started".into());
            ("▶", header, task_description.clone(), Theme::SUCCESS, None)
        }
        HookEventKind::SubagentStop => ("■", "Agent stopped".into(), None, Theme::MUTED_TEXT, None),
        HookEventKind::PreToolUse {
            tool_name,
            input_summary,
        } => {
            let detail = if input_summary.is_empty() {
                None
            } else {
                Some(input_summary.clone())
            };
            ("⚡", tool_name.clone(), detail, Theme::tool_color(tool_name), Some(tool_name.clone()))
        }
        HookEventKind::PostToolUse {
            tool_name,
            duration_ms,
            result_summary,
        } => {
            let duration_text = duration_ms
                .map(|ms| format!(" ({}ms)", ms))
                .unwrap_or_default();
            let header = format!("{}{}", tool_name, duration_text);
            let detail = if result_summary.is_empty() {
                None
            } else {
                Some(result_summary.clone())
            };
            ("✓", header, detail, Theme::tool_color(tool_name), Some(tool_name.clone()))
        }
        HookEventKind::Stop { reason } => {
            ("⏹", "Stopped".into(), reason.clone(), Theme::WARNING, None)
        }
        HookEventKind::Notification { message } => {
            ("ℹ", "Note".into(), Some(message.clone()), Theme::INFO, None)
        }
        HookEventKind::UserPromptSubmit => ("→", "User prompt".into(), None, Theme::INFO, None),
        HookEventKind::AssistantText { content } => {
            let truncated = if content.len() > 500 {
                format!("{}...", &content[..500])
            } else {
                content.clone()
            };
            ("💭", "Thinking".into(), Some(truncated), Theme::MUTED_TEXT, None)
        }
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
        let lines = build_filtered_event_lines(&state, None);

        assert_eq!(lines.len(), 1);
    }

    #[test]
    fn build_event_stream_items_shows_events_with_separators() {
        let mut state = AppState::new();

        let event1 = HookEvent::new(Utc::now(), HookEventKind::session_start());
        let event2 = HookEvent::new(Utc::now(), HookEventKind::session_end());

        state.events = VecDeque::from(vec![event1, event2]);

        let lines = build_filtered_event_lines(&state, None);

        // 2 events: header + separator + header = 3 lines
        assert_eq!(lines.len(), 3);
    }

    #[test]
    fn format_event_session_start() {
        let (icon, header, _, _, _) = format_event_lines(&HookEventKind::SessionStart);
        assert_eq!(header, "Session started");
        assert_eq!(icon, "●");
    }

    #[test]
    fn format_event_pre_tool_use() {
        let (_, header, detail, _, _) = format_event_lines(&HookEventKind::pre_tool_use(
            "Read".into(),
            "file.rs".into(),
        ));
        assert!(header.contains("Read"));
        assert_eq!(detail, Some("file.rs".into()));
    }

    #[test]
    fn format_event_post_tool_use_with_duration() {
        let (icon, header, detail, _, _) = format_event_lines(&HookEventKind::post_tool_use(
            "Bash".into(),
            "success".into(),
            Some(250),
        ));
        assert!(header.contains("Bash"));
        assert!(header.contains("250ms"));
        assert_eq!(detail, Some("success".into()));
        assert_eq!(icon, "✓");
    }

    #[test]
    fn clean_detail_strips_escapes() {
        let raw = r#"{"filePath":"src/main.rs","oldString":"fn main() {\n  println!(\"hello\");\n}"}"#;
        let cleaned = clean_detail(raw);
        assert!(!cleaned.contains("\\n"));
        assert!(!cleaned.contains("\\\""));
    }

    #[test]
    fn clean_detail_trims_whitespace() {
        assert_eq!(clean_detail("  foo   bar  "), "foo   bar");
    }

    #[test]
    fn short_id_truncates() {
        assert_eq!(short_id("a36f3e4abcdef"), "a36f3e4");
        assert_eq!(short_id("abc"), "abc");
    }

    #[test]
    fn agent_label_resolves_from_state() {
        use crate::model::Agent;

        let mut state = AppState::new();
        let mut agent = Agent::new("a01".into(), Utc::now());
        agent.agent_type = Some("Explore".into());
        state.agents.insert("a01".into(), agent);

        let event = HookEvent::new(
            Utc::now(),
            HookEventKind::pre_tool_use("Read".into(), "file.rs".into()),
        )
        .with_agent("a01".into());
        state.events = VecDeque::from(vec![event]);

        let lines = build_filtered_event_lines(&state, None);
        // Header line should contain "Explore"
        let header_text: String = lines[0]
            .spans
            .iter()
            .map(|s| s.content.to_string())
            .collect();
        assert!(header_text.contains("Explore"));
    }
}
