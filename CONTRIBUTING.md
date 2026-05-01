# Contributing

Thanks for contributing to `nexus`.

## Development workflow

1. Fork the repository.
2. Create a focused branch.
3. Make the smallest coherent change that solves the problem.
4. Run formatting, linting, and tests before opening a pull request.

Required checks:

```bash
cargo check --locked
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --locked
cargo deny check
```

## Development setup

```bash
git clone https://github.com/filipeaaoliveira/nexus.git
cd nexus
cargo build --locked
```

The project is currently maintained and tested on macOS and Linux. If you change process detection, terminal lifecycle handling, or filesystem behavior, verify the change on at least one supported platform beyond your local happy path.

`cargo deny check` is also enforced in CI. If you change dependencies, update `deny.toml` intentionally instead of adding broad ignores.

## Scope expectations

- The `Overview` workspace is passive and read-only. Changes that introduce writes or side effects there need explicit justification.
- The `Accounts` workspace manages profiles and credentials. Changes here must preserve the safe-launch model (profile auth isolation, repo-shared session continuity) and must not silently overwrite credentials.
- Prefer small, test-backed changes over large refactors.
- Preserve existing behavior unless the PR is explicitly fixing or changing it.

## Code style

- Follow idiomatic Rust and keep Clippy clean under `-D warnings`.
- Add tests for behavior changes and bug fixes.
- Avoid introducing public API unless it is intentionally supported.

## Pull requests

Please include:
- what changed
- why it changed
- how you verified it
- any follow-up work or limitations

If the change affects detection logic, include the real-world case or transcript shape that motivated it.

## Release-readiness expectations

Changes that affect public-user experience should be reviewed with an outside-user mindset:

- a missing dependency should not fail silently
- terminal startup and shutdown should not leave the shell in a broken state
- README instructions should match the commands maintainers actually verify
- platform limitations should be stated plainly

## Deep Internals

If you need the full internal model rather than the quick contributor summary, use the architecture docs:

- [`docs/architecture/README.md`](docs/architecture/README.md)
- [`docs/architecture/runtime-and-data-flow.md`](docs/architecture/runtime-and-data-flow.md)
- [`docs/architecture/detection.md`](docs/architecture/detection.md)
- [`docs/architecture/app-state-and-events.md`](docs/architecture/app-state-and-events.md)
- [`docs/architecture/ui-and-interaction.md`](docs/architecture/ui-and-interaction.md)
- [`docs/architecture/codex-and-claude-profiles.md`](docs/architecture/codex-and-claude-profiles.md)

For release/tagging workflow, see:

- [`docs/releasing.md`](docs/releasing.md)

## How Detection Works

Each supported tool (Claude, Codex, OpenCode) has its own detector under `src/detect/<tool>/`. Detectors run as independent async polling loops and emit `AgentEvent` variants over an `mpsc` channel.

- **Claude** — scans `~/.claude/projects/` for JSONL session files; infers status from event types (e.g. `tool_use`, `assistant` messages).
- **Codex** — reads a SQLite database at `~/.codex/state_5.sqlite`; reconciles a snapshot of threads and child threads against the previous poll to emit deltas.
- **OpenCode** — reads a SQLite database at `~/.local/share/opencode/opencode.db`; maps sessions, messages, and parts to agent state.

All detectors share helpers in `src/detect/`:
- `registry.rs` — deduplication registry that suppresses redundant events
- `worktree.rs` — git worktree detection
- `heuristics.rs` — shared text-classification helpers (review output, section markers)
- `process.rs` — PID-to-CWD resolution via `ps` / `lsof`

The app layer (`src/app/`) receives events from all detectors and applies them to `AppState`. The UI layer (`src/ui/`) reads `AppState` and renders it each frame.
