# Architecture Overview

This directory explains how `nexus` works internally.

The intent is not to restate the README. The README explains what the tool is for. These documents explain how the binary starts, how state flows through the system, how the TUI is composed, and where contributors can safely make changes without breaking subtle behavior.

If you want the current refactor target and migration status in one place, start with [refactor-checkpoint.md](refactor-checkpoint.md).

## System Summary

At a high level, `nexus` is a single-process Rust TUI application that combines:

- a passive monitoring surface for local AI coding-agent sessions
- an account-management surface for Codex and Claude profiles
- background detection and quota polling tasks running on top of `tokio`
- a `ratatui` + `crossterm` terminal frontend driven by an event/state loop

The binary is not a client/server system. There is no internal RPC layer, no daemon, and no separate renderer process. Everything happens inside one terminal process:

1. `main.rs` parses CLI arguments, builds `Config`, and hands control to the runtime service
2. `src/infra/config.rs` runs one-time legacy path migration
3. `src/services/runtime/logging.rs` initializes tracing to a log file
4. `src/services/runtime/terminal.rs` validates the terminal, installs panic restoration, and initializes the TUI
5. detectors are spawned and begin emitting `AgentEvent` values
6. `AppState` receives those events and becomes the single source of truth for visible UI state
7. the TUI renders from `AppState`
8. `src/input/` translates keyboard input into actions and applies them back into `AppState`

That loop continues until the user quits or the process receives a terminating signal.

## Major Subsystems

### Runtime and data flow

The runtime layer is centered in `src/services/runtime/`, with `src/main.rs` acting as a thin binary entrypoint.

It owns:

- startup ordering
- terminal initialization and restoration
- detector spawning
- quota polling tasks
- the top-level render/input/event loop
- shutdown behavior

Read next: [runtime-and-data-flow.md](runtime-and-data-flow.md)

### Detection

The detection layer lives under `src/detect/`.

It is responsible for:

- finding running local agent processes
- reading provider-specific state stores
- enriching sessions with branch/worktree context
- emitting `AgentEvent` values instead of mutating UI state directly

Detection is intentionally side-effect-light from the perspective of the application UI. It observes external state, translates it into domain events, and leaves state reconciliation to the app layer.

Read next: [detection.md](detection.md)

### App state and events

The behavioral core of the application lives in:

- `src/event.rs`
- `src/app/mod.rs`
- `src/app/reducer/`
- `src/app/action.rs`
- `src/app/effect.rs`
- `src/app/accounts.rs`

`AppState` is the bridge between low-level detector events and high-level UI behavior. It owns selections, filters, cached visible-agent indices, profile-management state, status messages, and quota-related UI state.

Read next: [app-state-and-events.md](app-state-and-events.md)

### UI and interaction

The render and interaction layer lives in:

- `src/ui/`
- `src/input/`

This layer is deliberately split into two halves:

- `src/input/` converts terminal input into application actions
- `ui/` renders visual output from `AppState`

The UI layer should not become a second state machine. It reads state, formats it, and renders it.

Read next: [ui-and-interaction.md](ui-and-interaction.md)

### Profile and quota management

The account-management and runtime-isolation logic lives mainly in:

- `src/codex_profiles/`
- `src/claude_profiles/`
- `src/app/accounts.rs`

This is the part of the codebase with the most sensitive file-handling and external API behavior. It is where profile import, auth writes, quota refreshes, and Codex runtime materialization happen.

Read next: [codex-and-claude-profiles.md](codex-and-claude-profiles.md)

## Dependency Direction

The intended dependency flow is:

1. provider/detection/config/runtime helpers produce data or events
2. `AppState` consumes those events and owns behavior
3. `ui/` renders from `AppState`
4. `src/input/` produces actions that are applied back into `AppState`

The important boundary is that `ui/` should not become responsible for business logic and detectors should not directly know about presentation concerns.

## Key Architectural Properties

### Event-driven state reconciliation

The app does not try to keep the UI synchronized by polling state directly from render code. Instead, external state is translated into `AgentEvent` values and folded into `AppState`.

This gives the codebase a clean separation:

- detectors answer "what changed outside the app?"
- `AppState` answers "what does that mean for the app?"
- render code answers "what should the user see?"

### Terminal safety first

Because this is a TUI, terminal lifecycle behavior is part of the architecture, not a peripheral concern. Panic restore hooks, terminal initialization order, and shutdown handling are core requirements.

### Best-effort external integrations

Several features depend on external tools or undocumented/internal upstream APIs:

- `ps`
- `lsof`
- `git`
- Codex/OpenCode SQLite schemas
- provider quota endpoints

The application is designed to degrade rather than claim stronger guarantees than it can provide. That is why degraded detector-health states and best-effort quota semantics matter.

## Glossary

### Detector

A background task that inspects provider-specific local state and emits `AgentEvent` values.

### Agent event

A domain event in `src/event.rs` that describes something the app observed, such as discovery, status change, process exit, subagent activity, or detector health degradation.

### Normalized state

A cross-provider interpretation of runtime state used to present a consistent UI even though Claude, Codex, and OpenCode expose different raw signals.

### Runtime plan

The Codex-specific plan that materializes an isolated runtime home, shared per-repo session storage, and SQLite home before launching Codex under a selected profile.

### Accounts tab

The profile-management workspace for importing, editing, selecting, deleting, and writing Codex and Claude profile credentials.

## Suggested Reading Order

If you need a complete mental model:

1. [runtime-and-data-flow.md](runtime-and-data-flow.md)
2. [detection.md](detection.md)
3. [app-state-and-events.md](app-state-and-events.md)
4. [ui-and-interaction.md](ui-and-interaction.md)
5. [codex-and-claude-profiles.md](codex-and-claude-profiles.md)

If you are changing one area:

- detector behavior: start with [detection.md](detection.md)
- view behavior or selection/filter logic: start with [app-state-and-events.md](app-state-and-events.md)
- rendering/input/layout: start with [ui-and-interaction.md](ui-and-interaction.md)
- account switching/runtime isolation/quota logic: start with [codex-and-claude-profiles.md](codex-and-claude-profiles.md)
