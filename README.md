# nexus

Terminal dashboard and monitor for local AI coding agents, with partial account-management surfaces for Codex and Claude profiles.

`nexus` now combines two workspaces in one TUI:

- `Overview`: passive monitoring for local agent sessions
- `Accounts`: manages Codex and Claude profiles, best-effort quota inspection, and auth management workflows

The goal is one terminal surface for both observing active agents and managing Codex and Claude account workflows without opening a second TUI.

## What It Solves

`nexus` is for people who already use local terminal-based coding agents and want one place to:

- see which agent sessions are active
- inspect their current state and recent activity
- switch between Codex and Claude account profiles when auth/quota workflows matter

It is not a general-purpose terminal multiplexer, IDE, or remote orchestration system. It is a local operator dashboard for existing CLI-based agent workflows.

## Intended Audience

- developers who already run Claude, Codex, and/or OpenCode locally
- terminal-first users who want passive session visibility
- maintainers who need to switch local auth profiles without manually copying credential files around

If you are not already using these tools locally, `nexus` will not be useful on first run.

## Status

This project is still early-stage, but the core two-workspace model is in place.

Current focus:

- local terminal-first workflow
- passive monitoring for multiple agent tools
- account-specific operational controls isolated to the `Accounts` tab

## Installation

### From GitHub Releases

Tagged releases provide pre-built macOS and Linux archives on the GitHub Releases page:

- `nexus-vX.Y.Z-x86_64-unknown-linux-gnu.tar.gz`
- `nexus-vX.Y.Z-<macos-host-triple>.tar.gz`

Download the archive for your platform, extract it, and run the included `nexus` binary from your terminal.

### From source

```bash
git clone https://github.com/filipeaaoliveira/nexus.git
cd nexus
cargo build --release --locked
./target/release/nexus
```

### From Git

```bash
cargo install --git https://github.com/filipeaaoliveira/nexus.git --locked
nexus
```

CI validates locked build, lint, test, and release-build workflows. Tagged releases additionally build and attach macOS/Linux binary archives.
`cargo install --git` is supported, but it is not currently a dedicated CI smoke-test path.
The repository is kept `cargo fmt --all -- --check`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo test --locked`, `cargo build --release --locked`, and `cargo deny check` clean on the main tree.

## Requirements

- Rust `1.94.1`
- macOS or Linux
- An interactive terminal
- `ps` — used for process detection (standard on macOS and Linux)
- `lsof` — used to resolve process working directories (standard on macOS; available via `util-linux` or `lsof` package on Linux)
- `git` — used for branch and worktree detection (must be on `PATH`)
- Codex installed locally if you want to use Codex-specific actions in the `Accounts` workspace
- OpenCode installed locally if you want OpenCode sessions to appear in the `Overview` workspace

If `lsof` is absent, the `Overview` workspace will show a degraded process-scanner warning and agent detection will not work until `lsof` is installed. Install it with your system package manager if needed (e.g. `apt install lsof`).

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

## Quick Start

Typical first-use path:

1. Start one of the supported tools locally (`claude`, `codex`, or OpenCode).
2. Launch `nexus`.
3. Stay on `Overview` to confirm the session appears in the left pane.
4. Press `?` to open the full keybinding overlay.
5. Press `Tab` to switch to `Accounts` only when you need profile or quota workflows.

If no supported agents are running, `Overview` should be empty. If scanner dependencies are missing, `Overview` should show a degraded warning instead of a fake healthy empty state.

Sample workflows:

- passive monitoring: start `codex` or `claude`, then run `nexus` in another terminal and watch status changes in `Overview`
- account switching: open `Accounts`, import a profile, select it, then press `w` to write it back to the tool home directory
- troubleshooting: run `RUST_LOG=debug nexus --log-file ~/.cache/nexus/debug.log` and inspect the log file after reproducing the issue

## Configuration Reference

`nexus` does not currently use a config file. Runtime behavior is controlled by CLI flags, environment variables, and the on-disk state of the tools it reads from.

CLI flags:

```text
--poll-interval <SECONDS>
--log-file <PATH>
```

- `--poll-interval <SECONDS>`: polling interval for detector refreshes
- `--log-file <PATH>`: override the default log file path

Examples:

```bash
nexus --poll-interval 2
nexus --log-file ~/.cache/nexus/debug.log
```

Environment variables:

- `RUST_LOG`: controls the log level for file logging; default is `info`

Logs go to the log file, never to stdout, so they do not interfere with the TUI.

```bash
RUST_LOG=debug nexus
```

Relevant file and state locations:

- Claude home: `~/.claude`
- Codex runtime/detection state: `~/.codex`
- OpenCode runtime/detection state: `~/.local/share/opencode`
- nexus-managed Codex profile data: platform local data dir under `nexus/codex/`
- default log file: cache dir such as `~/.cache/nexus/nexus.log`

If none of the supported agents (Claude, Codex, OpenCode) are running, the `Overview` workspace will be empty. Start any of the supported tools and `nexus` will detect them within one poll interval.

## First Run

What to expect on first launch:

- `Overview` opens first
- if supported agents are already running, they appear in the left pane within one poll interval
- if no supported agents are running, the overview stays empty
- if process detection dependencies are missing, the overview shows a degraded scanner warning instead of pretending everything is fine

For account-management workflows:

1. Press `Tab` to switch to `Accounts`
2. Import a Codex profile with `i` or a Claude profile with `I`
3. Select the profile you want
4. Press `w` to write that profile back to the tool home directory when you want to switch the active local account

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
- auth management workflows (import, write to home, delete)

The intent is to keep account-management behavior out of the `Overview` workspace.

## Accounts Tab

The `Accounts` tab manages Codex and Claude profiles.

Primary workflow:

1. Import a Codex profile (`i`) or a Claude profile (`I`)
2. Select the desired profile in the `Accounts` tab
3. Use `w` to write the selected profile's auth to the tool's home directory
4. When one account hits a limit, switch profiles and continue

## Claude Credential Import

The `I` keybinding imports Claude credentials into a new profile.

- **macOS**: tries the macOS Keychain first (`security find-generic-password`). If the keychain entry is absent, run `claude` once to log in and the entry will be created.
- **Linux**: reads `~/.claude/.credentials.json` directly. The file must already exist (run `claude` once to create it).

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
- `Enter`: confirm pending action (write, delete)
- `/`: start filter input
- `f`: cycle provider filter
- `i`: import Codex auth
- `I`: import Claude credentials
- `w`: write selected profile to home auth file (confirms)
- `e`: edit selected profile
- `d`: delete selected profile (confirms)
- `R`: refresh quota

## Troubleshooting

### Overview shows "No agents detected"

Check the basics first:

- make sure Claude, Codex, or OpenCode is actually running locally
- wait at least one poll interval before assuming detection failed
- confirm the tool stores session/runtime state in the expected default location

### Overview shows a degraded process-scanner warning

`nexus` depends on external system tools for process-to-working-directory resolution:

- `ps`
- `lsof`
- `git`

On Linux, `lsof` is the dependency most likely to be missing. Install it with your system package manager and restart `nexus`.

### Accounts import does not find Claude credentials

- on macOS, run `claude` once so the Keychain item exists
- on Linux, make sure `~/.claude/.credentials.json` already exists

### Quota data is missing or looks stale

- quota refresh is best-effort and depends on undocumented or internal upstream APIs
- try manual refresh with `R`
- check the log file with `RUST_LOG=debug`
- do not treat the displayed value as billing truth

### Need logs for a bug report

By default, logs are written to a cache location such as `~/.cache/nexus/nexus.log`. You can override the path with `--log-file`.

## Platform Support

- macOS: supported
- Linux: supported
- Windows: not supported at this time

The current implementation relies on Unix-oriented process and filesystem behavior, including `ps`, `lsof`, and Unix symlink semantics.

## Architecture

For a deep explanation of how the application works internally, start with:

- [`docs/architecture/README.md`](docs/architecture/README.md)

That architecture set covers:

- runtime lifecycle and data flow
- detection internals
- app state and event reconciliation
- UI/rendering/input structure
- Codex and Claude profile-management internals

Maintainers preparing tagged releases should also use:

- [`docs/releasing.md`](docs/releasing.md)

## Development

Development setup:

```bash
git clone https://github.com/filipeaaoliveira/nexus.git
cd nexus
cargo build --locked
```

Format:

```bash
cargo fmt --all
```

Check:

```bash
cargo check --locked
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

Release build:

```bash
cargo build --release --locked
```

Tagged-release process:

- release discipline and tag workflow: [`docs/releasing.md`](docs/releasing.md)
- changelog source of truth: [`CHANGELOG.md`](CHANGELOG.md)

## Network activity

When Codex or Claude profiles are present, `nexus` makes outbound HTTPS requests using the stored OAuth credentials to fetch quota state. These run automatically in the background every 30 seconds and on manual refresh (`R`).

Endpoints contacted:

| Provider | Endpoint | Purpose |
|---|---|---|
| Codex | `https://chatgpt.com/backend-api/wham/usage` | quota fetch |
| Claude | `https://api.anthropic.com/api/oauth/usage` | quota fetch |
| Claude | `https://api.anthropic.com/v1/oauth/token` | token refresh |

No telemetry, analytics, or data is sent anywhere else. Credentials are only sent to the provider that issued them.

## Notes

- `Overview` remains a passive monitor for agent discovery and activity.
- `Accounts` is the explicit control surface for Codex and Claude profile workflows.
- The app requires a real terminal for startup.
- Logs default to a cache location such as `~/.cache/nexus/nexus.log` and can be overridden with `--log-file`.

## Known limitations

### Codex detection

Detection reads `~/.codex/state_5.sqlite` directly. If Codex changes its SQLite schema or bumps the database version number, detection will stop working until updated. The Codex quota endpoint (`https://chatgpt.com/backend-api/wham/usage`) is an undocumented API and may change without notice.

### Claude quota

The Claude quota endpoint (`https://api.anthropic.com/api/oauth/usage`) is an internal API. Quota data is best-effort — it may be stale, unavailable, or become inaccurate if the API changes.

### OpenCode detection

Detection reads `~/.local/share/opencode/opencode.db`. Schema changes in OpenCode will require a corresponding update here.

## Contributing

Contributions are welcome. Start with [`CONTRIBUTING.md`](CONTRIBUTING.md).

## License

MIT. See [`LICENSE`](LICENSE).
