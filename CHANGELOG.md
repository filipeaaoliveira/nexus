# Changelog

All notable changes to this project will be documented here.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
This project uses [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0] - 2026-04-10

### Added
- Claude accounts tab: import, manage, and switch Claude profiles alongside Codex profiles
- Per-profile quota inspection for both Claude and Codex accounts
- `I` keybinding to import Claude credentials from keychain or `~/.claude/.credentials.json`
- `R` keybinding to manually refresh quota for the selected profile
- `f` keybinding to cycle provider filter (All / Codex / Claude) in the Accounts tab
- Help overlay (`?`) covering all workspaces and modes

### Changed
- Renamed `Codex` workspace to `Accounts` to reflect multi-provider scope
- Background quota polling now runs per-profile for both Codex and Claude accounts
- Terminal is correctly restored and reinitialised after a Codex session subprocess exits

### Fixed
- Panic hook now restores terminal state before propagating, preventing broken terminal on crash
- Credential import no longer uses a predictable fixed temp file path

