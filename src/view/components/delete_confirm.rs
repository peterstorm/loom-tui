use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};

use crate::app::state::DeleteConfirmState;
use crate::model::Theme;

/// Render delete confirmation popup overlay.
pub fn render_delete_confirm(frame: &mut Frame, area: Rect, confirm: &DeleteConfirmState) {
    let session_ids = match confirm {
        DeleteConfirmState::Open { session_ids } => session_ids,
        DeleteConfirmState::Closed => return,
    };

    let popup_area = centered_rect(40, 30, area);
    frame.render_widget(Clear, popup_area);

    let count = session_ids.len();
    let mut lines: Vec<Line> = vec![
        Line::from(""),
        Line::from(Span::styled(
            format!("Delete {count} session(s)?"),
            Style::default()
                .fg(Theme::WARNING)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
    ];

    // Show up to 5 session IDs
    let show_count = session_ids.len().min(5);
    for id in &session_ids[..show_count] {
        lines.push(Line::from(Span::styled(
            format!("  {id}"),
            Style::default().fg(Theme::TEXT),
        )));
    }
    if session_ids.len() > 5 {
        lines.push(Line::from(Span::styled(
            format!("  ...and {} more", session_ids.len() - 5),
            Style::default().fg(Theme::MUTED_TEXT),
        )));
    }

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "y:confirm  n:cancel",
        Style::default().fg(Theme::MUTED_TEXT),
    )));

    let paragraph = Paragraph::new(lines).block(
        Block::default()
            .title(Line::from(Span::styled(
                " Confirm Delete ",
                Style::default()
                    .fg(Theme::WARNING)
                    .add_modifier(Modifier::BOLD),
            )))
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Theme::WARNING)),
    );

    frame.render_widget(paragraph, popup_area);
}

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
    fn renders_without_panic_when_open() {
        let backend = TestBackend::new(80, 40);
        let mut terminal = Terminal::new(backend).unwrap();
        let confirm = DeleteConfirmState::Open {
            session_ids: vec!["s1".into(), "s2".into()],
        };

        terminal
            .draw(|frame| {
                render_delete_confirm(frame, frame.area(), &confirm);
            })
            .unwrap();
    }

    #[test]
    fn does_nothing_when_closed() {
        let backend = TestBackend::new(80, 40);
        let mut terminal = Terminal::new(backend).unwrap();
        let confirm = DeleteConfirmState::Closed;

        terminal
            .draw(|frame| {
                render_delete_confirm(frame, frame.area(), &confirm);
            })
            .unwrap();
    }

    #[test]
    fn shows_truncated_list_for_many_sessions() {
        let backend = TestBackend::new(80, 40);
        let mut terminal = Terminal::new(backend).unwrap();
        let ids: Vec<_> = (0..8).map(|i| format!("s{i}").into()).collect();
        let confirm = DeleteConfirmState::Open { session_ids: ids };

        terminal
            .draw(|frame| {
                render_delete_confirm(frame, frame.area(), &confirm);
            })
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

        assert!(buffer_str.contains("and 3 more"));
    }
}
