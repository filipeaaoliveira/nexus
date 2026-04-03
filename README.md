# ai-overview

Read-only terminal dashboard for monitoring local AI coding agents.

`ai-overview` watches local agent state for:
- Codex
- Claude Code
- OpenCode

It aggregates process discovery, session state, tool calls, subagents, and todos/plans into a single TUI so you can see what is active without switching between terminals.

## Status

This project is early-stage but already usable.

Current focus:
- passive local monitoring only
- no remote control or mutation
- local terminal-first workflow

## Installation

### From source

```bash
git clone https://github.com/filipeaaoliveira/ai-overview.git
cd ai-overview
cargo build --release
./target/release/ai-overview
```

### Directly from Git

```bash
cargo install --git https://github.com/filipeaaoliveira/ai-overview.git
ai-overview
```

## Requirements

- Rust `1.94.1`
- macOS or another environment with compatible `ps`, `lsof`, and `git` command behavior
- an interactive terminal

The app currently reads local state from these default locations:
- Claude: `~/.claude`
- Codex: `~/.codex`
- OpenCode: `~/.local/share/opencode`

## Usage

Run:

```bash
ai-overview
```

Current CLI flag:

```text
--poll-interval <SECONDS>
--log-file <PATH>
```

Example:

```bash
ai-overview --poll-interval 2
ai-overview --log-file ~/.cache/ai-overview/debug.log
```

## Keybindings

- `j` / `Down`: move selection down
- `k` / `Up`: move selection up
- `/`: start filter
- `s`: cycle sort mode
- `d`: remove selected agent from the view
- `D`: remove all exited agents from the view
- `r`: rename selected agent locally in the UI
- `?`: toggle help
- `Esc`: dismiss overlay or clear active input mode
- `q`: quit

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

## Notes

- This is a read-only dashboard. It does not send commands to agents.
- The app currently requires a real terminal for startup, including normal CLI execution.
- Logs default to a standard cache location such as `~/.cache/ai-overview/ai-overview.log` and can be overridden with `--log-file`.
- Process discovery currently targets Unix-like environments and depends on `ps`, `lsof`, and `git` output formats. macOS is the primary supported environment today.

## Contributing

Contributions are welcome. Start with [`CONTRIBUTING.md`](CONTRIBUTING.md).

## License

MIT. See [`LICENSE`](LICENSE).
