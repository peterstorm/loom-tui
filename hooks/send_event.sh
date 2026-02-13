#!/bin/sh
# loom-tui event hook script
# Receives JSON from Claude Code hooks system on stdin
# Appends event to JSONL file for TUI consumption
# Exit 0 always (passthrough - never block Claude Code)

set -e

# Always use /tmp (not $TMPDIR) — TUI hardcodes /tmp, nix-shell changes $TMPDIR
EVENT_DIR="/tmp/loom-tui"
EVENT_FILE="$EVENT_DIR/events.jsonl"

# Create event directory if missing
mkdir -p "$EVENT_DIR"

# Read hook JSON from stdin
HOOK_JSON=$(cat)

# Extract hook event name from JSON payload (preferred) or env var (fallback)
HOOK_NAME=$(echo "$HOOK_JSON" | jq -r '.hook_event_name // empty' 2>/dev/null || echo "")
if [ -z "$HOOK_NAME" ]; then
  HOOK_NAME="${CLAUDE_HOOK_NAME:-unknown}"
fi

# Build ISO8601 timestamp
TIMESTAMP=$(date -u +"%Y-%m-%dT%H:%M:%SZ")

# Extract common fields
SESSION_ID=$(echo "$HOOK_JSON" | jq -r '.session_id // empty' 2>/dev/null || echo "")
AGENT_ID=$(echo "$HOOK_JSON" | jq -r '.agent_id // empty' 2>/dev/null || echo "")

# Map hook name to TUI event format (snake_case "event" tag + required fields)
case "$HOOK_NAME" in
  PreToolUse|pre-tool-use)
    TOOL_NAME=$(echo "$HOOK_JSON" | jq -r '.tool_name // "unknown"' 2>/dev/null || echo "unknown")
    # Extract meaningful summary from tool_input, varying by tool
    INPUT=$(echo "$HOOK_JSON" | jq -r --arg tn "$TOOL_NAME" '
      .tool_input // {} |
      if $tn == "Edit" then
        (.file_path // "") + "\n" +
        ((.old_string // "" | split("\n") | .[0:30] | map("- " + .) | join("\n")) // "") +
        (if ((.old_string // "" | split("\n") | length) > 30) then "\n  ..." else "" end) + "\n" +
        ((.new_string // "" | split("\n") | .[0:30] | map("+ " + .) | join("\n")) // "") +
        (if ((.new_string // "" | split("\n") | length) > 30) then "\n  ..." else "" end)
      elif $tn == "Write" then
        (.file_path // "") + " (new file)\n" +
        ((.content // "" | split("\n") | .[0:30] | map("+ " + .) | join("\n")) // "") +
        (if ((.content // "" | split("\n") | length) > 30) then "\n  ..." else "" end)
      elif .file_path then .file_path
      elif .command then (.description // .command)
      elif .pattern then .pattern
      elif .prompt then (.description // .prompt)
      elif .query then .query
      elif .url then .url
      elif .skill then .skill
      elif .subject then .subject
      else tostring
      end' 2>/dev/null | head -c 4000)
    jq -cn \
      --arg ts "$TIMESTAMP" \
      --arg sid "$SESSION_ID" \
      --arg aid "$AGENT_ID" \
      --arg tn "$TOOL_NAME" \
      --arg inp "$INPUT" \
      '{timestamp: $ts, event: "pre_tool_use", tool_name: $tn, input_summary: $inp, session_id: (if $sid == "" then null else $sid end), agent_id: (if $aid == "" then null else $aid end)}' \
      >> "$EVENT_FILE"
    ;;
  PostToolUse|post-tool-use)
    TOOL_NAME=$(echo "$HOOK_JSON" | jq -r '.tool_name // "unknown"' 2>/dev/null || echo "unknown")
    # Extract clean human-readable result from tool_response
    RESULT=$(echo "$HOOK_JSON" | jq -r '
      .tool_response // .tool_output // .output // {} |
      if type == "string" then .
      elif type == "object" then
        if .filePath then "ok: " + (.filePath | split("/") | last)
        elif .stdout then (.stdout | split("\n") | map(select(. != "")) | last // "ok")
        elif .content then (.content | if length > 100 then .[0:100] + "..." else . end)
        elif .error then "error: " + .error
        else "ok"
        end
      else tostring
      end' 2>/dev/null | head -c 2000)
    DURATION=$(echo "$HOOK_JSON" | jq -r '.duration_ms // empty' 2>/dev/null || echo "")
    jq -cn \
      --arg ts "$TIMESTAMP" \
      --arg sid "$SESSION_ID" \
      --arg aid "$AGENT_ID" \
      --arg tn "$TOOL_NAME" \
      --arg res "$RESULT" \
      --arg dur "$DURATION" \
      '{timestamp: $ts, event: "post_tool_use", tool_name: $tn, result_summary: $res, duration_ms: (if $dur == "" then null else ($dur | tonumber) end), session_id: (if $sid == "" then null else $sid end), agent_id: (if $aid == "" then null else $aid end)}' \
      >> "$EVENT_FILE"
    ;;
  SubagentStart|subagent-start)
    AGENT_TYPE=$(echo "$HOOK_JSON" | jq -r '.agent_type // empty' 2>/dev/null || echo "")
    TASK_DESC=$(echo "$HOOK_JSON" | jq -r '.task_description // .prompt // empty' 2>/dev/null || echo "")
    jq -cn \
      --arg ts "$TIMESTAMP" \
      --arg sid "$SESSION_ID" \
      --arg aid "$AGENT_ID" \
      --arg at "$AGENT_TYPE" \
      --arg td "$TASK_DESC" \
      '{timestamp: $ts, event: "subagent_start", agent_type: (if $at == "" then null else $at end), task_description: (if $td == "" then null else $td end), session_id: (if $sid == "" then null else $sid end), agent_id: (if $aid == "" then null else $aid end)}' \
      >> "$EVENT_FILE"
    ;;
  SubagentStop|subagent-stop)
    jq -cn \
      --arg ts "$TIMESTAMP" \
      --arg sid "$SESSION_ID" \
      --arg aid "$AGENT_ID" \
      '{timestamp: $ts, event: "subagent_stop", session_id: (if $sid == "" then null else $sid end), agent_id: (if $aid == "" then null else $aid end)}' \
      >> "$EVENT_FILE"
    ;;
  Stop|stop)
    REASON=$(echo "$HOOK_JSON" | jq -r '.reason // empty' 2>/dev/null || echo "")
    jq -cn \
      --arg ts "$TIMESTAMP" \
      --arg sid "$SESSION_ID" \
      --arg aid "$AGENT_ID" \
      --arg r "$REASON" \
      '{timestamp: $ts, event: "stop", reason: (if $r == "" then null else $r end), session_id: (if $sid == "" then null else $sid end), agent_id: (if $aid == "" then null else $aid end)}' \
      >> "$EVENT_FILE"
    ;;
  Notification|notification)
    MESSAGE=$(echo "$HOOK_JSON" | jq -r '.message // ""' 2>/dev/null || echo "")
    jq -cn \
      --arg ts "$TIMESTAMP" \
      --arg sid "$SESSION_ID" \
      --arg aid "$AGENT_ID" \
      --arg msg "$MESSAGE" \
      '{timestamp: $ts, event: "notification", message: $msg, session_id: (if $sid == "" then null else $sid end), agent_id: (if $aid == "" then null else $aid end)}' \
      >> "$EVENT_FILE"
    ;;
  UserPromptSubmit|user-prompt-submit)
    jq -cn \
      --arg ts "$TIMESTAMP" \
      --arg sid "$SESSION_ID" \
      '{timestamp: $ts, event: "user_prompt_submit", session_id: (if $sid == "" then null else $sid end)}' \
      >> "$EVENT_FILE"
    ;;
  session-start|SessionStart)
    CWD=$(echo "$HOOK_JSON" | jq -r '.cwd // empty' 2>/dev/null || echo "")
    jq -cn \
      --arg ts "$TIMESTAMP" \
      --arg sid "$SESSION_ID" \
      --arg cwd "$CWD" \
      '{timestamp: $ts, event: "session_start", session_id: (if $sid == "" then null else $sid end), cwd: (if $cwd == "" then null else $cwd end)}' \
      >> "$EVENT_FILE"
    ;;
  session-end|SessionEnd)
    jq -cn \
      --arg ts "$TIMESTAMP" \
      --arg sid "$SESSION_ID" \
      '{timestamp: $ts, event: "session_end", session_id: (if $sid == "" then null else $sid end)}' \
      >> "$EVENT_FILE"
    ;;
  *)
    # Unknown hook - emit as notification
    jq -cn \
      --arg ts "$TIMESTAMP" \
      --arg sid "$SESSION_ID" \
      --arg aid "$AGENT_ID" \
      --arg hn "$HOOK_NAME" \
      '{timestamp: $ts, event: "notification", message: ("unknown hook: " + $hn), session_id: (if $sid == "" then null else $sid end), agent_id: (if $aid == "" then null else $aid end)}' \
      >> "$EVENT_FILE"
    ;;
esac

# Always exit 0 (passthrough)
exit 0
