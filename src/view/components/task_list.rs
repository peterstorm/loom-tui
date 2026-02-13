use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem},
    Frame,
};

use crate::app::{AppState, PanelFocus};
use crate::model::{TaskStatus, Theme};

/// Render task list panel.
/// Shows scrollable list of tasks with status indicators.
/// Respects scroll offset from state.
pub fn render_task_list(frame: &mut Frame, area: Rect, state: &AppState) {
    let items = build_task_list_items(state);

    let is_focused = matches!(state.ui.focus, PanelFocus::Left);

    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(if is_focused {
                    Theme::ACTIVE_BORDER
                } else {
                    Theme::PANEL_BORDER
                }))
                .title("Tasks"),
        )
        .style(Style::default().fg(Theme::TEXT));

    frame.render_widget(list, area);
}

/// Pure function: build task list items from state.
/// Highlights the selected task and applies filter if active.
fn build_task_list_items(state: &AppState) -> Vec<ListItem<'static>> {
    match &state.domain.task_graph {
        Some(graph) if !graph.waves.is_empty() => {
            let mut items = Vec::new();
            let mut task_index: usize = 0;
            let filter = state.ui.filter.as_deref().unwrap_or("");

            for wave in &graph.waves {
                // Collect visible tasks for this wave (after filter)
                let wave_tasks: Vec<_> = wave.tasks.iter().enumerate().filter(|(_, task)| {
                    if filter.is_empty() {
                        return true;
                    }
                    let lower = filter.to_lowercase();
                    task.description.to_lowercase().contains(&lower)
                        || task.id.to_lowercase().contains(&lower)
                        || task.agent_id.as_ref().map(|a| a.to_lowercase().contains(&lower)).unwrap_or(false)
                }).collect();

                if wave_tasks.is_empty() && !filter.is_empty() {
                    // Skip entirely empty waves when filtering
                    task_index += wave.tasks.len();
                    continue;
                }

                // Wave header — compact style
                let completed = wave.tasks.iter().filter(|t| matches!(t.status, TaskStatus::Completed)).count();
                let total = wave.tasks.len();
                items.push(ListItem::new(Line::from(vec![
                    Span::styled(
                        format!("── Wave {} ", wave.number),
                        Style::default().fg(Theme::INFO).add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        format!("{}/{} ", completed, total),
                        Style::default().fg(if completed == total { Theme::SUCCESS } else { Theme::MUTED_TEXT }),
                    ),
                    Span::styled(
                        "─".repeat(20),
                        Style::default().fg(Theme::SEPARATOR),
                    ),
                ])));

                // Tasks in wave
                for (original_idx, task) in wave_tasks {
                    let flat_idx = task_index + original_idx;
                    let is_selected = state.ui.selected_task_index == Some(flat_idx);

                    let (status_symbol, status_color) = task_status_display(&task.status);
                    let bg = if is_selected { Theme::SELECTION_BG } else { Theme::BACKGROUND };

                    let mut spans = vec![
                        Span::styled("  ", Style::default().bg(bg)),
                        Span::styled(status_symbol.to_string(), Style::default().fg(status_color).bg(bg)),
                        Span::styled(" ", Style::default().bg(bg)),
                        Span::styled(task.id.clone(), Style::default().fg(Theme::INFO).bg(bg)),
                        Span::styled(" ", Style::default().bg(bg)),
                    ];

                    let description = if task.description.len() > 50 {
                        format!("{}...", &task.description[..47])
                    } else {
                        task.description.clone()
                    };
                    spans.push(Span::styled(description, Style::default().fg(Theme::TEXT).bg(bg)));

                    if let Some(ref agent_id) = task.agent_id {
                        spans.push(Span::styled(
                            format!("  {}", &agent_id[..agent_id.len().min(7)]),
                            Style::default().fg(Theme::AGENT_LABEL).bg(bg),
                        ));
                    }

                    items.push(ListItem::new(Line::from(spans)));
                }

                task_index += wave.tasks.len();

                // Spacing between waves
                items.push(ListItem::new(Line::from("")));
            }

            items
        }
        _ => vec![ListItem::new(Line::from(Span::styled(
            "No tasks — waiting for task graph",
            Style::default().fg(Theme::MUTED_TEXT),
        )))],
    }
}

/// Get display symbol and color for task status.
fn task_status_display(status: &TaskStatus) -> (&'static str, ratatui::style::Color) {
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
    fn render_task_list_does_not_panic_with_empty_state() {
        let backend = TestBackend::new(40, 20);
        let mut terminal = Terminal::new(backend).unwrap();

        let state = AppState::new();

        terminal
            .draw(|frame| {
                render_task_list(frame, frame.area(), &state);
            })
            .unwrap();
    }

    #[test]
    fn build_task_list_items_shows_no_tasks_when_empty() {
        let state = AppState::new();
        let items = build_task_list_items(&state);

        assert_eq!(items.len(), 1);
    }

    #[test]
    fn build_task_list_items_shows_wave_headers() {
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

        let items = build_task_list_items(&state);

        // 2 wave headers + 2 tasks + 2 spacing lines = 6
        assert_eq!(items.len(), 6);
    }

    #[test]
    fn build_task_list_items_includes_task_details() {
        let waves = vec![Wave::new(
            1,
            vec![Task {
                id: "T1".into(),
                description: "Test task".into(),
                agent_id: Some("a01".into()),
                status: TaskStatus::Running,
                review_status: Default::default(),
                files_modified: vec![],
                tests_passed: None,
            }],
        )];

        let mut state = AppState::new();
        state.domain.task_graph = Some(TaskGraph::new(waves));

        let items = build_task_list_items(&state);

        // 1 wave header + 1 task + 1 spacing = 3
        assert_eq!(items.len(), 3);
    }

    #[test]
    fn build_task_list_items_truncates_long_descriptions() {
        let long_desc = "a".repeat(100);
        let waves = vec![Wave::new(
            1,
            vec![Task::new("T1".into(), long_desc, TaskStatus::Pending)],
        )];

        let mut state = AppState::new();
        state.domain.task_graph = Some(TaskGraph::new(waves));

        let items = build_task_list_items(&state);

        // 1 wave header + 1 task + 1 spacing = 3
        assert_eq!(items.len(), 3);
    }

    #[test]
    fn task_status_display_returns_correct_symbols() {
        assert_eq!(task_status_display(&TaskStatus::Pending).0, "○");
        assert_eq!(task_status_display(&TaskStatus::Running).0, "◐");
        assert_eq!(task_status_display(&TaskStatus::Implemented).0, "◑");
        assert_eq!(task_status_display(&TaskStatus::Completed).0, "●");
        assert_eq!(
            task_status_display(&TaskStatus::Failed {
                reason: "test".into(),
                retry_count: 0
            })
            .0,
            "✗"
        );
    }
}
