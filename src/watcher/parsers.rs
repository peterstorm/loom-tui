use crate::error::ParseError;
use crate::model::{AgentMessage, HookEvent, HookEventKind, Task, TaskGraph, TokenUsage, Wave};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::Value;
use std::collections::{BTreeMap, HashMap};
use std::path::Path;

/// Safely truncate a string to a maximum character count (not bytes).
/// Prevents panics from slicing on multibyte UTF-8 character boundaries.
pub(crate) fn truncate_str(s: &str, max_chars: usize) -> String {
    let char_count = s.chars().count();
    if char_count <= max_chars {
        s.to_string()
    } else {
        s.chars().take(max_chars).collect::<String>() + "..."
    }
}

/// Parse task graph JSON file into TaskGraph model.
/// Supports both native TUI format and loom orchestration format.
///
/// # Functional Core
/// Pure function - no I/O, just string parsing.
pub fn parse_task_graph(content: &str) -> Result<TaskGraph, ParseError> {
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

fn parse_loom_format(content: &str) -> Result<TaskGraph, ParseError> {
    let loom: LoomFormat =
        serde_json::from_str(content).map_err(|e| ParseError::Json(e.to_string()))?;

    let mut wave_map: BTreeMap<u32, Vec<Task>> = BTreeMap::new();
    for lt in loom.tasks {
        let task = Task {
            id: lt.id.into(),
            description: lt.description,
            agent_id: lt.agent.map(Into::into),
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
/// Returns ParseError if any line is malformed JSON.
/// Skips empty lines gracefully.
pub fn parse_transcript(content: &str) -> Result<Vec<AgentMessage>, ParseError> {
    let mut messages = Vec::new();

    for (line_num, line) in content.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        match serde_json::from_str::<AgentMessage>(trimmed) {
            Ok(msg) => messages.push(msg),
            Err(e) => {
                return Err(ParseError::Json(format!(
                    "Line {}: {}",
                    line_num + 1,
                    e
                )));
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
/// Returns ParseError if any line is malformed JSON.
/// Skips empty lines gracefully.
pub fn parse_hook_events(content: &str) -> Result<Vec<HookEvent>, ParseError> {
    let mut events = Vec::new();

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        // Parse as raw Value first, then deserialize struct, preserving raw for extra fields (cwd etc)
        // Skip unparseable lines (concurrent writes can produce partial JSON)
        let raw_value = match serde_json::from_str::<serde_json::Value>(trimmed) {
            Ok(v) => v,
            Err(_) => continue,
        };
        let mut event = match serde_json::from_value::<HookEvent>(raw_value.clone()) {
            Ok(e) => e,
            Err(_) => continue,
        };
        event.raw = raw_value;
        events.push(event);
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
        "Read" | "Glob" => input
            .get("file_path")
            .or_else(|| input.get("path"))
            .or_else(|| input.get("pattern"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        // Grep: hook script uses .pattern (the regex), not .path (the directory).
        // Must match hook extraction order for dedup to work.
        "Grep" => input
            .get("pattern")
            .or_else(|| input.get("path"))
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
            .map(|s| truncate_str(s, 200))
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
    truncate_str(&summary, 8000)
}

/// Parse Claude Code transcript JSONL incrementally, extracting assistant text and tool_use blocks.
///
/// # Functional Core
/// Pure function — takes raw content + session_id, returns HookEvents.
/// Extracts `type: "text"` blocks as AssistantText and `type: "tool_use"` blocks as PreToolUse.
/// Captures `agentId` from transcript entries for proper agent attribution.
/// Skips `type: "thinking"` blocks (too verbose). Truncates text to 4000 chars per block.
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

        let agent_id = entry.get("agentId").and_then(|v| v.as_str());

        let content_blocks = match entry.get("message").and_then(|m| m.get("content")) {
            Some(Value::Array(blocks)) => blocks,
            _ => continue,
        };

        for block in content_blocks {
            let block_type = block.get("type").and_then(|v| v.as_str()).unwrap_or("");

            match block_type {
                "text" => {
                    let text = match block.get("text").and_then(|v| v.as_str()) {
                        Some(t) if !t.trim().is_empty() => t,
                        _ => continue,
                    };
                    let truncated = truncate_str(text, 4000);
                    let mut event = HookEvent::new(timestamp, HookEventKind::assistant_text(truncated))
                        .with_session(session_id.to_string());
                    if let Some(aid) = agent_id {
                        event = event.with_agent(aid.to_string());
                    }
                    events.push(event);
                }
                "tool_use" => {
                    let tool_name = block
                        .get("name")
                        .and_then(|v| v.as_str())
                        .unwrap_or("unknown")
                        .to_string();
                    let input = block.get("input").cloned().unwrap_or(Value::Null);
                    let input_summary = extract_tool_input_summary(&tool_name, &input);
                    let mut event = HookEvent::new(
                        timestamp,
                        HookEventKind::pre_tool_use(tool_name, input_summary),
                    )
                    .with_session(session_id.to_string());
                    if let Some(aid) = agent_id {
                        event = event.with_agent(aid.to_string());
                    }
                    events.push(event);
                }
                _ => {}
            }
        }
    }

    events
}

/// Metadata extracted from a Claude Code subagent transcript.
#[derive(Debug, Clone, Default)]
pub struct TranscriptMetadata {
    pub model: Option<String>,
    pub token_usage: TokenUsage,
    pub skills: Vec<String>,
}

/// Parse Claude Code transcript JSONL to extract model, token usage, and skills.
///
/// # Functional Core
/// Pure function — no I/O, just string parsing.
///
/// Deduplicates assistant entries by `message.id` (streaming writes produce
/// multiple JSONL lines per API call). Stores the LAST unique message's usage
/// as `token_usage` — its `context_window()` approximates Claude Code's
/// reported `total_tokens`.
///
/// For each JSONL line:
/// - `type:"assistant"` → extract `.message.model` (keep first), deduplicate usage by message ID
/// - `type:"user"` → scan content text blocks for `<command-name>X</command-name>` tags
pub fn parse_transcript_metadata(content: &str) -> TranscriptMetadata {
    let mut meta = TranscriptMetadata::default();
    // Track per-message-ID usage; last write per ID wins (streaming dedup).
    // Preserve insertion order so we can pick the last unique message.
    let mut msg_order: Vec<String> = Vec::new();
    let mut msg_usage: HashMap<String, TokenUsage> = HashMap::new();

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let entry: Value = match serde_json::from_str(trimmed) {
            Ok(v) => v,
            Err(_) => continue,
        };

        let entry_type = entry.get("type").and_then(|v| v.as_str()).unwrap_or("");

        match entry_type {
            "assistant" => {
                // Extract model (keep first non-None)
                if meta.model.is_none() {
                    if let Some(model_id) = entry
                        .get("message")
                        .and_then(|m| m.get("model"))
                        .and_then(|v| v.as_str())
                    {
                        meta.model = Some(shorten_model_id(model_id));
                    }
                }

                // Deduplicate usage by message.id
                if let Some(usage) = entry.get("message").and_then(|m| m.get("usage")) {
                    let msg_id = entry
                        .get("message")
                        .and_then(|m| m.get("id"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();

                    let tu = TokenUsage {
                        input_tokens: usage
                            .get("input_tokens")
                            .and_then(|v| v.as_u64())
                            .unwrap_or(0),
                        output_tokens: usage
                            .get("output_tokens")
                            .and_then(|v| v.as_u64())
                            .unwrap_or(0),
                        cache_creation_input_tokens: usage
                            .get("cache_creation_input_tokens")
                            .and_then(|v| v.as_u64())
                            .unwrap_or(0),
                        cache_read_input_tokens: usage
                            .get("cache_read_input_tokens")
                            .and_then(|v| v.as_u64())
                            .unwrap_or(0),
                    };

                    if !msg_usage.contains_key(&msg_id) {
                        msg_order.push(msg_id.clone());
                    }
                    msg_usage.insert(msg_id, tu);
                }
            }
            "human" | "user" => {
                // Scan content for <command-name>X</command-name>
                // Content can be either a string or an array of blocks
                match entry.get("message").and_then(|m| m.get("content")) {
                    Some(Value::Array(blocks)) => {
                        for block in blocks {
                            if block.get("type").and_then(|v| v.as_str()) != Some("text") {
                                continue;
                            }
                            if let Some(text) = block.get("text").and_then(|v| v.as_str()) {
                                extract_command_names(text, &mut meta.skills);
                            }
                        }
                    }
                    Some(Value::String(text)) => {
                        extract_command_names(text, &mut meta.skills);
                    }
                    _ => continue,
                };
            }
            _ => {}
        }
    }

    // Use the last unique message's usage as token_usage (context window).
    if let Some(last_id) = msg_order.last() {
        if let Some(tu) = msg_usage.remove(last_id) {
            meta.token_usage = tu;
        }
    }

    meta.skills.sort();
    meta.skills.dedup();
    meta
}

/// Extract `<command-name>X</command-name>` tags from text via string search.
fn extract_command_names(text: &str, skills: &mut Vec<String>) {
    const OPEN: &str = "<command-name>";
    const CLOSE: &str = "</command-name>";

    let mut search_from = 0;
    while let Some(start) = text[search_from..].find(OPEN) {
        let name_start = search_from + start + OPEN.len();
        if let Some(end) = text[name_start..].find(CLOSE) {
            let name = &text[name_start..name_start + end];
            if !name.is_empty() && !skills.contains(&name.to_string()) {
                skills.push(name.to_string());
            }
            search_from = name_start + end + CLOSE.len();
        } else {
            break;
        }
    }
}

/// Shorten a full Claude model ID to a human-friendly display name.
fn shorten_model_id(full: &str) -> String {
    // Known prefixes → short names (order matters: more specific first)
    if full.starts_with("claude-sonnet-4-5") {
        return "sonnet-4.5".to_string();
    }
    if full.starts_with("claude-opus-4") {
        return "opus-4".to_string();
    }
    if full.starts_with("claude-haiku-4-5") {
        return "haiku-4.5".to_string();
    }
    if full.starts_with("claude-haiku-3-5") || full.starts_with("claude-3-5-haiku") {
        return "haiku-3.5".to_string();
    }
    if full.starts_with("claude-sonnet-4") || full.starts_with("claude-4-sonnet") {
        return "sonnet-4".to_string();
    }
    if full.starts_with("claude-3-5-sonnet") {
        return "sonnet-3.5".to_string();
    }
    if full.starts_with("claude-3-opus") {
        return "opus-3".to_string();
    }

    // Fallback: last segment before date suffix, or full string
    full.split('-')
        .take_while(|s| s.parse::<u32>().is_err() || s.len() <= 2)
        .collect::<Vec<_>>()
        .join("-")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{AgentId, MessageKind, SessionId, TaskStatus};

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
        assert_eq!(graph.total_tasks(), 2);
        assert_eq!(graph.completed_tasks(), 0);
        assert_eq!(graph.waves.len(), 1);
        assert_eq!(graph.waves[0].tasks.len(), 2);
    }

    #[test]
    fn test_parse_task_graph_invalid_json() {
        let invalid = "not json at all";
        let result = parse_task_graph(invalid);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("JSON"));
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
        assert_eq!(graph.total_tasks(), 0);
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
                assert_eq!(call.tool_name.as_str(), "Read");
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
        assert!(result.unwrap_err().to_string().contains("Line 2"));
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
    fn test_parse_hook_events_skips_invalid_lines() {
        let jsonl = r#"{"timestamp":"2026-02-11T10:00:00Z","event":"session_start"}
invalid
{"timestamp":"2026-02-11T10:02:00Z","event":"session_end"}"#;

        let result = parse_hook_events(jsonl).unwrap();
        // Invalid line skipped, valid events preserved
        assert_eq!(result.len(), 2);
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
                assert_eq!(call.tool_name.as_str(), "Bash");
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
        assert_eq!(graph.total_tasks(), 3);

        // Wave 1 has 2 tasks
        assert_eq!(graph.waves[0].number, 1);
        assert_eq!(graph.waves[0].tasks.len(), 2);
        assert_eq!(graph.waves[0].tasks[0].agent_id, Some("dotfiles-agent".into()));

        // Wave 2 has 1 task
        assert_eq!(graph.waves[1].number, 2);
        assert_eq!(graph.waves[1].tasks.len(), 1);
        assert_eq!(graph.waves[1].tasks[0].id.as_str(), "T3");
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
        assert_eq!(events[0].session_id.as_ref(), Some(&SessionId::new("s1")));
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
        let long_text = "x".repeat(5000);
        let jsonl = format!(
            r#"{{"type":"assistant","timestamp":"2026-02-11T10:00:00Z","message":{{"content":[{{"type":"text","text":"{}"}}]}}}}"#,
            long_text
        );

        let events = parse_claude_transcript_incremental(&jsonl, "s1");
        assert_eq!(events.len(), 1);
        match &events[0].kind {
            HookEventKind::AssistantText { content } => {
                assert!(content.len() <= 4003); // 4000 + "..."
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
        assert_eq!(events[0].agent_id.as_ref(), Some(&AgentId::new("a01")));
        assert_eq!(events[0].session_id.as_ref(), Some(&SessionId::new("s1")));
        match &events[0].kind {
            HookEventKind::PreToolUse { tool_name, input_summary, .. } => {
                assert_eq!(tool_name.as_str(), "Read");
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
        assert!(events.iter().all(|e| e.agent_id.as_ref() == Some(&AgentId::new("a02"))));
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

    #[test]
    fn truncate_str_under_limit() {
        assert_eq!(truncate_str("hello", 10), "hello");
    }

    #[test]
    fn truncate_str_over_limit() {
        assert_eq!(truncate_str("hello world", 5), "hello...");
    }

    #[test]
    fn truncate_str_exact_boundary() {
        assert_eq!(truncate_str("hello", 5), "hello");
    }

    #[test]
    fn truncate_str_empty() {
        assert_eq!(truncate_str("", 5), "");
    }

    #[test]
    fn truncate_str_multibyte_cjk() {
        let cjk = "日本語テスト文字列";
        let result = truncate_str(cjk, 3);
        assert_eq!(result, "日本語...");
    }

    #[test]
    fn truncate_str_emoji() {
        let emoji = "🎉🎊🎈🎁🎂";
        let result = truncate_str(emoji, 2);
        assert_eq!(result, "🎉🎊...");
    }

    // ============================================================================
    // parse_transcript_metadata tests
    // ============================================================================

    #[test]
    fn transcript_metadata_empty_content() {
        let meta = parse_transcript_metadata("");
        assert!(meta.model.is_none());
        assert!(meta.token_usage.is_empty());
        assert!(meta.skills.is_empty());
    }

    #[test]
    fn transcript_metadata_single_assistant_turn() {
        let jsonl = r#"{"type":"assistant","message":{"id":"msg_01","model":"claude-sonnet-4-5-20250929","usage":{"input_tokens":100,"output_tokens":50,"cache_creation_input_tokens":10,"cache_read_input_tokens":5},"content":[{"type":"text","text":"hello"}]}}"#;
        let meta = parse_transcript_metadata(jsonl);
        assert_eq!(meta.model.as_deref(), Some("sonnet-4.5"));
        assert_eq!(meta.token_usage.input_tokens, 100);
        assert_eq!(meta.token_usage.output_tokens, 50);
        assert_eq!(meta.token_usage.cache_creation_input_tokens, 10);
        assert_eq!(meta.token_usage.cache_read_input_tokens, 5);
        assert_eq!(meta.token_usage.context_window(), 115); // 100 + 10 + 5
    }

    #[test]
    fn transcript_metadata_multi_turn_uses_last_message() {
        // Two distinct messages (different IDs) — last message's usage wins
        let jsonl = concat!(
            r#"{"type":"assistant","message":{"id":"msg_01","model":"claude-opus-4-20250514","usage":{"input_tokens":100,"output_tokens":50,"cache_creation_input_tokens":0,"cache_read_input_tokens":0},"content":[]}}"#,
            "\n",
            r#"{"type":"assistant","message":{"id":"msg_02","model":"claude-opus-4-20250514","usage":{"input_tokens":200,"output_tokens":75,"cache_creation_input_tokens":500,"cache_read_input_tokens":100},"content":[]}}"#,
        );
        let meta = parse_transcript_metadata(jsonl);
        assert_eq!(meta.model.as_deref(), Some("opus-4"));
        // Last unique message's usage
        assert_eq!(meta.token_usage.input_tokens, 200);
        assert_eq!(meta.token_usage.output_tokens, 75);
        assert_eq!(meta.token_usage.cache_creation_input_tokens, 500);
        assert_eq!(meta.token_usage.cache_read_input_tokens, 100);
        assert_eq!(meta.token_usage.context_window(), 800); // 200 + 500 + 100
    }

    #[test]
    fn transcript_metadata_deduplicates_streaming_entries() {
        // Same message ID appears 3 times (streaming writes) — only counted once
        let jsonl = concat!(
            r#"{"type":"assistant","message":{"id":"msg_01","model":"claude-opus-4-20250514","usage":{"input_tokens":100,"output_tokens":1,"cache_creation_input_tokens":500,"cache_read_input_tokens":10000},"content":[]}}"#,
            "\n",
            r#"{"type":"assistant","message":{"id":"msg_01","model":"claude-opus-4-20250514","usage":{"input_tokens":100,"output_tokens":1,"cache_creation_input_tokens":500,"cache_read_input_tokens":10000},"content":[]}}"#,
            "\n",
            r#"{"type":"assistant","message":{"id":"msg_01","model":"claude-opus-4-20250514","usage":{"input_tokens":100,"output_tokens":1,"cache_creation_input_tokens":500,"cache_read_input_tokens":10000},"content":[]}}"#,
        );
        let meta = parse_transcript_metadata(jsonl);
        // Should be last entry's values, not 3x sum
        assert_eq!(meta.token_usage.input_tokens, 100);
        assert_eq!(meta.token_usage.context_window(), 10600); // 100 + 500 + 10000
    }

    #[test]
    fn transcript_metadata_model_keeps_first() {
        let jsonl = concat!(
            r#"{"type":"assistant","message":{"id":"msg_01","model":"claude-sonnet-4-5-20250929","usage":{"input_tokens":10,"output_tokens":5,"cache_creation_input_tokens":0,"cache_read_input_tokens":0},"content":[]}}"#,
            "\n",
            r#"{"type":"assistant","message":{"id":"msg_02","model":"claude-opus-4-20250514","usage":{"input_tokens":10,"output_tokens":5,"cache_creation_input_tokens":0,"cache_read_input_tokens":0},"content":[]}}"#,
        );
        let meta = parse_transcript_metadata(jsonl);
        assert_eq!(meta.model.as_deref(), Some("sonnet-4.5"));
    }

    #[test]
    fn transcript_metadata_skill_extraction() {
        let jsonl = r#"{"type":"human","message":{"content":[{"type":"text","text":"<command-name>code-implementer</command-name> loaded"}]}}"#;
        let meta = parse_transcript_metadata(jsonl);
        assert_eq!(meta.skills, vec!["code-implementer"]);
    }

    #[test]
    fn transcript_metadata_skill_dedup() {
        let jsonl = concat!(
            r#"{"type":"user","message":{"content":[{"type":"text","text":"<command-name>commit</command-name>"}]}}"#,
            "\n",
            r#"{"type":"user","message":{"content":[{"type":"text","text":"<command-name>commit</command-name>"}]}}"#,
        );
        let meta = parse_transcript_metadata(jsonl);
        assert_eq!(meta.skills, vec!["commit"]);
    }

    #[test]
    fn transcript_metadata_multiple_skills_in_one_block() {
        let jsonl = r#"{"type":"human","message":{"content":[{"type":"text","text":"<command-name>alpha</command-name> and <command-name>beta</command-name>"}]}}"#;
        let meta = parse_transcript_metadata(jsonl);
        assert_eq!(meta.skills, vec!["alpha", "beta"]);
    }

    #[test]
    fn transcript_metadata_mixed_content() {
        let jsonl = concat!(
            r#"{"type":"assistant","message":{"id":"msg_01","model":"claude-sonnet-4-5-20250929","usage":{"input_tokens":50,"output_tokens":25,"cache_creation_input_tokens":0,"cache_read_input_tokens":0},"content":[{"type":"text","text":"hi"}]}}"#,
            "\n",
            r#"{"type":"human","message":{"content":[{"type":"text","text":"<command-name>review-pr</command-name>"}]}}"#,
            "\n",
            r#"{"type":"result","data":{}}"#,
            "\n",
            r#"{"type":"assistant","message":{"id":"msg_02","model":"claude-sonnet-4-5-20250929","usage":{"input_tokens":100,"output_tokens":75,"cache_creation_input_tokens":0,"cache_read_input_tokens":0},"content":[]}}"#,
        );
        let meta = parse_transcript_metadata(jsonl);
        assert_eq!(meta.model.as_deref(), Some("sonnet-4.5"));
        // Last unique message's usage (msg_02)
        assert_eq!(meta.token_usage.input_tokens, 100);
        assert_eq!(meta.token_usage.output_tokens, 75);
        assert_eq!(meta.skills, vec!["review-pr"]);
    }

    #[test]
    fn transcript_metadata_malformed_lines_skipped() {
        let jsonl = "not json\n{\"type\":\"assistant\",\"message\":{\"id\":\"msg_01\",\"model\":\"claude-opus-4-20250514\",\"usage\":{\"input_tokens\":10,\"output_tokens\":5,\"cache_creation_input_tokens\":0,\"cache_read_input_tokens\":0},\"content\":[]}}";
        let meta = parse_transcript_metadata(jsonl);
        assert_eq!(meta.model.as_deref(), Some("opus-4"));
        assert_eq!(meta.token_usage.input_tokens, 10);
    }

    // ============================================================================
    // shorten_model_id tests
    // ============================================================================

    #[test]
    fn shorten_model_sonnet_4_5() {
        assert_eq!(shorten_model_id("claude-sonnet-4-5-20250929"), "sonnet-4.5");
    }

    #[test]
    fn shorten_model_opus_4() {
        assert_eq!(shorten_model_id("claude-opus-4-20250514"), "opus-4");
    }

    #[test]
    fn shorten_model_haiku_4_5() {
        assert_eq!(shorten_model_id("claude-haiku-4-5-20251001"), "haiku-4.5");
    }

    #[test]
    fn shorten_model_haiku_3_5() {
        assert_eq!(shorten_model_id("claude-3-5-haiku-20241022"), "haiku-3.5");
        assert_eq!(shorten_model_id("claude-haiku-3-5-20241022"), "haiku-3.5");
    }

    #[test]
    fn shorten_model_sonnet_4() {
        assert_eq!(shorten_model_id("claude-sonnet-4-20250514"), "sonnet-4");
        assert_eq!(shorten_model_id("claude-4-sonnet-20250514"), "sonnet-4");
    }

    #[test]
    fn shorten_model_sonnet_3_5() {
        assert_eq!(shorten_model_id("claude-3-5-sonnet-20250620"), "sonnet-3.5");
    }

    #[test]
    fn shorten_model_opus_3() {
        assert_eq!(shorten_model_id("claude-3-opus-20240229"), "opus-3");
    }

    #[test]
    fn shorten_model_unknown_fallback() {
        let result = shorten_model_id("some-unknown-model-20260101");
        assert!(!result.is_empty());
    }

    // ============================================================================
    // extract_command_names tests
    // ============================================================================

    #[test]
    fn extract_command_names_none() {
        let mut skills = Vec::new();
        extract_command_names("no tags here", &mut skills);
        assert!(skills.is_empty());
    }

    #[test]
    fn extract_command_names_one() {
        let mut skills = Vec::new();
        extract_command_names("<command-name>commit</command-name>", &mut skills);
        assert_eq!(skills, vec!["commit"]);
    }

    #[test]
    fn extract_command_names_multiple() {
        let mut skills = Vec::new();
        extract_command_names(
            "<command-name>a</command-name> text <command-name>b</command-name>",
            &mut skills,
        );
        assert_eq!(skills, vec!["a", "b"]);
    }

    #[test]
    fn extract_command_names_unclosed_tag() {
        let mut skills = Vec::new();
        extract_command_names("<command-name>broken", &mut skills);
        assert!(skills.is_empty());
    }
}
