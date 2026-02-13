use crate::model::{AgentMessage, HookEvent, HookEventKind, Task, TaskGraph, Wave};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::Path;

/// Parse task graph JSON file into TaskGraph model.
/// Supports both native TUI format and loom orchestration format.
///
/// # Functional Core
/// Pure function - no I/O, just string parsing.
pub fn parse_task_graph(content: &str) -> Result<TaskGraph, String> {
    if let Ok(graph) = serde_json::from_str::<TaskGraph>(content) {
        return Ok(graph);
    }
    parse_loom_format(content)
}

#[derive(Deserialize)]
struct LoomFormat {
    tasks: Vec<LoomTask>,
}

#[derive(Deserialize)]
struct LoomTask {
    id: String,
    description: String,
    #[serde(default)]
    agent: Option<String>,
    #[serde(default = "default_wave")]
    wave: u32,
    #[serde(default)]
    status: crate::model::TaskStatus,
    #[serde(default)]
    review_status: crate::model::ReviewStatus,
    #[serde(default)]
    files_modified: Vec<String>,
    #[serde(default)]
    tests_passed: Option<bool>,
}

fn default_wave() -> u32 {
    1
}

fn parse_loom_format(content: &str) -> Result<TaskGraph, String> {
    let loom: LoomFormat =
        serde_json::from_str(content).map_err(|e| format!("JSON parse error: {}", e))?;

    let mut wave_map: BTreeMap<u32, Vec<Task>> = BTreeMap::new();
    for lt in loom.tasks {
        let task = Task {
            id: lt.id,
            description: lt.description,
            agent_id: lt.agent,
            status: lt.status,
            review_status: lt.review_status,
            files_modified: lt.files_modified,
            tests_passed: lt.tests_passed,
        };
        wave_map.entry(lt.wave).or_default().push(task);
    }

    let waves: Vec<Wave> = wave_map
        .into_iter()
        .map(|(num, tasks)| Wave::new(num, tasks))
        .collect();

    Ok(TaskGraph::new(waves))
}

/// Parse agent transcript JSONL file into vector of messages.
///
/// # Functional Core
/// Pure function - no I/O, just string parsing.
/// Each line is a separate JSON object representing an AgentMessage.
///
/// # Errors
/// Returns error string if any line is malformed JSON.
/// Skips empty lines gracefully.
pub fn parse_transcript(content: &str) -> Result<Vec<AgentMessage>, String> {
    let mut messages = Vec::new();

    for (line_num, line) in content.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        match serde_json::from_str::<AgentMessage>(trimmed) {
            Ok(msg) => messages.push(msg),
            Err(e) => {
                return Err(format!(
                    "Line {}: JSON parse error: {}",
                    line_num + 1,
                    e
                ));
            }
        }
    }

    Ok(messages)
}

/// Parse hook events JSONL file into vector of events.
///
/// # Functional Core
/// Pure function - no I/O, just string parsing.
/// Each line is a separate JSON object representing a HookEvent.
///
/// # Errors
/// Returns error string if any line is malformed JSON.
/// Skips empty lines gracefully.
pub fn parse_hook_events(content: &str) -> Result<Vec<HookEvent>, String> {
    let mut events = Vec::new();

    for (line_num, line) in content.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        // Parse as raw Value first, then deserialize struct, preserving raw for extra fields (cwd etc)
        match serde_json::from_str::<serde_json::Value>(trimmed) {
            Ok(raw_value) => {
                match serde_json::from_value::<HookEvent>(raw_value.clone()) {
                    Ok(mut event) => {
                        event.raw = raw_value;
                        events.push(event);
                    }
                    Err(e) => {
                        return Err(format!(
                            "Line {}: JSON parse error: {}",
                            line_num + 1,
                            e
                        ));
                    }
                }
            }
            Err(e) => {
                return Err(format!(
                    "Line {}: JSON parse error: {}",
                    line_num + 1,
                    e
                ));
            }
        }
    }

    Ok(events)
}

/// Scan active agents directory and return list of active agent IDs.
///
/// # Functional Core
/// Takes list of file paths as input (I/O already done by caller).
/// Pure extraction of agent IDs from filenames.
///
/// # Returns
/// Vector of agent IDs extracted from *.active filenames.
pub fn extract_active_agent_ids(paths: &[&Path]) -> Vec<String> {
    paths
        .iter()
        .filter(|p| p.extension() == Some("active".as_ref()))
        .filter_map(|p| {
            p.file_stem()
                .and_then(|s| s.to_str())
                .map(|s| s.to_string())
        })
        .collect()
}

/// Parse Claude Code transcript JSONL incrementally, extracting agent_progress tool calls.
/// These entries exist in the PARENT transcript and contain `agentId` per tool call,
/// solving the attribution problem when multiple subagents run in parallel.
///
/// # Functional Core
/// Pure function — extracts PreToolUse events with agent_id from agent_progress entries.
pub fn parse_agent_progress_tool_calls(
    content: &str,
    session_id: &str,
) -> Vec<HookEvent> {
    let mut events = Vec::new();

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let entry: Value = match serde_json::from_str(trimmed) {
            Ok(v) => v,
            Err(_) => continue,
        };

        // Only process agent_progress entries
        if entry.get("type").and_then(|v| v.as_str()) != Some("progress") {
            continue;
        }
        let data = match entry.get("data") {
            Some(d) => d,
            None => continue,
        };
        if data.get("type").and_then(|v| v.as_str()) != Some("agent_progress") {
            continue;
        }

        let agent_id = match data.get("agentId").and_then(|v| v.as_str()) {
            Some(id) => id,
            None => continue,
        };

        // Extract timestamp
        let timestamp: DateTime<Utc> = entry
            .get("timestamp")
            .and_then(|v| v.as_str())
            .and_then(|s| s.parse().ok())
            .or_else(|| {
                data.get("message")
                    .and_then(|m| m.get("timestamp"))
                    .and_then(|v| v.as_str())
                    .and_then(|s| s.parse().ok())
            })
            .unwrap_or_else(Utc::now);

        // Look for tool_use blocks in data.message.message.content[]
        let content_blocks = match data
            .get("message")
            .and_then(|m| m.get("message"))
            .and_then(|m| m.get("content"))
        {
            Some(Value::Array(blocks)) => blocks,
            _ => continue,
        };

        for block in content_blocks {
            if block.get("type").and_then(|v| v.as_str()) != Some("tool_use") {
                continue;
            }

            let tool_name = block
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown")
                .to_string();

            let input = block.get("input").cloned().unwrap_or(Value::Null);
            let input_summary = extract_tool_input_summary(&tool_name, &input);

            let event = HookEvent::new(
                timestamp,
                HookEventKind::pre_tool_use(tool_name, input_summary),
            )
            .with_session(session_id.to_string())
            .with_agent(agent_id.to_string());

            events.push(event);
        }
    }

    events
}

/// Extract a human-readable summary from tool input JSON (mirrors send_event.sh logic).
fn extract_tool_input_summary(tool_name: &str, input: &Value) -> String {
    let summary = match tool_name {
        "Read" | "Glob" | "Grep" => input
            .get("file_path")
            .or_else(|| input.get("path"))
            .or_else(|| input.get("pattern"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        "Edit" | "Write" => input
            .get("file_path")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        "Bash" => input
            .get("description")
            .or_else(|| input.get("command"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        "Task" => input
            .get("description")
            .or_else(|| input.get("prompt"))
            .and_then(|v| v.as_str())
            .map(|s| if s.len() > 80 { format!("{}...", &s[..80]) } else { s.to_string() })
            .unwrap_or_default(),
        _ => input
            .get("file_path")
            .or_else(|| input.get("command"))
            .or_else(|| input.get("pattern"))
            .or_else(|| input.get("query"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
    };
    if summary.len() > 500 {
        format!("{}...", &summary[..500])
    } else {
        summary
    }
}

/// Parse Claude Code transcript JSONL incrementally, extracting assistant text blocks.
///
/// # Functional Core
/// Pure function — takes raw content + byte offset, returns HookEvents.
/// Only extracts `type: "text"` blocks from `type: "assistant"` entries.
/// Skips `type: "thinking"` blocks (too verbose). Truncates to 500 chars per block.
///
/// # Arguments
/// * `content` - Raw JSONL content (full file or tail segment)
/// * `session_id` - Session ID to attribute events to
///
/// # Returns
/// Vector of HookEvents with AssistantText kind, plus the number of bytes consumed.
pub fn parse_claude_transcript_incremental(
    content: &str,
    session_id: &str,
) -> Vec<HookEvent> {
    let mut events = Vec::new();

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let entry: Value = match serde_json::from_str(trimmed) {
            Ok(v) => v,
            Err(_) => continue,
        };

        // Only process assistant entries
        if entry.get("type").and_then(|v| v.as_str()) != Some("assistant") {
            continue;
        }

        let timestamp: DateTime<Utc> = entry
            .get("timestamp")
            .and_then(|v| v.as_str())
            .and_then(|s| s.parse().ok())
            .unwrap_or_else(Utc::now);

        // Extract text blocks from message.content[]
        let content_blocks = match entry.get("message").and_then(|m| m.get("content")) {
            Some(Value::Array(blocks)) => blocks,
            _ => continue,
        };

        for block in content_blocks {
            let block_type = block.get("type").and_then(|v| v.as_str()).unwrap_or("");
            if block_type != "text" {
                continue;
            }

            let text = match block.get("text").and_then(|v| v.as_str()) {
                Some(t) if !t.trim().is_empty() => t,
                _ => continue,
            };

            let truncated = if text.len() > 500 {
                format!("{}...", &text[..500])
            } else {
                text.to_string()
            };

            let event = HookEvent::new(timestamp, HookEventKind::assistant_text(truncated))
                .with_session(session_id.to_string());
            events.push(event);
        }
    }

    events
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{MessageKind, TaskStatus};

    #[test]
    fn test_parse_task_graph_valid() {
        let json = r#"{
            "waves": [
                {
                    "number": 1,
                    "tasks": [
                        {
                            "id": "T1",
                            "description": "Implement feature",
                            "status": "pending"
                        },
                        {
                            "id": "T2",
                            "description": "Write tests",
                            "status": "running"
                        }
                    ]
                }
            ],
            "total_tasks": 2,
            "completed_tasks": 0
        }"#;

        let result = parse_task_graph(json);
        assert!(result.is_ok());

        let graph = result.unwrap();
        assert_eq!(graph.total_tasks, 2);
        assert_eq!(graph.completed_tasks, 0);
        assert_eq!(graph.waves.len(), 1);
        assert_eq!(graph.waves[0].tasks.len(), 2);
    }

    #[test]
    fn test_parse_task_graph_invalid_json() {
        let invalid = "not json at all";
        let result = parse_task_graph(invalid);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("JSON parse error"));
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
        assert_eq!(graph.total_tasks, 0);
        assert_eq!(graph.waves.len(), 0);
    }

    #[test]
    fn test_parse_transcript_valid() {
        let jsonl = r#"{"timestamp":"2026-02-11T10:00:00Z","type":"reasoning","content":"Starting task"}
{"timestamp":"2026-02-11T10:01:00Z","type":"tool","tool_name":"Read","input_summary":"file.rs"}"#;

        let result = parse_transcript(jsonl);
        assert!(result.is_ok());

        let messages = result.unwrap();
        assert_eq!(messages.len(), 2);

        match &messages[0].kind {
            MessageKind::Reasoning { content } => {
                assert_eq!(content, "Starting task");
            }
            _ => panic!("Expected Reasoning message"),
        }

        match &messages[1].kind {
            MessageKind::Tool(call) => {
                assert_eq!(call.tool_name, "Read");
                assert_eq!(call.input_summary, "file.rs");
            }
            _ => panic!("Expected Tool message"),
        }
    }

    #[test]
    fn test_parse_transcript_empty_lines() {
        let jsonl = r#"{"timestamp":"2026-02-11T10:00:00Z","type":"reasoning","content":"test"}

{"timestamp":"2026-02-11T10:01:00Z","type":"reasoning","content":"test2"}
"#;

        let result = parse_transcript(jsonl);
        assert!(result.is_ok());

        let messages = result.unwrap();
        assert_eq!(messages.len(), 2);
    }

    #[test]
    fn test_parse_transcript_invalid_line() {
        let jsonl = r#"{"timestamp":"2026-02-11T10:00:00Z","type":"reasoning","content":"valid"}
not valid json
{"timestamp":"2026-02-11T10:01:00Z","type":"reasoning","content":"also valid"}"#;

        let result = parse_transcript(jsonl);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Line 2"));
    }

    #[test]
    fn test_parse_hook_events_valid() {
        let jsonl = r#"{"timestamp":"2026-02-11T10:00:00Z","event":"session_start"}
{"timestamp":"2026-02-11T10:01:00Z","event":"subagent_start","task_description":"Test task"}
{"timestamp":"2026-02-11T10:02:00Z","event":"notification","message":"Test message"}"#;

        let result = parse_hook_events(jsonl);
        assert!(result.is_ok());

        let events = result.unwrap();
        assert_eq!(events.len(), 3);
    }

    #[test]
    fn test_parse_hook_events_empty() {
        let jsonl = "";
        let result = parse_hook_events(jsonl);
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    #[test]
    fn test_parse_hook_events_invalid_line() {
        let jsonl = r#"{"timestamp":"2026-02-11T10:00:00Z","event":"session_start"}
invalid
{"timestamp":"2026-02-11T10:02:00Z","event":"session_end"}"#;

        let result = parse_hook_events(jsonl);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Line 2"));
    }

    #[test]
    fn test_extract_active_agent_ids() {
        let paths = vec![
            Path::new("/tmp/claude-subagents/a01.active"),
            Path::new("/tmp/claude-subagents/a02.active"),
            Path::new("/tmp/claude-subagents/a03.txt"), // wrong extension
            Path::new("/tmp/claude-subagents/other.file"),
        ];

        let agent_ids = extract_active_agent_ids(&paths);
        assert_eq!(agent_ids.len(), 2);
        assert!(agent_ids.contains(&"a01".to_string()));
        assert!(agent_ids.contains(&"a02".to_string()));
    }

    #[test]
    fn test_extract_active_agent_ids_empty() {
        let paths: Vec<&Path> = vec![];
        let agent_ids = extract_active_agent_ids(&paths);
        assert!(agent_ids.is_empty());
    }

    #[test]
    fn test_extract_active_agent_ids_no_active_files() {
        let paths = vec![
            Path::new("/tmp/claude-subagents/file.txt"),
            Path::new("/tmp/claude-subagents/other.log"),
        ];

        let agent_ids = extract_active_agent_ids(&paths);
        assert!(agent_ids.is_empty());
    }

    #[test]
    fn test_parse_transcript_with_tool_duration() {
        let jsonl = r#"{"timestamp":"2026-02-11T10:00:00Z","type":"tool","tool_name":"Bash","input_summary":"cargo test","duration":1500,"success":true}"#;

        let result = parse_transcript(jsonl);
        assert!(result.is_ok());

        let messages = result.unwrap();
        assert_eq!(messages.len(), 1);

        match &messages[0].kind {
            MessageKind::Tool(call) => {
                assert_eq!(call.tool_name, "Bash");
                assert_eq!(call.duration, Some(std::time::Duration::from_millis(1500)));
                assert_eq!(call.success, Some(true));
            }
            _ => panic!("Expected Tool message"),
        }
    }

    #[test]
    fn test_parse_task_graph_loom_format() {
        let json = r#"{
            "current_phase": "execute",
            "tasks": [
                {
                    "id": "T1",
                    "description": "Create scaffold",
                    "agent": "dotfiles-agent",
                    "wave": 1,
                    "status": "completed",
                    "review_status": "passed",
                    "files_modified": ["Cargo.toml"],
                    "tests_passed": true
                },
                {
                    "id": "T2",
                    "description": "Implement models",
                    "agent": "code-implementer-agent",
                    "wave": 1,
                    "status": "completed",
                    "spec_anchors": ["FR-020"],
                    "depends_on": []
                },
                {
                    "id": "T3",
                    "description": "Wire main loop",
                    "agent": "code-implementer-agent",
                    "wave": 2,
                    "status": "running"
                }
            ]
        }"#;

        let result = parse_task_graph(json);
        assert!(result.is_ok());

        let graph = result.unwrap();
        assert_eq!(graph.waves.len(), 2);
        assert_eq!(graph.total_tasks, 3);

        // Wave 1 has 2 tasks
        assert_eq!(graph.waves[0].number, 1);
        assert_eq!(graph.waves[0].tasks.len(), 2);
        assert_eq!(graph.waves[0].tasks[0].agent_id, Some("dotfiles-agent".into()));

        // Wave 2 has 1 task
        assert_eq!(graph.waves[1].number, 2);
        assert_eq!(graph.waves[1].tasks.len(), 1);
        assert_eq!(graph.waves[1].tasks[0].id, "T3");
    }

    #[test]
    fn test_parse_claude_transcript_extracts_text_blocks() {
        let jsonl = r#"{"type":"assistant","timestamp":"2026-02-11T10:00:00Z","message":{"content":[{"type":"text","text":"Let me read the file."}]},"sessionId":"s1"}"#;

        let events = parse_claude_transcript_incremental(jsonl, "s1");
        assert_eq!(events.len(), 1);
        match &events[0].kind {
            HookEventKind::AssistantText { content } => {
                assert_eq!(content, "Let me read the file.");
            }
            _ => panic!("Expected AssistantText"),
        }
        assert_eq!(events[0].session_id.as_deref(), Some("s1"));
    }

    #[test]
    fn test_parse_claude_transcript_skips_thinking_blocks() {
        let jsonl = r#"{"type":"assistant","timestamp":"2026-02-11T10:00:00Z","message":{"content":[{"type":"thinking","thinking":"internal thought"},{"type":"text","text":"visible"}]}}"#;

        let events = parse_claude_transcript_incremental(jsonl, "s1");
        assert_eq!(events.len(), 1);
        match &events[0].kind {
            HookEventKind::AssistantText { content } => {
                assert_eq!(content, "visible");
            }
            _ => panic!("Expected AssistantText"),
        }
    }

    #[test]
    fn test_parse_claude_transcript_skips_non_assistant() {
        let jsonl = r#"{"type":"human","timestamp":"2026-02-11T10:00:00Z","message":{"content":[{"type":"text","text":"user message"}]}}
{"type":"tool_result","timestamp":"2026-02-11T10:00:01Z","message":{"content":[]}}"#;

        let events = parse_claude_transcript_incremental(jsonl, "s1");
        assert!(events.is_empty());
    }

    #[test]
    fn test_parse_claude_transcript_truncates_long_text() {
        let long_text = "x".repeat(600);
        let jsonl = format!(
            r#"{{"type":"assistant","timestamp":"2026-02-11T10:00:00Z","message":{{"content":[{{"type":"text","text":"{}"}}]}}}}"#,
            long_text
        );

        let events = parse_claude_transcript_incremental(&jsonl, "s1");
        assert_eq!(events.len(), 1);
        match &events[0].kind {
            HookEventKind::AssistantText { content } => {
                assert!(content.len() <= 503); // 500 + "..."
                assert!(content.ends_with("..."));
            }
            _ => panic!("Expected AssistantText"),
        }
    }

    #[test]
    fn test_parse_claude_transcript_empty_content() {
        let jsonl = "";
        let events = parse_claude_transcript_incremental(jsonl, "s1");
        assert!(events.is_empty());
    }

    #[test]
    fn test_parse_claude_transcript_skips_malformed_lines() {
        let jsonl = "not json\n{\"type\":\"assistant\",\"timestamp\":\"2026-02-11T10:00:00Z\",\"message\":{\"content\":[{\"type\":\"text\",\"text\":\"ok\"}]}}";

        let events = parse_claude_transcript_incremental(jsonl, "s1");
        assert_eq!(events.len(), 1);
    }

    #[test]
    fn test_parse_task_graph_with_failed_task() {
        let json = r#"{
            "waves": [
                {
                    "number": 1,
                    "tasks": [
                        {
                            "id": "T1",
                            "description": "Task",
                            "status": {
                                "failed": {
                                    "reason": "Test failure",
                                    "retry_count": 1
                                }
                            }
                        }
                    ]
                }
            ],
            "total_tasks": 1,
            "completed_tasks": 0
        }"#;

        let result = parse_task_graph(json);
        assert!(result.is_ok());

        let graph = result.unwrap();
        match &graph.waves[0].tasks[0].status {
            TaskStatus::Failed { reason, retry_count } => {
                assert_eq!(reason, "Test failure");
                assert_eq!(*retry_count, 1);
            }
            _ => panic!("Expected Failed status"),
        }
    }

    #[test]
    fn test_parse_agent_progress_extracts_tool_calls() {
        let jsonl = r#"{"type":"progress","data":{"type":"agent_progress","agentId":"a01","message":{"timestamp":"2026-02-12T10:00:00Z","message":{"role":"assistant","content":[{"type":"tool_use","name":"Read","input":{"file_path":"/tmp/foo.rs"}}]}}},"timestamp":"2026-02-12T10:00:00Z"}"#;

        let events = parse_agent_progress_tool_calls(jsonl, "s1");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].agent_id.as_deref(), Some("a01"));
        assert_eq!(events[0].session_id.as_deref(), Some("s1"));
        match &events[0].kind {
            HookEventKind::PreToolUse { tool_name, input_summary } => {
                assert_eq!(tool_name, "Read");
                assert_eq!(input_summary, "/tmp/foo.rs");
            }
            _ => panic!("Expected PreToolUse"),
        }
    }

    #[test]
    fn test_parse_agent_progress_skips_non_progress() {
        let jsonl = r#"{"type":"assistant","data":{},"timestamp":"2026-02-12T10:00:00Z"}"#;
        let events = parse_agent_progress_tool_calls(jsonl, "s1");
        assert!(events.is_empty());
    }

    #[test]
    fn test_parse_agent_progress_skips_no_agent_id() {
        let jsonl = r#"{"type":"progress","data":{"type":"agent_progress","message":{"message":{"content":[{"type":"tool_use","name":"Bash","input":{"command":"ls"}}]}}},"timestamp":"2026-02-12T10:00:00Z"}"#;
        let events = parse_agent_progress_tool_calls(jsonl, "s1");
        assert!(events.is_empty());
    }

    #[test]
    fn test_parse_agent_progress_multiple_tools_in_one_entry() {
        let jsonl = r#"{"type":"progress","data":{"type":"agent_progress","agentId":"a02","message":{"timestamp":"2026-02-12T10:00:00Z","message":{"content":[{"type":"tool_use","name":"Read","input":{"file_path":"a.rs"}},{"type":"tool_use","name":"Read","input":{"file_path":"b.rs"}}]}}},"timestamp":"2026-02-12T10:00:00Z"}"#;

        let events = parse_agent_progress_tool_calls(jsonl, "s1");
        assert_eq!(events.len(), 2);
        assert!(events.iter().all(|e| e.agent_id.as_deref() == Some("a02")));
    }

    #[test]
    fn test_extract_tool_input_summary() {
        assert_eq!(
            extract_tool_input_summary("Read", &serde_json::json!({"file_path": "/tmp/foo"})),
            "/tmp/foo"
        );
        assert_eq!(
            extract_tool_input_summary("Bash", &serde_json::json!({"command": "ls", "description": "list files"})),
            "list files"
        );
        assert_eq!(
            extract_tool_input_summary("Edit", &serde_json::json!({"file_path": "/tmp/bar.rs"})),
            "/tmp/bar.rs"
        );
    }
}
