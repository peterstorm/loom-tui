# Feature: Multi-Agent Observability TUI

**Spec ID:** 2026-02-11-loom-tui
**Created:** 2026-02-11
**Status:** Clarified
**Owner:** peterstorm

## Summary

Rust TUI dashboard for monitoring Claude Code agent orchestration in real-time and analyzing historical sessions. Provides passive observation interface for developers debugging multi-agent workflows, inspecting task progress, and understanding agent tool usage patterns. Supports both loom-orchestrated sessions (with task graphs) and general Claude Code sessions.

---

## User Scenarios

### US1: [P1] Real-Time Loom Session Monitoring

**As a** developer using loom for multi-agent orchestration
**I want to** view live task progress, wave execution, and agent activity
**So that** I can track work completion and identify bottlenecks

**Why this priority:** Core value proposition - real-time visibility into orchestration

**Acceptance Scenarios:**
- Given active loom session, When I launch loom-tui, Then dashboard shows current wave progress, active tasks, and recent events
- Given task transitions to completed, When state file updates, Then dashboard reflects status change within 500ms
- Given new agent spawns, When SubagentStart event fires, Then agent appears in active agents list
- Given wave completes, When all tasks finish, Then wave progress indicator updates to show completion

### US2: [P1] Agent Activity Inspection

**As a** developer debugging agent behavior
**I want to** drill into individual agent details to see tool calls and reasoning
**So that** I can understand decision-making and identify issues

**Why this priority:** Essential for debugging - primary inspection workflow

**Acceptance Scenarios:**
- Given active agent, When I select agent and press Enter, Then detail view shows tool call timeline and reasoning text
- Given agent completes Bash tool, When transcript updates, Then tool call appears with exit code and duration
- Given agent writes reasoning text, When message appends to transcript, Then reasoning panel updates
- Given Edit tool with large diff, When I view tool call, Then summary shows file path and line counts

### US3: [P1] Historical Session Review

**As a** developer analyzing past orchestration runs
**I want to** browse archived sessions and load them for inspection
**So that** I can compare runs, review failures, and understand patterns

**Why this priority:** Post-mortem analysis required for workflow improvement

**Acceptance Scenarios:**
- Given archived sessions, When I switch to sessions view, Then table shows all sessions with metadata (timestamp, duration, agent count, status)
- Given selected session, When I press Enter, Then dashboard loads that session's data
- Given active session running, When I view sessions list, Then active session is highlighted
- Given old sessions taking space, When I select and delete session, Then data removed from storage

### US4: [P2] Event Stream Analysis

**As a** developer monitoring system activity
**I want to** view chronological stream of all hook events
**So that** I can trace execution flow and spot patterns

**Why this priority:** Useful for understanding timeline, but task/agent views are primary

**Acceptance Scenarios:**
- Given hook events firing, When I view dashboard, Then events panel shows chronological stream
- Given high event volume, When panel scrolls, Then auto-scroll keeps latest visible unless paused
- Given filtering active, When I type "/bash", Then only Bash tool events shown
- Given event selected, When I press Enter, Then full event JSON displayed

### US5: [P2] Navigation and Focus Control

**As a** user interacting with multi-panel interface
**I want to** switch views, focus panels, and scroll independently
**So that** I can efficiently explore different aspects of session

**Why this priority:** Usability requirement for multi-panel interface

**Acceptance Scenarios:**
- Given dashboard view, When I press Tab, Then focus switches between tasks and events panels
- Given focused panel, When I press j/k, Then panel scrolls up/down
- Given any view, When I press 1/2/3, Then view switches to dashboard/agent/sessions
- Given drill-down view, When I press Esc, Then return to previous view

### US6: [P3] Hook Auto-Installation

**As a** first-time user
**I want to** have hooks installed automatically or with minimal friction
**So that** I can start monitoring without manual setup

**Why this priority:** Nice UX improvement, but manual install is acceptable fallback

**Acceptance Scenarios:**
- Given no hooks installed, When I first launch loom-tui, Then prompt offers to install hooks
- Given user accepts, When installation runs, Then hook script copied to `.claude/hooks/send_event.sh`
- Given hooks already installed, When I launch loom-tui, Then no installation prompt shown
- Given installation fails, When error occurs, Then manual installation instructions displayed

### US7: [P3] Display Customization

**As a** user with specific viewing preferences
**I want to** adjust panel layouts and information density
**So that** I can optimize display for my workflow

**Why this priority:** Quality-of-life feature, defer if time-constrained

**Acceptance Scenarios:**
- Given agent detail view, When I press 't', Then reasoning panel toggles visibility
- Given agent detail view, When I press 'w', Then tool calls panel expands to full width
- Given any view, When I press '+' or '-', Then row density increases/decreases
- Given events scrolling, When I press Space, Then auto-scroll pauses/resumes

---

## Functional Requirements

### Core Requirements

- FR-001: System MUST display real-time task status updates from active_task_graph.json
- FR-002: System MUST display real-time agent activity from agent-*.jsonl transcripts
- FR-003: System MUST display hook events from events.jsonl stream
- FR-004: System MUST detect file changes within 500ms of write
- FR-005: System MUST archive completed sessions to persistent storage
- FR-006: System MUST load historical sessions for review
- FR-007: System MUST support navigation between dashboard, agent detail, and sessions views
- FR-008: System MUST allow drilling into agents from dashboard task list
- FR-009: System MUST render without emojis (color and text only)
- FR-010: System MUST delete archived sessions on user request

### Data Requirements

- FR-020: System MUST read task graph from `.claude/state/active_task_graph.json`
- FR-021: System MUST read agent transcripts from `.claude/state/subagents/agent-*.jsonl`
- FR-022: System MUST read hook events from platform-appropriate temp dir: `$TMPDIR/loom-tui/events.jsonl` (macOS/Linux compatible, defaults to `/tmp` when unset)
- FR-023: System MUST watch `/tmp/claude-subagents/*.active` for agent lifecycle
- FR-024: System MUST store archived sessions in `~/.local/share/loom-tui/sessions/`
- FR-025: System MUST persist session metadata: timestamp, duration, status (success/failed/cancelled), agent count, project path, git branch, task count, event count, loom plan ID, wave count, failed task names
- FR-026: System MUST handle missing or malformed state files gracefully

### Display Requirements

- FR-030: System MUST show wave progress indicator on dashboard
- FR-031: System MUST group tasks by wave in task panel
- FR-032: System MUST show task status with visual indicators (pending, running, completed, failed)
- FR-033: System MUST show event stream with timestamps and tool types
- FR-034: System MUST show agent tool calls with duration and result status
- FR-035: System MUST show agent reasoning text between tool calls
- FR-036: System MUST color-code tool types distinctly
- FR-037: System MUST color-code task statuses distinctly
- FR-038: System MUST show active agent count in header
- FR-039: System MUST show elapsed time for active agents
- FR-040: System SHOULD use dark theme with defined color palette

### Navigation Requirements

- FR-050: System MUST support keyboard navigation (no mouse required)
- FR-051: System MUST support view switching with number keys (1/2/3)
- FR-052: System MUST support panel focus switching with Tab or h/l
- FR-053: System MUST support scrolling with j/k or arrow keys
- FR-054: System MUST support drill-down with Enter
- FR-055: System MUST support back navigation with Esc
- FR-056: System MUST support quit with 'q'
- FR-057: System SHOULD support filter/search with '/'
- FR-058: System MAY support help overlay with '?'

### Integration Requirements

- FR-070: System MUST install hook script to `.claude/hooks/send_event.sh`
- FR-071: Hook script MUST append JSON lines to events.jsonl for all event types
- FR-072: Hook script MUST handle SessionStart, SessionEnd, SubagentStart, SubagentStop, PreToolUse, PostToolUse, Stop, Notification, UserPromptSubmit
- FR-073: System SHOULD show non-blocking banner on first run when hooks missing, with 'i' keypress to install; on failure display manual instructions

---

## Non-Functional Requirements

### Performance

- NFR-001: File change detection MUST complete in <500ms (p95)
- NFR-002: View rendering MUST complete in <100ms for standard session (8 tasks, 5 agents, 500 events) at p95
- NFR-003: Session list MUST handle 100+ archived sessions without lag
- NFR-004: Dashboard MUST handle 20+ active agents without performance degradation
- NFR-005: Event stream MUST retain up to 10,000 events in memory (~5MB); oldest events evicted beyond limit

### Reliability

- NFR-010: System MUST NOT crash on malformed state files
- NFR-011: System MUST NOT crash on missing state files
- NFR-012: System MUST handle rapid file updates without race conditions
- NFR-013: System MUST recover from inotify watch errors

### Usability

- NFR-020: Keybindings MUST be shown in view footers
- NFR-021: Status changes MUST be visible without scrolling
- NFR-022: Panel focus MUST be clearly indicated
- NFR-023: Color palette MUST be accessible (sufficient contrast)

---

## Success Criteria

Measurable outcomes that define "done":

- SC-001: Developer can launch loom-tui and see active session within 2 seconds
- SC-002: Task status updates visible within 500ms of state file change
- SC-003: Agent detail view shows all tool calls from transcript with correct timestamps
- SC-004: User can navigate all views without documentation (keybindings in footer)
- SC-005: Historical sessions load and display correctly for sessions up to 1 week old
- SC-006: Zero crashes on valid state files (tested with 10 real loom sessions)
- SC-007: 90%+ of code unit testable without mocks (pure functions for state updates)

**Measurement approach:** Integration tests with fixture state files, manual testing with real loom sessions, unit tests for model/update logic

---

## Out of Scope

Explicitly NOT part of this feature:

- Interactive controls (pause/resume tasks, kill agents, manual task creation)
- Side-by-side session comparison mode
- Auto-detection of loom vs non-loom sessions (treat all as potential loom initially)
- Publishing to crates.io (Nix flake deployment only)
- Custom configuration file (hardcoded paths and colors initially)
- Log rotation for events.jsonl (rely on session archiving)
- Network-based monitoring (local filesystem only)
- Multi-user support or access control
- Performance metrics collection or export
- Integration with external monitoring tools

---

## Resolved Decisions

1. **Session metadata:** Full set — timestamp, duration, status, agent count, project path, git branch, task count, event count, loom plan ID, wave count, failed task names
2. **Hook auto-install UX:** Non-blocking banner at top of dashboard, press 'i' to install; on failure show manual instructions
3. **Events.jsonl:** No rotation; rely on session archiving. In-memory cap of 10,000 events
4. **Performance benchmarks:** Standard session = 8 tasks, 5 agents, 500 events (p95 targets). Stress session = 20 tasks, 15 agents, 5,000 events (no-crash guarantee)
5. **Temp paths:** Use `$TMPDIR/loom-tui/` for cross-platform macOS/Linux support (defaults to `/tmp` when unset)
6. **Session capture:** Periodic auto-save + final snapshot on SessionEnd (resilient to crashes)

---

## Dependencies

External factors this feature depends on:

- Existing `.claude/hooks/` system in Claude Code
- File system watching via inotify (Linux), FSEvents (macOS), or ReadDirectoryChangesW (Windows)
- Rust ecosystem crates: ratatui, tokio, notify, serde_json, chrono, crossterm
- Loom state file format (active_task_graph.json schema)
- Agent transcript format (agent-*.jsonl schema)
- Hook event schema for all event types

---

## Risks

Known risks and mitigation thoughts (not solutions):

| Risk | Impact | Mitigation Direction |
|------|--------|---------------------|
| State file format changes breaking parsing | High | Schema versioning, graceful degradation on unknown fields |
| File watcher resource exhaustion on large sessions | Medium | Limit watch count, poll fallback for overflow |
| Race conditions on rapid file updates | Medium | Debouncing, atomic file reads, schema validation |
| Hook installation failure blocking usage | Low | Manual installation fallback with clear instructions |
| Terminal compatibility issues across platforms | Medium | Stick to standard ANSI sequences, test on major terminals |
| Session archive storage growth unbounded | Low | Document cleanup workflow, provide deletion UI |

---

## Appendix: Glossary

| Term | Definition |
|------|------------|
| Wave | Logical grouping of tasks in loom orchestration, executed in phases |
| Task graph | DAG of tasks with dependencies, status, and wave assignments |
| Agent transcript | JSONL log of agent's tool calls and reasoning messages |
| Hook event | JSON event emitted by Claude Code hook scripts on lifecycle events |
| Session | Single Claude Code orchestration run from start to stop |
| Subagent | Individual agent spawned by loom to work on specific task |
| Tool call | Invocation of a tool (Bash, Read, Edit, etc.) by an agent |

---

## Change Log

| Date | Change | Author |
|------|--------|--------|
| 2026-02-11 | Initial draft | peterstorm |
| 2026-02-11 | Resolved all 11 clarification markers | peterstorm |
