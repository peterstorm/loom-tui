use ratatui::{
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};

use crate::model::theme::Theme;

/// Render the help overlay.
/// Displayed as centered popup when show_help is true.
/// Lists all keybindings grouped by category.
pub fn render_help_overlay(frame: &mut Frame) {
    let area = frame.area();

    // Create centered popup area (60% width, 70% height)
    let popup_area = centered_rect(60, 70, area);

    // Clear the area behind the popup
    frame.render_widget(Clear, popup_area);

    let help_text = build_help_text();

    let paragraph = Paragraph::new(help_text)
        .block(
            Block::default()
                .title(" Help - Press ? or Esc to close ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Theme::ACTIVE_BORDER)),
        )
        .alignment(Alignment::Left)
        .style(Style::default().bg(Theme::BACKGROUND).fg(Theme::TEXT));

    frame.render_widget(paragraph, popup_area);
}

/// Build help text with keybindings grouped by category.
fn build_help_text() -> Vec<Line<'static>> {
    vec![
        Line::from(""),
        Line::from(Span::styled(
            "NAVIGATION",
            Style::default()
                .fg(Theme::INFO)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from("  1           - Dashboard view"),
        Line::from("  2           - Agent detail view"),
        Line::from("  3           - Sessions view"),
        Line::from("  Tab         - Switch panel focus"),
        Line::from("  h / l       - Focus left / right panel"),
        Line::from(""),
        Line::from(Span::styled(
            "SCROLLING",
            Style::default()
                .fg(Theme::INFO)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from("  j / k       - Scroll down / up"),
        Line::from("  Ctrl+d / u  - Page down / up"),
        Line::from("  g / G       - Jump to top / bottom"),
        Line::from("  Space       - Toggle auto-scroll (event stream)"),
        Line::from(""),
        Line::from(Span::styled(
            "ACTIONS",
            Style::default()
                .fg(Theme::INFO)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from("  Enter       - Drill down / select"),
        Line::from("  Esc         - Go back / close"),
        Line::from("  /           - Start filter/search"),
        Line::from("  ?           - Toggle help overlay"),
        Line::from("  q           - Quit application"),
        Line::from(""),
        Line::from(Span::styled(
            "VIEW-SPECIFIC",
            Style::default()
                .fg(Theme::INFO)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from("  Dashboard:"),
        Line::from("    Enter on task  - Jump to agent detail"),
        Line::from(""),
        Line::from("  Sessions:"),
        Line::from("    Enter      - Load archived session"),
        Line::from(""),
    ]
}

/// Create a centered rect using up certain percentage of the available rect.
fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::vertical([
        Constraint::Percentage((100 - percent_y) / 2),
        Constraint::Percentage(percent_y),
        Constraint::Percentage((100 - percent_y) / 2),
    ])
    .split(r);

    Layout::horizontal([
        Constraint::Percentage((100 - percent_x) / 2),
        Constraint::Percentage(percent_x),
        Constraint::Percentage((100 - percent_x) / 2),
    ])
    .split(popup_layout[1])[1]
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    #[test]
    fn test_render_help_overlay() {
        let backend = TestBackend::new(120, 40);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal.draw(|frame| render_help_overlay(frame)).unwrap();

        let buffer = terminal.backend().buffer();

        // Convert buffer to string for easier searching
        let buffer_str: String = (0..buffer.area.height)
            .map(|y| {
                (0..buffer.area.width)
                    .map(|x| buffer.cell((x, y)).unwrap().symbol())
                    .collect::<String>()
            })
            .collect::<Vec<String>>()
            .join("\n");

        // Check for key sections in help text
        assert!(buffer_str.contains("NAVIGATION"), "NAVIGATION section should be displayed");
        assert!(buffer_str.contains("SCROLLING"), "SCROLLING section should be displayed");
        assert!(buffer_str.contains("ACTIONS"), "ACTIONS section should be displayed");
    }

    #[test]
    fn test_help_overlay_has_quit_keybinding() {
        let backend = TestBackend::new(120, 40);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal.draw(|frame| render_help_overlay(frame)).unwrap();

        let buffer = terminal.backend().buffer();

        // Convert buffer to string for easier searching
        let buffer_str: String = (0..buffer.area.height)
            .map(|y| {
                (0..buffer.area.width)
                    .map(|x| buffer.cell((x, y)).unwrap().symbol())
                    .collect::<String>()
            })
            .collect::<Vec<String>>()
            .join("\n");

        // Check for quit keybinding
        assert!(buffer_str.contains("Quit application"), "Quit keybinding should be documented");
    }

    #[test]
    fn test_centered_rect() {
        let full_area = Rect {
            x: 0,
            y: 0,
            width: 100,
            height: 50,
        };

        let centered = centered_rect(60, 70, full_area);

        // Should be roughly centered (within rounding)
        assert!(centered.width <= 60);
        assert!(centered.height <= 35); // 70% of 50
        assert!(centered.x > 0);
        assert!(centered.y > 0);
    }

    #[test]
    fn test_build_help_text_contains_all_categories() {
        let help_lines = build_help_text();
        let help_str: String = help_lines
            .iter()
            .map(|line| {
                line.spans
                    .iter()
                    .map(|span| span.content.as_ref())
                    .collect::<String>()
            })
            .collect::<Vec<String>>()
            .join("\n");

        assert!(help_str.contains("NAVIGATION"));
        assert!(help_str.contains("SCROLLING"));
        assert!(help_str.contains("ACTIONS"));
        assert!(help_str.contains("VIEW-SPECIFIC"));
    }
}
