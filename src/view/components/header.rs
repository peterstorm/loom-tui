use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

use crate::app::{AppState, ViewState};
use crate::model::Theme;
use super::format::format_elapsed;

/// Render header bar.
/// Shows: view indicator, wave, task progress, agents, elapsed time.
pub fn render_header(frame: &mut Frame, area: Rect, state: &AppState) {
    let header_text = build_header_text(state);

    let header = Paragraph::new(header_text).style(
        Style::default()
            .fg(Theme::TEXT)
            .bg(Theme::HEADER_BG)
            .add_modifier(Modifier::BOLD),
    );

    frame.render_widget(header, area);
}

/// Pure function: build header text from state.
fn build_header_text(state: &AppState) -> Line<'static> {
    let active_agents = state.domain.agents.values().filter(|a| a.finished_at.is_none()).count();
    let elapsed = format_elapsed(state.meta.started_at.elapsed().as_secs() as i64);

    let view_indicator = match state.ui.view {
        ViewState::Dashboard => "[1:Dashboard]",
        ViewState::AgentDetail => "[2:Agents]",
        ViewState::Sessions => "[3:Sessions]",
        ViewState::SessionDetail => "[3:Session Detail]",
    };

    let project_name = if state.meta.project_path.is_empty() {
        "loom".to_string()
    } else {
        state.meta.project_path
            .rsplit('/')
            .find(|s| !s.is_empty())
            .unwrap_or("loom")
            .to_string()
    };

    let mut spans = vec![
        Span::styled(project_name, Style::default().fg(Theme::ACCENT)),
        Span::styled(" ", Style::default()),
        Span::styled(view_indicator, Style::default().fg(Theme::INFO)),
    ];

    match &state.domain.task_graph {
        Some(graph) => {
            let current_wave = graph.current_wave();
            let progress = format!("{}/{}", graph.completed_tasks(), graph.total_tasks());

            spans.push(Span::styled(
                "  W",
                Style::default().fg(Theme::MUTED_TEXT),
            ));
            spans.push(Span::styled(
                format!("{}", current_wave),
                Style::default().fg(Theme::ACCENT_WARM),
            ));
            spans.push(Span::styled(
                format!("  {}", progress),
                Style::default().fg(Theme::SUCCESS),
            ));
        }
        None => {
            spans.push(Span::styled(
                "  No tasks",
                Style::default().fg(Theme::MUTED_TEXT),
            ));
        }
    }

    if active_agents > 0 {
        spans.push(Span::styled(
            format!("  {} agents", active_agents),
            Style::default().fg(Theme::ACCENT_WARM),
        ));
    }

    spans.push(Span::styled(
        format!("  {}", elapsed),
        Style::default().fg(Theme::MUTED_TEXT),
    ));

    Line::from(spans)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{AgentId, Task, TaskGraph, TaskStatus, Wave};
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    #[test]
    fn render_header_does_not_panic_with_empty_state() {
        let backend = TestBackend::new(80, 1);
        let mut terminal = Terminal::new(backend).unwrap();

        let state = AppState::new();

        terminal
            .draw(|frame| {
                render_header(frame, frame.area(), &state);
            })
            .unwrap();
    }

    #[test]
    fn render_header_shows_no_tasks_when_graph_none() {
        let backend = TestBackend::new(80, 1);
        let mut terminal = Terminal::new(backend).unwrap();

        let state = AppState::new();

        let result = terminal
            .draw(|frame| {
                render_header(frame, frame.area(), &state);
            })
            .unwrap();

        let buffer = result.buffer;
        let content = buffer.content();
        let text: String = content.iter().map(|c| c.symbol()).collect();

        assert!(text.contains("loom"));
        assert!(text.contains("No tasks"));
    }

    #[test]
    fn build_header_text_with_tasks() {
        use crate::model::Agent;
        use chrono::Utc;

        let waves = vec![Wave::new(
            1,
            vec![
                Task::new("T1", "Task 1".to_string(), TaskStatus::Completed),
                Task::new("T2", "Task 2".to_string(), TaskStatus::Running),
            ],
        )];

        let mut state = AppState::new();
        state.domain.task_graph = Some(TaskGraph::new(waves));

        let now = Utc::now();
        state.domain.agents.insert(AgentId::new("a01"), Agent::new("a01", now));

        let line = build_header_text(&state);
        let text: String = line.spans.iter().map(|s| s.content.as_ref()).collect();

        assert!(text.contains("loom"));
        assert!(text.contains("W1"));
        assert!(text.contains("1/2"));
        assert!(text.contains("1 agents"));
    }

    #[test]
    fn build_header_text_shows_active_agents() {
        use crate::model::Agent;
        use chrono::Utc;

        let waves = vec![Wave::new(
            1,
            vec![Task::new("T1", "Task 1".to_string(), TaskStatus::Running)],
        )];

        let mut state = AppState::new();
        state.domain.task_graph = Some(TaskGraph::new(waves));

        let now = Utc::now();
        let later = now + chrono::Duration::seconds(10);

        state.domain.agents.insert(AgentId::new("a01"), Agent::new("a01", now));
        state.domain.agents.insert(AgentId::new("a02"), Agent::new("a02", now).finish(later));

        let line = build_header_text(&state);
        let text: String = line.spans.iter().map(|s| s.content.as_ref()).collect();

        assert!(text.contains("1 agents"), "Expected '1 agents' but got: {}", text);
    }

    #[test]
    fn build_header_text_shows_agents_without_task_graph() {
        use crate::model::Agent;
        use chrono::Utc;

        let mut state = AppState::new();
        let now = Utc::now();

        state.domain.agents.insert(AgentId::new("a01"), Agent::new("a01", now));

        let line = build_header_text(&state);
        let text: String = line.spans.iter().map(|s| s.content.as_ref()).collect();

        assert!(text.contains("No tasks"));
        assert!(text.contains("1 agents"));
    }

    #[test]
    fn build_header_text_hides_zero_agents_without_task_graph() {
        let state = AppState::new();

        let line = build_header_text(&state);
        let text: String = line.spans.iter().map(|s| s.content.as_ref()).collect();

        assert!(text.contains("No tasks"));
        assert!(!text.contains("agents"), "Should not show '0 agents'");
    }

    #[test]
    fn build_header_text_shows_view_indicator() {
        let state = AppState::new();
        let line = build_header_text(&state);
        let text: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(text.contains("[1:Dashboard]"));
    }

    #[test]
    fn build_header_text_shows_elapsed() {
        let state = AppState::new();
        let line = build_header_text(&state);
        let text: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
        // Should have some elapsed time indicator (0s or 1s)
        assert!(text.contains('s'), "Should contain elapsed seconds");
    }
}
