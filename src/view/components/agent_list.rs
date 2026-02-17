use chrono::Utc;
use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem},
    Frame,
};

use crate::app::{AppState, PanelFocus};
use crate::model::{Agent, Theme};
use super::format::format_elapsed;

/// Render agent list panel for agent detail view (uses global state).
pub fn render_agent_list(frame: &mut Frame, area: Rect, state: &AppState) {
    let sorted_keys = state.sorted_agent_keys();
    let agents: Vec<&Agent> = sorted_keys
        .iter()
        .filter_map(|k| state.domain.agents.get(k))
        .collect();
    let tool_counts: Vec<usize> = sorted_keys
        .iter()
        .map(|k| state.agent_tool_count(k))
        .collect();
    let is_focused = matches!(state.ui.focus, PanelFocus::Left);

    render_agent_list_generic(
        frame,
        area,
        &agents,
        state.ui.selected_agent_index,
        Some(&tool_counts),
        is_focused,
    );
}

/// Render agent list panel from a generic agent slice.
/// Reusable across agent detail and session detail views.
pub fn render_agent_list_generic(
    frame: &mut Frame,
    area: Rect,
    agents: &[&Agent],
    selected: Option<usize>,
    tool_counts: Option<&[usize]>,
    is_focused: bool,
) {
    let items = build_agent_items_generic(agents, selected, tool_counts);

    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(if is_focused {
                    Theme::ACTIVE_BORDER
                } else {
                    Theme::PANEL_BORDER
                }))
                .title("Agents"),
        )
        .highlight_style(Style::default().bg(Theme::SELECTION_BG));

    frame.render_widget(list, area);
}

/// Pure function: build list items from an agent slice.
fn build_agent_items_generic(
    agents: &[&Agent],
    selected: Option<usize>,
    tool_counts: Option<&[usize]>,
) -> Vec<ListItem<'static>> {
    if agents.is_empty() {
        return vec![ListItem::new(Line::from(Span::styled(
            "No agents",
            Style::default().fg(Theme::MUTED_TEXT),
        )))];
    }

    let now = Utc::now();

    // Count display names to detect duplicates
    let name_counts: std::collections::HashMap<String, usize> = agents
        .iter()
        .map(|a| a.display_name().to_string())
        .fold(std::collections::HashMap::new(), |mut acc, name| {
            *acc.entry(name).or_insert(0) += 1;
            acc
        });

    agents
        .iter()
        .enumerate()
        .map(|(idx, agent)| {
            let is_active = agent.finished_at.is_none();
            let (icon, icon_color) = if is_active {
                ("◐", Theme::ACCENT_WARM)
            } else {
                ("●", Theme::MUTED_TEXT)
            };

            let base_name = agent.display_name().to_string();
            let name = if name_counts.get(&base_name).copied().unwrap_or(0) > 1 {
                let short_id = &agent.id.as_str()[..agent.id.as_str().len().min(7)];
                format!("{} ({})", base_name, short_id)
            } else {
                base_name
            };

            let elapsed = if is_active {
                let secs = (now - agent.started_at).num_seconds();
                format_elapsed(secs)
            } else if let Some(end) = agent.finished_at {
                let secs = (end - agent.started_at).num_seconds();
                format_elapsed(secs)
            } else {
                String::new()
            };

            let tool_count = tool_counts
                .and_then(|tc| tc.get(idx).copied())
                .unwrap_or(0);

            let is_selected = selected == Some(idx);
            let bg = if is_selected {
                Theme::SELECTION_BG
            } else {
                Theme::BACKGROUND
            };
            let name_style = if is_selected {
                Style::default().fg(Theme::ACCENT).bg(bg).add_modifier(Modifier::BOLD)
            } else if is_active {
                Style::default().fg(Theme::TEXT).bg(bg)
            } else {
                Style::default().fg(Theme::MUTED_TEXT).bg(bg)
            };

            let mut spans = vec![
                Span::styled(format!("{} ", icon), Style::default().fg(icon_color).bg(bg)),
                Span::styled(name, name_style),
                Span::styled(
                    format!("  {}", elapsed),
                    Style::default().fg(Theme::MUTED_TEXT).bg(bg),
                ),
            ];

            if tool_count > 0 {
                spans.push(Span::styled(
                    format!("  {} tools", tool_count),
                    Style::default().fg(Theme::MUTED_TEXT).bg(bg),
                ));
            }

            let ctx_tokens = agent.token_usage.context_window();
            if ctx_tokens > 0 {
                spans.push(Span::styled(
                    format!("  ~{}tok", format_token_count(ctx_tokens)),
                    Style::default().fg(Theme::MUTED_TEXT).bg(bg),
                ));
            }

            ListItem::new(Line::from(spans))
        })
        .collect()
}

/// Format a token count for compact display: 42k, 1.2M, etc.
fn format_token_count(n: u64) -> String {
    if n >= 1_000_000 {
        let m = n as f64 / 1_000_000.0;
        if m >= 10.0 {
            format!("{}M", m as u64)
        } else {
            format!("{:.1}M", m)
        }
    } else if n >= 1_000 {
        let k = n as f64 / 1_000.0;
        if k >= 10.0 {
            format!("{}k", k as u64)
        } else {
            format!("{:.1}k", k)
        }
    } else {
        format!("{}", n)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_agent_items_empty() {
        let items = build_agent_items_generic(&[], None, None);
        assert_eq!(items.len(), 1); // "No agents"
    }

    #[test]
    fn build_agent_items_with_agents() {
        let mut a1 = Agent::new("a01", Utc::now());
        a1.agent_type = Some("Explore".into());
        let a2 = Agent::new("a02", Utc::now());
        let agents: Vec<&Agent> = vec![&a1, &a2];

        let items = build_agent_items_generic(&agents, Some(0), None);
        assert_eq!(items.len(), 2);
    }

    #[test]
    fn format_token_count_small() {
        assert_eq!(format_token_count(500), "500");
    }

    #[test]
    fn format_token_count_thousands() {
        assert_eq!(format_token_count(1_200), "1.2k");
        assert_eq!(format_token_count(42_000), "42k");
    }

    #[test]
    fn format_token_count_millions() {
        assert_eq!(format_token_count(1_200_000), "1.2M");
        assert_eq!(format_token_count(15_000_000), "15M");
    }
}
