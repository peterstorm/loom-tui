use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};

use crate::app::state::LayoutPickerState;
use crate::model::Theme;
use crate::tmux::LayoutPreset;

/// Render layout picker popup overlay.
pub fn render_layout_picker(frame: &mut Frame, area: Rect, picker: &LayoutPickerState) {
    let selected = match picker {
        LayoutPickerState::Open { selected } => *selected,
        LayoutPickerState::Closed => return,
    };

    let popup_area = centered_rect(44, 50, area);
    frame.render_widget(Clear, popup_area);

    let mut lines: Vec<Line> = vec![
        Line::from(""),
        Line::from(Span::styled(
            " Select layout (j/k, Enter, Esc)",
            Style::default().fg(Theme::MUTED_TEXT),
        )),
        Line::from(""),
    ];

    for (i, preset) in LayoutPreset::ALL.iter().enumerate() {
        let is_selected = i == selected;
        let marker = if is_selected { "▸ " } else { "  " };
        let style = if is_selected {
            Style::default()
                .fg(Theme::ACCENT)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Theme::TEXT)
        };

        lines.push(Line::from(Span::styled(
            format!("{}{}", marker, preset.label()),
            style,
        )));

        // ASCII preview
        let preview_style = if is_selected {
            Style::default().fg(Theme::ACCENT)
        } else {
            Style::default().fg(Theme::MUTED_TEXT)
        };
        for preview_line in preset.ascii_preview().lines() {
            lines.push(Line::from(Span::styled(
                format!("    {preview_line}"),
                preview_style,
            )));
        }
        lines.push(Line::from(""));
    }

    let paragraph = Paragraph::new(lines).block(
        Block::default()
            .title(Line::from(Span::styled(
                " Tmux Layout ",
                Style::default()
                    .fg(Theme::ACCENT)
                    .add_modifier(Modifier::BOLD),
            )))
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Theme::ACTIVE_BORDER)),
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
        let picker = LayoutPickerState::Open { selected: 0 };

        terminal
            .draw(|frame| {
                render_layout_picker(frame, frame.area(), &picker);
            })
            .unwrap();
    }

    #[test]
    fn does_nothing_when_closed() {
        let backend = TestBackend::new(80, 40);
        let mut terminal = Terminal::new(backend).unwrap();
        let picker = LayoutPickerState::Closed;

        terminal
            .draw(|frame| {
                render_layout_picker(frame, frame.area(), &picker);
            })
            .unwrap();
    }
}
