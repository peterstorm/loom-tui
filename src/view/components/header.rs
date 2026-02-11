use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

use crate::app::AppState;
use crate::model::Theme;

/// Render header bar.
/// Shows: project name, current wave, task progress, active agents.
///
/// Format: "loom-tui | Wave N | X/Y tasks | N agents"
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
    let project_name = "loom-tui";
    let active_agents = state.agents.values().filter(|a| a.finished_at.is_none()).count();

    match &state.task_graph {
        Some(graph) => {
            let current_wave = calculate_current_wave(graph);
            let progress = format!("{}/{} tasks", graph.completed_tasks, graph.total_tasks);

            Line::from(vec![
                Span::styled(project_name, Style::default().fg(Theme::INFO)),
                Span::raw(" | "),
                Span::styled(
                    format!("Wave {}", current_wave),
                    Style::default().fg(Theme::WARNING),
                ),
                Span::raw(" | "),
                Span::styled(progress, Style::default().fg(Theme::SUCCESS)),
                Span::raw(" | "),
                Span::styled(
                    format!("{} agents", active_agents),
                    Style::default().fg(if active_agents > 0 { Theme::TASK_RUNNING } else { Theme::MUTED_TEXT }),
                ),
            ])
        }
        None => {
            let mut spans = vec![
                Span::styled(project_name, Style::default().fg(Theme::INFO)),
                Span::raw(" | "),
                Span::styled("No tasks loaded", Style::default().fg(Theme::MUTED_TEXT)),
            ];

            if active_agents > 0 {
                spans.push(Span::raw(" | "));
                spans.push(Span::styled(
                    format!("{} agents", active_agents),
                    Style::default().fg(Theme::TASK_RUNNING),
                ));
            }

            Line::from(spans)
        }
    }
}

/// Calculate current wave number from task graph.
/// Current wave = first wave with incomplete tasks, or last wave if all complete.
fn calculate_current_wave(graph: &crate::model::TaskGraph) -> u32 {
    for wave in &graph.waves {
        let all_complete = wave
            .tasks
            .iter()
            .all(|t| matches!(t.status, crate::model::TaskStatus::Completed));

        if !all_complete {
            return wave.number;
        }
    }

    // All waves complete, return last wave number
    graph.waves.last().map(|w| w.number).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Task, TaskGraph, TaskStatus, Wave};
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

        assert!(text.contains("loom-tui"));
        assert!(text.contains("No tasks loaded"));
    }

    #[test]
    fn build_header_text_with_tasks() {
        use crate::model::Agent;
        use chrono::Utc;

        let waves = vec![Wave::new(
            1,
            vec![
                Task::new("T1".into(), "Task 1".into(), TaskStatus::Completed),
                Task::new("T2".into(), "Task 2".into(), TaskStatus::Running),
            ],
        )];

        let mut state = AppState::new();
        state.task_graph = Some(TaskGraph::new(waves));

        // Add one active agent
        let now = Utc::now();
        state.agents.insert("a01".into(), Agent::new("a01".into(), now));

        let line = build_header_text(&state);
        let text: String = line.spans.iter().map(|s| s.content.as_ref()).collect();

        assert!(text.contains("loom-tui"));
        assert!(text.contains("Wave 1"));
        assert!(text.contains("1/2 tasks"));
        assert!(text.contains("1 agents"));
    }

    #[test]
    fn calculate_current_wave_returns_first_incomplete() {
        let waves = vec![
            Wave::new(
                1,
                vec![Task::new("T1".into(), "Task 1".into(), TaskStatus::Completed)],
            ),
            Wave::new(
                2,
                vec![Task::new("T2".into(), "Task 2".into(), TaskStatus::Running)],
            ),
            Wave::new(
                3,
                vec![Task::new("T3".into(), "Task 3".into(), TaskStatus::Pending)],
            ),
        ];

        let graph = TaskGraph::new(waves);
        assert_eq!(calculate_current_wave(&graph), 2);
    }

    #[test]
    fn calculate_current_wave_returns_last_if_all_complete() {
        let waves = vec![
            Wave::new(
                1,
                vec![Task::new("T1".into(), "Task 1".into(), TaskStatus::Completed)],
            ),
            Wave::new(
                2,
                vec![Task::new("T2".into(), "Task 2".into(), TaskStatus::Completed)],
            ),
        ];

        let graph = TaskGraph::new(waves);
        assert_eq!(calculate_current_wave(&graph), 2);
    }

    #[test]
    fn calculate_current_wave_returns_zero_for_empty_graph() {
        let graph = TaskGraph::empty();
        assert_eq!(calculate_current_wave(&graph), 0);
    }

    #[test]
    fn build_header_text_shows_active_agents() {
        use crate::model::Agent;
        use chrono::Utc;

        let waves = vec![Wave::new(
            1,
            vec![Task::new("T1".into(), "Task 1".into(), TaskStatus::Running)],
        )];

        let mut state = AppState::new();
        state.task_graph = Some(TaskGraph::new(waves));

        let now = Utc::now();
        let later = now + chrono::Duration::seconds(10);

        // Add 2 agents: 1 active, 1 finished
        state.agents.insert("a01".into(), Agent::new("a01".into(), now));
        state.agents.insert("a02".into(), Agent::new("a02".into(), now).finish(later));

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

        // Add active agent but no task graph
        state.agents.insert("a01".into(), Agent::new("a01".into(), now));

        let line = build_header_text(&state);
        let text: String = line.spans.iter().map(|s| s.content.as_ref()).collect();

        assert!(text.contains("No tasks loaded"));
        assert!(text.contains("1 agents"));
    }

    #[test]
    fn build_header_text_hides_zero_agents_without_task_graph() {
        let state = AppState::new();

        let line = build_header_text(&state);
        let text: String = line.spans.iter().map(|s| s.content.as_ref()).collect();

        assert!(text.contains("No tasks loaded"));
        assert!(!text.contains("agents"), "Should not show '0 agents'");
    }
}
