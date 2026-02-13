use loom_tui::app::{AppState, HookStatus, PanelFocus, ViewState};
use loom_tui::model::{HookEvent, HookEventKind, Task, TaskGraph, TaskStatus, Wave};
use ratatui::backend::TestBackend;
use ratatui::Terminal;

use chrono::Utc;
use std::collections::VecDeque;

// Dashboard view tests

#[test]
fn dashboard_renders_without_panic_empty_state() {
    let backend = TestBackend::new(80, 24);
    let mut terminal = Terminal::new(backend).unwrap();

    let state = AppState::new();

    terminal
        .draw(|frame| {
            loom_tui::view::render_dashboard(frame, &state, frame.area());
        })
        .unwrap();
}

#[test]
fn dashboard_renders_without_panic_with_tasks() {
    let backend = TestBackend::new(80, 24);
    let mut terminal = Terminal::new(backend).unwrap();

    let waves = vec![
        Wave::new(
            1,
            vec![
                Task::new("T1", "Task 1".to_string(), TaskStatus::Completed),
                Task::new("T2", "Task 2".to_string(), TaskStatus::Running),
            ],
        ),
        Wave::new(
            2,
            vec![Task::new("T3", "Task 3".to_string(), TaskStatus::Pending)],
        ),
    ];

    let mut state = AppState::new();
    state.domain.task_graph = Some(TaskGraph::new(waves));

    terminal
        .draw(|frame| {
            loom_tui::view::render_dashboard(frame, &state, frame.area());
        })
        .unwrap();
}

#[test]
fn dashboard_renders_without_panic_with_events() {
    let backend = TestBackend::new(80, 24);
    let mut terminal = Terminal::new(backend).unwrap();

    let mut state = AppState::new();

    let events = vec![
        HookEvent::new(Utc::now(), HookEventKind::session_start()),
        HookEvent::new(
            Utc::now(),
            HookEventKind::pre_tool_use("Read", "file.rs".to_string()),
        ),
        HookEvent::new(
            Utc::now(),
            HookEventKind::post_tool_use("Read", "success".to_string(), Some(150)),
        ),
    ];

    state.domain.events = VecDeque::from(events);

    terminal
        .draw(|frame| {
            loom_tui::view::render_dashboard(frame, &state, frame.area());
        })
        .unwrap();
}

#[test]
fn dashboard_renders_with_hook_missing_banner() {
    let backend = TestBackend::new(80, 24);
    let mut terminal = Terminal::new(backend).unwrap();

    let state = AppState::with_hook_status(HookStatus::Missing);

    terminal
        .draw(|frame| {
            loom_tui::view::render_dashboard(frame, &state, frame.area());
        })
        .unwrap();
}

#[test]
fn dashboard_renders_with_hook_failed_banner() {
    let backend = TestBackend::new(80, 24);
    let mut terminal = Terminal::new(backend).unwrap();

    let state = AppState::with_hook_status(HookStatus::InstallFailed("test error".into()));

    terminal
        .draw(|frame| {
            loom_tui::view::render_dashboard(frame, &state, frame.area());
        })
        .unwrap();
}

#[test]
fn dashboard_renders_with_small_terminal() {
    let backend = TestBackend::new(40, 12);
    let mut terminal = Terminal::new(backend).unwrap();

    let state = AppState::new();

    terminal
        .draw(|frame| {
            loom_tui::view::render_dashboard(frame, &state, frame.area());
        })
        .unwrap();
}

#[test]
fn dashboard_renders_with_minimal_terminal() {
    let backend = TestBackend::new(20, 6);
    let mut terminal = Terminal::new(backend).unwrap();

    let state = AppState::new();

    terminal
        .draw(|frame| {
            loom_tui::view::render_dashboard(frame, &state, frame.area());
        })
        .unwrap();
}

// Component-specific tests

#[test]
fn header_renders_with_no_tasks() {
    let backend = TestBackend::new(80, 1);
    let mut terminal = Terminal::new(backend).unwrap();

    let state = AppState::new();

    let result = terminal
        .draw(|frame| {
            loom_tui::view::components::render_header(frame, frame.area(), &state);
        })
        .unwrap();

    let buffer = result.buffer;
    let content: String = buffer.content().iter().map(|c| c.symbol()).collect();

    assert!(content.contains("loom"));
    assert!(content.contains("No tasks"));
}

#[test]
fn header_renders_with_tasks() {
    let backend = TestBackend::new(80, 1);
    let mut terminal = Terminal::new(backend).unwrap();

    let waves = vec![Wave::new(
        1,
        vec![
            Task::new("T1", "Task 1".to_string(), TaskStatus::Completed),
            Task::new("T2", "Task 2".to_string(), TaskStatus::Running),
        ],
    )];

    let mut state = AppState::new();
    state.domain.task_graph = Some(TaskGraph::new(waves));

    let result = terminal
        .draw(|frame| {
            loom_tui::view::components::render_header(frame, frame.area(), &state);
        })
        .unwrap();

    let buffer = result.buffer;
    let content: String = buffer.content().iter().map(|c| c.symbol()).collect();

    assert!(content.contains("loom"));
    assert!(content.contains("W1"));
    assert!(content.contains("1/2"));
}

#[test]
fn footer_renders_for_dashboard() {
    let backend = TestBackend::new(80, 1);
    let mut terminal = Terminal::new(backend).unwrap();

    let state = AppState::new();

    let result = terminal
        .draw(|frame| {
            loom_tui::view::components::render_footer(frame, frame.area(), &state);
        })
        .unwrap();

    let buffer = result.buffer;
    let content: String = buffer.content().iter().map(|c| c.symbol()).collect();

    assert!(content.contains("q:quit"));
}

#[test]
fn footer_renders_for_agent_detail() {
    let backend = TestBackend::new(80, 1);
    let mut terminal = Terminal::new(backend).unwrap();

    let state = AppState::with_view(ViewState::AgentDetail);

    terminal
        .draw(|frame| {
            loom_tui::view::components::render_footer(frame, frame.area(), &state);
        })
        .unwrap();
}

#[test]
fn wave_river_renders_empty_state() {
    let backend = TestBackend::new(80, 3);
    let mut terminal = Terminal::new(backend).unwrap();

    let state = AppState::new();

    let result = terminal
        .draw(|frame| {
            loom_tui::view::components::render_wave_river(frame, frame.area(), &state);
        })
        .unwrap();

    let buffer = result.buffer;
    let content: String = buffer.content().iter().map(|c| c.symbol()).collect();

    assert!(content.contains("No waves"));
}

#[test]
fn wave_river_renders_multiple_waves() {
    let backend = TestBackend::new(80, 3);
    let mut terminal = Terminal::new(backend).unwrap();

    let waves = vec![
        Wave::new(
            1,
            vec![
                Task::new("T1", "Task 1".to_string(), TaskStatus::Completed),
                Task::new("T2", "Task 2".to_string(), TaskStatus::Running),
            ],
        ),
        Wave::new(
            2,
            vec![Task::new("T3", "Task 3".to_string(), TaskStatus::Pending)],
        ),
    ];

    let mut state = AppState::new();
    state.domain.task_graph = Some(TaskGraph::new(waves));

    let result = terminal
        .draw(|frame| {
            loom_tui::view::components::render_wave_river(frame, frame.area(), &state);
        })
        .unwrap();

    let buffer = result.buffer;
    let content: String = buffer.content().iter().map(|c| c.symbol()).collect();

    assert!(content.contains("W1"));
    assert!(content.contains("W2"));
}

#[test]
fn task_list_renders_empty_state() {
    let backend = TestBackend::new(40, 20);
    let mut terminal = Terminal::new(backend).unwrap();

    let state = AppState::new();

    terminal
        .draw(|frame| {
            loom_tui::view::components::render_task_list(frame, frame.area(), &state);
        })
        .unwrap();
}

#[test]
fn task_list_renders_with_focus() {
    let backend = TestBackend::new(40, 20);
    let mut terminal = Terminal::new(backend).unwrap();

    let waves = vec![Wave::new(
        1,
        vec![Task::new("T1", "Task 1".to_string(), TaskStatus::Running)],
    )];

    let mut state = AppState::new();
    state.domain.task_graph = Some(TaskGraph::new(waves));
    state.ui.focus = PanelFocus::Left;

    terminal
        .draw(|frame| {
            loom_tui::view::components::render_task_list(frame, frame.area(), &state);
        })
        .unwrap();
}

#[test]
fn event_stream_renders_empty_state() {
    let backend = TestBackend::new(40, 20);
    let mut terminal = Terminal::new(backend).unwrap();

    let state = AppState::new();

    terminal
        .draw(|frame| {
            loom_tui::view::components::render_event_stream(frame, frame.area(), &state);
        })
        .unwrap();
}

#[test]
fn event_stream_renders_with_events() {
    let backend = TestBackend::new(40, 20);
    let mut terminal = Terminal::new(backend).unwrap();

    let mut state = AppState::new();

    let events = vec![
        HookEvent::new(Utc::now(), HookEventKind::session_start()),
        HookEvent::new(
            Utc::now(),
            HookEventKind::pre_tool_use("Read", "file.rs".to_string()),
        ),
    ];

    state.domain.events = VecDeque::from(events);

    terminal
        .draw(|frame| {
            loom_tui::view::components::render_event_stream(frame, frame.area(), &state);
        })
        .unwrap();
}

#[test]
fn event_stream_renders_with_auto_scroll() {
    let backend = TestBackend::new(40, 20);
    let mut terminal = Terminal::new(backend).unwrap();

    let mut state = AppState::new();
    state.ui.auto_scroll = true;

    terminal
        .draw(|frame| {
            loom_tui::view::components::render_event_stream(frame, frame.area(), &state);
        })
        .unwrap();
}

#[test]
fn event_stream_renders_with_focus() {
    let backend = TestBackend::new(40, 20);
    let mut terminal = Terminal::new(backend).unwrap();

    let mut state = AppState::new();
    state.ui.focus = PanelFocus::Right;

    terminal
        .draw(|frame| {
            loom_tui::view::components::render_event_stream(frame, frame.area(), &state);
        })
        .unwrap();
}

#[test]
fn banner_renders_for_missing_hooks() {
    let backend = TestBackend::new(80, 2);
    let mut terminal = Terminal::new(backend).unwrap();

    let state = AppState::with_hook_status(HookStatus::Missing);

    terminal
        .draw(|frame| {
            loom_tui::view::components::render_banner(frame, frame.area(), &state);
        })
        .unwrap();
}

#[test]
fn banner_renders_for_failed_install() {
    let backend = TestBackend::new(80, 2);
    let mut terminal = Terminal::new(backend).unwrap();

    let state = AppState::with_hook_status(HookStatus::InstallFailed("test error".into()));

    terminal
        .draw(|frame| {
            loom_tui::view::components::render_banner(frame, frame.area(), &state);
        })
        .unwrap();
}

// Layout calculation tests

#[test]
fn dashboard_layout_with_all_status_types() {
    let backend = TestBackend::new(80, 24);
    let mut terminal = Terminal::new(backend).unwrap();

    let waves = vec![Wave::new(
        1,
        vec![
            Task::new("T1", "Pending".to_string(), TaskStatus::Pending),
            Task::new("T2", "Running".to_string(), TaskStatus::Running),
            Task::new("T3", "Implemented".to_string(), TaskStatus::Implemented),
            Task::new("T4", "Completed".to_string(), TaskStatus::Completed),
            Task {
                id: "T5".into(),
                description: "Failed".into(),
                agent_id: None,
                status: TaskStatus::Failed {
                    reason: "test error".into(),
                    retry_count: 1,
                },
                review_status: Default::default(),
                files_modified: vec![],
                tests_passed: None,
            },
        ],
    )];

    let mut state = AppState::new();
    state.domain.task_graph = Some(TaskGraph::new(waves));

    terminal
        .draw(|frame| {
            loom_tui::view::render_dashboard(frame, &state, frame.area());
        })
        .unwrap();
}

#[test]
fn dashboard_layout_with_long_task_descriptions() {
    let backend = TestBackend::new(80, 24);
    let mut terminal = Terminal::new(backend).unwrap();

    let long_desc = "This is a very long task description that should be truncated by the rendering logic to avoid layout issues. ".repeat(5);

    let waves = vec![Wave::new(
        1,
        vec![Task::new("T1", long_desc, TaskStatus::Running)],
    )];

    let mut state = AppState::new();
    state.domain.task_graph = Some(TaskGraph::new(waves));

    terminal
        .draw(|frame| {
            loom_tui::view::render_dashboard(frame, &state, frame.area());
        })
        .unwrap();
}

#[test]
fn dashboard_layout_with_many_events() {
    let backend = TestBackend::new(80, 24);
    let mut terminal = Terminal::new(backend).unwrap();

    let mut state = AppState::new();

    for i in 0..100 {
        state.domain.events.push_back(HookEvent::new(
            Utc::now(),
            HookEventKind::notification(format!("Event {}", i)),
        ));
    }

    terminal
        .draw(|frame| {
            loom_tui::view::render_dashboard(frame, &state, frame.area());
        })
        .unwrap();
}

#[test]
fn dashboard_layout_with_scroll_offsets() {
    let backend = TestBackend::new(80, 24);
    let mut terminal = Terminal::new(backend).unwrap();

    let waves = vec![Wave::new(
        1,
        vec![
            Task::new("T1", "Task 1".to_string(), TaskStatus::Completed),
            Task::new("T2", "Task 2".to_string(), TaskStatus::Running),
        ],
    )];

    let mut state = AppState::new();
    state.domain.task_graph = Some(TaskGraph::new(waves));
    state.ui.scroll_offsets.task_list = 5;
    state.ui.scroll_offsets.event_stream = 10;

    terminal
        .draw(|frame| {
            loom_tui::view::render_dashboard(frame, &state, frame.area());
        })
        .unwrap();
}

// View dispatch tests

#[test]
fn view_dispatch_sessions() {
    let backend = TestBackend::new(120, 40);
    let mut terminal = Terminal::new(backend).unwrap();

    let state = AppState::with_view(ViewState::Sessions);

    terminal
        .draw(|frame| loom_tui::view::render(&state, frame))
        .unwrap();
}

#[test]
fn view_dispatch_dashboard() {
    let backend = TestBackend::new(120, 40);
    let mut terminal = Terminal::new(backend).unwrap();

    let state = AppState::with_view(ViewState::Dashboard);

    terminal
        .draw(|frame| loom_tui::view::render(&state, frame))
        .unwrap();
}

#[test]
fn view_dispatch_agent_detail() {
    let backend = TestBackend::new(120, 40);
    let mut terminal = Terminal::new(backend).unwrap();

    let state = AppState::with_view(ViewState::AgentDetail);

    terminal
        .draw(|frame| loom_tui::view::render(&state, frame))
        .unwrap();
}

#[test]
fn view_render_with_filter_overlay() {
    let backend = TestBackend::new(120, 40);
    let mut terminal = Terminal::new(backend).unwrap();

    let mut state = AppState::new();
    state.ui.filter = Some("test".to_string());

    terminal
        .draw(|frame| loom_tui::view::render(&state, frame))
        .unwrap();

    let buffer = terminal.backend().buffer();

    let buffer_str: String = (0..buffer.area.height)
        .map(|y| {
            (0..buffer.area.width)
                .map(|x| buffer.cell((x, y)).unwrap().symbol())
                .collect::<String>()
        })
        .collect::<Vec<String>>()
        .join("\n");

    assert!(buffer_str.contains("test"), "Filter text should be visible");
}

#[test]
fn view_render_with_help_overlay() {
    let backend = TestBackend::new(120, 40);
    let mut terminal = Terminal::new(backend).unwrap();

    let mut state = AppState::new();
    state.ui.show_help = true;

    terminal
        .draw(|frame| loom_tui::view::render(&state, frame))
        .unwrap();

    let buffer = terminal.backend().buffer();

    let buffer_str: String = (0..buffer.area.height)
        .map(|y| {
            (0..buffer.area.width)
                .map(|x| buffer.cell((x, y)).unwrap().symbol())
                .collect::<String>()
        })
        .collect::<Vec<String>>()
        .join("\n");

    assert!(
        buffer_str.contains("NAVIGATION"),
        "Help overlay should be visible"
    );
}

#[test]
fn view_render_with_both_overlays() {
    let backend = TestBackend::new(120, 40);
    let mut terminal = Terminal::new(backend).unwrap();

    let mut state = AppState::new();
    state.ui.filter = Some("query".to_string());
    state.ui.show_help = true;

    terminal
        .draw(|frame| loom_tui::view::render(&state, frame))
        .unwrap();
}
