#!/bin/sh
# loom-tui event hook script
# Receives JSON from Claude Code hooks system on stdin
# Appends event to JSONL file for TUI consumption
# Exit 0 always (passthrough - never block Claude Code)

set -e

# Resolve temp directory (cross-platform)
TMPDIR="${TMPDIR:-/tmp}"
EVENT_DIR="$TMPDIR/loom-tui"
EVENT_FILE="$EVENT_DIR/events.jsonl"

# Create event directory if missing
mkdir -p "$EVENT_DIR"

# Read hook JSON from stdin
HOOK_JSON=$(cat)

# Extract hook name from environment (Claude Code sets this)
HOOK_NAME="${CLAUDE_HOOK_NAME:-unknown}"

# Extract session_id if present in JSON
SESSION_ID=$(echo "$HOOK_JSON" | jq -r '.session_id // empty' 2>/dev/null || echo "")

# Extract agent_id if present in JSON
AGENT_ID=$(echo "$HOOK_JSON" | jq -r '.subagent_id // .agent_id // empty' 2>/dev/null || echo "")

# Map hook name to event kind and extract relevant fields
case "$HOOK_NAME" in
  session-start)
    EVENT_KIND="SessionStart"
    ;;
  session-end)
    EVENT_KIND="SessionEnd"
    ;;
  subagent-start)
    EVENT_KIND="SubagentStart"
    TASK_DESC=$(echo "$HOOK_JSON" | jq -r '.task_description // empty' 2>/dev/null || echo "")
    ;;
  subagent-stop)
    EVENT_KIND="SubagentStop"
    ;;
  pre-tool-use)
    EVENT_KIND="PreToolUse"
    TOOL_NAME=$(echo "$HOOK_JSON" | jq -r '.tool_name // empty' 2>/dev/null || echo "")
    ;;
  post-tool-use)
    EVENT_KIND="PostToolUse"
    TOOL_NAME=$(echo "$HOOK_JSON" | jq -r '.tool_name // empty' 2>/dev/null || echo "")
    DURATION=$(echo "$HOOK_JSON" | jq -r '.duration_ms // empty' 2>/dev/null || echo "")
    ;;
  stop)
    EVENT_KIND="Stop"
    REASON=$(echo "$HOOK_JSON" | jq -r '.reason // empty' 2>/dev/null || echo "")
    ;;
  notification)
    EVENT_KIND="Notification"
    MESSAGE=$(echo "$HOOK_JSON" | jq -r '.message // empty' 2>/dev/null || echo "")
    ;;
  user-prompt-submit)
    EVENT_KIND="UserPromptSubmit"
    ;;
  *)
    # Unknown hook - still capture it
    EVENT_KIND="Unknown"
    ;;
esac

# Build event JSON line with ISO8601 timestamp
TIMESTAMP=$(date -u +"%Y-%m-%dT%H:%M:%SZ")

# Construct JSON event (using jq for proper escaping)
jq -n \
  --arg ts "$TIMESTAMP" \
  --arg kind "$EVENT_KIND" \
  --arg sid "$SESSION_ID" \
  --arg aid "$AGENT_ID" \
  --argjson raw "$HOOK_JSON" \
  '{
    timestamp: $ts,
    kind: $kind,
    session_id: (if $sid == "" then null else $sid end),
    agent_id: (if $aid == "" then null else $aid end),
    raw: $raw
  }' >> "$EVENT_FILE"

# Always exit 0 (passthrough)
exit 0
