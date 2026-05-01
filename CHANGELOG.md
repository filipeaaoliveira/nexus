# Changelog

All notable changes to this project will be documented here.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
This project uses [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0] - 2026-05-01

### Added
- Passive `Overview` workspace for monitoring Claude, Codex, and OpenCode agent activity.
- `Accounts` workspace for importing, managing, and switching Claude and Codex profiles.
- Per-profile quota inspection for Claude and Codex accounts.
- Keybindings for profile import, quota refresh, provider filtering, help, and workspace navigation.
- GitHub Actions CI for formatting, clippy, tests, docs, release builds, and dependency policy checks.
- Tagged GitHub release workflow that builds and uploads macOS and Linux binary archives.

### Changed
- Extracted shared domain types into the `nexus-domain` workspace crate.
- Runtime configuration now builds paths directly from CLI and platform defaults.
- Background quota polling runs per profile for Claude and Codex accounts.
- GitHub Actions JavaScript actions opt into the Node.js 24 runtime.
- Public documentation now reflects the current release, CI, dependency, and architecture state.

### Fixed
- Terminal state is restored during panic handling and after Codex subprocess sessions.
- Credential import avoids predictable fixed temporary file paths.
- Reducer session and PID handling avoids duplicate or stale agent rows.
- External process scanning uses timeouts for `ps` and `lsof` calls.
- Profile metadata parsing reports typed corruption errors instead of stringly error wrappers.
- `cargo deny check` passes without warnings.
