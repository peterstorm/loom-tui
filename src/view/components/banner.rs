use ratatui::{
    layout::Rect,
    style::Style,
    text::Line,
    widgets::{Block, Borders, Paragraph},
    Frame,
};

use crate::app::state::AppState;
use crate::model::Theme;

/// Render banner for hook status warnings
pub fn render_banner(frame: &mut Frame, area: Rect, state: &AppState) {
    let message = match &state.hook_status {
        crate::app::state::HookStatus::Missing => "Hooks not installed. Press 'i' to install.",
        crate::app::state::HookStatus::InstallFailed(err) => {
            return render_failed_banner(frame, area, err);
        }
        _ => return,
    };

    let paragraph = Paragraph::new(Line::from(message))
        .style(Style::default().fg(Theme::WARNING).bg(Theme::HEADER_BG))
        .block(Block::default().borders(Borders::NONE));

    frame.render_widget(paragraph, area);
}

fn render_failed_banner(frame: &mut Frame, area: Rect, error: &str) {
    let message = format!("Hook installation failed: {}", error);
    let paragraph = Paragraph::new(Line::from(message))
        .style(Style::default().fg(Theme::ERROR).bg(Theme::HEADER_BG))
        .block(Block::default().borders(Borders::NONE));

    frame.render_widget(paragraph, area);
}
