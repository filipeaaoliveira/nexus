## What changed

<!-- Describe the change. -->

## Why

<!-- Explain the motivation. Link to an issue if one exists. -->

## How I verified it

<!-- List the steps or commands you ran to confirm this works. -->

## Limitations or follow-up work

<!-- Anything left out of scope, or known limitations of this change. -->

---

Pre-submit checklist:

- [ ] `cargo fmt --all` passes
- [ ] `cargo clippy --all-targets --all-features -- -D warnings` passes
- [ ] `cargo test --locked` passes
- [ ] If detection logic changed: the real-world case or transcript shape that motivated it is described above
