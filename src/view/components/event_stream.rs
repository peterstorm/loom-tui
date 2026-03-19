use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
    Frame,
};

use crate::app::{AppState, PanelFocus};
use crate::model::{Theme, TranscriptEventKind};

/// Render event stream panel.
/// Shows scrollable log of recent transcript events with timestamps.
/// Uses Paragraph with word wrap so long lines don't clip.
pub fn render_event_stream(frame: &mut Frame, area: Rect, state: &AppState) {
    let lines = build_filtered_event_lines(state, None);

    let is_focused = matches!(state.ui.focus, PanelFocus::Right);

    let title = if state.ui.auto_scroll {
        "Events [auto-scroll]"
    } else {
        "Events"
    };

    // Clamp scroll_offset to u16::MAX to prevent silent truncation overflow
    // Additionally clamp to a reasonable maximum to avoid ratatui internal panics
    let scroll = state.ui.scroll_offsets.event_stream
        .min(u16::MAX as usize)
        .min(10000) as u16;

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
        .wrap(Wrap { trim: false })
        .scroll((scroll, 0));

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

    let title = if state.ui.auto_scroll {
        "Activity [auto-scroll]"
    } else {
        "Activity"
    };

    // Clamp scroll_offset to u16::MAX to prevent silent truncation overflow
    // Additionally clamp to a reasonable maximum to avoid ratatui internal panics
    let scroll = scroll_offset
        .min(u16::MAX as usize)
        .min(10000) as u16;

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
        .wrap(Wrap { trim: false })
        .scroll((scroll, 0));

    frame.render_widget(paragraph, area);
}

/// Pure function: build lines from events, optionally filtered by agent_id.
fn build_filtered_event_lines(state: &AppState, agent_filter: Option<&str>) -> Vec<Line<'static>> {
    // When filtering by agent, also include unattributed events from the same session.
    // Some transcript events from subagent files may arrive without agent_id
    // before the watcher attributes them. Fall back to session_id matching.
    let agent_session = agent_filter.and_then(|aid| {
        state.domain.agents.get(&crate::model::AgentId::new(aid))
            .and_then(|a| a.session_id.clone())
    });

    // Get search filter from state (only applies when agent_filter is None - dashboard view)
    let search_filter = if agent_filter.is_none() {
        state.ui.filter.as_deref()
    } else {
        None
    };

    // Optimize: only lowercase the query once if we have a search filter
    let search_query_lower = search_filter
        .filter(|q| !q.is_empty())
        .map(|q| q.to_lowercase());

    let filtered: Vec<_> = state
        .domain.events
        .iter()
        .rev()
        .filter(|e| {
            // First, filter by agent if specified
            let agent_match = match agent_filter {
                Some(aid) => {
                    let direct = e.agent_id.as_ref().map(|id| id.as_str()) == Some(aid);
                    let shared = e.agent_id.is_none()
                        && agent_session.is_some()
                        && e.session_id == agent_session;
                    direct || shared
                }
                None => true,
            };

            if !agent_match {
                return false;
            }

            // Then, filter by search text if specified
            if let Some(ref query_lower) = search_query_lower {
                event_matches_search_transcript(&e.kind, query_lower, e.agent_id.as_ref())
            } else {
                true
            }
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
        let (icon, header, detail, event_color, tool_name) = format_transcript_event_lines(&event.kind);

        // Resolve agent display name
        let agent_label = event.agent_id.as_ref().map(|aid| {
            state
                .domain.agents
                .get(aid)
                .map(|a| a.display_name().to_string())
                .unwrap_or_else(|| short_id(aid.as_str()))
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

        // Line 2+: detail if present, with markdown rendering
        if let Some(detail_text) = detail {
            let clean = clean_detail(&detail_text);
            if !clean.is_empty() {
                if tool_name.is_none() {
                    // Assistant messages: full markdown rendering via tui_markdown
                    let rendered = tui_markdown::from_str(&clean);
                    lines.extend(own_text_lines(rendered));
                } else {
                    // Tool use/result: custom rendering with syntax highlighting + diff coloring
                    let (start_line, offset_clean) = extract_line_offset(&clean);
                    let ext_hint = tool_name
                        .as_ref()
                        .filter(|t| {
                            matches!(
                                t.as_str(),
                                "Read" | "Edit" | "Write" | "Grep" | "Glob"
                            )
                        })
                        .and_then(|_| {
                            offset_clean
                                .lines()
                                .take(5)
                                .find_map(super::syntax::detect_extension)
                        });
                    lines.extend(markdown_to_lines(offset_clean, ext_hint.as_deref(), start_line));
                }
            }
        }
    }

    lines
}

/// Strip JSON escapes and control chars from detail text for clean display.
/// Converts escaped newlines (\\n) to actual newlines for diff-style content.
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

/// Extract `@offset:N\n` prefix from text. Returns (start_line, rest).
/// The watcher parser prepends this prefix for Read tool results that include a line offset.
pub fn extract_line_offset(text: &str) -> (usize, &str) {
    if let Some(rest) = text.strip_prefix("@offset:") {
        if let Some(nl_pos) = rest.find('\n') {
            if let Ok(offset) = rest[..nl_pos].parse::<usize>() {
                return (offset, &rest[nl_pos + 1..]);
            }
        }
    }
    (1, text)
}

/// Public entry point for rendering detail text with markdown + syntax highlighting.
/// Used by both the dashboard event stream and session detail view.
pub fn render_detail_lines(text: &str, ext_hint: Option<&str>) -> Vec<Line<'static>> {
    let (start_line, clean_text) = extract_line_offset(text);
    markdown_to_lines(clean_text, ext_hint, start_line)
}

/// Convert markdown-ish text to styled ratatui Lines.
/// Handles: code blocks (syntax highlighted), inline code, bold, headers, diff lines, plain text.
/// When `ext_hint` is provided, diff lines and untagged code blocks get syntax highlighting.
/// `start_line` sets the first gutter number for code blocks.
fn markdown_to_lines(text: &str, ext_hint: Option<&str>, start_line: usize) -> Vec<Line<'static>> {
    let raw_lines: Vec<&str> = text.split('\n').collect();
    let mut result = Vec::new();
    let mut i = 0;

    while i < raw_lines.len() {
        let line = raw_lines[i];

        // Code block fences
        if line.trim_start().starts_with("```") {
            let fence_rest = line.trim_start().trim_start_matches('`');
            let lang = if fence_rest.is_empty() {
                None
            } else {
                Some(fence_rest.to_string())
            };

            let mut code_lines = Vec::new();
            i += 1;
            while i < raw_lines.len() && !raw_lines[i].trim_start().starts_with("```") {
                code_lines.push(raw_lines[i]);
                i += 1;
            }
            if i < raw_lines.len() {
                i += 1; // skip closing fence
            }

            if code_lines.is_empty() {
                result.push(Line::from(Span::styled(
                    "  (empty code block)",
                    Style::default()
                        .fg(Theme::MUTED_TEXT)
                        .add_modifier(Modifier::DIM),
                )));
            } else {
                let ext = lang
                    .as_deref()
                    .map(super::syntax::lang_to_extension)
                    .or_else(|| ext_hint.map(|e| e.to_string()))
                    .unwrap_or_else(|| "txt".to_string());
                result.extend(super::syntax::highlight_code_block(&code_lines, &ext, start_line));
            }
            continue;
        }

        // Consecutive diff lines
        if line.starts_with("+ ") || line.starts_with("- ") {
            let mut diff_lines = Vec::new();
            while i < raw_lines.len()
                && (raw_lines[i].starts_with("+ ") || raw_lines[i].starts_with("- "))
            {
                diff_lines.push(raw_lines[i]);
                i += 1;
            }

            if let Some(ext) = ext_hint {
                result.extend(super::syntax::highlight_diff_block(&diff_lines, ext, start_line));
            } else {
                // Fallback: flat coloring (no extension context)
                for dl in diff_lines {
                    let color = if dl.starts_with("+ ") {
                        Theme::SUCCESS
                    } else {
                        Theme::ERROR
                    };
                    result.push(Line::from(Span::styled(
                        dl.to_string(),
                        Style::default().fg(color),
                    )));
                }
            }
            continue;
        }

        // Headers
        if let Some(stripped) = line.strip_prefix("### ") {
            result.push(Line::from(Span::styled(
                stripped.to_string(),
                Style::default().fg(Theme::ACCENT).add_modifier(Modifier::BOLD),
            )));
            i += 1;
            continue;
        }
        if let Some(stripped) = line.strip_prefix("## ") {
            result.push(Line::from(Span::styled(
                stripped.to_string(),
                Style::default().fg(Theme::ACCENT).add_modifier(Modifier::BOLD),
            )));
            i += 1;
            continue;
        }
        if let Some(stripped) = line.strip_prefix("# ") {
            result.push(Line::from(Span::styled(
                stripped.to_string(),
                Style::default().fg(Theme::ACCENT).add_modifier(Modifier::BOLD),
            )));
            i += 1;
            continue;
        }

        // List items — render bullet, parse inline markdown for rest
        if let Some(rest) = line.strip_prefix("* ") {
            let mut spans = vec![Span::styled(
                "• ".to_string(),
                Style::default().fg(Theme::MUTED_TEXT),
            )];
            spans.extend(parse_inline_markdown(rest));
            result.push(Line::from(spans));
            i += 1;
            continue;
        }

        // Regular line — parse inline markdown
        result.push(Line::from(parse_inline_markdown(line)));
        i += 1;
    }

    result
}

/// Parse inline markdown: **bold**, `code`, plain text.
fn parse_inline_markdown(text: &str) -> Vec<Span<'static>> {
    let mut spans = Vec::new();
    let mut remaining = text;

    while !remaining.is_empty() {
        // Find earliest marker
        let bold_pos = remaining.find("**");
        let code_pos = remaining.find('`');

        let next = match (bold_pos, code_pos) {
            (Some(b), Some(c)) => {
                if b <= c { Some(("**", b)) } else { Some(("`", c)) }
            }
            (Some(b), None) => Some(("**", b)),
            (None, Some(c)) => Some(("`", c)),
            (None, None) => None,
        };

        match next {
            Some(("**", pos)) => {
                // Push text before marker
                if pos > 0 {
                    spans.push(Span::styled(
                        remaining[..pos].to_string(),
                        Style::default().fg(Theme::MUTED_TEXT),
                    ));
                }
                remaining = &remaining[pos + 2..];
                // Find closing **
                if let Some(end) = remaining.find("**") {
                    spans.push(Span::styled(
                        remaining[..end].to_string(),
                        Style::default().fg(Theme::TEXT).add_modifier(Modifier::BOLD),
                    ));
                    remaining = &remaining[end + 2..];
                } else {
                    // No closing ** — emit as plain
                    spans.push(Span::styled(
                        format!("**{}", remaining),
                        Style::default().fg(Theme::MUTED_TEXT),
                    ));
                    remaining = "";
                }
            }
            Some(("`", pos)) => {
                if pos > 0 {
                    spans.push(Span::styled(
                        remaining[..pos].to_string(),
                        Style::default().fg(Theme::MUTED_TEXT),
                    ));
                }
                remaining = &remaining[pos + 1..];
                if let Some(end) = remaining.find('`') {
                    spans.push(Span::styled(
                        remaining[..end].to_string(),
                        Style::default().fg(Theme::ACCENT),
                    ));
                    remaining = &remaining[end + 1..];
                } else {
                    spans.push(Span::styled(
                        format!("`{}", remaining),
                        Style::default().fg(Theme::MUTED_TEXT),
                    ));
                    remaining = "";
                }
            }
            _ => {
                spans.push(Span::styled(
                    remaining.to_string(),
                    Style::default().fg(Theme::MUTED_TEXT),
                ));
                remaining = "";
            }
        }
    }

    if spans.is_empty() {
        spans.push(Span::styled(String::new(), Style::default().fg(Theme::MUTED_TEXT)));
    }

    spans
}

/// Convert tui_markdown's `Text` to owned `Vec<Line<'static>>`.
/// Merges line-level styles (used by tui_markdown for headings) into
/// span-level styles so they survive the lifetime conversion.
/// Strips the raw `# ` prefix spans that tui_markdown leaves on headings.
pub fn own_text_lines(text: ratatui::text::Text<'_>) -> Vec<Line<'static>> {
    text.lines
        .into_iter()
        .map(|line| {
            let line_style = line.style;
            let owned_spans: Vec<Span<'static>> = line
                .spans
                .into_iter()
                .filter(|s| {
                    // Strip tui_markdown's heading prefix spans ("# ", "## ", etc.)
                    let t = s.content.trim_end();
                    !(t.chars().all(|c| c == '#') && !t.is_empty())
                })
                .map(|s| {
                    let merged = line_style.patch(s.style);
                    Span::styled(s.content.to_string(), merged)
                })
                .collect();
            Line::from(owned_spans)
        })
        .collect()
}

/// Shorten an agent ID to first 7 chars (like git short hash).
fn short_id(id: &str) -> String {
    if id.chars().count() > 7 {
        id.chars().take(7).collect()
    } else {
        id.to_string()
    }
}

/// Check if a TranscriptEvent matches the search query.
fn event_matches_search_transcript(kind: &TranscriptEventKind, query: &str, agent_id: Option<&crate::model::AgentId>) -> bool {
    let (_, header, detail, _, tool_name) = format_transcript_event_lines(kind);

    if header.to_lowercase().contains(query) {
        return true;
    }
    if let Some(detail_text) = detail {
        if detail_text.to_lowercase().contains(query) {
            return true;
        }
    }
    if let Some(tool) = tool_name {
        if tool.to_lowercase().contains(query) {
            return true;
        }
    }
    if let Some(aid) = agent_id {
        if aid.as_str().to_lowercase().contains(query) {
            return true;
        }
    }
    false
}


/// Format a TranscriptEventKind into (icon, header, optional detail, color, optional tool_name).
pub fn format_transcript_event_lines(kind: &TranscriptEventKind) -> (&'static str, String, Option<String>, ratatui::style::Color, Option<String>) {
    match kind {
        TranscriptEventKind::UserMessage => ("→", "User message".into(), None, Theme::INFO, None),
        TranscriptEventKind::AssistantMessage { content } => {
            ("💭", "Assistant".into(), Some(content.clone()), Theme::MUTED_TEXT, None)
        }
        TranscriptEventKind::ToolUse { tool_name, input_summary } => {
            let detail = if input_summary.is_empty() {
                None
            } else {
                Some(input_summary.clone())
            };
            ("⚡", tool_name.to_string(), detail, Theme::tool_color(tool_name.as_str()), Some(tool_name.to_string()))
        }
        TranscriptEventKind::ToolResult { tool_name, result_summary, duration_ms } => {
            let duration_text = duration_ms
                .map(|ms| format!(" ({}ms)", ms))
                .unwrap_or_default();
            let header = format!("{}{}", tool_name, duration_text);
            let detail = if result_summary.is_empty() {
                None
            } else {
                Some(result_summary.clone())
            };
            ("✓", header, detail, Theme::tool_color(tool_name.as_str()), Some(tool_name.to_string()))
        }
        TranscriptEventKind::Unknown { entry_type } => {
            ("?", entry_type.clone(), None, Theme::MUTED_TEXT, None)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
        use crate::model::{TranscriptEvent, TranscriptEventKind};

        let mut state = AppState::new();

        // Use two events without detail so we get exactly 1 line per event
        let event1 = TranscriptEvent::new(Utc::now(), TranscriptEventKind::UserMessage);
        let event2 = TranscriptEvent::new(Utc::now(), TranscriptEventKind::UserMessage);

        state.domain.events = VecDeque::from(vec![event1, event2]);

        let lines = build_filtered_event_lines(&state, None);

        // 2 events (each 1 line): header + separator + header = 3 lines
        assert_eq!(lines.len(), 3);
    }

    #[test]
    fn format_transcript_event_user_message() {
        let (icon, header, _, _, _) = format_transcript_event_lines(&TranscriptEventKind::UserMessage);
        assert_eq!(header, "User message");
        assert_eq!(icon, "→");
    }

    #[test]
    fn format_transcript_event_tool_use() {
        let (_, header, detail, _, tool_name) = format_transcript_event_lines(&TranscriptEventKind::ToolUse {
            tool_name: "Read".into(),
            input_summary: "file.rs".to_string(),
        });
        assert!(header.contains("Read"));
        assert_eq!(detail, Some("file.rs".into()));
        assert_eq!(tool_name, Some("Read".to_string()));
    }

    #[test]
    fn format_transcript_event_tool_result_with_duration() {
        let (icon, header, detail, _, _) = format_transcript_event_lines(&TranscriptEventKind::ToolResult {
            tool_name: "Bash".into(),
            result_summary: "success".to_string(),
            duration_ms: Some(250),
        });
        assert!(header.contains("Bash"));
        assert!(header.contains("250ms"));
        assert_eq!(detail, Some("success".into()));
        assert_eq!(icon, "✓");
    }

    #[test]
    fn format_transcript_event_unknown() {
        let (icon, header, _, _, _) = format_transcript_event_lines(&TranscriptEventKind::Unknown {
            entry_type: "some_new_type".to_string(),
        });
        assert_eq!(icon, "?");
        assert_eq!(header, "some_new_type");
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
    fn markdown_renders_code_blocks() {
        let md = "before\n```rust\nfn main() {}\n```\nafter";
        let lines = markdown_to_lines(md, None, 1);
        // before, indented code line, after = 3 lines (fences stripped)
        assert_eq!(lines.len(), 3);
        let code_text: String = lines[1].spans.iter().map(|s| s.content.to_string()).collect();
        assert!(code_text.contains("fn main()"));
    }

    #[test]
    fn markdown_renders_inline_code() {
        let lines = markdown_to_lines("use `foo` here", None, 1);
        let spans = &lines[0].spans;
        assert!(spans.len() >= 3); // "use " + "foo" + " here"
        assert_eq!(spans[1].content.as_ref(), "foo");
        assert_eq!(spans[1].style.fg, Some(Theme::ACCENT));
    }

    #[test]
    fn markdown_renders_bold() {
        let lines = markdown_to_lines("this is **bold** text", None, 1);
        let spans = &lines[0].spans;
        let bold_span = spans.iter().find(|s| s.content.as_ref() == "bold").unwrap();
        assert!(bold_span.style.add_modifier.contains(Modifier::BOLD));
    }

    #[test]
    fn markdown_renders_headers() {
        let lines = markdown_to_lines("# Title\n## Sub\ntext", None, 1);
        assert_eq!(lines.len(), 3);
        let title_text: String = lines[0].spans.iter().map(|s| s.content.to_string()).collect();
        assert_eq!(title_text, "Title");
        assert!(lines[0].spans[0].style.add_modifier.contains(Modifier::BOLD));
    }

    #[test]
    fn markdown_renders_diff_lines() {
        let lines = markdown_to_lines("- removed\n+ added", None, 1);
        assert_eq!(lines[0].spans[0].style.fg, Some(Theme::ERROR));
        assert_eq!(lines[1].spans[0].style.fg, Some(Theme::SUCCESS));
    }

    #[test]
    fn markdown_renders_list_items() {
        let lines = markdown_to_lines("* item one\n* item two", None, 1);
        assert_eq!(lines.len(), 2);
        let first: String = lines[0].spans.iter().map(|s| s.content.to_string()).collect();
        assert!(first.starts_with("• "));
    }

    #[test]
    fn markdown_plain_text_unchanged() {
        let lines = markdown_to_lines("just plain text", None, 1);
        assert_eq!(lines.len(), 1);
        let text: String = lines[0].spans.iter().map(|s| s.content.to_string()).collect();
        assert_eq!(text, "just plain text");
    }

    #[test]
    fn extract_line_offset_parses_prefix() {
        let (offset, rest) = extract_line_offset("@offset:42\nfile contents here");
        assert_eq!(offset, 42);
        assert_eq!(rest, "file contents here");
    }

    #[test]
    fn extract_line_offset_no_prefix() {
        let (offset, rest) = extract_line_offset("just normal text");
        assert_eq!(offset, 1);
        assert_eq!(rest, "just normal text");
    }

    #[test]
    fn extract_line_offset_invalid_number() {
        let (offset, rest) = extract_line_offset("@offset:abc\ntext");
        assert_eq!(offset, 1);
        assert_eq!(rest, "@offset:abc\ntext");
    }

    #[test]
    fn extract_line_offset_no_newline() {
        let (offset, rest) = extract_line_offset("@offset:42");
        assert_eq!(offset, 1);
        assert_eq!(rest, "@offset:42");
    }

    #[test]
    fn agent_label_resolves_from_state() {
        use crate::model::{Agent, TranscriptEvent, TranscriptEventKind};

        let mut state = AppState::new();
        let mut agent = Agent::new("a01", Utc::now());
        agent.agent_type = Some("Explore".into());
        state.domain.agents.insert("a01".into(), agent);

        let event = TranscriptEvent::new(
            Utc::now(),
            TranscriptEventKind::ToolUse {
                tool_name: "Read".into(),
                input_summary: "file.rs".to_string(),
            },
        )
        .with_agent("a01");
        state.domain.events = VecDeque::from(vec![event]);

        let lines = build_filtered_event_lines(&state, None);
        // Header line should contain "Explore"
        let header_text: String = lines[0]
            .spans
            .iter()
            .map(|s| s.content.to_string())
            .collect();
        assert!(header_text.contains("Explore"));
    }

    #[test]
    fn event_matches_search_transcript_tool_use() {
        use crate::model::TranscriptEventKind;
        let kind = TranscriptEventKind::ToolUse {
            tool_name: "Read".into(),
            input_summary: "my_file.rs".to_string(),
        };
        assert!(event_matches_search_transcript(&kind, "read", None));
        assert!(event_matches_search_transcript(&kind, "my_file", None));
        assert!(!event_matches_search_transcript(&kind, "write", None));
    }

    #[test]
    fn event_matches_search_transcript_assistant_message() {
        use crate::model::TranscriptEventKind;
        let kind = TranscriptEventKind::AssistantMessage {
            content: "Here is the analysis".to_string(),
        };
        assert!(event_matches_search_transcript(&kind, "analysis", None));
        assert!(!event_matches_search_transcript(&kind, "other", None));
    }

    #[test]
    fn event_matches_search_transcript_user_message() {
        use crate::model::TranscriptEventKind;
        let kind = TranscriptEventKind::UserMessage;
        // "User message" is the header
        assert!(event_matches_search_transcript(&kind, "user", None));
        assert!(event_matches_search_transcript(&kind, "message", None));
    }

    #[test]
    fn event_matches_search_transcript_in_agent_id() {
        use crate::model::TranscriptEventKind;
        let kind = TranscriptEventKind::UserMessage;
        let agent_id = crate::model::AgentId::new("explore-agent-123");
        assert!(event_matches_search_transcript(&kind, "explore", Some(&agent_id)));
        assert!(event_matches_search_transcript(&kind, "123", Some(&agent_id)));
        assert!(!event_matches_search_transcript(&kind, "write", Some(&agent_id)));
    }

    #[test]
    fn event_matches_search_transcript_special_chars_no_panic() {
        use crate::model::TranscriptEventKind;
        let kind = TranscriptEventKind::ToolUse {
            tool_name: "Read".into(),
            input_summary: "file[1].rs".to_string(),
        };
        let _ = event_matches_search_transcript(&kind, "a.*[b]", None);
        let _ = event_matches_search_transcript(&kind, "[1]", None);
        let _ = event_matches_search_transcript(&kind, "(test)", None);
    }

    #[test]
    fn event_matches_search_transcript_unicode() {
        use crate::model::TranscriptEventKind;
        let kind = TranscriptEventKind::ToolUse {
            tool_name: "Read".into(),
            input_summary: "日本語.rs".to_string(),
        };
        assert!(event_matches_search_transcript(&kind, "日本", None));
        assert!(event_matches_search_transcript(&kind, "本語", None));
        assert!(!event_matches_search_transcript(&kind, "中文", None));
    }
}
