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

/// Separator between keybinding groups.
fn sep() -> Span<'static> {
    Span::styled(" │ ", Style::default().fg(Theme::SEPARATOR))
}

/// Key:label pair.
fn kb(key: &'static str, label: &'static str) -> Vec<Span<'static>> {
    vec![
        Span::styled(key, Style::default().fg(Theme::ACCENT)),
        Span::styled(label, Style::default().fg(Theme::MUTED_TEXT)),
    ]
}

/// Pure function: build footer text based on current view and state.
fn build_footer_text(state: &AppState) -> Line<'static> {
    let mut spans = Vec::new();

    // Navigation group
    spans.extend(kb("q", ":quit"));

    match &state.view {
        ViewState::Dashboard => {
            spans.push(sep());
            spans.extend(kb("1-3", ":views"));
            spans.push(sep());
            spans.extend(kb("Tab", ":focus "));
            spans.extend(kb("j/k", ":scroll "));
            spans.extend(kb("g/G", ":top/bottom"));
            spans.push(sep());
            spans.extend(kb("Enter", ":drill "));
            spans.extend(kb("Space", ":auto-scroll "));
            spans.extend(kb("/", ":filter "));
            spans.extend(kb("?", ":help"));
        }
        ViewState::AgentDetail => {
            spans.push(sep());
            spans.extend(kb("Esc", ":back"));
            spans.push(sep());
            spans.extend(kb("Tab", ":focus "));
            spans.extend(kb("j/k", ":scroll "));
            spans.extend(kb("g/G", ":top/bottom"));
            spans.push(sep());
            spans.extend(kb("?", ":help"));
        }
        ViewState::Sessions => {
            spans.push(sep());
            spans.extend(kb("Esc", ":back"));
            spans.push(sep());
            spans.extend(kb("j/k", ":scroll "));
            spans.extend(kb("g/G", ":top/bottom "));
            spans.extend(kb("Enter", ":detail"));
            spans.push(sep());
            spans.extend(kb("?", ":help"));
        }
        ViewState::SessionDetail => {
            spans.push(sep());
            spans.extend(kb("Esc", ":back"));
            spans.push(sep());
            spans.extend(kb("Tab", ":focus "));
            spans.extend(kb("j/k", ":scroll "));
            spans.extend(kb("g/G", ":top/bottom"));
            spans.push(sep());
            spans.extend(kb("?", ":help"));
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
        assert!(text.contains("Tab:focus"));
        assert!(text.contains("j/k:scroll"));
        assert!(text.contains("g/G:top/bottom"));
        assert!(text.contains("Enter:drill"));
        assert!(text.contains("Space:auto-scroll"));
        assert!(text.contains("/:filter"));
        assert!(text.contains("?:help"));
    }

    #[test]
    fn build_footer_text_agent_detail() {
        let state = AppState::with_view(ViewState::AgentDetail);
        let line = build_footer_text(&state);
        let text: String = line.spans.iter().map(|s| s.content.as_ref()).collect();

        assert!(text.contains("Esc:back"));
        assert!(text.contains("Tab:focus"));
        assert!(text.contains("j/k:scroll"));
    }

    #[test]
    fn build_footer_text_sessions() {
        let state = AppState::with_view(ViewState::Sessions);
        let line = build_footer_text(&state);
        let text: String = line.spans.iter().map(|s| s.content.as_ref()).collect();

        assert!(text.contains("Esc:back"));
        assert!(text.contains("j/k:scroll"));
        assert!(text.contains("Enter:detail"));
    }

    #[test]
    fn build_footer_text_session_detail() {
        let state = AppState::with_view(ViewState::SessionDetail);
        let line = build_footer_text(&state);
        let text: String = line.spans.iter().map(|s| s.content.as_ref()).collect();

        assert!(text.contains("Esc:back"));
        assert!(text.contains("Tab:focus"));
        assert!(text.contains("j/k:scroll"));
    }

    #[test]
    fn build_footer_text_has_separators() {
        let state = AppState::new();
        let line = build_footer_text(&state);
        let text: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(text.contains("│"), "Should have separator characters");
    }
}
