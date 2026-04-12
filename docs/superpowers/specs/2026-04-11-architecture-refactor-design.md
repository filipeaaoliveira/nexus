# Nexus Architecture Refactor Design

**Date:** 2026-04-11

## Goal

Refactor `nexus` into a maintainable Rust application architecture before public release, with explicit domain boundaries, reducer/effect application flow, isolated services for side effects, and a product model that is coherent for both users and contributors.

## Why This Refactor Exists

The current codebase has a solid top-level split between detection, state, input, and UI, but important internal boundaries have softened:

- `AppState` has become a coordination-heavy state object that mixes workflow logic with persistence and async result handling.
- Accounts logic mixes reducer behavior, filesystem operations, quota refresh, and UI workflow state.
- Several files are large enough that future contributor changes will be riskier than they should be, especially `src/app/mod.rs` and `src/ui/accounts_tab.rs`.
- The product model is not explicit enough in code. The app conceptually has workspaces, normalized agent states, detector health, and modal workflows, but those concepts are spread across booleans, helper methods, and UI conventions.

This refactor is intended to fix the architecture now rather than carrying an avoidable maintenance burden into the public lifecycle of the repository.

## Refactor Principles

The refactor should follow these rules:

- Domain types must not depend on ratatui, crossterm, tokio, clap, or filesystem APIs.
- Reducers own decisions and state transitions.
- Services own side effects and external integration.
- UI owns presentation only.
- Input translates terminal keys into semantic actions.
- Global state should be compositional and explicit, not a hidden god object.
- Invalid modal-state combinations should be made unrepresentable where possible.
- User-facing concepts should match code concepts.

## Target Architecture

The target codebase is organized into explicit application layers:

```text
src/
  app/
    app.rs
    state.rs
    action.rs
    effect.rs
    reducer/
    dispatch/
  domain/
    agents/
    accounts/
    system/
  services/
    detect/
    accounts/
    runtime/
  ui/
    overview/
    accounts/
    shared/
  input/
    keymap.rs
  infra/
    config.rs
```

### Layer responsibilities

#### `domain/`

Pure domain model and business concepts:

- agent identity, status, activity, metadata
- account profiles, provider identity, quota state
- detector and system health concepts
- workflow enums and other typed application concepts that are not tied to UI or IO

#### `app/`

Application orchestration and state transitions:

- root state composition
- semantic actions
- effect requests/results
- reducers per subsystem
- effect dispatch coordination

#### `services/`

All external integration and side effects:

- detector polling and translation
- process scanning
- profile storage and auth import/write
- quota refresh and cache persistence
- terminal/runtime integration helpers

#### `ui/`

Rendering only:

- workspace layout
- help overlay
- list/detail/footer views
- degraded and empty states

#### `input/`

Input mapping only:

- crossterm key events
- semantic action translation based on current workflow mode

#### `infra/`

Bootstrap concerns:

- config loading
- logging initialization
- startup wiring

## Target State Model

The root state remains one object, but only as a composed container:

```rust
pub struct AppState {
    pub workspace: WorkspaceState,
    pub overview: OverviewState,
    pub accounts: AccountsState,
    pub system: SystemState,
}
```

### `WorkspaceState`

Owns:

- active workspace
- workspace-level navigation and routing state

### `OverviewState`

Owns:

- known agents
- selected agent
- filter and sort state
- rename workflow state
- dismissed visibility policy
- detector-health projections relevant to Overview

### `AccountsState`

Owns:

- loaded profile collections
- selected profile
- provider filter
- edit and confirm workflows
- visible notifications/status messages
- current quota state for selected profile

### `SystemState`

Owns:

- help overlay state
- global notifications
- detector health registry
- startup/shutdown and degraded runtime state

## Action and Effect Model

The application should move from ad hoc state methods to semantic action handling.

### Actions

Actions are application intents or external events:

```rust
enum Action {
    Tick,
    QuitRequested,
    Workspace(WorkspaceAction),
    Overview(OverviewAction),
    Accounts(AccountsAction),
    System(SystemAction),
    Detector(DetectorAction),
    EffectResult(EffectResult),
}
```

### Effects

Effects are explicit requests for side effects:

```rust
enum Effect {
    LoadProfiles,
    RefreshCodexQuota { profile_id: ProfileId },
    RefreshClaudeQuota { profile_id: ProfileId },
    ImportCodexProfile,
    ImportClaudeProfile,
    WriteProfileToHome { provider: Provider, profile_id: ProfileId },
    DeleteProfile { provider: Provider, profile_id: ProfileId },
}
```

Reducers mutate state and emit effects. Services do the operational work. Results re-enter the application through typed result actions.

This is the architectural center of the refactor.

## Product Model Cleanup

The refactor should make current product concepts explicit:

- `Tab` becomes `Workspace`
- `Overview` and `Accounts` become first-class workspace modules
- normalized agent state becomes the primary user-facing status model
- raw primary/runtime state becomes diagnostic/internal
- detector health becomes explicit system state
- UI modes become workflow enums rather than overlapping booleans

### Workflow enums

Instead of multiple booleans such as `filter_mode`, `rename_mode`, `edit_mode`, and `pending_confirm`, use explicit workflow types:

```rust
pub enum OverviewMode {
    Normal,
    Filtering { input: String },
    Renaming { agent_id: AgentId, input: String },
}

pub enum AccountsMode {
    Normal,
    Filtering { input: String },
    Editing { draft: ProfileDraft, field: EditField },
    Confirming { action: ConfirmAction },
}

pub enum Overlay {
    None,
    Help { scroll: u16 },
}
```

This makes invalid combinations unrepresentable and simplifies both input routing and rendering.

## Domain Model Changes

### Agent domain

The agent model should be split into clear substructures:

- identity
- runtime data
- user-facing normalized state
- activity/subagents/todos
- metadata such as branch, worktree, alias

Raw detector/runtime status should remain available, but normalized status should drive user-visible summaries and list behavior.

### Accounts domain

The accounts model should include typed concepts for:

- `ProfileId`
- `Provider`
- `ProfileSummary`
- `ProfileDetails`
- `ProfileDraft`
- `QuotaState`
- `ConfirmAction`

Quota should be a typed UI/domain state rather than a loose mix of cache, loading booleans, and optional snapshots.

### System domain

Detector health should become a first-class domain concept:

```rust
pub struct DetectorHealth {
    pub detector: DetectorKind,
    pub status: HealthStatus,
    pub message: String,
    pub updated_at: DateTime<Utc>,
}
```

This makes degraded detection behavior explicit and testable.

## Migration Map From Current Structure

### Current `src/app/mod.rs`

Split into:

- `app/state.rs`
- `app/reducer/root.rs`
- `app/reducer/overview.rs`
- `app/reducer/accounts.rs`
- `app/reducer/system.rs`

### Current `src/app/overview.rs`

Move primarily into:

- `app/reducer/overview.rs`

### Current `src/app/accounts.rs`

Split into:

- `app/reducer/accounts.rs`
- `services/accounts/storage.rs`
- `services/accounts/codex.rs`
- `services/accounts/claude.rs`
- `services/accounts/quota.rs`

### Current `src/ui/accounts_tab.rs`

Split into:

- `ui/accounts/list.rs`
- `ui/accounts/detail.rs`
- `ui/accounts/edit.rs`
- `ui/accounts/confirm.rs`
- `ui/accounts/footer.rs`

### Current `src/input.rs`

Split into:

- `input/mod.rs`
- `input/keymap.rs`

### Current `src/types.rs`

Dismantle into:

- `domain/agents/*`
- `domain/accounts/*`
- `domain/system/*`

## Sequencing

The refactor should proceed in this order:

1. Strengthen behavior tests around intended current behavior.
2. Introduce the target architecture skeleton without moving all logic at once.
3. Move domain types out of `types.rs`.
4. Split reducers from side effects.
5. Replace boolean modes with explicit workflow enums.
6. Break large UI modules into focused render modules.
7. Simplify `main.rs` and runtime dispatch wiring.
8. Remove compatibility scaffolding before finishing.

## Guardrails

Avoid these failure modes:

- folder-only refactors without responsibility changes
- keeping `AppState` as a disguised god object
- letting services mutate state directly
- moving business rules into UI modules
- introducing unnecessary trait abstraction where concrete types are sufficient

## Testing Strategy

The refactor should strengthen test layering:

- reducer tests for pure state transitions and emitted effects
- service tests for filesystem/network/quota/profile operations
- focused UI tests for rendering of critical states
- a small number of smoke tests for startup, shutdown, and end-to-end workflow seams

## Success Criteria

The refactor is complete when:

- the codebase has explicit domain, app, service, UI, and input boundaries
- state transitions are testable without running side effects
- accounts operations no longer live directly inside the main state object
- `types.rs` no longer acts as a global catch-all
- large UI files are split into maintainable render modules
- public-facing concepts such as workspaces, normalized state, and detector health are reflected clearly in code and UI

## Non-Goals

This refactor is not intended to:

- add new providers
- add a new workspace
- redesign the project into a framework
- preserve every current name or internal path for backward compatibility

The goal is architectural clarity and long-term maintainability before open-source release.
