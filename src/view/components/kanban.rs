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

            // Truncate description
            let description = if kt.task.description.len() > 15 {
                format!("{}...", &kt.task.description[..12])
            } else {
                kt.task.description.clone()
            };
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
