# Detection Architecture

This document explains how `nexus` discovers external agent state and turns it into application events.

## Detection Philosophy

Detection is observational, not authoritative.

The app does not control Claude, Codex, or OpenCode. It watches the state they leave behind:

- processes
- provider-specific local databases or transcripts
- rollout/event files
- git worktree metadata

Because the app is downstream of other tools, the detection layer is the part of the codebase most exposed to upstream drift.

## Module Layout

The detection system lives under `src/detect/`.

Key modules:

- `mod.rs`: detector orchestration and event forwarding
- `process.rs`: process discovery and cwd resolution
- `registry.rs`: launch/session binding and deduplication support
- `worktree.rs`: git branch/worktree lookup
- `heuristics.rs`: shared text classification helpers
- provider detectors under `detect/claude`, `detect/codex`, and `detect/opencode`

The modules are intentionally split between:

- provider-agnostic plumbing
- provider-specific readers/parsers

## Process Scanner

The process scanner is the base layer for runtime discovery.

It uses:

- `ps` to enumerate processes
- `lsof` to map process IDs to working directories

The working-directory resolution matters because it lets the app correlate visible runtime processes with repository context.

Architecturally, the scanner does more than just find PIDs:

- it extracts provider-specific launch hints from command lines
- it caches CWD lookups
- it emits `Discovered` and `ProcessExited` events
- it now emits detector-health degradation state when required dependencies are missing or unusable

This scanner is not optional in practice. If it fails, the Overview loses a large part of its usefulness.

## Registry

`registry.rs` is a coordination helper between different detection sources.

Its job is to remember relationships such as:

- which PID was observed for which provider/cwd
- launch metadata for Claude/Codex/OpenCode
- Codex thread-to-process relationships when they can be inferred

Without the registry, provider-specific detectors would be forced to guess too much from partial local state. The registry lets them combine process evidence with database/transcript evidence more safely.

## Worktree Enrichment

The forwarding path in `detect/mod.rs` enriches discovered sessions with branch and worktree information.

The flow is:

1. raw detectors emit a `Discovered` event
2. the forwarder notices it and asynchronously looks up git context
3. a `SessionInfo` event is emitted with branch/worktree metadata

That split keeps branch lookup out of the hot discovery path and lets enrichment remain a separate concern from raw detection.

## Claude Detection

Claude detection is transcript-oriented.

It scans Claude project/session files and infers state from transcript structure and event types such as:

- assistant messages
- tool-use/tool-result transitions
- completion/blocking markers
- TODO-like structures

The detector does not simply map raw transcript entries to UI strings. It builds intermediate interpretations that are later normalized into cross-provider state.

This detector is relatively parser-heavy because Claude state is richer and more text/event-sequence oriented than a simple process table.

## Codex Detection

Codex detection is database-oriented with supplemental rollout parsing.

It reads:

- `~/.codex/state_5.sqlite`
- thread metadata
- thread-spawn edges
- rollout files referenced by `rollout_path`

Codex detection is the most schema-sensitive part of the codebase. Changes in:

- table names
- column names
- expected thread/edge shapes
- rollout event structures

can degrade or break detection behavior.

This is why Codex-specific tests are numerous and why backend-degraded conditions are explicitly modeled.

## OpenCode Detection

OpenCode detection is also database-oriented, but the semantics differ from Codex.

It reads the OpenCode local database and reconstructs session/message/part state into the app’s event model. Like Codex, it is vulnerable to upstream schema drift, but it uses a different shape of source data and a different set of assumptions.

## Event Emission

Detectors do not mutate `AppState` directly.

They emit `AgentEvent` values such as:

- discovery
- status changes
- activity changes
- subagent lifecycle events
- TODO updates
- session metadata
- detector-health changes

This is the key boundary that keeps the detection layer understandable. A detector should answer:

"What did I observe?"

not

"How should the UI behave?"

## Failure Model

There are two broad failure categories:

### Hard detector failure

Examples:

- SQLite open/query failure
- unexpected provider backend breakage

These are handled by restart logic with backoff in `detect/mod.rs`.

### Soft degradation

Examples:

- missing `lsof`
- unavailable cwd resolution
- partial provider drift that leaves some state unreadable

These should degrade the feature and surface useful state without crashing the whole app.

## What Contributors Most Often Break

The riskiest changes in detection are:

- relaxing matching logic until unrelated sessions collapse together
- tightening matching logic until real sessions stop binding
- changing transcript/database parsing assumptions without updating tests
- forgetting that discovery and enrichment are separate phases
- adding presentation concerns to detectors instead of emitting cleaner events

If you change detection behavior, review:

- provider-specific tests
- `tests/detect_*`
- state reconciliation expectations in `tests/app_test.rs`

Detection changes often look local but produce user-visible regressions one layer later.
