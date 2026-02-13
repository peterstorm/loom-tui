use ratatui::{
    layout::{Alignment, Rect},
    style::Style,
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

use crate::app::state::AppState;
use crate::model::theme::Theme;

/// Render the filter/search bar overlay.
/// Displayed at bottom of screen when filter is active.
/// Shows "/ " prefix with current filter text and cursor.
pub fn render_filter_bar(frame: &mut Frame, state: &AppState) {
    if let Some(ref filter_text) = state.filter {
        let area = frame.area();

        // Position at bottom of screen, height of 3 lines (includes border)
        let filter_area = Rect {
            x: area.x,
            y: area.height.saturating_sub(3),
            width: area.width,
            height: 3,
        };

        let text = Line::from(vec![
            Span::styled("/ ", Style::default().fg(Theme::INFO)),
            Span::styled(filter_text.clone(), Style::default().fg(Theme::TEXT)),
            Span::styled("█", Style::default().fg(Theme::ACTIVE_BORDER)), // Cursor
        ]);

        let paragraph = Paragraph::new(text)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Theme::ACTIVE_BORDER)),
            )
            .alignment(Alignment::Left);

        frame.render_widget(paragraph, filter_area);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::state::AppState;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    #[test]
    fn test_render_filter_bar_with_text() {
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();

        let mut state = AppState::new();
        state.filter = Some("test query".to_string());

        terminal
            .draw(|frame| render_filter_bar(frame, &state))
            .unwrap();

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

        // Check for filter text in buffer
        assert!(buffer_str.contains("test query"), "Filter text should be displayed");
    }

    #[test]
    fn test_render_filter_bar_empty_no_render() {
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();

        let state = AppState::new(); // No filter

        terminal
            .draw(|frame| render_filter_bar(frame, &state))
            .unwrap();

        // Should not panic, just not render anything
        // Test passes if no panic
    }

    #[test]
    fn test_filter_bar_shows_cursor() {
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();

        let mut state = AppState::new();
        state.filter = Some("abc".to_string());

        terminal
            .draw(|frame| render_filter_bar(frame, &state))
            .unwrap();

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

        // Check for cursor character
        assert!(buffer_str.contains("█"), "Cursor should be displayed");
    }
}
