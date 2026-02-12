use chrono::Utc;
use ratatui::{
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem},
    Frame,
};

use crate::app::{AppState, PanelFocus};
use crate::model::Theme;

/// Render agent list panel for agent detail view.
/// Shows all agents with selection highlight.
pub fn render_agent_list(frame: &mut Frame, area: Rect, state: &AppState) {
    let items = build_agent_items(state);
    let is_focused = matches!(state.focus, PanelFocus::Left);

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

/// Pure function: build list items from agents map.
fn build_agent_items(state: &AppState) -> Vec<ListItem<'static>> {
    if state.agents.is_empty() {
        return vec![ListItem::new(Line::from(Span::styled(
            "No agents",
            Style::default().fg(Theme::MUTED_TEXT),
        )))];
    }

    let now = Utc::now();
    let selected = state.selected_agent_index;
    let sorted_keys = state.sorted_agent_keys();

    sorted_keys
        .iter()
        .enumerate()
        .map(|(idx, key)| {
            let agent = &state.agents[key];
            let is_active = agent.finished_at.is_none();
            let (icon, icon_color) = if is_active {
                ("◐", Theme::SUCCESS)
            } else {
                ("●", Theme::MUTED_TEXT)
            };

            let name = agent.display_name().to_string();

            let elapsed = if is_active {
                let secs = (now - agent.started_at).num_seconds();
                format_elapsed(secs)
            } else if let Some(end) = agent.finished_at {
                let secs = (end - agent.started_at).num_seconds();
                format_elapsed(secs)
            } else {
                String::new()
            };

            let is_selected = selected == Some(idx);
            let bg = if is_selected {
                Theme::SELECTION_BG
            } else {
                Theme::BACKGROUND
            };

            let line = Line::from(vec![
                Span::styled(format!("{} ", icon), Style::default().fg(icon_color).bg(bg)),
                Span::styled(
                    name,
                    Style::default()
                        .fg(if is_active { Theme::TEXT } else { Theme::MUTED_TEXT })
                        .bg(bg),
                ),
                Span::styled(
                    format!("  {}", elapsed),
                    Style::default().fg(Theme::MUTED_TEXT).bg(bg),
                ),
            ]);

            ListItem::new(line)
        })
        .collect()
}

fn format_elapsed(secs: i64) -> String {
    if secs < 60 {
        format!("{}s", secs)
    } else if secs < 3600 {
        format!("{}m{}s", secs / 60, secs % 60)
    } else {
        format!("{}h{}m", secs / 3600, (secs % 3600) / 60)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Agent;

    #[test]
    fn build_agent_items_empty() {
        let state = AppState::new();
        let items = build_agent_items(&state);
        assert_eq!(items.len(), 1); // "No agents"
    }

    #[test]
    fn build_agent_items_with_agents() {
        let mut state = AppState::new();
        let mut a1 = Agent::new("a01".into(), Utc::now());
        a1.agent_type = Some("Explore".into());
        state.agents.insert("a01".into(), a1);
        state.agents.insert("a02".into(), Agent::new("a02".into(), Utc::now()));
        state.selected_agent_index = Some(0);

        let items = build_agent_items(&state);
        assert_eq!(items.len(), 2);
    }

    #[test]
    fn format_elapsed_seconds() {
        assert_eq!(format_elapsed(45), "45s");
    }

    #[test]
    fn format_elapsed_minutes() {
        assert_eq!(format_elapsed(125), "2m5s");
    }

    #[test]
    fn format_elapsed_hours() {
        assert_eq!(format_elapsed(3661), "1h1m");
    }
}
