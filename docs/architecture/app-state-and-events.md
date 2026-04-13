# App State And Events

This document explains the behavioral core of `nexus`: the event model and `AppState`.

## Why This Layer Matters

`AppState` is where external observations become application behavior.

Detectors only know what they saw.
Render code only knows what to show.

`AppState` is the place that decides:

- which agents exist in the UI
- what their effective state is
- which rows are visible
- which item is selected
- which workspace is active
- what the Accounts tab is currently doing

If you want to understand how the application behaves, this is the layer to read first after the runtime overview.

## Key Files

- `src/event.rs`
- `src/app/mod.rs`
- `src/app/core.rs`
- `src/app/system.rs`
- `src/app/state/`
- `src/app/reducer/overview.rs`
- `src/app/reducer/accounts.rs`
- `src/app/reducer/system.rs`
- `src/app/accounts.rs`
- `src/app/action.rs`
- `src/app/effect.rs`

## Event Model

`src/event.rs` defines the event vocabulary used by the rest of the app.

Important event families include:

- lifecycle: `Discovered`, `ProcessExited`, `SessionInfo`
- state changes: `StatusChanged`, `ActivityChanged`, `NormalizedStateChanged`
- subagent changes: `SubagentSpawned`, `SubagentCompleted`, `SubagentsUpdated`
- interaction traces: `ToolCallStarted`, `ToolCallCompleted`, `TodosUpdated`
- infrastructure state: `DetectorHealthChanged`

Detector-health events now use a typed `DetectorKind` identifier instead of free-form strings, so reducer/state code is keyed by explicit domain values rather than string conventions.

These events are intentionally domain-oriented rather than raw provider-specific records.

## `AppState` Responsibilities

`AppState` owns:

- the root application state in `root`
- the rich `agents: Vec<Agent>` detail store used for diagnostics, recent activity, subagents, and todos

It exposes read helpers that join root-owned workspace state back to rich agent detail where needed, but it is intentionally no longer the owner of runtime configuration, clocks, or async bookkeeping.

`RuntimeSession` owns:

- `Config`
- startup timing used by empty/degraded render states
- transient quota-refresh bookkeeping
- async effect dispatch and effect-result application for Accounts workflows

## Overview State

The Overview side of state is centered on:

- `root.overview.agents`
- `selected_index`
- `sort_mode`
- `filter`
- `mode`

`root.overview` is authoritative for the visible list model: selection, filtering, sorting, rename workflow state, and summarized agent rows. `AppState.agents` remains the richer runtime-detail store keyed by agent identity. The UI list path reads from `root.overview`; detail views join those rows back to rich `Agent` records when they need activity history, todos, or subagent state.

## Accounts State

The Accounts tab has its own focused state object: `AccountsState`.

It contains:

- account entries and selected visible index
- loaded profiles keyed by provider/profile identity
- quota snapshots keyed by provider/profile identity
- active provider filter
- filter/edit/confirmation modes
- transient status message state
- active profile badges and loading state

This separation matters because the Accounts workspace behaves like a small form/workflow subsystem, not just another list view.

## Event Handling

`AppState::handle_event` is the central dispatcher for external events.

The important split is:

- detector events that affect Overview or detector health are translated onto the semantic `Action::Detector` reducer path
- semantic keyboard actions are handed back into `RuntimeSession::dispatch_action`, which routes workspace, overview, accounts, and system actions onto the right reducer/effect path while keeping async effect work outside the pure model
- rich runtime details such as activity traces, subagents, tool calls, and todos still update `AppState.agents`
- Accounts actions and effect results flow through reducer/effect modules, with `src/services/runtime/session.rs` coordinating dispatch and async result application

That organization keeps the top-level event dispatcher readable while still letting the app support many event families.

Important behavior:

- discovery can create placeholder agents
- later session info can enrich or reconcile earlier process rows
- process exits can retire or update multiple related rows
- normalized state changes influence cross-provider UI behavior
- detector-health changes update the empty-state/degraded-state messaging path

This layer is where correctness is hardest to reason about because timing differences matter. Events may arrive in different orders depending on the provider and polling cycle.

## Identity And Reconciliation

One of the subtle jobs of this layer is reconciling different kinds of identity:

- process identity
- session identity
- provider-specific thread identity

The application cannot assume that a provider gives one perfect stable key for every situation. Instead, it uses event sequencing, registry hints, and reconciliation logic to avoid duplicate rows or false merges.

This is one reason the app has a relatively large behavioral test suite.

## Overview Authority Boundary

The important architecture rule is:

- `root.overview` owns what the Overview list is
- `AppState.agents` owns the richer operational detail attached to those rows

That means render code should not derive list ordering or visibility from `AppState.agents` directly. It should ask `root.overview` which rows exist and in what order, then join back to `Agent` data only when richer detail is actually needed.

## Selection, Filter, And Sort Semantics

Overview selection is expressed in terms of `root.overview.visible_agents()`, not raw `AppState.agents` indices. That means:

- filtering may clamp selection
- sorting changes how the selection maps onto raw agent positions
- remove/rename operations must resolve visible indices back to real indices safely

This is the sort of logic that looks trivial until it starts producing off-by-one bugs or renaming the wrong row. The current structure exists to make those transformations explicit.

## Accounts Workflow Semantics

The Accounts logic split is now:

- reducers own selection/filter/edit/confirm transitions in `root.accounts`
- services own storage, import, quota refresh, and write-to-home behavior
- `src/services/runtime/session.rs` coordinates effect dispatch and result application for the live runtime

User-facing workflows still include:

- import Codex auth
- import Claude credentials
- edit profile metadata
- write selected profile back to the provider home directory
- delete profiles
- refresh quota

These are stateful workflows with confirmation, edit-mode, and transient status behavior. Keeping them in the app layer instead of render code avoids turning widget code into a workflow engine.

## Why This Layer Should Stay Behavioral, Not Presentational

A useful rule:

- if the question is "what happened?" or "what state should the app be in?", it belongs here
- if the question is "how should that look on screen?", it belongs in `ui/`

Violating that boundary makes the codebase much harder to maintain because behavior ends up fragmented across handlers and widgets.

## Where To Be Careful

High-risk changes in this layer include:

- identity/reconciliation logic
- keeping the root-overview summary and rich agent detail stores aligned by identity
- mixing Accounts behavior into Overview assumptions
- turning provider-specific quirks into hardcoded UI behavior
- adding new state without deciding who owns invalidation and cleanup

When in doubt, prefer explicit state ownership over clever derived behavior hidden in render code.
