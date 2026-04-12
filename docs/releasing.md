# Releasing nexus

This document defines the minimum release discipline for publishing `nexus` as a public repository with tagged releases.

## Goals

Each release should:

- point users at a stable Git tag instead of asking them to trust a moving `main`
- have human-readable release notes
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

Use annotated tags.

Example:

```bash
git tag -a v0.3.0 -m "nexus v0.3.0"
git push origin v0.3.0
```

Annotated tags are preferable because they carry release intent and are better artifacts for public distribution than lightweight tags.

## GitHub Release Notes

After pushing the tag:

1. create a GitHub release for the tag
2. use the changelog entry as the basis for release notes
3. summarize major user-facing changes near the top
4. call out known limitations if they materially affect first-time users

Do not make GitHub release notes say something broader than the changelog says.

## README Alignment

Once tagged releases exist, revisit the install guidance.

Preferred progression:

- current state: source build or `cargo install --git`
- after reliable tagging: recommend tagged installs rather than a moving `main`
- only recommend additional distribution channels after they are actually maintained

The docs should describe the release story the project truly has, not the one it hopes to have later.

## Suggested Release Checklist

- [ ] `CHANGELOG.md` updated
- [ ] release version chosen intentionally
- [ ] `cargo test --locked` passes
- [ ] `cargo clippy --all-targets --all-features -- -D warnings` passes
- [ ] `cargo build --release --locked` passes
- [ ] README and public docs match current behavior
- [ ] annotated tag created
- [ ] tag pushed
- [ ] GitHub release created from changelog notes

## After Release

Immediately reopen the changelog with a fresh `Unreleased` section and continue documenting changes there. That keeps release-note discipline continuous instead of forcing a scramble at the next tag.
