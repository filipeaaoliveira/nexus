# nexus

Terminal dashboard and control surface for local AI coding agents.

`nexus` now combines two workspaces in one TUI:

- `Overview`: passive monitoring for local agent sessions
- `Codex`: Codex profile management, best-effort quota inspection, and safe Codex launch workflows

The goal is one terminal surface for both observing active agents and continuing Codex work under another account without opening a second TUI.

## Status

This project is still early-stage, but the core two-workspace model is in place.

Current focus:

- local terminal-first workflow
- passive monitoring for multiple agent tools
- Codex-specific operational controls isolated to the `Codex` tab

## Installation

### Quick install (recommended)

```bash
curl -sL https://raw.githubusercontent.com/filipeaaoliveira/nexus/main/install.sh | bash
```

Or manually:

```bash
git clone https://github.com/filipeaaoliveira/nexus.git
cd nexus
./install.sh
```

### From source

```bash
git clone https://github.com/filipeaaoliveira/nexus.git
cd nexus
cargo build --release
./target/release/nexus
```

### Directly from Git

```bash
cargo install --git https://github.com/filipeaaoliveira/nexus.git
nexus
```

## Requirements

- Rust `1.94.1`
- macOS or another environment with compatible `ps`, `lsof`, and `git` command behavior
- an interactive terminal
- Codex installed locally if you want to use the `Codex` workspace launch flow

Default local state roots:

- Claude: `~/.claude`
- Codex agent detection: `~/.codex`
- nexus Codex management data: platform local data dir under `nexus/codex/`
- OpenCode: platform local data dir under `nexus/opencode/`

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

### Codex

The `Codex` workspace is the operational workspace for Codex account switching:

- profile list
- selected profile details
- best-effort quota state
- launch-oriented Codex session flow

The intent is to keep Codex mutation and launch behavior out of the `Overview` workspace.

## Codex Tab

The `Codex` tab is designed around per-profile auth and repo-shared session continuity.

Primary workflow:

1. Import or manage a Codex profile
2. Select the desired profile in the `Codex` tab
3. Launch Codex for the current repo through `nexus`
4. When one account hits a limit, switch profiles and continue the same repo work

This is built specifically so `codex resume <id>` can continue working across account switches for the same repository.

## Safe Launch Model

Codex account switching is implemented with:

- profile-specific auth/config state
- repo-shared session state
- repo-shared sqlite runtime state

`nexus` does not rely on swapping a single shared `~/.codex/auth.json` during the normal flow.

Instead, launch behavior is intended to:

- keep profile auth isolated
- keep repo session continuity stable
- avoid breaking concurrent Codex tabs that are already running

Compatibility/global activation remains a separate, explicit workflow rather than the default.

## Quota Limitations

Quota information in the `Codex` tab is best-effort only.

Rules:

- it is based on local Codex-visible output only
- it may be parsed, cached, stale, or unavailable
- it should not be treated as authoritative billing or quota truth

The UI should distinguish:

- exact
- parsed
- cached
- unavailable

If quota parsing fails, the app should prefer showing stale cached data or “unavailable” rather than claiming precision it cannot justify.

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

Codex:

- `j` / `Down`: move selected profile
- `k` / `Up`: move selected profile
- `/`: start Codex profile filter
- additional Codex launch/profile actions are being layered in behind this workspace

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
- `Codex` is no longer passive-only; it is the explicit control surface for Codex profile workflows.
- The app requires a real terminal for startup.
- Logs default to a cache location such as `~/.cache/nexus/nexus.log` and can be overridden with `--log-file`.
- Process discovery still depends on Unix-like `ps`, `lsof`, and `git` output formats. macOS remains the primary supported environment.

## Contributing

Contributions are welcome. Start with [`CONTRIBUTING.md`](CONTRIBUTING.md).

## License

MIT. See [`LICENSE`](LICENSE).
