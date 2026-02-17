# Codebase Review — loom-tui (main)

**Date:** 2026-02-14
**Scope:** Full codebase, ~9.5k LOC, 36 Rust files, 334 passing tests
**Reviewers:** 6 opus agents (code, errors, tests, types, comments, architecture)

---

## Critical Issues (must fix)

| # | Agent | Issue | Location |
|---|-------|-------|----------|
| C1 | code | `scan_counter: u32` overflows after ~9.9 days continuous run | `watcher/mod.rs:204` |
| C2 | code | `scroll_offset as u16` silently truncates at 65536, scroll jumps to 0 | `event_stream.rs:38`, `session_detail.rs:644` |
| C3 | errors | 14x `let _ = tx.send(...)` — watcher threads silently run forever if main loop dies | `watcher/mod.rs` (14 sites) |
| C4 | errors | `auto_save_tick` discards save errors — user loses session data silently | `session/mod.rs:287-302` |
| C5 | errors | `eprintln!` invisible in TUI alternate screen mode | `main.rs:145,153` |

---

## Important Issues (should fix)

| # | Agent | Issue | Location |
|---|-------|-------|----------|
| I1 | code | Unused deps: `tokio` (full) + `clap` — slow compiles for nothing | `Cargo.toml:8-9,16` |
| I2 | code+arch | `duration_opt_millis` serde module duplicated | `agent.rs:151`, `session.rs:169` |
| I3 | code | `format_elapsed` duplicated 3x with different signatures | `agent_list.rs:129`, `header.rs:96`, `sessions.rs:164` |
| I4 | code | `calculate_current_wave` duplicated 2x | `header.rs:110`, `wave_river.rs:109` |
| I5 | code | `install_hook` returns `Result<(), String>` — stringly-typed | `hook_install/mod.rs:110` |
| I6 | code | 5 unused imports producing compiler warnings | `update.rs:5`, `event/mod.rs:7`, etc. |
| I7 | code+arch | Hook install side-effect outside Elm Architecture update loop | `main.rs:108-119` |
| I8 | code | Agent tool count: O(n) scan of 10k events per agent per frame | `agent_list.rs:89-92` |
| I9 | arch | `Utc::now()` in `update.rs:254,261` breaks functional core purity | `update.rs:254,261` |
| I10 | arch | Circular dep: `session` <-> `app` via `build_archive` | `session/mod.rs:8`, `update.rs:7` |
| I11 | arch | Navigation scroll logic ~200 lines duplicated across 6 functions | `navigation.rs:124-382` |
| I12 | errors | `list_sessions`/`list_session_metas` silently skip corrupt files | `session/mod.rs:189-256` |
| I13 | errors | `create_dir_all().ok()` — events dir setup silently fails | `watcher/mod.rs:159` |
| I14 | errors | `watch_path` silently succeeds on nonexistent paths | `watcher/mod.rs:315-320` |
| I15 | errors | Transcript polling wildcard `_` catches both empty + errors | `watcher/mod.rs:236-247` |
| I16 | errors | `.to_string()` error conversions destroy error kind/context | `error.rs:29-43` |
| I17 | errors | `TailState` doesn't detect file truncation/rotation — permanent data loss | `watcher/tail.rs:42-58` |
| I18 | comments | `Paths.events` doc claims `$TMPDIR` usage, code hardcodes `/tmp` | `paths.rs:16,40` |
| I19 | comments | `start_transcript_polling` doc says "500ms", code is 200ms/2s | `watcher/mod.rs:197-198` |
| I20 | tests | `recompute_sorted_keys` sorting logic untested | `state.rs:288-301` |
| I21 | tests | `TaskGraph::new` computed fields edge cases untested | `task.rs:13-26` |
| I22 | tests | `build_archive` session filtering logic has minimal coverage | `session/mod.rs:79-103` |
| I23 | tests | Redundant test suites (inline + integration) — 112 navigation, 84 update tests with heavy overlap | `navigation.rs`, `tests/navigation_tests.rs` |
| I24 | types | All model fields `pub` — builders exist but bypassed, invariants unprotected | `agent.rs`, `task.rs`, `session.rs` |
| I25 | types | ID newtypes accept empty strings — `AgentId::new("")` used in production | `ids.rs:11`, `agent.rs:29` |
| I26 | types | No lifecycle state enum for Agent — active/finished via `Option<DateTime>` | `agent.rs:16` |
| I27 | types | `TaskGraph.total_tasks`/`completed_tasks` are `pub` and can desync from waves | `task.rs:7-9` |

---

## Advisory (nice to have)

- `pub use parsers::*` glob re-export (`watcher/mod.rs:4`)
- `known_files` BTreeMap grows unbounded in transcript polling (`watcher/mod.rs:208`)
- 3 polling threads have no shutdown mechanism (`watcher/mod.rs:169-186`)
- `TranscriptUpdated` silently drops messages for unknown agents (`update.rs:28-30`)
- `raw: Value` escape hatch in `HookEvent` undermines type safety (`hook_event.rs:17`)
- `lib.rs:1` stale scaffolding comment: "will be populated by subsequent tasks"
- ~9 unnecessary restating comments in `main.rs`
- Excessive per-function "Pure function" / "Functional Core" annotations in `session/mod.rs`
- `detect_hook` doc says "Pure I/O boundary function" — contradictory terminology
- `project_path: String` should be `PathBuf` in multiple locations
- `errors: VecDeque<String>` should store structured `LoomError`
- Trivial tests in `main.rs` (e.g., `assert!(Duration::from_millis(250).as_millis() == 250)`)
- `ToolName` should be an enum since `tool_color` already exhaustively matches known names

---

## Architecture Assessment

**Testability Score: ~82%** — strong for a TUI app. Functional core is genuinely pure (with 2 exceptions). I/O properly at edges.

```
main.rs (shell) -> watcher (I/O) -> events -> update (core) -> state -> model
                                                                  |
main.rs (shell) -> view (render) <- state <- model
```

**Top 3 architecture priorities:**
1. Eliminate `Utc::now()` from `update.rs` — make it truly `(State, Event) -> State`
2. Extract scroll logic to collapse 6 functions (~260 LOC) into ~60 LOC
3. Break `session` <-> `app` circular dependency

---

## Type Design Ratings

| Type | Encapsulation | Expression | Usefulness | Enforcement | Avg |
|------|:---:|:---:|:---:|:---:|:---:|
| ID Newtypes (ids.rs) | 6 | 4 | 8 | 3 | 5.3 |
| Agent (agent.rs) | 3 | 3 | 6 | 2 | 3.5 |
| TaskGraph/Task (task.rs) | 4 | 7 | 8 | 4 | 5.8 |
| SessionMeta/Archive (session.rs) | 3 | 5 | 7 | 3 | 4.5 |
| HookEvent/Kind (hook_event.rs) | 5 | 7 | 8 | 5 | 6.3 |
| AppState (state.rs) | 5 | 5 | 7 | 4 | 5.3 |
| AppEvent (event/mod.rs) | 7 | 6 | 8 | 5 | 6.5 |
| Errors (error.rs) | 7 | 5 | 7 | 5 | 6.0 |
| Theme (theme.rs) | 8 | 6 | 7 | 5 | 6.5 |
| Kanban types (kanban.rs) | 7 | 6 | 7 | 5 | 6.3 |

**Top 5 systemic type issues:**
1. All model fields `pub` — builders exist but bypassed, invariants unprotected
2. Primitive obsession with `String` in ~10 places where newtypes/PathBuf needed
3. ID newtypes accept empty strings with no validation
4. Computed fields stored as `pub` data (can desync)
5. No lifecycle state machine — Agent active/finished via `Option<DateTime>`

---

## Test Coverage Assessment

**334 tests, 0 failures.** Strong coverage of pure functions (update, navigation, parsers, models).

**Key gaps:**
- `recompute_sorted_keys` sorting logic (state.rs)
- `TaskGraph::new` computed fields edge cases
- `build_archive` session filtering
- `drill_down` for active sessions
- `SessionDetail` single-step scroll navigation

**Quality issues:**
- Trivial tests in main.rs that verify stdlib behavior
- Redundant inline + integration test suites (navigation, update)

---

## Strengths

- Clean Elm Architecture with proper FC/IS separation
- 334 tests, zero failures, strong coverage of pure logic
- Proper thiserror enum hierarchy
- Newtype IDs preventing cross-domain substitution
- Ring buffer eviction, incremental file tailing, cache dirty flags
- AppState decomposition into UiState/DomainState/AppMeta/CacheState
- View layer is read-only — no business logic leaks into rendering
- Agent attribution logic is exhaustively tested with 4-step strategy
- Session lifecycle (start, track, end, archive) has end-to-end coverage
- UTF-8 safety in `truncate_str` tested with CJK/emoji

---

### Machine Summary
CRITICAL_COUNT: 5
ADVISORY_COUNT: 22 important + 13 advisory
CRITICAL: scan_counter u32 overflow after ~9.9 days
CRITICAL: scroll_offset as u16 truncation at 65536
CRITICAL: 14x let _ = tx.send() silently drops channel failures across watcher
CRITICAL: auto_save_tick discards save errors silently
CRITICAL: eprintln invisible in TUI alternate screen
ADVISORY: unused deps tokio+clap, DRY violations (duration_opt_millis, format_elapsed, calculate_current_wave, scroll logic), Utc::now() in functional core, circular session<->app dep, stringly-typed hook install error, unused imports, O(n) tool count per frame, corrupt sessions silently skipped, create_dir_all().ok(), watch_path silent on nonexistent paths, error context loss via .to_string(), TailState no truncation detection, stale doc comments, pub model fields, empty ID newtypes, no agent lifecycle enum, desyncable computed fields
