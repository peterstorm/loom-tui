# Plan: Fix Critical + Important Codebase Review Findings

**Source:** GitHub Issue #7
**Scope:** 32 findings (C1-C5, I1-I27)
**Codebase:** loom-tui, ~9.5k LOC, 36 Rust files, 334 passing tests

## Summary

Fix all critical (5) and important (27) findings from codebase review #7. Organized into 5 waves based on file-conflict avoidance and dependency ordering. Error type changes land first (wave 1) since downstream fixes depend on them. DRY extractions (wave 2) create shared utilities consumed by later waves. Critical runtime bugs (wave 3) use the new error types and shared code. Architecture/purity fixes (wave 4) are independent of earlier work but may touch same files. Tests and docs (wave 5) finalize coverage.

## Architectural Decisions

### AD-1: Error type restructuring (I5, I16)

**Chosen:** Preserve `WatcherError::Notify(String)` and `WatcherError::Io(String)` as-is (they must be `Clone` for `AppEvent`). Add `HookInstallError` thiserror enum for `install_hook`. For `LoomError`, replace `Session(String)` with `Session(#[from] SessionError)` -- but `SessionError` contains `std::io::Error` (not `Clone`), so wrap it: store `SessionError` as `Session(String)` still but include the original error's kind. Alternative rejected: making all errors `Arc`-wrapped -- too invasive for this scope.

**Rationale:** `notify::Error` and `std::io::Error` don't implement `Clone`. The event channel requires `Clone` on `LoomError`. Keeping string wrappers at the Clone boundary is pragmatic. The real fix is `HookInstallError` which has no Clone constraint.

### AD-2: Shared serde module location (I2)

**Chosen:** `src/model/serde_utils.rs` with `pub mod duration_opt_millis`. Both `agent.rs` and `session.rs` reference it via `crate::model::serde_utils::duration_opt_millis`.

**Rationale:** Both files are in the model layer. A shared module there avoids cross-layer imports.

### AD-3: Shared format_elapsed location (I3, I4)

**Chosen:** Add `current_wave()` method to `TaskGraph` in `task.rs`. Add `pub fn format_elapsed(secs: i64) -> String` and `pub fn format_duration(d: Option<Duration>) -> String` in a new `src/view/components/format.rs`. Views import from there.

**Rationale:** `current_wave` is domain logic belonging on the type. `format_elapsed` is presentation logic belonging in the view layer.

### AD-4: Navigation scroll deduplication (I11)

**Chosen:** Extract `fn active_scroll_offset(state: &AppState) -> &usize` / `fn active_scroll_offset_mut(state: &mut AppState) -> &mut usize` and `fn item_count(state: &AppState) -> Option<usize>` to collapse `scroll_down/up/page_down/page_up/jump_top/bottom` from ~260 LOC to ~80 LOC.

**Rationale:** All 6 functions follow the same pattern: resolve which scroll offset/selection to modify based on `(view, focus)`, then apply arithmetic. The view+focus dispatch is the common code.

### AD-5: auto_save_tick return type (C4)

**Chosen:** Change `auto_save_tick` to return `Result<Option<Instant>, SessionError>`. Caller in `main.rs` event loop handles the error via the error ring buffer.

**Rationale:** The 3-state enum alternative adds a new type for one call site. `Result<Option<_>>` is idiomatic -- None means "didn't save (interval not reached)", Some means "saved", Err means "save failed".

### AD-6: build_archive circular dep (I10)

**Chosen:** Keep `build_archive` in `session/mod.rs` but change its signature from `(&DomainState, &SessionMeta)` to take explicit params `(task_graph: Option<&TaskGraph>, events: &VecDeque<HookEvent>, agents: &BTreeMap<AgentId, Agent>, meta: &SessionMeta)`. Remove `use crate::app::state::DomainState` from session module. Callers pass fields explicitly.

**Rationale:** Moving to model layer adds a dependency on `VecDeque<HookEvent>` and `BTreeMap<AgentId, Agent>` which are app-layer aggregate shapes. Parameterizing breaks the circular dep without restructuring. The function remains pure.

### AD-7: Agent lifecycle enum (I26) -- defer

**Chosen:** Defer I26. Adding `AgentLifecycle { Active { started_at }, Finished { started_at, finished_at } }` touches 30+ sites that read `finished_at`. Too invasive for this fix batch.

**Rationale:** Risk/reward ratio is poor. The current `Option<DateTime>` pattern works and is tested. Flag for future refactor.

### AD-8: Model field visibility (I24) -- defer

**Chosen:** Defer I24 (making model fields private). Too many call sites (~100+) directly access `pub` fields on `Agent`, `SessionMeta`, `TaskGraph`, etc. Would require getters/setters everywhere.

**Rationale:** The builders exist and are used for construction. Direct field access is Rust-idiomatic for value objects. Invariant protection via `TaskGraph::new()` computed fields (I27) is the targeted fix.

### AD-9: scan_counter overflow (C1)

**Chosen:** Replace `scan_counter += 1` with `scan_counter = scan_counter.wrapping_add(1)`. The `% 10 == 1` check works correctly with wrapping arithmetic since the modulo operation cares only about the remainder.

**Rationale:** Simplest fix. `wrapping_add` is explicit about intent. No behavioral change.

### AD-10: Channel send failure handling (C3)

**Chosen:** For `let _ = tx.send(...)` in watcher callbacks and `load_existing_files`, replace with `if tx.send(...).is_err() { return; }` where the function can exit, and log-ignore where it cannot (watcher callback closure). The key zombie-thread sites are the polling loops and tail worker -- those already handle send failures correctly. Focus on `handle_watch_event`, `handle_task_graph_update`, `handle_transcript_update`, and `load_existing_files`.

**Rationale:** Watcher callback runs in notify's thread -- cannot "return" from it. But we can break out of loop-based polling threads. The tail worker already returns on send failure (line 62-63). The main risk is `handle_task_graph_update` and `handle_transcript_update` silently continuing after channel close.

## File Structure

### Wave 1: Error types + unused deps/imports (foundation)
**Task 1.1: Remove unused deps + imports (I1, I6)**
- `Cargo.toml` -- remove `tokio` and `clap`
- `src/app/update.rs` -- remove unused imports (line 5)
- `src/event/mod.rs` -- remove unused imports (line 7)
- Other files with unused imports (check `cargo build` warnings)

**Task 1.2: Error type fixes (I5, I16)**
- `src/error.rs` -- add `HookInstallError` enum; keep `WatcherError` Clone-compatible wrappers
- `src/hook_install/mod.rs` -- change `install_hook` return type to `Result<(), HookInstallError>`
- `src/app/state.rs` -- change `HookStatus::InstallFailed(String)` to `HookStatus::InstallFailed(HookInstallError)`
- `src/main.rs` -- update error handling for new `HookInstallError` type

### Wave 2: DRY extractions (shared code)
**Task 2.1: Extract shared serde + format utilities (I2, I3, I4)**
- `src/model/serde_utils.rs` (new) -- shared `duration_opt_millis` module
- `src/model/mod.rs` -- add `pub mod serde_utils`
- `src/model/agent.rs` -- replace inline `duration_opt_millis` with `crate::model::serde_utils::duration_opt_millis`
- `src/model/session.rs` -- replace inline `duration_opt_millis` with `crate::model::serde_utils::duration_opt_millis`
- `src/model/task.rs` -- add `pub fn current_wave(&self) -> u32` method to `TaskGraph`
- `src/view/components/format.rs` (new) -- shared `format_elapsed(secs: i64) -> String`, `format_duration(d: Option<Duration>) -> String`
- `src/view/components/mod.rs` -- add `pub mod format`
- `src/view/components/agent_list.rs` -- use shared `format_elapsed`
- `src/view/components/header.rs` -- use shared `format_elapsed` and `TaskGraph::current_wave()`
- `src/view/components/wave_river.rs` -- use `TaskGraph::current_wave()`
- `src/view/sessions.rs` -- use shared `format_duration`
- `src/view/session_detail.rs` -- use shared `format_duration`

**Task 2.2: Break session<->app circular dep (I10)**
- `src/session/mod.rs` -- change `build_archive` to take explicit params, remove `use crate::app::state::DomainState`
- `src/app/update.rs` -- update `build_archive` call sites to pass fields explicitly

### Wave 3: Critical runtime fixes
**Task 3.1: Watcher critical fixes (C1, C3, I13, I14, I15)**
- `src/watcher/mod.rs` -- fix `scan_counter` overflow (wrapping_add), propagate `create_dir_all` error, fix `watch_path` to warn on nonexistent, separate empty vs error in transcript polling, fix channel send failures in `handle_task_graph_update`/`handle_transcript_update`/`load_existing_files`

**Task 3.2: Session + TUI critical fixes (C2, C4, C5)**
- `src/session/mod.rs` -- change `auto_save_tick` to return `Result<Option<Instant>, SessionError>`
- `src/view/components/event_stream.rs` -- clamp `scroll_offset as u16` to `u16::MAX`
- `src/view/session_detail.rs` -- clamp `scroll_offset as u16` to `u16::MAX`
- `src/main.rs` -- remove `eprintln!` calls (C5), handle `auto_save_tick` errors

**Task 3.3: TailState truncation detection (I17)**
- `src/watcher/tail.rs` -- compare file length vs stored offset; reset if file shrank (truncation/rotation)

### Wave 4: Architecture + error handling improvements
**Task 4.1: Functional core purity + hook install to event loop (I7, I9)**
- `src/app/update.rs` -- remove `Utc::now()` calls in `AgentStarted`/`AgentStopped` handlers; use timestamp from event
- `src/event/mod.rs` -- add `DateTime<Utc>` field to `AgentStarted`/`AgentStopped` variants
- `src/main.rs` -- move hook install side-effect into event loop as `AppEvent::InstallHookRequested` pattern (or keep outside but route through update)

**Task 4.2: Navigation scroll consolidation (I11)**
- `src/app/navigation.rs` -- extract `active_scroll_offset`/`active_scroll_offset_mut`/`item_count` helpers; collapse 6 scroll functions

**Task 4.3: Session error reporting + agent tool count cache (I8, I12)**
- `src/session/mod.rs` -- change `list_sessions`/`list_session_metas` to return `(Vec<_>, Vec<SessionError>)` tuple
- `src/app/state.rs` -- add `agent_tool_counts: BTreeMap<AgentId, usize>` to `CacheState`
- `src/app/update.rs` -- maintain `agent_tool_counts` cache on `PostToolUse` events; update `list_session_metas` call site
- `src/view/components/agent_list.rs` -- read tool count from cache instead of O(n) scan
- `src/watcher/mod.rs` -- handle errors from `list_session_metas` tuple return

### Wave 5: Tests + docs
**Task 5.1: Fix docs (I18, I19)**
- `src/paths.rs` -- fix `events` doc comment to say `/tmp` not `$TMPDIR`
- `src/watcher/mod.rs` -- fix `start_transcript_polling` doc to say 200ms/2s not 500ms

**Task 5.2: Add missing tests (I20, I21, I22)**
- `src/app/state.rs` -- add tests for `recompute_sorted_keys`: active-first ordering, empty state, all-finished, mixed
- `src/model/task.rs` -- add tests for `TaskGraph::new` edge cases: empty waves, all completed, all pending, mixed statuses, single task
- `src/session/mod.rs` -- add tests for `build_archive`: filters by session_id, excludes other session's agents/events, handles empty domain

**Task 5.3: Type safety fixes (I25, I27)**
- `src/model/ids.rs` -- add validation to `id_newtype!` macro: panic or return `Result` on empty string for `AgentId`/`SessionId`/`TaskId`
- `src/model/task.rs` -- make `total_tasks`/`completed_tasks` private, expose via getters; ensure they can only be set via `TaskGraph::new()`
- `src/model/agent.rs` -- update `Agent::default()` to not use `AgentId::new("")`

**Task 5.4: Consolidate test suites (I23)**
- Review inline vs integration test overlap; remove redundant integration tests that duplicate inline tests
- This is a cleanup task -- no new functionality

## Component Design

### Error types boundary (Wave 1)
`HookInstallError` is a new thiserror enum in `src/error.rs`:
```
HookInstallError { CreateDir { path, source: io::Error }, WriteScript { path, source: io::Error }, SetPermissions { path, source: io::Error } }
```
`HookStatus::InstallFailed` changes from `String` to `HookInstallError`. Since `HookInstallError` contains `io::Error` (not Clone), and `HookStatus` derives `Clone` + `PartialEq`, change `HookInstallError` variants to store `String` descriptions. Alternative: make `HookInstallError` Clone by storing error message strings -- chosen for simplicity.

Actually, reviewing `HookStatus` in `state.rs:143-156` -- it already stores `String`. Keep it as `InstallFailed(String)` but the *creation* site uses the new structured error. The error flows: `install_hook() -> Result<(), HookInstallError>` -> caller does `.map_err(|e| e.to_string())` -> stores in `HookStatus::InstallFailed(String)`. This way the function signature is clean but the storage remains Clone-compatible.

### Shared utilities boundary (Wave 2)
- `model::serde_utils` -- pure serde module, no dependencies
- `view::components::format` -- pure formatting functions, depends only on std
- `TaskGraph::current_wave()` -- pure method on model type, no new deps
- `build_archive` -- pure function, depends only on model types (no app state import)

### Cache extension boundary (Wave 4)
`CacheState.agent_tool_counts: BTreeMap<AgentId, usize>` -- maintained in `update.rs` on `PostToolUse` events. Incremented atomically. Read in `agent_list.rs` via `state.agent_tool_count(id)` accessor on `AppState`.

## Implementation Phases

### Phase 1 (Wave 1): Foundation -- error types + cleanup
- **Task 1.1:** Remove unused deps + imports (I1, I6)
  - depends_on: none
  - files: `Cargo.toml`, `src/app/update.rs`, `src/event/mod.rs`
- **Task 1.2:** Error type fixes (I5, I16)
  - depends_on: none
  - files: `src/error.rs`, `src/hook_install/mod.rs`, `src/main.rs`, `src/app/state.rs`

### Phase 2 (Wave 2): Shared code extraction
- **Task 2.1:** Extract serde + format + current_wave (I2, I3, I4)
  - depends_on: none
  - files: `src/model/serde_utils.rs` (new), `src/model/mod.rs`, `src/model/agent.rs`, `src/model/session.rs`, `src/model/task.rs`, `src/view/components/format.rs` (new), `src/view/components/mod.rs`, `src/view/components/agent_list.rs`, `src/view/components/header.rs`, `src/view/components/wave_river.rs`, `src/view/sessions.rs`, `src/view/session_detail.rs`
- **Task 2.2:** Break session<->app circular dep (I10)
  - depends_on: none
  - files: `src/session/mod.rs`, `src/app/update.rs`

### Phase 3 (Wave 3): Critical runtime fixes
- **Task 3.1:** Watcher critical fixes (C1, C3, I13, I14, I15)
  - depends_on: Task 1.2 (uses error types)
  - files: `src/watcher/mod.rs`
- **Task 3.2:** Session + TUI critical fixes (C2, C4, C5)
  - depends_on: Task 1.2 (error types), Task 2.1 (format_duration shared)
  - files: `src/session/mod.rs`, `src/view/components/event_stream.rs`, `src/view/session_detail.rs`, `src/main.rs`
- **Task 3.3:** TailState truncation detection (I17)
  - depends_on: none
  - files: `src/watcher/tail.rs`

### Phase 4 (Wave 4): Architecture improvements
- **Task 4.1:** Functional core purity + hook install event (I7, I9)
  - depends_on: Task 1.2 (error types), Task 2.2 (build_archive signature)
  - files: `src/app/update.rs`, `src/event/mod.rs`, `src/main.rs`
- **Task 4.2:** Navigation scroll consolidation (I11)
  - depends_on: none
  - files: `src/app/navigation.rs`
- **Task 4.3:** Session error reporting + tool count cache (I8, I12)
  - depends_on: Task 2.2 (session module changes)
  - files: `src/session/mod.rs`, `src/app/state.rs`, `src/app/update.rs`, `src/view/components/agent_list.rs`, `src/watcher/mod.rs`

### Phase 5 (Wave 5): Tests + docs + type safety
- **Task 5.1:** Fix docs (I18, I19)
  - depends_on: none
  - files: `src/paths.rs`, `src/watcher/mod.rs`
- **Task 5.2:** Add missing tests (I20, I21, I22)
  - depends_on: Task 2.1 (TaskGraph::current_wave), Task 2.2 (build_archive new signature)
  - files: `src/app/state.rs`, `src/model/task.rs`, `src/session/mod.rs`
- **Task 5.3:** Type safety fixes (I25, I27)
  - depends_on: none
  - files: `src/model/ids.rs`, `src/model/task.rs`, `src/model/agent.rs`
- **Task 5.4:** Consolidate test suites (I23)
  - depends_on: Task 5.2 (new tests added first)
  - files: various test files (review-only task)

## Testing Strategy

### Per-component testing

**Error types (Task 1.2):**
- Unit: `HookInstallError` Display impl produces readable messages
- Unit: `install_hook` returns structured error on permission failure
- Existing tests in `hook_install/mod.rs` continue passing

**Shared utilities (Task 2.1):**
- Unit: `duration_opt_millis` round-trip serialization (move existing tests)
- Unit: `format_elapsed` edge cases (0, 59, 60, 3600, negative)
- Unit: `format_duration` None and boundary values
- Unit: `TaskGraph::current_wave()` -- reuse existing `calculate_current_wave` tests

**Circular dep break (Task 2.2):**
- Unit: `build_archive` filters events by session_id
- Unit: `build_archive` filters agents by session_id
- Unit: `build_archive` with empty domain state
- Unit: `build_archive` includes task_graph when present

**Watcher fixes (Task 3.1):**
- Unit: `scan_counter` wraps at u32::MAX (verify `% 10` still works after wrap)
- Unit: `watch_path` on nonexistent path returns Ok but warns
- Integration: channel send failure causes thread exit (verify with dropped receiver)

**Session + TUI fixes (Task 3.2):**
- Unit: `auto_save_tick` returns error on save failure (using tempdir with read-only perms)
- Unit: scroll offset clamping at u16::MAX boundary
- Verify: no `eprintln!` calls remain in codebase

**TailState truncation (Task 3.3):**
- Unit: file truncated to shorter length resets offset and re-reads from start
- Unit: file replaced with different content detected via size comparison
- Unit: normal append still works after truncation detection added

**Purity fixes (Task 4.1):**
- Unit: `AgentStarted`/`AgentStopped` events use provided timestamp, not `Utc::now()`
- Verify: no `Utc::now()` calls in `update.rs`

**Navigation consolidation (Task 4.2):**
- All existing navigation tests pass unchanged (behavioral parity)
- No new tests needed -- this is a refactor

**Tool count cache (Task 4.3):**
- Unit: tool count incremented on `PostToolUse` event
- Unit: tool count accessible via `AppState` accessor
- Unit: tool count 0 for agent with no tool events

**Type safety (Task 5.3):**
- Unit: `AgentId::new("")` panics (or returns Err if using fallible constructor)
- Unit: `TaskGraph` `total_tasks`/`completed_tasks` not publicly settable
- Unit: `TaskGraph::new` computes correct totals (covered by I21 tests)

## Verification

After all waves complete:
1. `cargo build` -- zero warnings
2. `cargo clippy` -- zero warnings
3. `cargo test` -- all 334+ tests pass (new tests added)
4. Each critical fix (C1-C5) has at least 1 new test
5. No `eprintln!` in non-test code
6. No `Utc::now()` in `update.rs`
7. No `use crate::app` in `session/mod.rs`
8. `duration_opt_millis` exists in exactly 1 location
9. `format_elapsed` exists in exactly 1 location (view layer)
10. `calculate_current_wave` exists as `TaskGraph::current_wave()` only

## Deferred Items

- **I24** (pub model fields) -- too many call sites, low ROI
- **I26** (Agent lifecycle enum) -- touches 30+ sites, separate PR
- **I23** (test consolidation) -- included as Task 5.4 but low priority; skip if time constrained
- All Advisory items from Issue #7 -- out of scope for this plan
