# Releasing nexus

This document defines the minimum release discipline for publishing `nexus` as a public repository with tagged releases.

## Goals

Each release should:

- point users at a stable Git tag instead of asking them to trust a moving `main`
- have human-readable release notes
- attach pre-built Linux and macOS binary archives
- match the commands and behavior maintainers actually verified
- leave a clear audit trail in the changelog and Git history

## Release Preconditions

Before cutting a release:

- the README installation/build commands must still match the verified workflow
- public-facing docs must reflect current behavior
- no known public-release blockers should remain open

Run the full verification set:

```bash
cargo test --locked
cargo clippy --all-targets --all-features -- -D warnings
cargo build --release --locked
cargo deny check
```

If the release changes docs or contributor-facing behavior in meaningful ways, do a manual sanity pass on:

- `README.md`
- `CONTRIBUTING.md`
- architecture docs
- changelog entries for the release

## Changelog Discipline

`CHANGELOG.md` is the source of truth for release notes.

Before tagging:

1. move relevant items from `Unreleased` into a new version heading
2. use the release date in `YYYY-MM-DD` format
3. keep entries grouped under clear headings such as `Added`, `Changed`, and `Fixed`
4. write for users and maintainers, not commit archaeology

If a change is not important enough to mention in release notes, think carefully about whether it matters as a release-visible change.

## Versioning

The project changelog says it follows Semantic Versioning.

In practice:

- patch: bug fixes and documentation corrections that do not change intended behavior
- minor: new features, new workflows, or meaningful UX/behavior changes that remain backward-compatible for users
- major: compatibility or workflow breaks that require users or maintainers to adjust behavior

Do not over-theorize versioning. The main requirement is that tags communicate change scope honestly.

## Tagging

Use annotated tags. Pushing a tag matching `v*.*.*` starts the release workflow, which builds binary archives and creates or updates the matching GitHub Release.

Example:

```bash
git tag -a v0.3.0 -m "nexus v0.3.0"
git push origin v0.3.0
```

Annotated tags are preferable because they carry release intent and are better artifacts for public distribution than lightweight tags.

## Release Workflow

The release workflow lives at `.github/workflows/release.yml`.

On a pushed `v*.*.*` tag, it:

1. checks out the tagged revision
2. builds `cargo build --release --locked` on Ubuntu and macOS runners
3. packages the binary with `README.md`, `LICENSE`, and `CHANGELOG.md`
4. uploads a Linux archive named with the runner host triple
5. uploads a macOS archive named with the runner host triple
6. creates or updates the GitHub Release for the tag

Release notes are extracted from the matching `CHANGELOG.md` section. For tag `v0.3.0`, the workflow looks for `## [0.3.0]`. If that section is missing, it creates a minimal fallback note pointing readers to the changelog.

The workflow can also be run manually with `workflow_dispatch` by entering an existing tag name such as `v0.3.0`.

Do not make release notes say something broader than the changelog says.

## README Alignment

README install guidance should match the maintained distribution channels:

- GitHub Release archives for tagged builds
- source build for users who want to build locally
- `cargo install --git` as a supported convenience path

Only recommend additional distribution channels after they are actually maintained.

## Suggested Release Checklist

- [ ] `CHANGELOG.md` updated
- [ ] release version chosen intentionally
- [ ] `cargo test --locked` passes
- [ ] `cargo clippy --all-targets --all-features -- -D warnings` passes
- [ ] `cargo build --release --locked` passes
- [ ] `cargo deny check` passes
- [ ] README and public docs match current behavior
- [ ] annotated tag created
- [ ] tag pushed
- [ ] release workflow completed successfully
- [ ] GitHub Release exists with Linux and macOS archives

## After Release

Immediately reopen the changelog with a fresh `Unreleased` section and continue documenting changes there. That keeps release-note discipline continuous instead of forcing a scramble at the next tag.
