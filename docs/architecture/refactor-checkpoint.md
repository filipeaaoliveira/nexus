# Architecture Refactor Checkpoint

**Date:** 2026-04-12

This document explains what the current pre-release refactor is trying to achieve, why it exists, what has already changed, what still remains, and what must be true before `nexus` should be considered architecturally ready for public release.

It is a checkpoint document, not a historical changelog. The goal is to preserve the intent of the refactor in one place so future work does not drift back toward the earlier design problems.

## Why This Refactor Exists

The project is being prepared for public release as a real open-source repository, not as a private side project.

That means the codebase needs to be:

- understandable by outside contributors without author hand-holding
- structured around explicit responsibilities instead of historical convenience
- safe to modify without reading the entire application first
- honest in its product model and architecture docs
- maintainable after release, not only publishable on release day

The pre-refactor code already had decent top-level separation between detection, app state, and UI, but it still had several structural problems:

- `AppState` had become a coordination-heavy object that mixed state, workflow rules, persistence, and side-effect handling.
- Accounts behavior was split between reducer-style logic and imperative helper methods.
- The TUI had shadow state and compatibility layers that made it hard to see which fields were actually authoritative.
- The code used “grown application” patterns such as compatibility shims and large stateful modules that would become harder to clean up after public release.
- The product model was not explicit enough in code: workspaces, normalized status, detector health, overlays, and modal workflows existed conceptually but were not consistently represented as first-class architecture.

The goal of the refactor is to fix that before publication rather than carry a long-term maintenance tax into the public lifecycle of the repository.

## What We Are Trying To Achieve

The target is a professional Rust application architecture with clear dependency direction and explicit ownership.

### Core architectural goals

- `crates/nexus-domain/` owns domain model and shared concepts
- `app/` owns semantic actions, reducers, root state, and effect/result flow
- `services/` owns external integration and side effects
- `ui/` owns rendering only
- `input/` owns key-to-intent translation only
- `src/config.rs` owns CLI/default runtime configuration

### Product-model goals

- the app is explicitly organized around two workspaces: `Overview` and `Accounts`
- normalized agent status is the primary user-facing status model
- detector health is explicit system state instead of a hidden edge case
- modal state is represented as typed workflow enums instead of overlapping booleans
- first-run degraded states are surfaced honestly instead of being silently treated as empty content

### Maintainability goals

- no hidden source-of-truth duplication for workspace state
- no broad compatibility shims left as permanent architecture
- fewer “god object” patterns
- smaller render modules with clear boundaries
- clearer tests that target reducers, services, and UI boundaries separately

## Target Architecture

The intended architecture is:

```text
crates/
  nexus-domain/
    src/
      agents/
      accounts/
      system/
      shared/
src/
  app/
    action.rs
    effect.rs
    state.rs
    reducer/
    dispatch/
  services/
    accounts/
    runtime/
  detect/
  ui/
    overview/
    accounts/
    shared/
  input/
  config.rs
```

### Architectural rules

- `crates/nexus-domain/` must not depend on ratatui, crossterm, tokio, clap, filesystem APIs, or networking
- reducers decide what should happen
- services perform operational work
- render code does not mutate application behavior
- input translation should emit semantic actions where practical
- invalid workflow combinations should be made unrepresentable by type design

## What Has Already Been Achieved

The refactor is no longer theoretical. A large amount of the target architecture is already live.

### Open-source readiness blockers already addressed

- detector dependency failure is surfaced instead of silently appearing as an empty Overview state
- README build/install flow now uses the locked dependency graph
- stale internal-facing design notes were corrected
- architecture docs, release docs, troubleshooting, first-run behavior, and contributor docs were substantially improved

### Domain extraction

- the old `src/types.rs` compatibility layer has been removed
- domain types now live under `crates/nexus-domain/src/agents`, `crates/nexus-domain/src/accounts`, `crates/nexus-domain/src/system`, and `crates/nexus-domain/src/shared`
- domain time parsing helpers now live in `crates/nexus-domain/src/shared/time.rs`
- UI duration formatting helpers live in `src/ui/shared/format.rs`
- boundary tests exist to stop the old `types` shim from coming back

### Runtime and bootstrap cleanup

- runtime bootstrapping moved into `src/services/runtime/`
- terminal lifecycle and panic restoration now live in dedicated runtime modules
- CLI/default path configuration lives in `src/config.rs`
- `src/main.rs` has been reduced back toward a thin entrypoint

### Input architecture

- the old monolithic `src/input.rs` was split into `src/input/mod.rs` and `src/input/keymap.rs`
- input mapping is now primarily semantic and workspace-aware
- runtime-only commands are narrowed to explicit cases that still need runtime coordination

### UI restructuring

- Accounts rendering was split into `src/ui/accounts/` modules
- shared rendering concerns were moved under `src/ui/shared/`
- `src/ui/accounts_tab.rs` was deleted
- UI boundary tests were added so old compatibility reads do not silently return

### Accounts architecture

This is the area where the largest cleanup has already landed.

- Accounts effects are dispatched through explicit effect/result flow
- profile loading, saving, quota handling, import, write-to-home, and delete flow through services instead of being buried in broad state helpers
- `root.accounts` is now the authoritative state for:
  - entries
  - selection
  - provider filter
  - edit mode
  - confirm mode
  - active profile badges
  - status message
  - quota cache
  - loading state
- the old Accounts shadow state was removed
- `accounts_tab` has been deleted from `AppState`

### Overview architecture

The Overview workspace is now fully on the intended authority model:

- `root.overview` owns live selection, sort mode, filter state, rename mode, and summarized Overview agents
- production detector updates now flow through the `Action::Detector` reducer path for Overview
- the old legacy Overview fields were removed from `AppState`
- `AppState.agents` remains as the rich runtime-detail store for activity, subagents, todos, recent events, and diagnostics
- UI list visibility/order now follows `root.overview`, while detail rendering joins back to rich agent records by identity
- system/workspace semantic actions now route back through dedicated app helpers instead of being interpreted inline in runtime code
- boundary tests enforce that the deleted legacy Overview fields do not come back

## What Still Remains

No structural refactor tasks remain in this checkpoint.

The last open Overview seam has been closed:

- production detector updates now hit the reducer path in real runtime flow, not only in isolated reducer tests
- `root.overview` is the authoritative source for Overview list/filter/sort state and summarized agent rows
- `AppState.agents` remains intentionally separate as the rich detail model keyed by agent identity
- detector-health messaging now reads from explicit root system state instead of a duplicate private app field
- architecture docs have been refreshed to describe the current behavior rather than the earlier bridge code

That means the remaining work before public release is ordinary release work, not unresolved architecture migration. Future changes should preserve the same authority boundary instead of reopening compatibility layers.

## End-State We Want Before Public Release

Before public release, the codebase should satisfy all of the following:

- all public workspace state is owned by explicit root substate objects
- no hidden compatibility shadow state remains for interactive behavior
- reducer/effect flow is real in production for the important mutable workflows
- services are the only place where IO/network/process side effects happen
- UI modules are render-only
- input modules are intent-only
- architecture docs describe the code that actually exists
- the repository can be trusted by a first-time contributor as a serious public project

## What This Means For Future Changes

When making new changes during this refactor, prefer:

- typed workflow state over booleans
- reducer/effect transitions over imperative state mutation
- root-owned workspace state over local compatibility fields
- service modules over direct filesystem/network work in the app layer
- explicit boundary tests when removing a compatibility layer

Avoid:

- reintroducing shadow state for convenience
- pushing business rules into render code
- adding new broad helper methods to `AppState` that mix state and side effects
- leaving “temporary” adapters undocumented

## Related Documents

- [Architecture Overview](README.md)
- [Runtime And Data Flow](runtime-and-data-flow.md)
- [Detection](detection.md)
- [App State And Events](app-state-and-events.md)
- [UI And Interaction](ui-and-interaction.md)
- [Codex And Claude Profiles](codex-and-claude-profiles.md)
