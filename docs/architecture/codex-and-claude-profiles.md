# Codex And Claude Profiles

This document explains the profile-management side of `nexus`.

This is the most operationally sensitive part of the codebase because it deals with:

- local credential files
- profile storage
- writing auth back into provider home directories
- best-effort quota refresh
- runtime isolation for launched Codex sessions

## Scope

There are two related but distinct concerns here:

1. profile persistence and account switching in the Accounts tab
2. provider-specific runtime and quota behavior

The code is grouped by provider under:

- `src/codex_profiles/`
- `src/claude_profiles/`

while the workflow logic that binds those pieces into the Accounts workspace currently lives in `src/app/accounts.rs` and the reducer/effect bridge around it.

## Storage Model

The app maintains its own profile storage under the platform data directory rather than treating provider home directories as the authoritative profile store.

That means:

- imported profiles are copied into app-managed storage
- metadata such as labels/notes/enabled state live alongside that managed storage
- writing a selected profile back to the provider home directory is an explicit user action

This is a good design for account switching because it separates:

- app-managed profile inventory
- provider-active runtime credential file

## Codex Profiles

Codex profile handling includes:

- importing `~/.codex/auth.json`
- storing imported auth in the app’s Codex profile directory
- extracting metadata such as email/plan where possible
- writing selected profile auth back to `~/.codex/auth.json`

The important architectural point is that profile import is a copy, not a live bind to the home auth file.

## Claude Profiles

Claude profile handling is similar in spirit but differs in sources and refresh behavior.

Import sources:

- macOS Keychain via `security`
- Linux `~/.claude/.credentials.json`

After import, credentials are stored in the app’s Claude profile directory with corresponding metadata.

Writing a selected Claude profile back to the home directory is also an explicit action.

## Codex Runtime Materialization

Codex has a dedicated runtime-isolation model in `src/codex_profiles/runtime.rs`.

`RuntimePlan` defines:

- profile directory
- repo directory
- runtime home
- auth path
- shared sessions path
- shared SQLite home

Materialization creates an isolated runtime directory and injects:

- `auth.json` copied from the chosen profile
- a symlink to the per-repo `sessions` directory
- a per-repo SQLite home path

This model is designed to balance two goals:

- isolate account credentials by selected profile
- preserve repo-local session continuity and state sharing where that is operationally useful

The repo runtime key is derived from the canonical repo path, which avoids the older fixed-key collision risk.

## Quota Refresh

Quota refresh is provider-specific.

### Codex

Codex quota refresh uses provider auth plus an API-backed fetch path, with fallback/helper parsing paths still present for legacy or degraded scenarios.

### Claude

Claude quota refresh can involve token refresh first, then a quota fetch request. The token refresh path may update the stored imported credentials.

In both cases, the UI treats quota as best-effort operational data rather than authoritative billing truth.

## Accounts Workflow Layer

`src/app/accounts.rs` still acts as the legacy bridge that ties profile storage and provider logic into user workflows while reducer/effect ownership continues to expand.

It owns:

- import actions
- edit mode lifecycle
- delete/write confirmation behavior
- status messages
- provider filter behavior
- manual quota refresh actions

This layer is where file operations become user-facing workflows.

## Persistence Boundaries

A useful way to think about the persistence model:

- provider home directories represent the active external runtime state
- app-managed profile directories represent stored switchable identities
- cache files represent best-effort operational metadata such as quota snapshots

Contributors should preserve these boundaries. Blurring them leads to surprising behavior, such as the app silently overwriting active provider credentials without explicit user intent.

## Security And Privacy Assumptions

The application is local-first, but it still handles sensitive auth material.

Important assumptions:

- credentials are stored locally on disk once imported
- writing back to provider home directories is explicit, not automatic
- quota refresh sends provider-issued credentials only to the issuing provider
- logs should not become an accidental credential sink

The codebase does not try to pretend this is a hardened secret-management system. It is a local operator tool, so safety comes from clear boundaries and explicit workflows rather than from a complex encryption layer.

## Operational Risks

The riskiest areas in this subsystem are:

- accidental credential overwrite behavior
- runtime layout changes that break Codex session continuity
- undocumented quota endpoints drifting
- metadata extraction assumptions becoming stale
- token refresh behavior persisting incomplete or invalid state

If you change these flows, review both the user-facing README guidance and the integration-oriented tests. This is one of the areas where a seemingly small file-handling change can create real operator frustration.
