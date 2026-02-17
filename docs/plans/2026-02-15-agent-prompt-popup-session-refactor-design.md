# Agent Prompt Popup + Session Detail Refactor

## Summary

Two related changes:
1. `p` hotkey opens modal popup showing full `task_description` of selected agent
2. Session detail view refactored: static agent table replaced with interactive agent list + per-agent filtered event stream

## 1. Prompt Popup Overlay

**State:** `show_prompt_popup: bool` + `prompt_popup_scroll: usize` on `UiState`.

**Trigger:** `p` in AgentDetail or SessionDetail views when an agent is selected. `Esc` or `p` dismisses. `j/k` scrolls. All other keys ignored while popup open (modal).

**Rendering:** Centered `Clear` + bordered `Paragraph`, ~80% width, ~60% height. Title: agent display name. Content: full `task_description` with word wrap. Shows "No prompt available" if `task_description` is `None`.

**Component:** `src/view/components/prompt_popup.rs` -- pure function `render_prompt_popup(frame, area, agent_name, text, scroll)`.

## 2. Session Detail Refactor

**Left panel (unchanged top, new bottom):**
- Session info block stays (started, duration, event count, agent count)
- Static agent table replaced by shared `agent_list` component (interactive, selectable)

**Right panel:**
- Tool stats panel removed
- Shows per-agent filtered events for the currently selected agent
- When session has zero agents, shows "No agents" placeholder

**Agent selection:** First agent auto-selected when session loads (same pattern as agent detail view). `j/k` on left panel moves selection. No deselect/overview toggle -- right panel always shows selected agent's events.

## 3. Key Handling

| Context | Key | Action |
|---------|-----|--------|
| AgentDetail/SessionDetail, popup closed | `p` | Open popup for selected agent |
| Popup open | `p` / `Esc` | Dismiss popup |
| Popup open | `j` / `k` | Scroll popup content |
| Popup open | anything else | Ignored |
| SessionDetail, left panel focused | `j` / `k` | Move agent selection |
| SessionDetail, no popup | `Esc` | Back to session list |
| SessionDetail | `h` / `l` | Switch panel focus |

## 4. State Changes (`UiState`)

New fields:
- `show_prompt_popup: bool` (default false)
- `prompt_popup_scroll: usize` (default 0)
- `selected_session_agent_index: Option<usize>` (default None, set to Some(0) on session detail entry when agents exist)

## 5. Files Touched

| Order | File | Change |
|-------|------|--------|
| 1 | `src/app/state.rs` | Add 3 new fields to `UiState` |
| 2 | `src/view/components/prompt_popup.rs` | New -- popup render function |
| 3 | `src/view/components/mod.rs` | Register `prompt_popup` module |
| 4 | `src/view/components/agent_list.rs` | Generalize to accept `&[&Agent]` + selected index for reuse |
| 5 | `src/view/session_detail.rs` | Replace agent table with agent list, right panel with filtered events |
| 6 | `src/view/agent_detail.rs` | Call `render_prompt_popup` overlay |
| 7 | `src/app/handle_key.rs` | `p` toggle, popup scroll, session detail agent nav |
| 8 | Footer hints in both views | Add `p:prompt` |

## 6. Code Removed

From `src/view/session_detail.rs`:
- `render_agent_table`
- `render_tool_stats`
- `compute_agent_summary` / `AgentSummary`
- `compute_tool_stats` / `ToolStat`

## Unresolved Questions

None.
