use loom_tui::model::{AgentId, HookEventKind, MessageKind, SessionId, TaskStatus};
use loom_tui::watcher::{
    extract_active_agent_ids, parse_hook_events, parse_task_graph, parse_transcript, TailState,
};
use std::fs;
use std::io::Write;
use std::path::Path;
use tempfile::TempDir;

// ============================================================================
// Pure Parser Tests (Functional Core)
// ============================================================================

#[test]
fn test_parse_task_graph_comprehensive() {
    let json = r#"{
        "waves": [
            {
                "number": 1,
                "tasks": [
                    {
                        "id": "T1",
                        "description": "First task",
                        "status": "completed",
                        "agent_id": "a01",
                        "files_modified": ["file1.rs", "file2.rs"],
                        "tests_passed": true
                    },
                    {
                        "id": "T2",
                        "description": "Second task",
                        "status": "running",
                        "agent_id": "a02"
                    }
                ]
            },
            {
                "number": 2,
                "tasks": [
                    {
                        "id": "T3",
                        "description": "Third task",
                        "status": "pending"
                    }
                ]
            }
        ],
        "total_tasks": 3,
        "completed_tasks": 1
    }"#;

    let result = parse_task_graph(json);
    assert!(result.is_ok());

    let graph = result.unwrap();
    assert_eq!(graph.waves.len(), 2);
    assert_eq!(graph.total_tasks(), 3);
    assert_eq!(graph.completed_tasks(), 1);

    // Verify wave 1
    assert_eq!(graph.waves[0].number, 1);
    assert_eq!(graph.waves[0].tasks.len(), 2);
    assert_eq!(graph.waves[0].tasks[0].id.as_str(), "T1");
    assert_eq!(
        graph.waves[0].tasks[0].status,
        TaskStatus::Completed
    );
    assert_eq!(graph.waves[0].tasks[0].agent_id, Some(AgentId::new("a01")));
    assert_eq!(graph.waves[0].tasks[0].files_modified.len(), 2);
    assert_eq!(graph.waves[0].tasks[0].tests_passed, Some(true));

    // Verify wave 2
    assert_eq!(graph.waves[1].number, 2);
    assert_eq!(graph.waves[1].tasks.len(), 1);
    assert_eq!(
        graph.waves[1].tasks[0].status,
        TaskStatus::Pending
    );
}

#[test]
fn test_parse_task_graph_with_failed_task() {
    let json = r#"{
        "waves": [{
            "number": 1,
            "tasks": [{
                "id": "T1",
                "description": "Failed task",
                "status": {
                    "failed": {
                        "reason": "Compilation error",
                        "retry_count": 2
                    }
                }
            }]
        }],
        "total_tasks": 1,
        "completed_tasks": 0
    }"#;

    let result = parse_task_graph(json);
    assert!(result.is_ok());

    let graph = result.unwrap();
    match &graph.waves[0].tasks[0].status {
        TaskStatus::Failed { reason, retry_count } => {
            assert_eq!(reason, "Compilation error");
            assert_eq!(*retry_count, 2);
        }
        _ => panic!("Expected Failed status"),
    }
}

#[test]
fn test_parse_task_graph_empty() {
    let json = r#"{
        "waves": [],
        "total_tasks": 0,
        "completed_tasks": 0
    }"#;

    let result = parse_task_graph(json);
    assert!(result.is_ok());

    let graph = result.unwrap();
    assert_eq!(graph.waves.len(), 0);
    assert_eq!(graph.total_tasks(), 0);
}

#[test]
fn test_parse_task_graph_malformed_json() {
    let invalid = "{ not valid json }";
    let result = parse_task_graph(invalid);
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("JSON"));
}

#[test]
fn test_parse_transcript_reasoning_and_tools() {
    let jsonl = r#"{"timestamp":"2026-02-11T10:00:00Z","type":"reasoning","content":"Starting implementation"}
{"timestamp":"2026-02-11T10:01:00Z","type":"tool","tool_name":"Read","input_summary":"src/main.rs"}
{"timestamp":"2026-02-11T10:02:00Z","type":"tool","tool_name":"Bash","input_summary":"cargo test","duration":1500,"success":true,"result_summary":"All tests passed"}
{"timestamp":"2026-02-11T10:03:00Z","type":"reasoning","content":"Tests completed successfully"}"#;

    let result = parse_transcript(jsonl);
    assert!(result.is_ok());

    let messages = result.unwrap();
    assert_eq!(messages.len(), 4);

    // Verify reasoning message
    match &messages[0].kind {
        MessageKind::Reasoning { content } => {
            assert_eq!(content, "Starting implementation");
        }
        _ => panic!("Expected Reasoning"),
    }

    // Verify tool message without result
    match &messages[1].kind {
        MessageKind::Tool(call) => {
            assert_eq!(call.tool_name.as_str(), "Read");
            assert_eq!(call.input_summary, "src/main.rs");
            assert!(call.result_summary.is_none());
            assert!(call.duration.is_none());
        }
        _ => panic!("Expected Tool"),
    }

    // Verify tool message with result and duration
    match &messages[2].kind {
        MessageKind::Tool(call) => {
            assert_eq!(call.tool_name.as_str(), "Bash");
            assert_eq!(call.duration, Some(std::time::Duration::from_millis(1500)));
            assert_eq!(call.success, Some(true));
            assert_eq!(call.result_summary, Some("All tests passed".to_string()));
        }
        _ => panic!("Expected Tool"),
    }
}

#[test]
fn test_parse_transcript_handles_empty_lines() {
    let jsonl = r#"{"timestamp":"2026-02-11T10:00:00Z","type":"reasoning","content":"Line 1"}

{"timestamp":"2026-02-11T10:01:00Z","type":"reasoning","content":"Line 2"}

{"timestamp":"2026-02-11T10:02:00Z","type":"reasoning","content":"Line 3"}
"#;

    let result = parse_transcript(jsonl);
    assert!(result.is_ok());
    assert_eq!(result.unwrap().len(), 3);
}

#[test]
fn test_parse_transcript_error_on_invalid_line() {
    let jsonl = r#"{"timestamp":"2026-02-11T10:00:00Z","type":"reasoning","content":"Valid"}
invalid json line
{"timestamp":"2026-02-11T10:02:00Z","type":"reasoning","content":"Also valid"}"#;

    let result = parse_transcript(jsonl);
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("Line 2"));
}

#[test]
fn test_parse_hook_events_all_kinds() {
    let jsonl = r#"{"timestamp":"2026-02-11T10:00:00Z","event":"session_start"}
{"timestamp":"2026-02-11T10:01:00Z","event":"subagent_start","task_description":"Implement feature X"}
{"timestamp":"2026-02-11T10:02:00Z","event":"pre_tool_use","tool_name":"Read","input_summary":"file.rs"}
{"timestamp":"2026-02-11T10:03:00Z","event":"post_tool_use","tool_name":"Read","result_summary":"File read","duration_ms":50}
{"timestamp":"2026-02-11T10:04:00Z","event":"notification","message":"Progress update"}
{"timestamp":"2026-02-11T10:05:00Z","event":"user_prompt_submit"}
{"timestamp":"2026-02-11T10:06:00Z","event":"subagent_stop"}
{"timestamp":"2026-02-11T10:07:00Z","event":"stop","reason":"Task completed"}
{"timestamp":"2026-02-11T10:08:00Z","event":"session_end"}"#;

    let result = parse_hook_events(jsonl);
    assert!(result.is_ok());

    let events = result.unwrap();
    assert_eq!(events.len(), 9);

    // Verify event kinds
    assert!(matches!(events[0].kind, HookEventKind::SessionStart));

    match &events[1].kind {
        HookEventKind::SubagentStart { task_description, .. } => {
            assert_eq!(task_description, &Some("Implement feature X".to_string()));
        }
        _ => panic!("Expected SubagentStart"),
    }

    match &events[2].kind {
        HookEventKind::PreToolUse { tool_name, input_summary } => {
            assert_eq!(tool_name.as_str(), "Read");
            assert_eq!(input_summary, "file.rs");
        }
        _ => panic!("Expected PreToolUse"),
    }

    match &events[3].kind {
        HookEventKind::PostToolUse { tool_name, result_summary, duration_ms } => {
            assert_eq!(tool_name.as_str(), "Read");
            assert_eq!(result_summary, "File read");
            assert_eq!(duration_ms, &Some(50));
        }
        _ => panic!("Expected PostToolUse"),
    }

    match &events[4].kind {
        HookEventKind::Notification { message } => {
            assert_eq!(message, "Progress update");
        }
        _ => panic!("Expected Notification"),
    }

    assert!(matches!(events[5].kind, HookEventKind::UserPromptSubmit));
    assert!(matches!(events[6].kind, HookEventKind::SubagentStop));

    match &events[7].kind {
        HookEventKind::Stop { reason } => {
            assert_eq!(reason, &Some("Task completed".to_string()));
        }
        _ => panic!("Expected Stop"),
    }

    assert!(matches!(events[8].kind, HookEventKind::SessionEnd));
}

#[test]
fn test_parse_hook_events_with_metadata() {
    let jsonl = r#"{"timestamp":"2026-02-11T10:00:00Z","event":"session_start","session_id":"s123","agent_id":"a01"}"#;

    let result = parse_hook_events(jsonl);
    assert!(result.is_ok());

    let events = result.unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].session_id, Some(SessionId::new("s123")));
    assert_eq!(events[0].agent_id, Some(AgentId::new("a01")));
}

#[test]
fn test_parse_hook_events_empty() {
    let result = parse_hook_events("");
    assert!(result.is_ok());
    assert!(result.unwrap().is_empty());
}

#[test]
fn test_parse_hook_events_skips_invalid_lines() {
    let jsonl = r#"{"timestamp":"2026-02-11T10:00:00Z","event":"session_start"}
not valid json"#;

    let result = parse_hook_events(jsonl).unwrap();
    // Invalid line skipped, valid event preserved
    assert_eq!(result.len(), 1);
}

#[test]
fn test_extract_active_agent_ids_mixed_files() {
    let paths = vec![
        Path::new("/tmp/claude-subagents/a01.active"),
        Path::new("/tmp/claude-subagents/a02.active"),
        Path::new("/tmp/claude-subagents/a03.txt"),
        Path::new("/tmp/claude-subagents/other.log"),
        Path::new("/tmp/claude-subagents/a04.active"),
    ];

    let agent_ids = extract_active_agent_ids(&paths);
    assert_eq!(agent_ids.len(), 3);
    assert!(agent_ids.contains(&"a01".to_string()));
    assert!(agent_ids.contains(&"a02".to_string()));
    assert!(agent_ids.contains(&"a04".to_string()));
}

#[test]
fn test_extract_active_agent_ids_empty_list() {
    let agent_ids = extract_active_agent_ids(&[]);
    assert!(agent_ids.is_empty());
}

// ============================================================================
// TailState Tests (Incremental Reading)
// ============================================================================

#[test]
fn test_tail_state_tracks_multiple_files() {
    let temp = TempDir::new().unwrap();
    let file1 = temp.path().join("events1.jsonl");
    let file2 = temp.path().join("events2.jsonl");

    fs::write(&file1, "Line 1\nLine 2\n").unwrap();
    fs::write(&file2, "Line A\nLine B\n").unwrap();

    let mut state = TailState::new();

    // Read both files initially
    let content1 = state.read_new_lines(&file1).unwrap();
    let content2 = state.read_new_lines(&file2).unwrap();

    assert_eq!(content1, "Line 1\nLine 2\n");
    assert_eq!(content2, "Line A\nLine B\n");

    // Append to file1 only
    let mut f1 = fs::OpenOptions::new().append(true).open(&file1).unwrap();
    writeln!(f1, "Line 3").unwrap();

    // Read both again - only file1 should have new content
    let new1 = state.read_new_lines(&file1).unwrap();
    let new2 = state.read_new_lines(&file2).unwrap();

    assert_eq!(new1, "Line 3\n");
    assert_eq!(new2, "");
}

#[test]
fn test_tail_state_reset_forces_full_reread() {
    let temp = TempDir::new().unwrap();
    let file = temp.path().join("events.jsonl");

    fs::write(&file, "Line 1\nLine 2\n").unwrap();

    let mut state = TailState::new();

    // Initial read
    state.read_new_lines(&file).unwrap();

    // Append more
    let mut f = fs::OpenOptions::new().append(true).open(&file).unwrap();
    writeln!(f, "Line 3").unwrap();

    // Reset and read - should get all content
    state.reset(&file);
    let content = state.read_new_lines(&file).unwrap();
    assert_eq!(content, "Line 1\nLine 2\nLine 3\n");
}

#[test]
fn test_tail_state_handles_empty_file() {
    let temp = TempDir::new().unwrap();
    let file = temp.path().join("empty.jsonl");

    fs::write(&file, "").unwrap();

    let mut state = TailState::new();
    let content = state.read_new_lines(&file).unwrap();

    assert_eq!(content, "");
    assert_eq!(state.get_offset(&file), 0);
}

#[test]
fn test_tail_state_error_on_nonexistent_file() {
    let mut state = TailState::new();
    let result = state.read_new_lines(Path::new("/nonexistent/file.txt"));
    assert!(result.is_err());
}

// ============================================================================
// Graceful Error Handling Tests
// ============================================================================

#[test]
fn test_parse_task_graph_missing_fields() {
    // Missing total_tasks and completed_tasks
    let json = r#"{
        "waves": []
    }"#;

    let result = parse_task_graph(json);
    assert!(result.is_err());
}

#[test]
fn test_parse_transcript_partial_tool_data() {
    // Tool message with minimal fields
    let jsonl = r#"{"timestamp":"2026-02-11T10:00:00Z","type":"tool","tool_name":"Read","input_summary":"file.rs"}"#;

    let result = parse_transcript(jsonl);
    assert!(result.is_ok());

    let messages = result.unwrap();
    match &messages[0].kind {
        MessageKind::Tool(call) => {
            assert_eq!(call.tool_name.as_str(), "Read");
            assert!(call.result_summary.is_none());
            assert!(call.duration.is_none());
            assert!(call.success.is_none());
        }
        _ => panic!("Expected Tool"),
    }
}

#[test]
fn test_parse_hook_events_minimal_fields() {
    // Minimal hook event
    let jsonl = r#"{"timestamp":"2026-02-11T10:00:00Z","event":"session_start"}"#;

    let result = parse_hook_events(jsonl);
    assert!(result.is_ok());

    let events = result.unwrap();
    assert_eq!(events.len(), 1);
    assert!(events[0].session_id.is_none());
    assert!(events[0].agent_id.is_none());
}

// ============================================================================
// Edge Cases
// ============================================================================

#[test]
fn test_parse_task_graph_unicode_content() {
    let json = r#"{
        "waves": [{
            "number": 1,
            "tasks": [{
                "id": "T1",
                "description": "Implement 日本語 support 🚀",
                "status": "pending"
            }]
        }],
        "total_tasks": 1,
        "completed_tasks": 0
    }"#;

    let result = parse_task_graph(json);
    assert!(result.is_ok());

    let graph = result.unwrap();
    assert!(graph.waves[0].tasks[0].description.contains("日本語"));
    assert!(graph.waves[0].tasks[0].description.contains("🚀"));
}

#[test]
fn test_parse_transcript_large_content() {
    let large_content = "x".repeat(10000);
    let jsonl = format!(
        r#"{{"timestamp":"2026-02-11T10:00:00Z","type":"reasoning","content":"{}"}}"#,
        large_content
    );

    let result = parse_transcript(&jsonl);
    assert!(result.is_ok());

    let messages = result.unwrap();
    match &messages[0].kind {
        MessageKind::Reasoning { content } => {
            assert_eq!(content.len(), 10000);
        }
        _ => panic!("Expected Reasoning"),
    }
}

#[test]
fn test_tail_state_concurrent_appends() {
    let temp = TempDir::new().unwrap();
    let file = temp.path().join("events.jsonl");

    fs::write(&file, "Line 1\n").unwrap();

    let mut state = TailState::new();

    // Read initial
    let content1 = state.read_new_lines(&file).unwrap();
    assert_eq!(content1, "Line 1\n");

    // Simulate multiple rapid appends
    let mut f = fs::OpenOptions::new().append(true).open(&file).unwrap();
    for i in 2..=10 {
        writeln!(f, "Line {}", i).unwrap();
    }

    // Read all new content
    let content2 = state.read_new_lines(&file).unwrap();
    assert_eq!(content2.lines().count(), 9); // Lines 2-10
}
