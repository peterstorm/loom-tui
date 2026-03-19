use chrono::Utc;
use loom_tui::model::*;
use std::collections::BTreeMap;
use std::time::Duration;

#[test]
fn parse_active_task_graph_fixture() {
    let json = std::fs::read_to_string("tests/fixtures/active_task_graph.json")
        .expect("Failed to read fixture");

    // The actual task graph format has a different structure
    // Parse as raw JSON first to understand structure
    let value: serde_json::Value =
        serde_json::from_str(&json).expect("Failed to parse fixture as JSON");

    // Extract tasks and group by wave
    let tasks = value["tasks"].as_array().expect("tasks should be array");

    let mut wave_1_tasks = Vec::new();
    let mut wave_2_tasks = Vec::new();

    for task_val in tasks {
        let wave = task_val["wave"].as_u64().expect("wave should be number") as u32;
        let id = task_val["id"].as_str().expect("id should be string");
        let description = task_val["description"]
            .as_str()
            .expect("description should be string");

        let status = match task_val["status"].as_str().expect("status should be string") {
            "pending" => TaskStatus::Pending,
            "running" => TaskStatus::Running,
            "implemented" => TaskStatus::Implemented,
            "completed" => TaskStatus::Completed,
            "failed" => {
                let reason = task_val["reason"]
                    .as_str()
                    .unwrap_or("unknown error")
                    .to_string();
                let retry_count = task_val["retry_count"].as_u64().unwrap_or(0) as u32;
                TaskStatus::Failed {
                    reason,
                    retry_count,
                }
            }
            _ => TaskStatus::Pending,
        };

        let agent_id = task_val["agent"].as_str().map(|s| AgentId::new(s));
        let files_modified: Vec<String> = task_val["files_modified"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();

        let tests_passed = task_val["tests_passed"]
            .as_bool()
            .or_else(|| task_val["tests_passed"].as_null().map(|_| false));

        let mut task = Task::new(id, description.to_string(), status);
        task.agent_id = agent_id;
        task.files_modified = files_modified;
        task.tests_passed = tests_passed;

        if wave == 1 {
            wave_1_tasks.push(task);
        } else if wave == 2 {
            wave_2_tasks.push(task);
        }
    }

    let waves = vec![
        Wave::new(1, wave_1_tasks),
        Wave::new(2, wave_2_tasks),
    ];

    let graph = TaskGraph::new(waves);

    // Verify parsing
    assert_eq!(graph.waves.len(), 2);
    assert_eq!(graph.total_tasks(), 5);

    // Verify first task
    let t1 = &graph.waves[0].tasks[0];
    assert_eq!(t1.id.as_str(), "T1");
    assert_eq!(t1.description, "Create project scaffold");
    assert!(matches!(t1.status, TaskStatus::Completed));
    assert_eq!(t1.agent_id, Some(AgentId::new("scaffold-agent")));
    assert!(t1.files_modified.contains(&"Cargo.toml".to_string()));

    // Verify failed task
    let t5 = graph.waves[1]
        .tasks
        .iter()
        .find(|t| t.id.as_str() == "T5")
        .expect("T5 should exist");

    match &t5.status {
        TaskStatus::Failed {
            reason,
            retry_count,
        } => {
            assert_eq!(reason, "Compilation error");
            assert_eq!(*retry_count, 2);
        }
        _ => panic!("T5 should be failed"),
    }
}

#[test]
fn parse_agent_transcript_jsonl() {
    let content = std::fs::read_to_string("tests/fixtures/agent-a04.jsonl")
        .expect("Failed to read fixture");

    let mut messages = Vec::new();

    for line in content.lines() {
        let value: serde_json::Value = serde_json::from_str(line).expect("Invalid JSON line");

        let timestamp = value["timestamp"]
            .as_str()
            .expect("timestamp required")
            .parse::<chrono::DateTime<Utc>>()
            .expect("Invalid timestamp");

        let msg_type = value["type"].as_str().expect("type required");

        let message = match msg_type {
            "reasoning" => {
                let content = value["content"].as_str().expect("content required");
                AgentMessage::reasoning(timestamp, content.to_string())
            }
            "tool" => {
                let tool_name = value["tool_name"].as_str().expect("tool_name required");
                let input_summary = value["input_summary"].as_str().expect("input_summary required");
                let result_summary = value["result_summary"].as_str().map(|s| s.to_string());
                let duration = value["duration"]
                    .as_u64()
                    .map(Duration::from_millis);
                let success = value["success"].as_bool();

                let mut call = ToolCall::new(tool_name.to_string(), input_summary.to_string());
                if let Some(result) = result_summary {
                    call = call.with_result(result, success.unwrap_or(false));
                }
                if let Some(dur) = duration {
                    call = call.with_duration(dur);
                }

                AgentMessage::tool(timestamp, call)
            }
            _ => panic!("Unknown message type: {}", msg_type),
        };

        messages.push(message);
    }

    assert_eq!(messages.len(), 6);

    // Verify first reasoning message
    match &messages[0].kind {
        MessageKind::Reasoning { content } => {
            assert!(content.contains("Starting implementation"));
        }
        _ => panic!("Expected reasoning"),
    }

    // Verify tool call with duration
    if let MessageKind::Tool(call) = &messages[1].kind {
        assert_eq!(call.tool_name.as_str(), "Read");
        assert_eq!(call.duration, Some(Duration::from_millis(150)));
        assert_eq!(call.success, Some(true));
    } else {
        panic!("Expected tool call");
    }
}

/// parse_hook_events_jsonl removed — HookEvent model deleted (SC-001).
/// Replaced by parse_transcript_events tests in watcher_tests.rs.

#[test]
fn parse_session_archive_fixture() {
    let json = std::fs::read_to_string("tests/fixtures/session_archive.json")
        .expect("Failed to read fixture");

    let archive: SessionArchive =
        serde_json::from_str(&json).expect("Failed to parse session archive");

    // Verify metadata
    assert_eq!(archive.meta.id.as_str(), "s20260211-095900");
    assert_eq!(archive.meta.status, SessionStatus::Completed);
    assert_eq!(archive.meta.agent_count, 2);
    assert_eq!(archive.meta.task_count, 3);
    assert_eq!(archive.meta.event_count, 12);
    assert_eq!(archive.meta.git_branch, Some("main".to_string()));
    assert_eq!(archive.meta.wave_count, Some(2));
    assert_eq!(
        archive.meta.duration,
        Some(Duration::from_millis(1265000))
    );

    // Verify task graph
    let graph = archive.task_graph.expect("Should have task graph");
    assert_eq!(graph.waves.len(), 2);
    assert_eq!(graph.total_tasks(), 3);
    assert_eq!(graph.completed_tasks(), 2);

    // Verify events
    assert_eq!(archive.events.len(), 3);

    // Verify agents
    assert_eq!(archive.agents.len(), 2);
    let agent_a01 = archive.agents.get(&AgentId::new("a01")).expect("Agent a01 should exist");
    assert_eq!(agent_a01.task_id, Some(TaskId::new("T1")));
    assert!(agent_a01.finished_at.is_some());
}

#[test]
fn task_graph_round_trip_serialization() {
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
            vec![Task::new(
                "T3",
                "Task 3".to_string(),
                TaskStatus::Failed {
                    reason: "Test error".to_string(),
                    retry_count: 1,
                },
            )],
        ),
    ];

    let original = TaskGraph::new(waves);
    let json = serde_json::to_string(&original).expect("Serialization failed");
    let restored: TaskGraph = serde_json::from_str(&json).expect("Deserialization failed");

    assert_eq!(original, restored);
}

#[test]
fn agent_round_trip_serialization() {
    let now = Utc::now();
    let later = now + chrono::Duration::seconds(60);

    let original = Agent::new("a01", now)
        .with_task("T1")
        .add_message(AgentMessage::reasoning(
            now,
            "Starting work".to_string(),
        ))
        .add_message(AgentMessage::tool(
            now,
            ToolCall::new("Read", "file.rs".to_string())
                .with_duration(Duration::from_millis(150)),
        ))
        .finish(later);

    let json = serde_json::to_string(&original).expect("Serialization failed");
    let restored: Agent = serde_json::from_str(&json).expect("Deserialization failed");

    assert_eq!(original, restored);
}

/// hook_event_round_trip_serialization removed — HookEvent model deleted (SC-001).

#[test]
fn session_archive_round_trip_serialization() {
    let meta = SessionMeta::new("s1", Utc::now(), "/proj".to_string())
        .with_status(SessionStatus::Completed)
        .with_duration(Duration::from_secs(300));

    let graph = TaskGraph::new(vec![Wave::new(
        1,
        vec![Task::new("T1", "Test".to_string(), TaskStatus::Completed)],
    )]);

    let events = vec![
        TranscriptEvent::new(Utc::now(), TranscriptEventKind::UserMessage),
        TranscriptEvent::new(
            Utc::now(),
            TranscriptEventKind::AssistantMessage {
                content: "Hello".to_string(),
            },
        ),
    ];

    let mut agents = BTreeMap::new();
    agents.insert(AgentId::new("a01"), Agent::new("a01", Utc::now()));

    let original = SessionArchive::new(meta)
        .with_task_graph(graph)
        .with_events(events)
        .with_agents(agents);

    let json = serde_json::to_string(&original).expect("Serialization failed");
    let restored: SessionArchive = serde_json::from_str(&json).expect("Deserialization failed");

    assert_eq!(original, restored);
}

#[test]
fn malformed_json_returns_error() {
    let bad_json = r#"{"invalid": json"#;

    let result: Result<TaskGraph, _> = serde_json::from_str(bad_json);
    assert!(result.is_err(), "Should fail on malformed JSON");

    let result: Result<Agent, _> = serde_json::from_str(bad_json);
    assert!(result.is_err(), "Should fail on malformed JSON");

    let result: Result<SessionArchive, _> = serde_json::from_str(bad_json);
    assert!(result.is_err(), "Should fail on malformed JSON");
}

#[test]
fn missing_optional_fields_use_defaults() {
    // With externally tagged enums, unit variants serialize as strings
    let json = r#"{
        "id": "T1",
        "description": "Test task",
        "status": "pending"
    }"#;

    let task: Task = serde_json::from_str(json).expect("Should parse with defaults");

    assert_eq!(task.id.as_str(), "T1");
    assert_eq!(task.agent_id, None);
    assert_eq!(task.review_status, ReviewStatus::Pending);
    assert_eq!(task.files_modified.len(), 0);
    assert_eq!(task.tests_passed, None);
}

#[test]
fn theme_task_status_colors() {
    use loom_tui::model::theme::Theme;

    // Verify all task statuses have colors
    assert_eq!(
        Theme::task_status_color(&TaskStatus::Pending),
        Theme::TASK_PENDING
    );
    assert_eq!(
        Theme::task_status_color(&TaskStatus::Running),
        Theme::TASK_RUNNING
    );
    assert_eq!(
        Theme::task_status_color(&TaskStatus::Implemented),
        Theme::TASK_IMPLEMENTED
    );
    assert_eq!(
        Theme::task_status_color(&TaskStatus::Completed),
        Theme::TASK_COMPLETED
    );
    assert_eq!(
        Theme::task_status_color(&TaskStatus::Failed {
            reason: "test".into(),
            retry_count: 0
        }),
        Theme::TASK_FAILED
    );
}

#[test]
fn theme_tool_colors() {
    use loom_tui::model::theme::Theme;

    // Verify common tools have distinct colors
    assert_eq!(Theme::tool_color("Bash"), Theme::TOOL_BASH);
    assert_eq!(Theme::tool_color("Read"), Theme::TOOL_READ);
    assert_eq!(Theme::tool_color("Write"), Theme::TOOL_WRITE);
    assert_eq!(Theme::tool_color("Edit"), Theme::TOOL_EDIT);
    assert_eq!(Theme::tool_color("Grep"), Theme::TOOL_GREP);
    assert_eq!(Theme::tool_color("Glob"), Theme::TOOL_GLOB);
    assert_eq!(Theme::tool_color("TaskCreate"), Theme::TOOL_TASK);
    assert_eq!(Theme::tool_color("WebFetch"), Theme::TOOL_WEBFETCH);
}

#[test]
fn task_graph_empty_constructor() {
    let graph = TaskGraph::empty();
    assert_eq!(graph.waves.len(), 0);
    assert_eq!(graph.total_tasks(), 0);
    assert_eq!(graph.completed_tasks(), 0);
}

/// HookEvent-specific deserialization tests removed (SC-001).
/// TranscriptEvent deserialization tests in model/transcript_event.rs unit tests.

#[test]
fn agent_serializes_with_model_field() {
    let agent = Agent::new("a01", Utc::now()).with_model("sonnet".to_string());
    let json = serde_json::to_string(&agent).unwrap();
    assert!(json.contains("\"model\":\"sonnet\""));

    let restored: Agent = serde_json::from_str(&json).unwrap();
    assert_eq!(restored.model.as_deref(), Some("sonnet"));
}

#[test]
fn agent_deserializes_without_model_field() {
    let json = r#"{
        "id": "a01",
        "started_at": "2026-02-17T10:00:00Z"
    }"#;
    let agent: Agent = serde_json::from_str(json).expect("Should parse without model");
    assert!(agent.model.is_none());
}

#[test]
fn review_status_blocked_with_issues() {
    let blocked = ReviewStatus::Blocked {
        critical: vec!["Missing tests".into()],
        advisory: vec!["Consider refactoring".into()],
    };

    let json = serde_json::to_string(&blocked).expect("Serialization failed");
    let restored: ReviewStatus = serde_json::from_str(&json).expect("Deserialization failed");

    match restored {
        ReviewStatus::Blocked { critical, advisory } => {
            assert_eq!(critical.len(), 1);
            assert_eq!(advisory.len(), 1);
        }
        _ => panic!("Expected Blocked status"),
    }
}
