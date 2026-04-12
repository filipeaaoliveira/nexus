# UI And Interaction

This document explains how the TUI is structured and how keyboard input becomes visible behavior.

## Design Boundary

The UI layer is intentionally split:

- `src/input/mod.rs` reads terminal events and maps them to application actions
- `src/input/keymap.rs` owns the key-to-action mapping table
- `src/ui/` renders `AppState` into widgets

This keeps the render path mostly declarative and prevents the visual layer from also owning core business logic.

## Render Entry Point

`src/ui/mod.rs` is the main render entry point.

It does three important things:

1. rejects too-small terminal sizes with a minimal fallback message
2. chooses the active workspace
3. composes the Overview or Accounts view from smaller render modules

The small-terminal guard is part of the architecture, not cosmetic sugar. It prevents widget layout assumptions from collapsing into unreadable output when the terminal is too small.

## Overview Composition

The Overview view is laid out as:

- header
- main body
- footer

Inside the main body:

- left pane: agent list
- right top: detail
- right bottom: activity

The help overlay is rendered on top when active.

This split maps well to the mental model of the app:

- list for navigation
- detail for current selection
- activity for temporal context

## Accounts Composition

The Accounts view has its own renderer because it is structurally different from Overview.

It combines:

- profile list
- profile details and quota display
- edit and confirmation overlays
- provider-specific status/metadata presentation

The Accounts screen is not just a variant of the Overview list/detail split. It is a workflow-heavy interface and therefore warrants a dedicated renderer.

## Render Data

The Overview renderer uses a `RenderData` helper derived from `AppState`.

That helper precomputes things like:

- visible agents
- selected agent
- header stats

This keeps widget functions from each independently recomputing state slices and makes the rendering layer easier to reason about.

## Input Mapping

`src/input/` translates raw `crossterm` events into an `Action` enum.

Important properties of this mapping:

- it filters for key press events
- it respects modal state such as filter, rename, edit, and confirmation modes
- it consumes resize events and lets the next draw use the new dimensions
- it maps only to application-level actions, not direct state mutations

That last point matters. Input mapping should answer:

"What did the user ask to do?"

not

"How exactly should state change?"

The state layer answers the second question.

## Footer And Help As Interaction Aids

The footer and help overlay are part of the interaction architecture, not just styling.

They serve two functions:

- discoverability for first-time users
- mode clarity for existing users

The footer changes with modal state and active workspace. The help overlay acts as an in-app command reference instead of forcing users back to the README.

## Empty And Degraded States

The agent list renderer owns the empty-state message for Overview.

The current design distinguishes between:

- early scanning state
- healthy empty state
- degraded detector state

This matters because a public-facing TUI should not make "nothing found yet" indistinguishable from "a required dependency is missing."

## Terminal Behavior Constraints

When changing the UI layer, preserve these constraints:

- never rely on stdout logging during TUI rendering
- keep rendering pure relative to `AppState`
- preserve the small-terminal fallback path
- treat overlays as render-time concerns backed by explicit state, not ad hoc widget-local behavior

The UI layer should remain easy to redraw repeatedly without hidden side effects.

## Styling And Theme

`src/ui/theme.rs` centralizes colors and style helpers.

That makes global visual changes easier and reduces the risk of each widget inventing inconsistent color semantics. It also makes it easier to reason about which colors communicate status, warnings, dimmed text, and structural boundaries.

## Where To Be Careful

Common ways to damage this layer:

- adding state mutation into render code
- duplicating filter/selection logic inside widgets
- introducing widget output that assumes large terminals
- making modal key handling inconsistent between footer/help/actual input logic
- coupling provider-specific business decisions directly into generic widgets

The UI layer should stay a view layer with disciplined, explicit boundaries to the app logic.
