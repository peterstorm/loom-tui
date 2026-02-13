use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    Frame,
};

use crate::app::AppState;

use super::components::{
    render_banner, render_event_stream, render_footer, render_task_list, render_wave_river,
};

/// Render dashboard view into the given content area.
/// Header is rendered globally by the view dispatcher.
pub fn render_dashboard(frame: &mut Frame, state: &AppState, area: Rect) {
    let main_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Wave river
            Constraint::Min(10),  // Main content area
            Constraint::Length(1), // Footer
        ])
        .split(area);

    // Render banner if hook status is Missing or InstallFailed
    let content_area = match &state.hook_status {
        crate::app::HookStatus::Missing | crate::app::HookStatus::InstallFailed(_) => {
            // Insert banner above content
            let banner_layout = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(2), // Banner
                    Constraint::Min(8),   // Content
                ])
                .split(main_layout[1]);

            render_banner(frame, banner_layout[0], state);
            banner_layout[1]
        }
        _ => main_layout[1],
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
    render_task_list(frame, content_columns[0], state);
    render_event_stream(frame, content_columns[1], state);
    render_footer(frame, main_layout[2], state);
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
