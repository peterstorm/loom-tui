use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem},
    Frame,
};

use crate::app::{AppState, PanelFocus};
use crate::model::{TaskStatus, Theme};

/// Render kanban board view of tasks grouped by status.
/// Shows 5 columns: Pending | Running | Implemented | Completed | Failed
pub fn render_kanban_board(frame: &mut Frame, area: Rect, state: &AppState) {
    let is_focused = matches!(state.ui.focus, PanelFocus::Left);

    // Split into 5 columns
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(20), // Pending
            Constraint::Percentage(20), // Running
            Constraint::Percentage(20), // Implemented
            Constraint::Percentage(20), // Completed
            Constraint::Percentage(20), // Failed
        ])
        .split(area);

    // Group tasks by status
    let task_graph = match &state.domain.task_graph {
        Some(g) => g,
        None => {
            // No tasks - render empty columns
            render_empty_kanban(frame, &columns, is_focused);
            return;
        }
    };

    let filter = state.ui.filter.as_deref().unwrap_or("");
    let grouped = group_tasks_by_status(task_graph, filter);

    // Render each column (ensure we have 5 columns)
    if columns.len() >= 5 {
        render_status_column(
            frame,
            columns[0],
            "Pending",
            Theme::TASK_PENDING,
            &grouped.pending,
            state,
            is_focused,
        );
        render_status_column(
            frame,
            columns[1],
            "Running",
            Theme::TASK_RUNNING,
            &grouped.running,
            state,
            is_focused,
        );
        render_status_column(
            frame,
            columns[2],
            "Implemented",
            Theme::TASK_IMPLEMENTED,
            &grouped.implemented,
            state,
            is_focused,
        );
        render_status_column(
            frame,
            columns[3],
            "Completed",
            Theme::TASK_COMPLETED,
            &grouped.completed,
            state,
            is_focused,
        );
        render_status_column(
            frame,
            columns[4],
            "Failed",
            Theme::TASK_FAILED,
            &grouped.failed,
            state,
            is_focused,
        );
    }
}

/// Render empty kanban columns when no task graph available
fn render_empty_kanban(frame: &mut Frame, columns: &[Rect], is_focused: bool) {
    let border_color = if is_focused {
        Theme::ACTIVE_BORDER
    } else {
        Theme::PANEL_BORDER
    };

    let statuses = [
        ("Pending", Theme::TASK_PENDING),
        ("Running", Theme::TASK_RUNNING),
        ("Implemented", Theme::TASK_IMPLEMENTED),
        ("Completed", Theme::TASK_COMPLETED),
        ("Failed", Theme::TASK_FAILED),
    ];

    for (i, (name, color)) in statuses.iter().enumerate() {
        if i >= columns.len() {
            break;
        }
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(border_color))
            .title(Span::styled(
                format!(" {} ", name),
                Style::default().fg(*color).add_modifier(Modifier::BOLD),
            ));
        frame.render_widget(block, columns[i]);
    }
}

/// Render a single status column
fn render_status_column(
    frame: &mut Frame,
    area: Rect,
    title: &str,
    title_color: ratatui::style::Color,
    tasks: &[KanbanTask],
    state: &AppState,
    is_focused: bool,
) {
    let items: Vec<ListItem> = tasks
        .iter()
        .map(|kt| {
            let is_selected = state.ui.selected_task_index == Some(kt.flat_index);
            let bg = if is_selected {
                Theme::SELECTION_BG
            } else {
                Theme::BACKGROUND
            };

            let mut spans = vec![
                Span::styled(" ", Style::default().bg(bg)),
                Span::styled(
                    kt.task.id.to_string(),
                    Style::default().fg(Theme::INFO).bg(bg),
                ),
                Span::styled(" ", Style::default().bg(bg)),
            ];

            let description = crate::watcher::truncate_str(&kt.task.description, 12);
            spans.push(Span::styled(
                description,
                Style::default().fg(Theme::TEXT).bg(bg),
            ));

            // Show wave badge
            spans.push(Span::styled(
                format!(" W{}", kt.wave_number),
                Style::default().fg(Theme::MUTED_TEXT).bg(bg),
            ));

            ListItem::new(Line::from(spans))
        })
        .collect();

    let count = tasks.len();
    let border_color = if is_focused {
        Theme::ACTIVE_BORDER
    } else {
        Theme::PANEL_BORDER
    };

    let list = List::new(items).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(border_color))
            .title(Span::styled(
                format!(" {} ({}) ", title, count),
                Style::default()
                    .fg(title_color)
                    .add_modifier(Modifier::BOLD),
            )),
    );

    frame.render_widget(list, area);
}

/// Group tasks by status, preserving wave context
struct GroupedTasks<'a> {
    pending: Vec<KanbanTask<'a>>,
    running: Vec<KanbanTask<'a>>,
    implemented: Vec<KanbanTask<'a>>,
    completed: Vec<KanbanTask<'a>>,
    failed: Vec<KanbanTask<'a>>,
}

/// Task with kanban metadata
struct KanbanTask<'a> {
    task: &'a crate::model::Task,
    wave_number: u32,
    flat_index: usize,
}

/// Group all tasks by status, applying filter
fn group_tasks_by_status<'a>(
    task_graph: &'a crate::model::TaskGraph,
    filter: &str,
) -> GroupedTasks<'a> {
    // Pre-allocate with reasonable capacity to reduce reallocations
    let mut pending = Vec::with_capacity(16);
    let mut running = Vec::with_capacity(16);
    let mut implemented = Vec::with_capacity(16);
    let mut completed = Vec::with_capacity(16);
    let mut failed = Vec::with_capacity(8);

    let has_filter = !filter.is_empty();
    let filter_lower = if has_filter { filter.to_lowercase() } else { String::new() };

    let mut flat_index = 0;
    for wave in &task_graph.waves {
        for task in &wave.tasks {
            // Apply filter - optimize by avoiding repeated allocations
            if has_filter {
                let desc_match = task.description.to_lowercase().contains(&filter_lower);
                let id_match = task.id.as_str().to_lowercase().contains(&filter_lower);
                let agent_match = task
                    .agent_id
                    .as_ref()
                    .map(|a| a.as_str().to_lowercase().contains(&filter_lower))
                    .unwrap_or(false);

                if !desc_match && !id_match && !agent_match {
                    flat_index += 1;
                    continue;
                }
            }

            let kt = KanbanTask {
                task,
                wave_number: wave.number,
                flat_index,
            };

            match task.status {
                TaskStatus::Pending => pending.push(kt),
                TaskStatus::Running => running.push(kt),
                TaskStatus::Implemented => implemented.push(kt),
                TaskStatus::Completed => completed.push(kt),
                TaskStatus::Failed { .. } => failed.push(kt),
            }

            flat_index += 1;
        }
    }

    GroupedTasks {
        pending,
        running,
        implemented,
        completed,
        failed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Task, TaskGraph, TaskStatus, Wave};

    #[test]
    fn group_tasks_by_status_all_statuses() {
        let tasks = vec![
            Task::new("t1", "Pending task".into(), TaskStatus::Pending),
            Task::new("t2", "Running task".into(), TaskStatus::Running),
            Task::new("t3", "Implemented task".into(), TaskStatus::Implemented),
            Task::new("t4", "Completed task".into(), TaskStatus::Completed),
            Task::new(
                "t5",
                "Failed task".into(),
                TaskStatus::Failed {
                    reason: "error".into(),
                    retry_count: 0,
                },
            ),
        ];

        let wave = Wave::new(1, tasks);
        let task_graph = TaskGraph::new(vec![wave]);
        let grouped = group_tasks_by_status(&task_graph, "");

        assert_eq!(grouped.pending.len(), 1);
        assert_eq!(grouped.running.len(), 1);
        assert_eq!(grouped.implemented.len(), 1);
        assert_eq!(grouped.completed.len(), 1);
        assert_eq!(grouped.failed.len(), 1);
    }

    #[test]
    fn group_tasks_by_status_empty_task_graph() {
        let task_graph = TaskGraph::new(vec![]);
        let grouped = group_tasks_by_status(&task_graph, "");

        assert_eq!(grouped.pending.len(), 0);
        assert_eq!(grouped.running.len(), 0);
        assert_eq!(grouped.implemented.len(), 0);
        assert_eq!(grouped.completed.len(), 0);
        assert_eq!(grouped.failed.len(), 0);
    }

    #[test]
    fn group_tasks_by_status_with_filter() {
        let tasks = vec![
            Task::new("t1", "Test task one".into(), TaskStatus::Pending),
            Task::new("t2", "Another task".into(), TaskStatus::Pending),
            Task::new("t3", "Test task two".into(), TaskStatus::Running),
        ];

        let wave = Wave::new(1, tasks);
        let task_graph = TaskGraph::new(vec![wave]);
        let grouped = group_tasks_by_status(&task_graph, "test");

        // Should only include tasks with "test" in description
        assert_eq!(grouped.pending.len(), 1);
        assert_eq!(grouped.running.len(), 1);
        assert_eq!(grouped.implemented.len(), 0);
    }

    #[test]
    fn group_tasks_by_status_filter_case_insensitive() {
        let tasks = vec![
            Task::new("t1", "TEST task".into(), TaskStatus::Pending),
            Task::new("t2", "test task".into(), TaskStatus::Running),
            Task::new("t3", "TeSt task".into(), TaskStatus::Completed),
        ];

        let wave = Wave::new(1, tasks);
        let task_graph = TaskGraph::new(vec![wave]);
        let grouped = group_tasks_by_status(&task_graph, "TEST");

        assert_eq!(grouped.pending.len(), 1);
        assert_eq!(grouped.running.len(), 1);
        assert_eq!(grouped.completed.len(), 1);
    }

    #[test]
    fn group_tasks_by_status_filter_by_task_id() {
        let tasks = vec![
            Task::new("task-123", "Some description".into(), TaskStatus::Pending),
            Task::new("other-456", "Another description".into(), TaskStatus::Running),
        ];

        let wave = Wave::new(1, tasks);
        let task_graph = TaskGraph::new(vec![wave]);
        let grouped = group_tasks_by_status(&task_graph, "123");

        assert_eq!(grouped.pending.len(), 1);
        assert_eq!(grouped.running.len(), 0);
    }

    #[test]
    fn group_tasks_by_status_filter_by_agent_id() {
        let mut task1 = Task::new("t1", "Task one".into(), TaskStatus::Pending);
        task1.agent_id = Some("explore-agent".into());

        let mut task2 = Task::new("t2", "Task two".into(), TaskStatus::Running);
        task2.agent_id = Some("code-agent".into());

        let wave = Wave::new(1, vec![task1, task2]);
        let task_graph = TaskGraph::new(vec![wave]);
        let grouped = group_tasks_by_status(&task_graph, "explore");

        assert_eq!(grouped.pending.len(), 1);
        assert_eq!(grouped.running.len(), 0);
    }

    #[test]
    fn group_tasks_by_status_flat_index_correctness() {
        let wave1 = Wave::new(
            1,
            vec![
                Task::new("t1", "Task 1".into(), TaskStatus::Completed),
                Task::new("t2", "Task 2".into(), TaskStatus::Pending),
            ],
        );
        let wave2 = Wave::new(
            2,
            vec![
                Task::new("t3", "Task 3".into(), TaskStatus::Running),
                Task::new("t4", "Task 4".into(), TaskStatus::Completed),
            ],
        );

        let task_graph = TaskGraph::new(vec![wave1, wave2]);
        let grouped = group_tasks_by_status(&task_graph, "");

        // Verify flat_index tracks correctly across waves
        assert_eq!(grouped.completed[0].flat_index, 0); // t1 is first
        assert_eq!(grouped.pending[0].flat_index, 1); // t2 is second
        assert_eq!(grouped.running[0].flat_index, 2); // t3 is third
        assert_eq!(grouped.completed[1].flat_index, 3); // t4 is fourth
    }

    #[test]
    fn group_tasks_by_status_wave_number_correctness() {
        let wave1 = Wave::new(1, vec![Task::new("t1", "Task 1".into(), TaskStatus::Pending)]);
        let wave2 = Wave::new(2, vec![Task::new("t2", "Task 2".into(), TaskStatus::Running)]);

        let task_graph = TaskGraph::new(vec![wave1, wave2]);
        let grouped = group_tasks_by_status(&task_graph, "");

        assert_eq!(grouped.pending[0].wave_number, 1);
        assert_eq!(grouped.running[0].wave_number, 2);
    }
}
