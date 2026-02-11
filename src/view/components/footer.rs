use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

use crate::app::{AppState, ViewState};
use crate::model::Theme;

/// Render footer status bar with keybinding hints.
/// Shows different keybindings based on current view.
pub fn render_footer(frame: &mut Frame, area: Rect, state: &AppState) {
    let footer_text = build_footer_text(state);

    let footer = Paragraph::new(footer_text).style(
        Style::default()
            .fg(Theme::TEXT)
            .bg(Theme::FOOTER_BG)
            .add_modifier(Modifier::DIM),
    );

    frame.render_widget(footer, area);
}

/// Pure function: build footer text based on current view and state.
fn build_footer_text(state: &AppState) -> Line<'static> {
    let mut spans = Vec::new();

    // Common keybindings
    spans.push(Span::styled("q", Style::default().fg(Theme::INFO)));
    spans.push(Span::raw(":quit "));

    // View-specific keybindings
    match &state.view {
        ViewState::Dashboard => {
            spans.push(Span::styled("1-3", Style::default().fg(Theme::INFO)));
            spans.push(Span::raw(":views "));

            spans.push(Span::styled("h/l", Style::default().fg(Theme::INFO)));
            spans.push(Span::raw(":focus "));

            spans.push(Span::styled("j/k", Style::default().fg(Theme::INFO)));
            spans.push(Span::raw(":scroll "));

            spans.push(Span::styled("Enter", Style::default().fg(Theme::INFO)));
            spans.push(Span::raw(":drill "));

            spans.push(Span::styled("Space", Style::default().fg(Theme::INFO)));
            spans.push(Span::raw(":auto-scroll "));

            spans.push(Span::styled("/", Style::default().fg(Theme::INFO)));
            spans.push(Span::raw(":filter "));

            spans.push(Span::styled("?", Style::default().fg(Theme::INFO)));
            spans.push(Span::raw(":help"));
        }
        ViewState::AgentDetail { .. } => {
            spans.push(Span::styled("Esc", Style::default().fg(Theme::INFO)));
            spans.push(Span::raw(":back "));

            spans.push(Span::styled("h/l", Style::default().fg(Theme::INFO)));
            spans.push(Span::raw(":focus "));

            spans.push(Span::styled("j/k", Style::default().fg(Theme::INFO)));
            spans.push(Span::raw(":scroll "));

            spans.push(Span::styled("?", Style::default().fg(Theme::INFO)));
            spans.push(Span::raw(":help"));
        }
        ViewState::Sessions => {
            spans.push(Span::styled("Esc", Style::default().fg(Theme::INFO)));
            spans.push(Span::raw(":back "));

            spans.push(Span::styled("j/k", Style::default().fg(Theme::INFO)));
            spans.push(Span::raw(":scroll "));

            spans.push(Span::styled("Enter", Style::default().fg(Theme::INFO)));
            spans.push(Span::raw(":load "));

            spans.push(Span::styled("?", Style::default().fg(Theme::INFO)));
            spans.push(Span::raw(":help"));
        }
    }

    Line::from(spans)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    #[test]
    fn render_footer_does_not_panic() {
        let backend = TestBackend::new(80, 1);
        let mut terminal = Terminal::new(backend).unwrap();

        let state = AppState::new();

        terminal
            .draw(|frame| {
                render_footer(frame, frame.area(), &state);
            })
            .unwrap();
    }

    #[test]
    fn build_footer_text_dashboard_includes_common_keys() {
        let state = AppState::new();
        let line = build_footer_text(&state);
        let text: String = line.spans.iter().map(|s| s.content.as_ref()).collect();

        assert!(text.contains("q:quit"));
    }

    #[test]
    fn build_footer_text_dashboard_includes_view_specific_keys() {
        let state = AppState::new();
        let line = build_footer_text(&state);
        let text: String = line.spans.iter().map(|s| s.content.as_ref()).collect();

        assert!(text.contains("1-3:views"));
        assert!(text.contains("h/l:focus"));
        assert!(text.contains("j/k:scroll"));
        assert!(text.contains("Enter:drill"));
        assert!(text.contains("Space:auto-scroll"));
        assert!(text.contains("/:filter"));
        assert!(text.contains("?:help"));
    }

    #[test]
    fn build_footer_text_agent_detail() {
        let state = AppState::with_view(ViewState::AgentDetail {
            agent_id: "a01".to_string(),
        });
        let line = build_footer_text(&state);
        let text: String = line.spans.iter().map(|s| s.content.as_ref()).collect();

        assert!(text.contains("Esc:back"));
        assert!(text.contains("h/l:focus"));
        assert!(text.contains("j/k:scroll"));
    }

    #[test]
    fn build_footer_text_sessions() {
        let state = AppState::with_view(ViewState::Sessions);
        let line = build_footer_text(&state);
        let text: String = line.spans.iter().map(|s| s.content.as_ref()).collect();

        assert!(text.contains("Esc:back"));
        assert!(text.contains("j/k:scroll"));
        assert!(text.contains("Enter:load"));
    }
}
