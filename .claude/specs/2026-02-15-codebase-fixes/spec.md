# Spec: Fix Critical + Important Codebase Review Findings

**Source:** GitHub Issue #7 — Codebase review: 5 critical, 27 important findings
**Scope:** 32 findings (C1-C5 critical, I1-I27 important)
**Codebase:** loom-tui, ~9.5k LOC, 36 Rust files, 334 passing tests

## Functional Requirements

### FR-C1: Fix scan_counter overflow
`scan_counter: u32` in `watcher/mod.rs:204` overflows after continuous run. Use counter reset logic.

### FR-C2: Fix scroll_offset u16 truncation
`scroll_offset as u16` silently truncates at 65536 in `event_stream.rs:38,72` + `session_detail.rs:644`. Clamp to `u16::MAX`.

### FR-C3: Handle channel send failures
11x `let _ = tx.send(...)` in `watcher/mod.rs` — watcher threads run forever if main loop dies. Exit thread on send failure.

### FR-C4: Propagate auto_save errors
`auto_save_tick` in `session/mod.rs:287-302` discards save errors. Return `Result`.

### FR-C5: Remove invisible eprintln
`eprintln!` in `main.rs:145,153` invisible in TUI alternate screen. Remove or use error event.

### FR-I1: Remove unused dependencies
tokio + clap declared but unused in Cargo.toml.

### FR-I2: Deduplicate duration_opt_millis
Serde module duplicated in `agent.rs:151` + `session.rs:169`. Extract to shared module.

### FR-I3: Unify format_elapsed
Duplicated 3x in `agent_list.rs:129`, `header.rs:96`, `sessions.rs:164`. Extract shared util.

### FR-I4: Deduplicate calculate_current_wave
Duplicated in `header.rs:108`, `wave_river.rs:108`. Extract to `TaskGraph` method.

### FR-I5: Use thiserror for hook install
`install_hook` returns `Result<(), String>` in `hook_install/mod.rs:110`. Use thiserror enum.

### FR-I6: Remove unused imports
5 unused imports in `update.rs:5`, `event/mod.rs:7`, etc.

### FR-I7: Move hook install to event loop
Hook install side-effect outside Elm update loop in `main.rs:108-119`.

### FR-I8: Cache agent tool counts
O(n) scan of 10k events per agent per frame in `agent_list.rs:89-92`. Cache in DomainState.

### FR-I9: Inject timestamps into update
`Utc::now()` in `update.rs:254,261` breaks functional core purity. Pass timestamp via event.

### FR-I10: Break circular dependency
session <-> app circular dep via `build_archive` in `session/mod.rs:8`, `update.rs:7`.

### FR-I11: Consolidate scroll logic
~200 lines duplicated across 6 functions in `navigation.rs:124-382`. Extract parameterized helper.

### FR-I12: Report corrupt session files
`list_sessions` silently skips corrupt files in `session/mod.rs:189-256`. Return `(results, errors)`.

### FR-I13: Propagate dir creation errors
`create_dir_all().ok()` in `watcher/mod.rs:159`. Propagate error.

### FR-I14: Handle nonexistent watch paths
`watch_path` silently succeeds on nonexistent paths in `watcher/mod.rs:315-320`.

### FR-I15: Separate empty vs error in polling
Transcript polling wildcard catches both empty + I/O errors in `watcher/mod.rs:236-247`.

### FR-I16: Preserve structured errors
`.to_string()` error conversions destroy context in `error.rs:29-43`. Use `#[from]`.

### FR-I17: Detect file truncation
TailState doesn't detect truncation in `watcher/tail.rs:42-58`. Compare file length vs offset.

### FR-I18: Fix paths.rs doc
`Paths.events` doc claims `$TMPDIR`, code hardcodes `/tmp` in `paths.rs:16,40`.

### FR-I19: Fix polling interval doc
`start_transcript_polling` doc says 500ms, code is 200ms/2s in `watcher/mod.rs:197-198`.

### FR-I20: Test recompute_sorted_keys
Sorting logic untested in `state.rs:288-301`.

### FR-I21: Test TaskGraph::new edge cases
Computed fields edge cases untested in `task.rs:13-26`.

### FR-I22: Test build_archive filtering
Session filtering logic has minimal coverage in `session/mod.rs:79-103`.

### FR-I23: Consolidate test suites
Redundant inline + integration test suites.

### FR-I24: Protect model invariants
All model fields `pub` — builders bypassed, invariants unprotected.

### FR-I25: Validate ID newtypes
`AgentId::new("")` accepts empty strings in `ids.rs:11`.

### FR-I26: Add Agent lifecycle enum
No lifecycle state for Agent — active/finished via `Option<DateTime>` in `agent.rs:16`.

### FR-I27: Prevent TaskGraph field desync
`total_tasks`/`completed_tasks` pub + can desync from waves in `task.rs:7-9`.

## Success Criteria

- SC-1: All 334 existing tests pass
- SC-2: No new clippy warnings
- SC-3: Each critical fix has at least 1 new test
- SC-4: cargo build succeeds with no warnings
