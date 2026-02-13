# Brainstorm Summary

**Building:** A Rust TUI dashboard (`loom-tui`) for monitoring Claude Code agent orchestration. Supports real-time debugging, post-mortem analysis, performance monitoring, and development aid. Passive viewer initially with architecture designed for future interactivity.

**Approach:** Elm Architecture with Functional Core - pure functional core (Model/Update/View) with imperative shell for I/O. Tokio event loop with three event sources (file watcher, keyboard, tick). Immutable state, pure update functions, all side effects in shell layer.

**Key Constraints:**
- Must align with FP/DDD principles (pure functions, immutable data, I/O at edges)
- 90%+ of code unit testable without mocks
- Read-only operations initially (navigation, viewing, deletion only)
- Nix flake deployment only (no crates.io publishing)
- Data sources: hook events (events.jsonl), loom state files (task graph, agent logs)

**In Scope:**
- Dashboard view (wave progress river, tasks panel 35%, events panel 65%)
- Agent detail view (tool calls 55%, reasoning 45%)
- Sessions view (historical session browser with table)
- Session deletion (manual cleanup via TUI)
- Hook auto-install (elegant UX on first run, fallback to manual if clunky)
- Full session archiving (indefinite retention in ~/.local/share/loom-tui/)
- Dark theme with status/tool type color coding (no emojis)

**Out of Scope (defer to v2):**
- Auto-detect loom vs non-loom sessions (show different dashboards)
- Comparison mode (side-by-side session comparison)
- Interactive controls (pause/resume tasks, kill agents, start new runs)
- Crates.io publishing

**Open Questions:**
- Session capture mechanics: how to reliably snapshot current loom run to persistent storage?
- Session metadata: what minimal fields needed for sessions table (timestamp, project, duration, status)?
- Hook installation UX: specific flow for elegant auto-install prompt?
- Log rotation for events.jsonl: needed or rely on session archiving?
- Config file or hardcoded paths initially?
