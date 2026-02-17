use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
    Frame,
};

use crate::model::{AgentMessage, MessageKind, Theme, TokenUsage};

/// Render a centered popup showing an agent's full task_description,
/// model badge, loaded skills/references, and token usage breakdown.
pub fn render_prompt_popup(
    frame: &mut Frame,
    area: Rect,
    agent_name: &str,
    model: Option<&str>,
    text: &str,
    messages: &[AgentMessage],
    skills: &[String],
    token_usage: &TokenUsage,
    scroll: usize,
) {
    let popup_area = centered_rect(80, 60, area);
    frame.render_widget(Clear, popup_area);

    let model_tag = model.unwrap_or("inherited");
    let title = format!(" {} [{}] — Prompt (Esc to close) ", agent_name, model_tag);

    let scroll_u16 = scroll.min(u16::MAX as usize).min(10_000) as u16;

    // Build content: prompt text + skills/references + token breakdown
    let refs = extract_references(messages);
    let mut all_refs: Vec<String> = skills.iter().map(|s| format!("skill:{}", s)).collect();
    for r in &refs {
        if !all_refs.contains(r) {
            all_refs.push(r.clone());
        }
    }
    all_refs.sort();
    all_refs.dedup();

    let mut full_text = text.to_string();

    if !all_refs.is_empty() {
        let ref_lines: String = all_refs.iter().map(|r| format!("  {}", r)).collect::<Vec<_>>().join("\n");
        full_text.push_str(&format!("\n\n--- Skills & References ---\n{}", ref_lines));
    }

    if !token_usage.is_empty() {
        full_text.push_str(&format!(
            "\n\n--- Token Usage (last turn) ---\n  Input:          {}\n  Cache Create:   {}\n  Cache Read:     {}\n  Context Window: ~{}",
            token_usage.input_tokens,
            token_usage.cache_creation_input_tokens,
            token_usage.cache_read_input_tokens,
            token_usage.context_window(),
        ));
    }

    let paragraph = Paragraph::new(full_text)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Theme::ACTIVE_BORDER))
                .title(Line::from(Span::styled(
                    title,
                    Style::default().fg(Theme::ACCENT).add_modifier(Modifier::BOLD),
                ))),
        )
        .style(Style::default().fg(Theme::TEXT))
        .wrap(Wrap { trim: false })
        .scroll((scroll_u16, 0));

    frame.render_widget(paragraph, popup_area);
}

/// Extract references (skills/rules/CLAUDE.md files) from agent messages.
/// Scans Read/Glob/Grep targeting `.claude/skills/`, `.claude/rules/`, `CLAUDE.md`
/// and any Skill tool calls. Deduplicates and returns sorted.
pub fn extract_references(messages: &[AgentMessage]) -> Vec<String> {
    let mut refs = Vec::new();

    for msg in messages {
        if let MessageKind::Tool(tc) = &msg.kind {
            let name = tc.tool_name.as_str();
            let input = &tc.input_summary;

            match name {
                "Skill" => {
                    refs.push(format!("skill:{}", input));
                }
                "Read" | "Glob" | "Grep" => {
                    if is_reference_path(input) {
                        // Extract just the filename for brevity
                        let short = input.rsplit('/').next().unwrap_or(input);
                        refs.push(short.to_string());
                    }
                }
                _ => {}
            }
        }
    }

    refs.sort();
    refs.dedup();
    refs
}

/// Check if a path looks like a config/rules/skills reference.
fn is_reference_path(path: &str) -> bool {
    path.contains(".claude/skills/")
        || path.contains(".claude/rules/")
        || path.contains("CLAUDE.md")
}

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{AgentMessage, ToolCall};
    use chrono::Utc;

    #[test]
    fn extract_references_empty_messages() {
        assert!(extract_references(&[]).is_empty());
    }

    #[test]
    fn extract_references_detects_skill_calls() {
        let msgs = vec![
            AgentMessage::tool(Utc::now(), ToolCall::new("Skill", "commit".to_string())),
        ];
        let refs = extract_references(&msgs);
        assert_eq!(refs, vec!["skill:commit"]);
    }

    #[test]
    fn extract_references_detects_claude_md_reads() {
        let msgs = vec![
            AgentMessage::tool(
                Utc::now(),
                ToolCall::new("Read", "/home/user/.claude/CLAUDE.md".to_string()),
            ),
        ];
        let refs = extract_references(&msgs);
        assert_eq!(refs, vec!["CLAUDE.md"]);
    }

    #[test]
    fn extract_references_detects_skills_and_rules() {
        let msgs = vec![
            AgentMessage::tool(
                Utc::now(),
                ToolCall::new("Read", "/proj/.claude/skills/commit.md".to_string()),
            ),
            AgentMessage::tool(
                Utc::now(),
                ToolCall::new("Read", "/proj/.claude/rules/architecture.md".to_string()),
            ),
        ];
        let refs = extract_references(&msgs);
        assert_eq!(refs, vec!["architecture.md", "commit.md"]);
    }

    #[test]
    fn extract_references_deduplicates() {
        let msgs = vec![
            AgentMessage::tool(
                Utc::now(),
                ToolCall::new("Read", "/a/.claude/skills/foo.md".to_string()),
            ),
            AgentMessage::tool(
                Utc::now(),
                ToolCall::new("Read", "/b/.claude/skills/foo.md".to_string()),
            ),
        ];
        let refs = extract_references(&msgs);
        assert_eq!(refs, vec!["foo.md"]);
    }

    #[test]
    fn extract_references_ignores_non_reference_reads() {
        let msgs = vec![
            AgentMessage::tool(
                Utc::now(),
                ToolCall::new("Read", "/proj/src/main.rs".to_string()),
            ),
            AgentMessage::tool(
                Utc::now(),
                ToolCall::new("Bash", "cargo test".to_string()),
            ),
        ];
        assert!(extract_references(&msgs).is_empty());
    }
}
