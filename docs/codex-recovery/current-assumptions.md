# Current Codex Maintenance Notes

This note exists to document the current Codex-specific integration points that are most likely to drift over time.

## Current runtime layout

- Profile auth is stored per imported profile under the nexus data directory.
- A profile-specific `CODEX_HOME` is materialized before launching Codex.
- Session history is shared through a symlinked per-repo `sessions` directory.
- SQLite state is shared through `-c sqlite_home=...`.
- Repo runtime state is keyed per canonical repo path, not by a fixed `current-repo` value.

## Current detection assumptions

- Codex detection reads `~/.codex/state_5.sqlite`.
- Thread metadata is read from the `threads` table.
- Subagent edges are read from the `thread_spawn_edges` table when present.
- Rollout events are read from JSONL files referenced by `rollout_path`.

## Current quota assumptions

- API-backed quota refresh uses the ChatGPT web quota endpoint.
- Legacy `codex status` text parsing remains as a fallback/helper path.
- Rollout `token_count` parsing remains as a fallback/helper path.
- The current UI expects the common `5h` and `weekly` windows, though the underlying quota model can store generic windows.

## Ongoing maintenance risks

- SQLite schema drift can break Codex detection.
- Undocumented upstream quota endpoints can change without notice.
- Rollout event shapes can drift and weaken fallback parsing.

## Historical note

Older development notes referenced a fixed `current-repo` runtime key and the resulting risk of cross-repo state collisions. That is no longer the current design and should not be treated as a live risk for the present implementation.
