use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

use crate::app::{AppState, TaskViewMode};
use crate::model::Theme;

use super::components::{
    render_banner, render_event_stream, render_footer, render_kanban_board, render_task_list,
    render_wave_river,
};

/// Render dashboard view into the given content area.
/// Header is rendered globally by the view dispatcher.
pub fn render_dashboard(frame: &mut Frame, state: &AppState, area: Rect) {
    // Add search bar if filter is active
    let has_search = state.ui.filter.is_some();

    let main_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints(if has_search {
            vec![
                Constraint::Length(3), // Wave river
                Constraint::Length(3), // Search bar
                Constraint::Min(10),  // Main content area
                Constraint::Length(1), // Footer
            ]
        } else {
            vec![
                Constraint::Length(3), // Wave river
                Constraint::Min(10),  // Main content area
                Constraint::Length(1), // Footer
            ]
        })
        .split(area);

    // Adjust indices based on whether search bar is present
    let (content_idx, footer_idx) = if has_search {
        (2, 3)
    } else {
        (1, 2)
    };

    // Render search bar if active
    if has_search {
        render_search_bar(frame, main_layout[1], state);
    }

    // Render banner if hook status is Missing or InstallFailed
    let content_area = match &state.meta.hook_status {
        crate::app::HookStatus::Missing | crate::app::HookStatus::InstallFailed(_) => {
            // Insert banner above content
            let banner_layout = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(2), // Banner
                    Constraint::Min(8),   // Content
                ])
                .split(main_layout[content_idx]);

            render_banner(frame, banner_layout[0], state);
            banner_layout[1]
        }
        _ => main_layout[content_idx],
    };

    // Split content area into two columns
    let content_columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(50), // Task list
            Constraint::Percentage(50), // Event stream
        ])
        .split(content_area);

    // Render all components
    render_wave_river(frame, main_layout[0], state);

    // Render task list OR kanban based on view mode
    match state.ui.task_view_mode {
        TaskViewMode::Wave => render_task_list(frame, content_columns[0], state),
        TaskViewMode::Kanban => render_kanban_board(frame, content_columns[0], state),
    }

    render_event_stream(frame, content_columns[1], state);
    render_footer(frame, main_layout[footer_idx], state);
}

/// Render search bar showing current filter text.
fn render_search_bar(frame: &mut Frame, area: Rect, state: &AppState) {
    let filter_text = state.ui.filter.as_deref().unwrap_or("");

    let content = Line::from(vec![
        Span::styled("/ ", Style::default().fg(Theme::INFO).add_modifier(Modifier::BOLD)),
        Span::styled(filter_text, Style::default().fg(Theme::TEXT)),
        Span::styled("_", Style::default().fg(Theme::MUTED_TEXT)),
    ]);

    let paragraph = Paragraph::new(content)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Theme::ACTIVE_BORDER))
                .title("Search (Esc to clear)"),
        );

    frame.render_widget(paragraph, area);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::HookStatus;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    #[test]
    fn render_dashboard_does_not_panic_with_empty_state() {
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();

        let state = AppState::new();

        terminal
            .draw(|frame| {
                render_dashboard(frame, &state, frame.area());
            })
            .unwrap();
    }

    #[test]
    fn render_dashboard_does_not_panic_with_hook_missing() {
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();

        let state = AppState::with_hook_status(HookStatus::Missing);

        terminal
            .draw(|frame| {
                render_dashboard(frame, &state, frame.area());
            })
            .unwrap();
    }

    #[test]
    fn render_dashboard_does_not_panic_with_hook_failed() {
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();

        let state = AppState::with_hook_status(HookStatus::InstallFailed("test".to_string()));

        terminal
            .draw(|frame| {
                render_dashboard(frame, &state, frame.area());
            })
            .unwrap();
    }

    #[test]
    fn render_dashboard_does_not_panic_with_small_terminal() {
        let backend = TestBackend::new(40, 12);
        let mut terminal = Terminal::new(backend).unwrap();

        let state = AppState::new();

        terminal
            .draw(|frame| {
                render_dashboard(frame, &state, frame.area());
            })
            .unwrap();
    }
}
