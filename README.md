# nexus

Terminal dashboard and monitor for local AI coding agents, with partial account-management surfaces for Codex and Claude profiles.

`nexus` now combines two workspaces in one TUI:

- `Overview`: passive monitoring for local agent sessions
- `Accounts`: manages Codex and Claude profiles, best-effort quota inspection, and partial Codex launch/profile workflows

The goal is one terminal surface for both observing active agents and managing Codex and Claude account workflows without opening a second TUI.

## Status

This project is still early-stage, but the core two-workspace model is in place.

Current focus:

- local terminal-first workflow
- passive monitoring for multiple agent tools
- account-specific operational controls isolated to the `Accounts` tab

## Installation

### From source

```bash
git clone https://github.com/filipeaaoliveira/nexus.git
cd nexus
cargo build --release
./target/release/nexus
```

### From Git

```bash
cargo install --git https://github.com/filipeaaoliveira/nexus.git
nexus
```

## Requirements

- Rust `1.94`
- macOS or another environment with compatible `ps`, `lsof`, and `git` command behavior
- an interactive terminal
- Codex installed locally if you want to use Codex-specific actions in the `Accounts` workspace

Default local state roots:

- Claude: `~/.claude`
- Codex agent detection: `~/.codex`
- nexus Codex management data: platform local data dir under `nexus/codex/`
- OpenCode: `~/.local/share/opencode`

## Usage

Run:

```bash
nexus
```

Current CLI flags:

```text
--poll-interval <SECONDS>
--log-file <PATH>
```

Examples:

```bash
nexus --poll-interval 2
nexus --log-file ~/.cache/nexus/debug.log
```

## Workspaces

### Overview

The `Overview` workspace is the original monitor:

- left pane: discovered agents
- right pane: details and recent activity
- detector-driven and read-heavy

### Accounts

The `Accounts` workspace is the operational workspace for Codex and Claude profile management:

- profile list
- selected profile details
- best-effort quota state
- launch/profile actions are still being layered in

> Codex detection and session continuity are currently in recovery after storage/runtime drift, and some Codex state remains best-effort until the app-server-backed integration lands.

The intent is to keep account-management behavior out of the `Overview` workspace.

## Accounts Tab

The `Accounts` tab is designed around per-profile auth and repo-shared session continuity for Codex workflows.

Primary workflow:

1. Import or manage a Codex or Claude profile
2. Select the desired profile in the `Accounts` tab
3. Launch and resume workflows are still being layered in
4. When one account hits a limit, switch profiles and continue the same repo work

This is the intended model for `codex resume <id>` across account switches for the same repository, but the full workflow is still being layered in.

## Safe Launch Model

The intended Codex account-switching model is built around:

- profile-specific auth state
- repo-shared session state
- repo-shared sqlite runtime state

The current TUI does not expose this as a fully active end-to-end launch flow.

The underlying model is meant to:

- keep profile auth isolated
- keep repo session continuity stable
- avoid breaking concurrent Codex tabs that are already running

Compatibility/global activation remains a separate, explicit workflow rather than the default.

## Quota Limitations

Quota data in the `Accounts` workspace is best-effort.

It may come from API-backed refreshes, parsing, cached values, or be unavailable. The underlying model tracks confidence, but the current UI does not surface those confidence states yet.

Quota should not be treated as authoritative billing or quota truth.

## Keybindings

Global:

- `Tab`: switch workspaces
- `?`: toggle help
- `Esc`: dismiss overlay or active input mode
- `q`: quit

Overview:

- `j` / `Down`: move selection down
- `k` / `Up`: move selection up
- `/`: start filter
- `s`: cycle sort mode
- `d`: remove selected agent from the view
- `D`: remove all exited agents from the view
- `r`: rename selected agent locally in the UI

Accounts:

- `j` / `Down`: move selected profile
- `k` / `Up`: move selected profile
- `/`: start filter input
- `f`: cycle provider filter
- `i`: import Codex auth
- `I`: import Claude credentials
- `w`: write selected profile to home auth file
- `e`: edit selected profile
- `d`: delete selected profile
- `Q`: quota edit
- `R`: refresh quota
- `Enter`: currently a no-op

## Development

Format:

```bash
cargo fmt --all
```

Lint:

```bash
cargo clippy --all-targets --all-features -- -D warnings
```

Test:

```bash
cargo test --locked
```

Build:

```bash
cargo build --locked
```

## Notes

- `Overview` remains a passive monitor for agent discovery and activity.
- `Accounts` is the explicit control surface for Codex and Claude profile workflows.
- The app requires a real terminal for startup.
- Logs default to a cache location such as `~/.cache/nexus/nexus.log` and can be overridden with `--log-file`.
- Process discovery still depends on Unix-like `ps`, `lsof`, and `git` output formats. macOS remains the primary supported environment.

## Contributing

Contributions are welcome. Start with [`CONTRIBUTING.md`](CONTRIBUTING.md).

## License

MIT. See [`LICENSE`](LICENSE).
