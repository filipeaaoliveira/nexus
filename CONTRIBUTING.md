# Contributing

Thanks for contributing to `ai-overview`.

## Development workflow

1. Fork the repository.
2. Create a focused branch.
3. Make the smallest coherent change that solves the problem.
4. Run formatting, linting, and tests before opening a pull request.

Required checks:

```bash
cargo fmt --all
cargo clippy --all-targets --all-features -- -D warnings
cargo test --locked
```

## Scope expectations

- Keep the project read-only unless a change is explicitly intended to alter that model.
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
