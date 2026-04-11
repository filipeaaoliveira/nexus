# nexus

Terminal dashboard and monitor for local AI coding agents, with partial account-management surfaces for Codex and Claude profiles.

`nexus` now combines two workspaces in one TUI:

- `Overview`: passive monitoring for local agent sessions
- `Accounts`: manages Codex and Claude profiles, best-effort quota inspection, and auth management workflows

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

There are no pre-built binaries. There are no tagged releases; the `main` branch is the current stable version. Install from source or via `cargo install --git`.

## Requirements

- Rust `1.94`
- macOS or Linux
- An interactive terminal
- `ps` — used for process detection (standard on macOS and Linux)
- `lsof` — used to resolve process working directories (standard on macOS; available via `util-linux` or `lsof` package on Linux)
- `git` — used for branch and worktree detection (must be on `PATH`)
- Codex installed locally if you want to use Codex-specific actions in the `Accounts` workspace
- OpenCode installed locally if you want OpenCode sessions to appear in the `Overview` workspace

If `lsof` is absent, process detection degrades silently: agents will not appear in the `Overview` workspace even if they are running. Install it with your system package manager if needed (e.g. `apt install lsof`).

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

The `RUST_LOG` environment variable controls the log level (default: `info`). Logs go to the log file, never to stdout, so they do not interfere with the TUI.

```bash
RUST_LOG=debug nexus
```

If none of the supported agents (Claude, Codex, OpenCode) are running, the `Overview` workspace will be empty. Start any of the supported tools and `nexus` will detect them within one poll interval.

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
