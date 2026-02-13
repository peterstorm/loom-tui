use ratatui::{
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

use crate::app::AppState;
use crate::model::{TaskStatus, Theme};

/// Render wave river: horizontal swim-lane showing waves and task statuses.
///
/// Format:
/// ```text
/// Wave 1 [●●○] 2/3  Wave 2 [○○○] 0/3
/// ```
///
/// Color coding:
/// - Pending: gray
/// - Running: yellow
/// - Implemented: cyan
/// - Completed: green
/// - Failed: red
pub fn render_wave_river(frame: &mut Frame, area: Rect, state: &AppState) {
    let wave_text = build_wave_river_text(state);

    let wave_river = Paragraph::new(wave_text)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Theme::PANEL_BORDER))
                .title("Waves"),
        )
        .style(Style::default().fg(Theme::TEXT));

    frame.render_widget(wave_river, area);
}

/// Pure function: build wave river text from state.
fn build_wave_river_text(state: &AppState) -> Vec<Line<'static>> {
    match &state.domain.task_graph {
        Some(graph) if !graph.waves.is_empty() => {
            let current_wave = calculate_current_wave_num(graph);
            let mut lines = Vec::new();

            let mut wave_spans = Vec::new();
            for (idx, wave) in graph.waves.iter().enumerate() {
                if idx > 0 {
                    wave_spans.push(Span::styled("  │  ", Style::default().fg(Theme::SEPARATOR)));
                }

                let completed = wave
                    .tasks
                    .iter()
                    .filter(|t| matches!(t.status, TaskStatus::Completed))
                    .count();
                let total = wave.tasks.len();
                let all_done = completed == total;

                // Wave indicator: ✓ for done, ▶ for current, number for future
                let (wave_icon, wave_color) = if all_done {
                    ("✓", Theme::SUCCESS)
                } else if wave.number == current_wave {
                    ("▶", Theme::ACCENT_WARM)
                } else {
                    ("○", Theme::MUTED_TEXT)
                };

                wave_spans.push(Span::styled(
                    format!("{} W{} ", wave_icon, wave.number),
                    Style::default().fg(wave_color),
                ));

                // Task status dots with spacing
                for (task_idx, task) in wave.tasks.iter().enumerate() {
                    if task_idx > 0 {
                        wave_spans.push(Span::raw("  "));
                    }

                    let (symbol, color) = task_status_symbol(&task.status);
                    wave_spans.push(Span::styled(symbol, Style::default().fg(color)));
                }

                wave_spans.push(Span::raw("  "));

                // Progress count
                wave_spans.push(Span::styled(
                    format!("{}/{}", completed, total),
                    Style::default().fg(if all_done {
                        Theme::SUCCESS
                    } else {
                        Theme::MUTED_TEXT
                    }),
                ));
            }

            lines.push(Line::from(wave_spans));
            lines
        }
        _ => vec![Line::from(Span::styled(
            "No waves — waiting for task graph",
            Style::default().fg(Theme::MUTED_TEXT),
        ))],
    }
}

/// Determine which wave is currently active (first incomplete).
fn calculate_current_wave_num(graph: &crate::model::TaskGraph) -> u32 {
    for wave in &graph.waves {
        let all_complete = wave.tasks.iter().all(|t| matches!(t.status, TaskStatus::Completed));
        if !all_complete {
            return wave.number;
        }
    }
    graph.waves.last().map(|w| w.number).unwrap_or(0)
}

/// Get symbol and color for task status.
fn task_status_symbol(status: &TaskStatus) -> (&'static str, ratatui::style::Color) {
    match status {
        TaskStatus::Pending => ("○", Theme::TASK_PENDING),
        TaskStatus::Running => ("◐", Theme::TASK_RUNNING),
        TaskStatus::Implemented => ("◑", Theme::TASK_IMPLEMENTED),
        TaskStatus::Completed => ("●", Theme::TASK_COMPLETED),
        TaskStatus::Failed { .. } => ("✗", Theme::TASK_FAILED),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Task, TaskGraph, Wave};
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    #[test]
    fn render_wave_river_does_not_panic_with_empty_state() {
        let backend = TestBackend::new(80, 3);
        let mut terminal = Terminal::new(backend).unwrap();

        let state = AppState::new();

        terminal
            .draw(|frame| {
                render_wave_river(frame, frame.area(), &state);
            })
            .unwrap();
    }

    #[test]
    fn build_wave_river_text_shows_no_waves_when_empty() {
        let state = AppState::new();
        let lines = build_wave_river_text(&state);

        assert_eq!(lines.len(), 1);
        let text: String = lines[0].spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(text.contains("No waves"));
    }

    #[test]
    fn build_wave_river_text_shows_wave_with_tasks() {
        let waves = vec![Wave::new(
            1,
            vec![
                Task::new("T1".into(), "Task 1".into(), TaskStatus::Completed),
                Task::new("T2".into(), "Task 2".into(), TaskStatus::Running),
                Task::new("T3".into(), "Task 3".into(), TaskStatus::Pending),
            ],
        )];

        let mut state = AppState::new();
        state.domain.task_graph = Some(TaskGraph::new(waves));

        let lines = build_wave_river_text(&state);
        assert!(!lines.is_empty());

        let text: String = lines[0].spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(text.contains("W1"));
        assert!(text.contains("1/3"));
    }

    #[test]
    fn build_wave_river_text_shows_multiple_waves() {
        let waves = vec![
            Wave::new(
                1,
                vec![Task::new("T1".into(), "Task 1".into(), TaskStatus::Completed)],
            ),
            Wave::new(
                2,
                vec![Task::new("T2".into(), "Task 2".into(), TaskStatus::Running)],
            ),
        ];

        let mut state = AppState::new();
        state.domain.task_graph = Some(TaskGraph::new(waves));

        let lines = build_wave_river_text(&state);
        let text: String = lines[0].spans.iter().map(|s| s.content.as_ref()).collect();

        assert!(text.contains("W1"));
        assert!(text.contains("W2"));
    }

    #[test]
    fn task_status_symbol_returns_correct_symbols() {
        assert_eq!(task_status_symbol(&TaskStatus::Pending).0, "○");
        assert_eq!(task_status_symbol(&TaskStatus::Running).0, "◐");
        assert_eq!(task_status_symbol(&TaskStatus::Implemented).0, "◑");
        assert_eq!(task_status_symbol(&TaskStatus::Completed).0, "●");
        assert_eq!(
            task_status_symbol(&TaskStatus::Failed {
                reason: "test".into(),
                retry_count: 0
            })
            .0,
            "✗"
        );
    }

    #[test]
    fn task_status_symbol_returns_correct_colors() {
        assert_eq!(
            task_status_symbol(&TaskStatus::Pending).1,
            Theme::TASK_PENDING
        );
        assert_eq!(
            task_status_symbol(&TaskStatus::Running).1,
            Theme::TASK_RUNNING
        );
        assert_eq!(
            task_status_symbol(&TaskStatus::Implemented).1,
            Theme::TASK_IMPLEMENTED
        );
        assert_eq!(
            task_status_symbol(&TaskStatus::Completed).1,
            Theme::TASK_COMPLETED
        );
        assert_eq!(
            task_status_symbol(&TaskStatus::Failed {
                reason: "test".into(),
                retry_count: 0
            })
            .1,
            Theme::TASK_FAILED
        );
    }
}
