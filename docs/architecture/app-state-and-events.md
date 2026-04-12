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
- `src/app/reducer/overview.rs`
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

These events are intentionally domain-oriented rather than raw provider-specific records.

## `AppState` Responsibilities

`AppState` owns:

- the active workspace
- the list of agents visible to the app
- selection state
- sorting and filtering state
- rename/filter modal state for Overview
- all Accounts-tab UI state
- detector-health issues
- visible-agent cache bookkeeping

It also exposes read helpers such as visible-agent queries and mutating handlers for both detector events and user actions.

## Overview State

The Overview side of state is centered on:

- `agents`
- `selected`
- `sort_mode`
- `filter`
- `filter_mode`
- `rename_mode`
- help overlay state

The Overview tab treats the agent list as the main domain surface. It is intentionally passive and read-only from the perspective of the outside world. Removing or renaming an item affects the app’s local representation, not the underlying provider process.

## Accounts State

The Accounts tab has its own focused state object: `AccountsTabState`.

It contains:

- profile IDs for Codex and Claude
- selected profile index
- active provider filter
- filter/edit/confirmation modes
- edit form fields
- cached quota snapshots
- transient status message state
- confirmation state for destructive actions

This separation matters because the Accounts workspace behaves like a small form/workflow subsystem, not just another list view.

## Event Handling

`AppState::handle_event` is the central dispatcher for external events.

The implementation now fans out through reducer and effect modules under `src/app/reducer/`, with `src/app/accounts.rs` still carrying some compatibility orchestration.

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

## Visible-Agent Cache

The app does not recompute the visible-agent list blindly on every read. It maintains a small cache keyed by:

- agent version
- filter
- sort mode

The cache is invalidated when relevant state changes.

This is not a performance micro-optimization so much as a clarity tool: it gives the rest of the UI one place to ask for the current visible ordering without duplicating filter/sort logic.

## Selection, Filter, And Sort Semantics

Overview selection is expressed in terms of the visible list, not raw `agents` indices. That means:

- filtering may clamp selection
- sorting changes how the selection maps onto raw agent positions
- remove/rename operations must resolve visible indices back to real indices safely

This is the sort of logic that looks trivial until it starts producing off-by-one bugs or renaming the wrong row. The current structure exists to make those transformations explicit.

## Accounts Workflow Semantics

The Accounts logic in `src/app/accounts.rs` handles workflows such as:

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
- selection/filter/cache invalidation behavior
- mixing Accounts behavior into Overview assumptions
- turning provider-specific quirks into hardcoded UI behavior
- adding new state without deciding who owns invalidation and cleanup

When in doubt, prefer explicit state ownership over clever derived behavior hidden in render code.
