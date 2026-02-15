use ratatui::{
    layout::{Alignment, Constraint, Rect},
    prelude::Stylize,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Row, Table},
    Frame,
};

use crate::app::state::AppState;
use crate::model::{theme::Theme, SessionMeta, SessionStatus};
use super::components::format::format_duration;

/// Render the sessions archive view into the given content area.
/// Global header is rendered by the view dispatcher.
pub fn render_sessions(frame: &mut Frame, state: &AppState, area: Rect) {

    // Combine confirmed active sessions + archived sessions for display
    let all_sessions: Vec<&SessionMeta> = state.domain.confirmed_active_sessions()
        .map(|(_, m)| m)
        .chain(state.domain.sessions.iter().map(|a| &a.meta))
        .collect();

    // Track which archived sessions are loading
    let active_count = state.domain.confirmed_active_count();

    // Empty state: no sessions at all
    if all_sessions.is_empty() {
        render_empty_state(frame, area);
        return;
    }

    // Build table rows from session list
    let _scroll_offset = state.ui.scroll_offsets.sessions; // TODO: implement scrolling
    let header_row = Row::new(vec![
        "Session ID",
        "Date",
        "Duration",
        "Status",
        "Agents",
        "Tasks",
        "Project",
    ])
    .style(
        Style::default()
            .fg(Theme::INFO)
            .add_modifier(Modifier::BOLD),
    );

    let rows: Vec<Row> = all_sessions
        .iter()
        .enumerate()
        .map(|(idx, session)| {
            let is_selected = state.ui.selected_session_index == Some(idx);
            let style = if is_selected {
                Style::default()
                    .bg(Theme::ACTIVE_BORDER)
                    .fg(Theme::BACKGROUND)
            } else {
                Style::default().fg(Theme::TEXT)
            };

            let status_color = match session.status {
                SessionStatus::Active => Theme::TASK_RUNNING,
                SessionStatus::Completed => Theme::TASK_COMPLETED,
                SessionStatus::Failed => Theme::TASK_FAILED,
                SessionStatus::Cancelled => Theme::MUTED_TEXT,
            };

            // All counts tracked per-session; active sessions get live duration
            let duration = if session.status == SessionStatus::Active {
                (chrono::Utc::now() - session.timestamp).to_std().ok()
            } else {
                session.duration
            };

            // Show loading indicator for session being loaded
            let is_loading = idx >= active_count
                && state.ui.loading_session == Some(idx - active_count);

            let status_str = if is_loading {
                "Loading…".to_string()
            } else {
                format_status(&session.status)
            };

            Row::new(vec![
                session.id.to_string(),
                session.timestamp.format("%Y-%m-%d %H:%M").to_string(),
                format_duration(duration),
                status_str,
                session.agent_count.to_string(),
                session.task_count.to_string(),
                session.project_path.clone(),
            ])
            .style(style)
            .fg(if is_selected {
                Theme::BACKGROUND
            } else if is_loading {
                Theme::WARNING
            } else {
                status_color
            })
        })
        .collect();

    let widths = [
        Constraint::Length(12), // Session ID
        Constraint::Length(16), // Date
        Constraint::Length(10), // Duration
        Constraint::Length(10), // Status
        Constraint::Length(7),  // Agents
        Constraint::Length(6),  // Tasks
        Constraint::Min(20),    // Project (flexible)
    ];

    let table = Table::new(rows, widths)
        .header(header_row)
        .block(
            Block::default()
                .title(" Archived Sessions ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Theme::PANEL_BORDER)),
        )
        .row_highlight_style(
            Style::default()
                .bg(Theme::ACTIVE_BORDER)
                .fg(Theme::BACKGROUND)
                .add_modifier(Modifier::BOLD),
        );

    // Apply scroll offset by skipping rows
    frame.render_widget(table, area);
}

/// Render empty state when no sessions exist.
fn render_empty_state(frame: &mut Frame, area: Rect) {
    let text = vec![
        Line::from(""),
        Line::from(Span::styled(
            "No archived sessions",
            Style::default()
                .fg(Theme::MUTED_TEXT)
                .add_modifier(Modifier::ITALIC),
        )),
        Line::from(""),
        Line::from(Span::styled(
            "Sessions will appear here after completion",
            Style::default().fg(Theme::MUTED_TEXT),
        )),
    ];

    let paragraph = Paragraph::new(text)
        .block(
            Block::default()
                .title(" Archived Sessions ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Theme::PANEL_BORDER)),
        )
        .alignment(Alignment::Center);

    frame.render_widget(paragraph, area);
}

/// Format session status as string.
fn format_status(status: &SessionStatus) -> String {
    match status {
        SessionStatus::Active => "Active",
        SessionStatus::Completed => "Done",
        SessionStatus::Failed => "Failed",
        SessionStatus::Cancelled => "Cancelled",
    }
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::state::AppState;
    use crate::model::{ArchivedSession, SessionMeta};
    use chrono::Utc;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;
    use std::path::PathBuf;
    use std::time::Duration;

    #[test]
    fn test_render_sessions_empty_state() {
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        let state = AppState::new();

        terminal
            .draw(|frame| render_sessions(frame, &state, frame.area()))
            .unwrap();

        let buffer = terminal.backend().buffer();

        let buffer_str: String = (0..buffer.area.height)
            .map(|y| {
                (0..buffer.area.width)
                    .map(|x| buffer.cell((x, y)).unwrap().symbol())
                    .collect::<String>()
            })
            .collect::<Vec<String>>()
            .join("\n");

        assert!(buffer_str.contains("No archived sessions"),
                "Empty state message should be displayed");
    }

    #[test]
    fn test_render_sessions_with_data() {
        let backend = TestBackend::new(120, 24);
        let mut terminal = Terminal::new(backend).unwrap();

        let mut state = AppState::new();
        state.domain.sessions = vec![
            ArchivedSession::new(
                SessionMeta::new("s1", Utc::now(), "/proj/foo".to_string())
                    .with_status(SessionStatus::Completed)
                    .with_duration(Duration::from_secs(300)),
                PathBuf::new(),
            ),
            ArchivedSession::new(
                SessionMeta::new("s2", Utc::now(), "/proj/bar".to_string())
                    .with_status(SessionStatus::Failed),
                PathBuf::new(),
            ),
        ];

        terminal
            .draw(|frame| render_sessions(frame, &state, frame.area()))
            .unwrap();

        let buffer = terminal.backend().buffer();

        let buffer_str: String = (0..buffer.area.height)
            .map(|y| {
                (0..buffer.area.width)
                    .map(|x| buffer.cell((x, y)).unwrap().symbol())
                    .collect::<String>()
            })
            .collect::<Vec<String>>()
            .join("\n");

        assert!(buffer_str.contains("s1"), "Session s1 should be displayed");
        assert!(buffer_str.contains("s2"), "Session s2 should be displayed");
    }

    #[test]
    fn test_format_status() {
        assert_eq!(format_status(&SessionStatus::Active), "Active");
        assert_eq!(format_status(&SessionStatus::Completed), "Done");
        assert_eq!(format_status(&SessionStatus::Failed), "Failed");
        assert_eq!(format_status(&SessionStatus::Cancelled), "Cancelled");
    }
}
