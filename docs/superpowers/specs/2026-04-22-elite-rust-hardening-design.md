# Elite Rust Hardening: Top 5 Quality Improvements

**Date:** 2026-04-22
**Branch:** `elite-rust-polish`
**Scope:** Internal code quality — no user-facing behavior changes

## Overview

Five architectural improvements to bring the codebase from "strong" to "elite" Rust quality. All changes are internal refactors that preserve existing behavior. Ordered by dependency: domain layer first, then detection, then app layer.

---

## 1. Domain Error Types and Validation

**Goal:** Make the `nexus-domain` crate's types trustworthy — validated at construction, explicit on failure.

### Changes

**`ProfileId` validation:**
- `ProfileId::new()` returns `Result<Self, ProfileIdError>`
- Rejects empty strings and whitespace-only strings
- Add `ProfileIdError` enum with `Empty` variant
- Keep a `pub(crate) fn new_unchecked(id: String) -> Self` escape hatch for internal construction from known-good data (e.g., deserialization from storage where IDs were validated at write time)

**`FromStr` for parse types:**
- `TodoStatus`: replace `parse_str(&str) -> Self` with `FromStr` returning `Result<Self, ParseError>`
- `TodoPriority`: same treatment
- Callers that need the old silent-default behavior use `.unwrap_or_default()`

**`#[non_exhaustive]` on public enums:**
- `Provider`, `ProviderFilter`, `PrimaryStatus`, `SortMode`, `LimitState`, `Confidence`, `TodoStatus`, `TodoPriority`, `NormalizedStatus`, `NormalizedReason`
- This is a semver-compatible addition since the crate is `publish = false`

**Move `EditableProfile` out of domain:**
- From `crates/nexus-domain/src/accounts/mod.rs` to `src/app/state/accounts.rs`
- It is a UI editing form, not a domain concept

**Move formatting out of domain:**
- `format_duration` and `format_duration_short` from `nexus-domain/src/shared/time.rs` to `src/ui/shared/format.rs`
- Domain crate retains only `from_epoch_millis`, `from_epoch_secs`, `from_rfc3339`

### Files touched
- `crates/nexus-domain/src/accounts/id.rs` — validation
- `crates/nexus-domain/src/accounts/mod.rs` — remove EditableProfile
- `crates/nexus-domain/src/agents/activity.rs` — FromStr impls
- `crates/nexus-domain/src/agents/status.rs` — #[non_exhaustive]
- `crates/nexus-domain/src/agents/model.rs` — #[non_exhaustive]
- `crates/nexus-domain/src/accounts/provider.rs` — #[non_exhaustive]
- `crates/nexus-domain/src/accounts/quota.rs` — #[non_exhaustive]
- `crates/nexus-domain/src/shared/time.rs` — remove format functions
- `crates/nexus-domain/src/lib.rs` — update re-exports
- `src/app/state/accounts.rs` — receive EditableProfile
- `src/ui/shared/format.rs` — new file, receive format functions
- `src/ui/shared/mod.rs` — re-export
- All callers of `ProfileId::new()`, `TodoStatus::parse_str()`, `TodoPriority::parse_str()`, format functions

---

## 2. Quota Domain Model Cleanup

**Goal:** Remove legacy compatibility logic from the domain type. Handle conversion at boundaries.

### Changes

**Simplify `QuotaSnapshot`:**
- Remove individual legacy fields: `daily_limit`, `daily_used_pct`, `long_limit`, `long_used_pct`
- Remove `refresh_legacy_fields()` method
- Remove `normalize()` from public API
- `QuotaSnapshot` holds: `windows: Vec<QuotaWindow>`, `refreshed_at`, `confidence`, `balance_label`, `summary`
- `effective_windows()` returns `&[QuotaWindow]` instead of cloning a Vec

**Typed window labels:**
- Add `WindowKind` enum: `Daily`, `Weekly`, `Monthly`, `LongTerm`, `Other(String)`
- `QuotaWindow.label` changes from `String` to `WindowKind`
- Window matching uses enum variants instead of string comparison

**Legacy construction at boundary:**
- `QuotaSnapshot::from_legacy(...)` constructor converts old-format data into `Vec<QuotaWindow>` at construction time
- Callers that currently build with individual fields use `from_legacy()` which internally creates synthetic QuotaWindows
- The domain type itself never carries dual representations

**`QuotaCache` key type:**
- Change `BTreeMap<String, QuotaSnapshot>` to `BTreeMap<ProfileId, QuotaSnapshot>` for type safety

### Files touched
- `crates/nexus-domain/src/accounts/quota.rs` — rewrite
- All callers: `src/app/reducer/accounts.rs`, `src/codex_profiles/quota.rs`, `src/claude_profiles/quota.rs`, `src/services/accounts/quota.rs`, `src/ui/` (quota display), integration tests

---

## 3. Detection Error Context

**Goal:** Make detection errors debuggable in production.

### Changes

**Expand `DetectorError`:**
```rust
#[derive(Debug, thiserror::Error)]
pub enum DetectorError {
    #[error("database error in {detector} reading {path}: {source}")]
    Database {
        detector: DetectorKind,
        path: PathBuf,
        source: rusqlite::Error,
    },
    #[error("I/O error in {detector}: {context}: {source}")]
    Io {
        detector: DetectorKind,
        context: &'static str,
        source: std::io::Error,
    },
}
```
- `is_transient()` preserved with same logic, now checking `source` field
- All call sites updated to provide detector kind + context

**Subprocess timeouts:**
- `worktree::lookup_branch_info`: wrap `Command` execution in `tokio::time::timeout(Duration::from_secs(5), ...)`
- Return `None` on timeout instead of hanging

**Claude file_state cache eviction:**
- In `ClaudeDetector::poll()`, after scanning, remove entries from `file_state` whose paths no longer exist on disk
- Check at most once per poll cycle, after the main scan completes

### Files touched
- `src/detect/error.rs` — rewrite error type
- `src/detect/claude/mod.rs` — update error construction, add cache eviction
- `src/detect/codex/mod.rs` — update error construction
- `src/detect/codex/sqlite.rs` — update error construction
- `src/detect/opencode/mod.rs` — update error construction
- `src/detect/opencode/query.rs` — update error construction
- `src/detect/worktree.rs` — add timeout
- `src/detect/process.rs` — update error construction

---

## 4. Unify Agent Matching

**Goal:** Replace 7 ad-hoc agent finder functions with a composable query abstraction.

### Changes

**New module: `src/app/reducer/agent_lookup.rs`**

Provides query methods over `&[Agent]` and `&mut [Agent]`:

```rust
/// Query helpers for finding agents in the agent list.
///
/// All methods are free functions taking `&[Agent]` or `&mut [Agent]`
/// to keep the reducer stateless.

pub fn find_by_id(agents: &mut [Agent], id: &AgentId) -> Option<&mut Agent>

pub fn find_by_pid(agents: &mut [Agent], tool: AgentTool, pid: u32) -> Option<&mut Agent>

pub fn find_process_agent(agents: &mut [Agent], tool: AgentTool, pid: u32) -> Option<&mut Agent>

pub fn session_indices_by_pid(agents: &[Agent], tool: AgentTool, pid: u32) -> Vec<usize>

pub fn exited_session_indices_by_pid(agents: &[Agent], tool: AgentTool, pid: u32) -> Vec<usize>
```

**Move tool-specific matching into `AgentTool`:**
- `AgentTool::matches_subagent(&self, subagent: &Subagent, name: &str) -> bool`
- Codex matches on name OR description; Claude/OpenCode match on name only
- Reducer code calls `id.tool().matches_subagent(...)` instead of inline `if tool == Codex`

**Remove the `expect`:**
- `insert_placeholder_agent` returns `&mut Agent` directly by computing the index before insertion, or by restructuring to avoid needing `last_mut()` after push

### Files touched
- `src/app/reducer/agent_lookup.rs` — new file
- `src/app/reducer/agents.rs` — remove inline finders, use agent_lookup, remove expect
- `src/app/reducer/mod.rs` — declare module
- `nexus-domain/src/agents/ids.rs` or `nexus-domain/src/agents/mod.rs` — `matches_subagent` on AgentTool

---

## 5. Enforce State Encapsulation

**Goal:** Make the Elm architecture a guarantee, not an aspiration.

### Changes

**Private state fields:**
- `OverviewState`: all fields become `pub(super)` (accessible to reducers in `reducer/` but not to `ui/` or `services/`)
- `AccountsState`: same
- `SystemState`: same
- `WorkspaceState`: same

Actually, since reducers live in `src/app/reducer/` and state lives in `src/app/state/`, and both are children of `src/app/`, we need `pub(in crate::app)` for reducer access. The key change is adding **read-only getters** for everything that `ui/` and `services/` need, then restricting external access to those getters.

**Getter methods on `AppState` (or delegate types):**
- `AppState` already has methods like `visible_agents()`, `agents()`, etc.
- Add getters for fields currently accessed directly: `filter`, `sort_mode`, `selected_index`, `help_scroll`, `show_help`, `mode`, `workflow`, `status_message`, etc.
- UI code changes from `state.root.overview.filter` to `state.filter()` (or similar)

**Fix tests:**
- `core_tests.rs`: rewrite to dispatch actions instead of mutating state directly
- Any other test that reaches into state fields gets the same treatment
- This validates that the action→reducer pathway actually works

**Visibility enforcement:**
- Fields stay `pub(in crate::app)` for now (reducer access)
- External access (UI, services) goes through `AppState` getters only
- The enforcement is structural: UI/services modules can't see the fields

### Files touched
- `src/app/state/overview.rs` — add getters
- `src/app/state/accounts.rs` — add getters
- `src/app/state/system.rs` — add getters
- `src/app/state/workspace.rs` — add getters
- `src/app/core.rs` — add delegating getters on AppState
- `src/app/core_tests.rs` — rewrite to use action dispatch
- `src/ui/**` — change direct field access to getter calls
- `src/services/runtime/session.rs` — change direct field access to getter calls
- `src/services/runtime/input.rs` — same

---

## Ordering

1. **#1 Domain error types** — no dependents, pure addition
2. **#2 Quota cleanup** — depends on domain layer being stable
3. **#3 Detection errors** — independent of domain changes
4. **#4 Agent matching** — app layer, may reference domain types from #1
5. **#5 State encapsulation** — last because it touches the most files and benefits from other changes being settled

Steps 1-3 can be parallelized. Steps 4-5 are sequential.

---

## Testing Strategy

- All existing tests must continue to pass after each step
- New unit tests for `ProfileId` validation, `FromStr` impls, `WindowKind` matching
- Agent lookup module gets exhaustive tests covering all current finder scenarios
- `core_tests.rs` rewrite validates the Elm architecture end-to-end
- Detection error tests verify context is preserved through error chains

## Non-Goals

- No user-facing behavior changes
- No new features
- No dependency changes (except possibly removing unused imports)
- No UI visual changes
- No performance optimization beyond removing unnecessary clones in quota
