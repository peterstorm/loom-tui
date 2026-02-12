use crate::model::{AgentMessage, HookEvent, Task, TaskGraph, Wave};
use serde::Deserialize;
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
}
