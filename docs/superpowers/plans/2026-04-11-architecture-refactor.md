# Architecture Refactor Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Refactor `nexus` into a domain-driven reducer/effect architecture with explicit services, workspace state, and smaller UI modules before public release.

**Architecture:** The refactor replaces the current `AppState`-centric coordination model with explicit root state, semantic actions, typed effects, pure reducers, and side-effect services. The work is phased so behavior is locked down first, architecture seams are introduced second, and large files are broken apart only after stable state and workflow models exist.

**Tech Stack:** Rust, tokio, ratatui, crossterm, anyhow, clap, tracing

---

## File Structure

Planned new files and responsibilities:

- Create: `src/app/app.rs` — top-level application orchestration and update loop helpers
- Create: `src/app/state.rs` — root composed state and workspace-specific state structs
- Create: `src/app/action.rs` — semantic actions and sub-action enums
- Create: `src/app/effect.rs` — typed effect requests and results
- Create: `src/app/reducer/mod.rs` — reducer module exports
- Create: `src/app/reducer/root.rs` — top-level reducer routing
- Create: `src/app/reducer/overview.rs` — overview reducer logic
- Create: `src/app/reducer/accounts.rs` — accounts reducer logic
- Create: `src/app/reducer/system.rs` — system/overlay/health reducer logic
- Create: `src/app/dispatch/mod.rs` — effect dispatch exports
- Create: `src/app/dispatch/effects.rs` — effect runner and result routing
- Create: `src/domain/mod.rs` — domain exports
- Create: `src/domain/agents/mod.rs` — agent domain exports
- Create: `src/domain/agents/model.rs` — agent structs and supporting substructures
- Create: `src/domain/agents/status.rs` — primary and normalized status models
- Create: `src/domain/agents/activity.rs` — activity, todos, subagents
- Create: `src/domain/agents/ids.rs` — agent identity types
- Create: `src/domain/accounts/mod.rs` — accounts domain exports
- Create: `src/domain/accounts/model.rs` — profile summary/details/draft types
- Create: `src/domain/accounts/provider.rs` — provider and filter enums
- Create: `src/domain/accounts/quota.rs` — quota state and quota presentation model
- Create: `src/domain/system/mod.rs` — system exports
- Create: `src/domain/system/health.rs` — detector/system health models
- Create: `src/domain/system/notification.rs` — global notification types
- Create: `src/services/mod.rs` — service exports
- Create: `src/services/accounts/mod.rs` — account service exports
- Create: `src/services/accounts/storage.rs` — profile CRUD and persistence
- Create: `src/services/accounts/codex.rs` — Codex auth/profile operations
- Create: `src/services/accounts/claude.rs` — Claude auth/profile operations
- Create: `src/services/accounts/quota.rs` — quota fetch/cache service helpers
- Create: `src/services/runtime/mod.rs` — runtime service exports
- Create: `src/services/runtime/terminal.rs` — terminal lifecycle helpers
- Create: `src/services/runtime/logging.rs` — tracing/logging setup
- Create: `src/ui/overview/mod.rs` — overview renderer exports
- Create: `src/ui/overview/list.rs` — overview list rendering
- Create: `src/ui/overview/detail.rs` — overview detail rendering
- Create: `src/ui/overview/activity.rs` — overview activity rendering
- Create: `src/ui/overview/footer.rs` — overview footer rendering
- Create: `src/ui/accounts/mod.rs` — accounts renderer exports
- Create: `src/ui/accounts/list.rs` — accounts list rendering
- Create: `src/ui/accounts/detail.rs` — accounts detail rendering
- Create: `src/ui/accounts/edit.rs` — accounts edit-mode rendering
- Create: `src/ui/accounts/confirm.rs` — accounts confirm/status rendering
- Create: `src/ui/accounts/footer.rs` — accounts footer rendering
- Create: `src/ui/shared/mod.rs` — shared UI exports
- Create: `src/ui/shared/help.rs` — help overlay rendering
- Create: `src/ui/shared/layout.rs` — layout helpers
- Create: `src/ui/shared/widgets.rs` — reusable widgets/helpers
- Create: `src/input/mod.rs` — input exports
- Create: `src/input/keymap.rs` — key event to action mapping
- Create: `src/infra/mod.rs` — infra exports
- Create: `src/infra/config.rs` — config wiring/re-export if retained

Primary existing files to shrink or replace:

- Modify heavily: `src/app/mod.rs`
- Modify heavily: `src/app/overview.rs`
- Modify heavily: `src/app/accounts.rs`
- Modify heavily: `src/ui/mod.rs`
- Modify heavily: `src/ui/accounts_tab.rs`
- Modify heavily: `src/input.rs`
- Modify heavily: `src/types.rs`
- Modify heavily: `src/main.rs`
- Test: `tests/app_test.rs`
- Test: `src/ui/mod.rs`
- Test: add reducer/service-focused test modules as new architecture lands

### Task 1: Freeze Current Intended Behavior With Tests

**Files:**
- Modify: `tests/app_test.rs`
- Modify: `src/ui/mod.rs`
- Modify: `src/input.rs`

- [ ] **Step 1: Write or tighten failing tests for intended current behavior**

Add/expand tests covering:

- overview rename workflow
- overview filter and sort behavior
- normalized header status counts
- degraded detector health empty states
- accounts help/footer hints
- accounts confirm/edit/filter mode transitions
- quota refresh result handling

- [ ] **Step 2: Run targeted tests to verify any missing coverage or failures**

Run:

```bash
cargo test --locked --test app_test
cargo test --locked src::ui::mod --lib
```

Expected:

- existing behavior stays green
- any missing assertions are identified before moving architecture

- [ ] **Step 3: Add the minimal missing assertions and fixture helpers**

Prefer extending current tests instead of adding redundant end-to-end tests.

- [ ] **Step 4: Re-run the targeted suites**

Run:

```bash
cargo test --locked --test app_test
cargo test --locked --lib
```

Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add tests/app_test.rs src/ui/mod.rs src/input.rs
git commit -m "test: lock down current app behavior"
```

### Task 2: Introduce Domain Modules And Move Core Types Out Of `types.rs`

**Files:**
- Create: `src/domain/mod.rs`
- Create: `src/domain/agents/mod.rs`
- Create: `src/domain/agents/model.rs`
- Create: `src/domain/agents/status.rs`
- Create: `src/domain/agents/activity.rs`
- Create: `src/domain/agents/ids.rs`
- Create: `src/domain/accounts/mod.rs`
- Create: `src/domain/accounts/model.rs`
- Create: `src/domain/accounts/provider.rs`
- Create: `src/domain/accounts/quota.rs`
- Create: `src/domain/system/mod.rs`
- Create: `src/domain/system/health.rs`
- Create: `src/domain/system/notification.rs`
- Modify: `src/types.rs`
- Modify: `src/lib.rs`

- [ ] **Step 1: Write failing compile-oriented tests or minimal imports proving new module paths**

Add a small unit test module that imports the new domain modules and exercises representative constructors/labels.

- [ ] **Step 2: Run the targeted test or `cargo check` to confirm missing modules**

Run:

```bash
cargo check --locked
```

Expected: FAIL with missing module/type paths

- [ ] **Step 3: Create domain modules and move types in small slices**

Start with:

- agent IDs and tool/provider-neutral identifiers
- normalized/primary status types
- activity/todo/subagent types
- provider/profile/quota models
- detector health models

Keep temporary re-exports in `src/types.rs` only as a transitional shim.

- [ ] **Step 4: Re-run compile/test checks**

Run:

```bash
cargo check --locked
cargo test --locked --lib
```

Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src/domain src/types.rs src/lib.rs
git commit -m "refactor: introduce domain modules"
```

### Task 3: Introduce Semantic Actions, Effects, And Root State

**Files:**
- Create: `src/app/state.rs`
- Create: `src/app/action.rs`
- Create: `src/app/effect.rs`
- Modify: `src/app/mod.rs`
- Modify: `src/lib.rs`

- [ ] **Step 1: Write failing tests for root state composition and mode enums**

Add tests for:

- workspace switching
- explicit overlay state
- overview/accounts mode initialization
- detector health storage in system state

- [ ] **Step 2: Run those tests to verify the skeleton is not implemented yet**

Run:

```bash
cargo test --locked --lib app::state
```

Expected: FAIL

- [ ] **Step 3: Implement root state, action, and effect definitions**

Create:

- composed `AppState`
- `Workspace` enum
- overview/accounts/system state structs
- overlay and workflow enums
- top-level action and effect enums

- [ ] **Step 4: Re-run the focused tests and full compile**

Run:

```bash
cargo test --locked --lib
cargo check --locked
```

Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src/app src/lib.rs
git commit -m "refactor: add root state action and effect model"
```

### Task 4: Split Overview Behavior Into Reducers

**Files:**
- Create: `src/app/reducer/mod.rs`
- Create: `src/app/reducer/root.rs`
- Create: `src/app/reducer/overview.rs`
- Modify: `src/app/overview.rs`
- Modify: `src/app/mod.rs`
- Test: `tests/app_test.rs`

- [ ] **Step 1: Write failing reducer tests for overview transitions**

Cover:

- discovered/session/status events update overview state
- rename start/confirm/cancel
- filter and sort transitions
- dismissed agent behavior
- normalized status aggregation inputs

- [ ] **Step 2: Run only the new overview reducer tests**

Run:

```bash
cargo test --locked overview_reducer
```

Expected: FAIL

- [ ] **Step 3: Move overview logic into pure reducer functions**

Implement overview reducers so they:

- mutate only overview state
- emit effects only when needed
- do not perform IO

- [ ] **Step 4: Re-run overview tests and existing app tests**

Run:

```bash
cargo test --locked overview_reducer
cargo test --locked --test app_test
```

Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src/app/reducer src/app/overview.rs src/app/mod.rs tests/app_test.rs
git commit -m "refactor: move overview logic into reducers"
```

### Task 5: Extract Accounts Side Effects Into Services

**Files:**
- Create: `src/services/mod.rs`
- Create: `src/services/accounts/mod.rs`
- Create: `src/services/accounts/storage.rs`
- Create: `src/services/accounts/codex.rs`
- Create: `src/services/accounts/claude.rs`
- Create: `src/services/accounts/quota.rs`
- Modify: `src/app/accounts.rs`
- Modify: `src/app/effect.rs`
- Modify: `src/lib.rs`
- Test: add service tests under `tests/` or module tests near services

- [ ] **Step 1: Write failing service tests for profile load/write/import/quota cache behavior**

Use tempdirs and fixture files to cover:

- loading profile metadata
- writing selected profile to tool home
- deleting a profile safely
- reading/writing quota cache

- [ ] **Step 2: Run the focused service tests**

Run:

```bash
cargo test --locked services::accounts
```

Expected: FAIL

- [ ] **Step 3: Move filesystem and provider-specific operations out of `AppState`**

Create service functions for:

- Codex import/write
- Claude import/write
- quota fetch/cache persistence
- profile CRUD

- [ ] **Step 4: Re-run service tests and compile**

Run:

```bash
cargo test --locked services::accounts
cargo check --locked
```

Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src/services src/app/accounts.rs src/app/effect.rs src/lib.rs
git commit -m "refactor: extract accounts services"
```

### Task 6: Move Accounts Workflow Logic Into Reducers And Modes

**Files:**
- Create: `src/app/reducer/accounts.rs`
- Modify: `src/app/state.rs`
- Modify: `src/app/action.rs`
- Modify: `src/app/effect.rs`
- Modify: `src/app/accounts.rs`
- Test: `tests/app_test.rs`

- [ ] **Step 1: Write failing reducer tests for accounts workflows**

Cover:

- selecting profiles
- cycling provider filter
- edit mode transitions
- confirm workflow transitions
- empty/failed/success quota states
- service result handling

- [ ] **Step 2: Run focused reducer tests**

Run:

```bash
cargo test --locked accounts_reducer
```

Expected: FAIL

- [ ] **Step 3: Implement accounts reducers using explicit workflow enums**

Replace boolean combinations with typed modes and reducer-driven transitions.

- [ ] **Step 4: Re-run focused reducer and existing app tests**

Run:

```bash
cargo test --locked accounts_reducer
cargo test --locked --test app_test
```

Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src/app/reducer/accounts.rs src/app/state.rs src/app/action.rs src/app/effect.rs src/app/accounts.rs tests/app_test.rs
git commit -m "refactor: move accounts workflows into reducers"
```

### Task 7: Add Effect Dispatch And Async Result Routing

**Files:**
- Create: `src/app/dispatch/mod.rs`
- Create: `src/app/dispatch/effects.rs`
- Modify: `src/main.rs`
- Modify: `src/app/app.rs`
- Modify: `src/app/mod.rs`
- Test: add dispatch-focused tests where practical

- [ ] **Step 1: Write failing tests for effect-to-result routing where feasible**

At minimum, cover translation of a reducer-emitted effect into a typed result action using mock service responses.

- [ ] **Step 2: Run focused tests or compile checks**

Run:

```bash
cargo check --locked
```

Expected: FAIL or require new dispatch wiring

- [ ] **Step 3: Implement effect dispatcher and wire it into the run loop**

Ensure:

- reducers emit typed effects
- dispatcher invokes services
- results flow back as typed actions
- background quota refresh uses this path rather than direct state mutation

- [ ] **Step 4: Re-run compile/test checks**

Run:

```bash
cargo test --locked
cargo check --locked
```

Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src/app/dispatch src/app/app.rs src/main.rs src/app/mod.rs
git commit -m "refactor: add effect dispatch pipeline"
```

### Task 8: Split Input Into Semantic Keymap Modules

**Files:**
- Create: `src/input/mod.rs`
- Create: `src/input/keymap.rs`
- Modify: `src/input.rs`
- Modify: `src/app/action.rs`
- Test: move or add tests around key-to-action mapping

- [ ] **Step 1: Write failing tests for semantic key mapping against explicit modes**

Cover:

- normal mode mappings
- overview rename/filter workflow mappings
- accounts edit/filter/confirm mappings
- help and quit actions

- [ ] **Step 2: Run the focused input tests**

Run:

```bash
cargo test --locked input
```

Expected: FAIL

- [ ] **Step 3: Implement `input/keymap.rs` and route through semantic actions**

Keep crossterm-specific parsing isolated here.

- [ ] **Step 4: Re-run input tests**

Run:

```bash
cargo test --locked input
```

Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src/input.rs src/input src/app/action.rs
git commit -m "refactor: split input into semantic keymap modules"
```

### Task 9: Split UI Into Workspace-Specific Render Modules

**Files:**
- Create: `src/ui/overview/mod.rs`
- Create: `src/ui/overview/list.rs`
- Create: `src/ui/overview/detail.rs`
- Create: `src/ui/overview/activity.rs`
- Create: `src/ui/overview/footer.rs`
- Create: `src/ui/accounts/mod.rs`
- Create: `src/ui/accounts/list.rs`
- Create: `src/ui/accounts/detail.rs`
- Create: `src/ui/accounts/edit.rs`
- Create: `src/ui/accounts/confirm.rs`
- Create: `src/ui/accounts/footer.rs`
- Create: `src/ui/shared/mod.rs`
- Create: `src/ui/shared/help.rs`
- Create: `src/ui/shared/layout.rs`
- Create: `src/ui/shared/widgets.rs`
- Modify: `src/ui/mod.rs`
- Modify: `src/ui/accounts_tab.rs`
- Test: `src/ui/mod.rs` and existing UI tests

- [ ] **Step 1: Write failing UI tests for the new module interfaces if needed**

At minimum, preserve tests for:

- small terminal fallback
- normalized header counts
- footer hints
- help overlay visibility

- [ ] **Step 2: Run the existing UI tests**

Run:

```bash
cargo test --locked --lib ui
```

Expected: baseline for later comparison

- [ ] **Step 3: Move rendering code into workspace and shared modules**

Keep `src/ui/mod.rs` as renderer routing and high-level frame orchestration only.

- [ ] **Step 4: Re-run UI tests and snapshot-like expectations**

Run:

```bash
cargo test --locked --lib ui
cargo test --locked --test app_test
```

Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src/ui
git commit -m "refactor: split UI into workspace render modules"
```

### Task 10: Simplify Runtime Bootstrap And Remove Transitional Shims

**Files:**
- Create: `src/services/runtime/mod.rs`
- Create: `src/services/runtime/terminal.rs`
- Create: `src/services/runtime/logging.rs`
- Create: `src/infra/mod.rs`
- Create: `src/infra/config.rs`
- Modify: `src/main.rs`
- Modify: `src/lib.rs`
- Modify: transitional compatibility shims added earlier

- [ ] **Step 1: Write failing tests for any extracted pure helpers**

Examples:

- config-derived path helpers
- migration helper behavior
- detector-health aggregation helpers

- [ ] **Step 2: Run compile and targeted helper tests**

Run:

```bash
cargo check --locked
```

Expected: may fail while old paths are being removed

- [ ] **Step 3: Move runtime/bootstrap helpers out of `main.rs` and remove temporary re-exports**

End state:

- thin `main.rs`
- no unnecessary compatibility layer
- no dead modules kept “just in case”

- [ ] **Step 4: Run the full verification suite**

Run:

```bash
cargo fmt --all
cargo check --locked
cargo clippy --all-targets --all-features -- -D warnings
cargo test --locked
cargo build --release --locked
```

Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src/main.rs src/services/runtime src/infra src/lib.rs
git commit -m "refactor: finalize runtime architecture"
```

## Final Cleanup Checklist

- [ ] Remove `src/types.rs` catch-all behavior or delete the file entirely
- [ ] Remove obsolete `app` helper methods superseded by reducers/effects
- [ ] Remove duplicate or transitional event routing paths
- [ ] Update architecture docs to match the final implemented structure
- [ ] Update `CONTRIBUTING.md` if contributor entry points changed materially
- [ ] Run the full verification suite one final time

## Execution Notes

- Keep commits small and behavior-preserving where possible.
- Do not start by splitting UI files before state/effect architecture exists.
- Prefer concrete types over premature trait abstraction.
- If a temporary compatibility re-export is introduced, add a follow-up checkbox to remove it in the same refactor sequence.
