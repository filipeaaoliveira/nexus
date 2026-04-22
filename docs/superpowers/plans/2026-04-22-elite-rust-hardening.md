# Elite Rust Hardening Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Bring the nexus codebase from "strong" to "elite" Rust quality by hardening domain types, error handling, state encapsulation, and code organization.

**Architecture:** Five independent improvements applied in dependency order: domain types first (#1, #2), detection layer (#3), then app layer (#4, #5). Each task produces a compilable, test-passing commit.

**Tech Stack:** Rust 1.94, thiserror, chrono, rusqlite, ratatui, tokio

---

## File Map

### New files
- `src/ui/shared/format.rs` — duration formatting functions moved from domain
- `src/app/reducer/agent_lookup.rs` — unified agent query helpers

### Modified files (by task)
| Task | Files |
|------|-------|
| 1 | `crates/nexus-domain/src/accounts/id.rs`, `crates/nexus-domain/src/accounts/mod.rs`, `crates/nexus-domain/src/agents/activity.rs`, `crates/nexus-domain/src/agents/status.rs`, `crates/nexus-domain/src/agents/model.rs`, `crates/nexus-domain/src/accounts/provider.rs`, `crates/nexus-domain/src/accounts/quota.rs`, `crates/nexus-domain/src/shared/time.rs`, `crates/nexus-domain/src/shared/mod.rs`, `crates/nexus-domain/src/lib.rs`, `src/app/state/accounts.rs`, `src/app/state/mod.rs`, `src/app/mod.rs`, `src/app/action.rs`, `src/ui/shared/mod.rs`, `src/ui/agent_list.rs`, `src/ui/detail.rs`, `src/ui/activity.rs`, `src/lib.rs`, `src/detect/opencode/query.rs`, `src/detect/codex/rollout.rs`, `src/detect/claude/parser.rs`, `src/profiles/stored.rs`, `tests/types_test.rs` |
| 2 | `crates/nexus-domain/src/accounts/quota.rs`, `src/ui/accounts/detail.rs`, `src/ui/accounts/list.rs`, `src/services/accounts/quota.rs`, `src/services/runtime/session.rs`, `src/codex_profiles/quota.rs`, `src/claude_profiles/quota.rs`, `src/app/effect.rs`, `tests/types_test.rs`, `tests/codex_quota_test.rs` |
| 3 | `src/detect/error.rs`, `src/detect/worktree.rs`, `src/detect/codex/sqlite.rs`, `src/detect/codex/mod.rs`, `src/detect/claude/mod.rs`, `src/detect/opencode/mod.rs`, `src/detect/opencode/query.rs`, `src/detect/process.rs`, `src/detect/mod.rs` |
| 4 | `src/app/reducer/agent_lookup.rs` (new), `src/app/reducer/agents.rs`, `src/app/reducer/mod.rs`, `crates/nexus-domain/src/agents/ids.rs` |
| 5 | `src/app/state/overview.rs`, `src/app/state/accounts.rs`, `src/app/state/system.rs`, `src/app/state/mod.rs`, `src/app/core.rs`, `src/app/core_tests.rs`, `src/app/overview.rs`, `src/app/accounts.rs`, `src/app/system.rs` |

---

## Task 1: Domain Error Types and Validation

### Task 1a: ProfileId validation

**Files:**
- Modify: `crates/nexus-domain/src/accounts/id.rs`
- Modify: `src/profiles/stored.rs:22`

- [ ] **Step 1: Add ProfileIdError and make new() fallible**

In `crates/nexus-domain/src/accounts/id.rs`, replace the entire file with:

```rust
use std::fmt;

/// Error returned when a `ProfileId` cannot be constructed from invalid input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProfileIdError {
    /// The input string was empty or contained only whitespace.
    Empty,
}

impl fmt::Display for ProfileIdError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => f.write_str("profile id must not be empty"),
        }
    }
}

impl std::error::Error for ProfileIdError {}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ProfileId(String);

impl ProfileId {
    pub fn new(id: impl Into<String>) -> Result<Self, ProfileIdError> {
        let id = id.into();
        if id.trim().is_empty() {
            return Err(ProfileIdError::Empty);
        }
        Ok(Self(id))
    }

    /// Construct without validation. Use only when the id is known-valid
    /// (e.g. loaded from storage that validated at write time).
    pub fn new_unchecked(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ProfileId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<String> for ProfileId {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<&str> for ProfileId {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

impl AsRef<str> for ProfileId {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl std::ops::Deref for ProfileId {
    type Target = str;
    fn deref(&self) -> &str {
        &self.0
    }
}

impl PartialEq<str> for ProfileId {
    fn eq(&self, other: &str) -> bool {
        self.0 == other
    }
}

impl PartialEq<&str> for ProfileId {
    fn eq(&self, other: &&str) -> bool {
        self.0 == *other
    }
}

impl PartialEq<String> for ProfileId {
    fn eq(&self, other: &String) -> bool {
        self.0 == *other
    }
}
```

- [ ] **Step 2: Update the one caller that uses `ProfileId::new`**

In `src/profiles/stored.rs:22`, change:
```rust
            id: ProfileId::new(self.id),
```
to:
```rust
            id: ProfileId::new_unchecked(self.id),
```

- [ ] **Step 3: Update re-exports**

In `crates/nexus-domain/src/accounts/mod.rs`, add to the `pub use id::` line:
```rust
pub use id::{ProfileId, ProfileIdError};
```

In `src/lib.rs` testing module, if `ProfileIdError` is needed by tests, add it to the accounts re-exports. Check if any test files reference it — if not, skip.

- [ ] **Step 4: Run tests**

Run: `cargo test --workspace`
Expected: All tests pass. The `From<&str>` and `From<String>` impls still bypass validation (they're convenience impls for internal use), so existing code using `.into()` continues to work.

- [ ] **Step 5: Add unit tests for ProfileId validation**

Add to the bottom of `crates/nexus-domain/src/accounts/id.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_rejects_empty_string() {
        assert_eq!(ProfileId::new(""), Err(ProfileIdError::Empty));
    }

    #[test]
    fn new_rejects_whitespace_only() {
        assert_eq!(ProfileId::new("   "), Err(ProfileIdError::Empty));
    }

    #[test]
    fn new_accepts_valid_id() {
        assert!(ProfileId::new("profile-01").is_ok());
    }

    #[test]
    fn new_unchecked_accepts_anything() {
        let id = ProfileId::new_unchecked("");
        assert_eq!(id.as_str(), "");
    }

    #[test]
    fn display_shows_inner_string() {
        let id = ProfileId::new("test-id").unwrap();
        assert_eq!(id.to_string(), "test-id");
    }
}
```

- [ ] **Step 6: Run tests and commit**

Run: `cargo test --workspace`
Expected: All pass including new tests.

```bash
git add crates/nexus-domain/src/accounts/id.rs crates/nexus-domain/src/accounts/mod.rs src/profiles/stored.rs
git commit -m "refactor: make ProfileId::new fallible, add validation"
```

### Task 1b: FromStr for TodoStatus and TodoPriority

**Files:**
- Modify: `crates/nexus-domain/src/agents/activity.rs`
- Modify: `src/detect/opencode/query.rs:190-191,331-332`
- Modify: `src/detect/codex/rollout.rs:220`
- Modify: `src/detect/claude/parser.rs:493-494`

- [ ] **Step 1: Replace parse_str with FromStr**

In `crates/nexus-domain/src/agents/activity.rs`, replace the `TodoStatus` and `TodoPriority` impl blocks:

```rust
use std::str::FromStr;

// ... (Activity enum and impls stay the same)

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParseTodoStatusError;

impl std::fmt::Display for ParseTodoStatusError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("unknown todo status")
    }
}

impl std::error::Error for ParseTodoStatusError {}

impl FromStr for TodoStatus {
    type Err = ParseTodoStatusError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "pending" => Ok(Self::Pending),
            "in_progress" => Ok(Self::InProgress),
            "completed" => Ok(Self::Completed),
            "cancelled" => Ok(Self::Cancelled),
            _ => Err(ParseTodoStatusError),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParseTodoPriorityError;

impl std::fmt::Display for ParseTodoPriorityError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("unknown todo priority")
    }
}

impl std::error::Error for ParseTodoPriorityError {}

impl TodoPriority {
    /// Default priority used when parsing fails.
    pub const DEFAULT: Self = Self::Medium;
}

impl FromStr for TodoPriority {
    type Err = ParseTodoPriorityError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "high" => Ok(Self::High),
            "medium" => Ok(Self::Medium),
            "low" => Ok(Self::Low),
            _ => Err(ParseTodoPriorityError),
        }
    }
}
```

Remove the old `parse_str` methods entirely.

- [ ] **Step 2: Update callers to use .parse().unwrap_or_default() or .parse().unwrap_or()**

In `src/detect/opencode/query.rs`, change lines 190-191:
```rust
            status: status.parse().unwrap_or(TodoStatus::Pending),
            priority: priority.parse().unwrap_or(TodoPriority::DEFAULT),
```

And lines 331-332:
```rust
            status: status.parse().unwrap_or(TodoStatus::Pending),
            priority: priority.parse().unwrap_or(TodoPriority::DEFAULT),
```

In `src/detect/codex/rollout.rs`, change line 220:
```rust
                            .map(|s| s.parse().unwrap_or(TodoStatus::Pending))
```

In `src/detect/claude/parser.rs`, change lines 493-494:
```rust
                                    status: status.parse().unwrap_or(TodoStatus::Pending),
                                    priority: priority.parse().unwrap_or(TodoPriority::DEFAULT),
```

- [ ] **Step 3: Add Default impl for TodoStatus**

In `crates/nexus-domain/src/agents/activity.rs`, add above `TodoStatus`:
```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TodoStatus {
    #[default]
    Pending,
    InProgress,
    Completed,
    Cancelled,
}
```
(Add `Default` to the derive and `#[default]` on `Pending`.)

- [ ] **Step 4: Run tests and commit**

Run: `cargo test --workspace`
Expected: All pass.

```bash
git add crates/nexus-domain/src/agents/activity.rs src/detect/opencode/query.rs src/detect/codex/rollout.rs src/detect/claude/parser.rs
git commit -m "refactor: replace parse_str with FromStr for TodoStatus and TodoPriority"
```

### Task 1c: Add #[non_exhaustive] to public domain enums

**Files:**
- Modify: `crates/nexus-domain/src/accounts/provider.rs`
- Modify: `crates/nexus-domain/src/accounts/quota.rs`
- Modify: `crates/nexus-domain/src/agents/status.rs`
- Modify: `crates/nexus-domain/src/agents/model.rs`
- Modify: `crates/nexus-domain/src/agents/activity.rs`

- [ ] **Step 1: Add #[non_exhaustive] to all public enums**

In `crates/nexus-domain/src/accounts/provider.rs`:
```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Provider {
```

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[non_exhaustive]
pub enum ProviderFilter {
```

In `crates/nexus-domain/src/accounts/quota.rs`:
```rust
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[non_exhaustive]
pub enum Confidence {
```

```rust
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[non_exhaustive]
pub enum LimitState {
```

In `crates/nexus-domain/src/agents/status.rs`:
```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
pub enum PrimaryStatus {
```

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
pub enum NormalizedStatus {
```

```rust
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum NormalizedReason {
```

In `crates/nexus-domain/src/agents/model.rs`:
```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum SortMode {
```

In `crates/nexus-domain/src/agents/activity.rs`:
```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Activity {
```

- [ ] **Step 2: Add wildcard arms to match statements in the binary crate**

Since the domain crate is `publish = false` and all match statements are in the same workspace, the compiler will flag any non-exhaustive matches. Fix each one by adding `_ => { /* unreachable for now */ }` arms ONLY where the compiler requires them. The binary crate is in the same workspace so `#[non_exhaustive]` only affects cross-crate usage — check if any compile errors arise.

Run: `cargo check --workspace`

If errors appear, add wildcard arms. If none appear (because same-workspace exemption), this step is a no-op.

- [ ] **Step 3: Run tests and commit**

Run: `cargo test --workspace`
Expected: All pass.

```bash
git add crates/nexus-domain/
git commit -m "refactor: add #[non_exhaustive] to public domain enums"
```

### Task 1d: Move EditableProfile out of domain crate

**Files:**
- Modify: `crates/nexus-domain/src/accounts/mod.rs` — remove EditableProfile
- Modify: `src/app/state/accounts.rs` — define EditableProfile here
- Modify: `src/app/state/mod.rs` — update re-export
- Modify: `src/app/mod.rs` — update re-export
- Modify: `src/app/action.rs` — update import

- [ ] **Step 1: Remove EditableProfile from domain crate**

In `crates/nexus-domain/src/accounts/mod.rs`, remove lines 13-19 (the EditableProfile struct and its doc comment):

```rust
/// A plain value object representing editable fields of an account profile.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct EditableProfile {
    pub label: String,
    pub notes: String,
    pub enabled: bool,
}
```

- [ ] **Step 2: Define EditableProfile in app state**

In `src/app/state/accounts.rs`, replace line 8:
```rust
pub use nexus_domain::accounts::EditableProfile;
```
with:
```rust
/// A plain value object representing editable fields of an account profile.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct EditableProfile {
    pub label: String,
    pub notes: String,
    pub enabled: bool,
}
```

- [ ] **Step 3: Verify re-exports still work**

`src/app/state/mod.rs` line 7 already re-exports `EditableProfile` from `accounts`. `src/app/mod.rs` line 32 already re-exports it from `state`. The chain should still work since we defined it in the same module that was previously re-exporting it.

Run: `cargo check --workspace`

- [ ] **Step 4: Run tests and commit**

Run: `cargo test --workspace`
Expected: All pass.

```bash
git add crates/nexus-domain/src/accounts/mod.rs src/app/state/accounts.rs
git commit -m "refactor: move EditableProfile from domain crate to app state"
```

### Task 1e: Move format functions out of domain crate

**Files:**
- Create: `src/ui/shared/format.rs`
- Modify: `src/ui/shared/mod.rs`
- Modify: `crates/nexus-domain/src/shared/time.rs` — remove format functions
- Modify: `crates/nexus-domain/src/shared/mod.rs` — remove re-exports
- Modify: `src/ui/agent_list.rs` — update import
- Modify: `src/ui/detail.rs` — update import
- Modify: `src/ui/activity.rs` — update import
- Modify: `src/lib.rs` — update testing re-exports

- [ ] **Step 1: Create format.rs in ui/shared**

Create `src/ui/shared/format.rs`:

```rust
/// Format a duration for display: "5s", "3m12s", "2h15m".
pub fn format_duration(d: chrono::Duration) -> String {
    let secs = d.num_seconds().max(0);
    if secs < 60 {
        format!("{secs}s")
    } else if secs < 3600 {
        format!("{}m{}s", secs / 60, secs % 60)
    } else {
        format!("{}h{}m", secs / 3600, (secs % 3600) / 60)
    }
}

/// Format a duration in short form: "5s", "3m", "2h".
pub fn format_duration_short(d: chrono::Duration) -> String {
    let secs = d.num_seconds().max(0);
    if secs < 60 {
        format!("{secs}s")
    } else if secs < 3600 {
        format!("{}m", secs / 60)
    } else {
        format!("{}h", secs / 3600)
    }
}
```

- [ ] **Step 2: Update ui/shared/mod.rs**

Change `src/ui/shared/mod.rs` to:
```rust
pub mod format;
pub mod help;
pub mod layout;
```

- [ ] **Step 3: Remove format functions from domain crate**

In `crates/nexus-domain/src/shared/time.rs`, remove `format_duration` and `format_duration_short` (lines 1-25, keeping the remaining `from_*` functions). Update the use statement at the top: remove `chrono::Duration` since it's no longer needed (the remaining functions only use `DateTime`).

The file should now be:
```rust
use chrono::{DateTime, Local};

/// Convert epoch milliseconds to local DateTime. Returns None for 0 or invalid values.
pub fn from_epoch_millis(ms: i64) -> Option<DateTime<Local>> {
    if ms == 0 {
        return None;
    }
    chrono::DateTime::from_timestamp_millis(ms).map(|dt| dt.with_timezone(&Local))
}

/// Convert epoch seconds to local DateTime. Returns None for 0 or invalid values.
pub fn from_epoch_secs(secs: i64) -> Option<DateTime<Local>> {
    if secs == 0 {
        return None;
    }
    chrono::DateTime::from_timestamp(secs, 0).map(|dt| dt.with_timezone(&Local))
}

/// Parse an RFC3339 timestamp string to local DateTime. Returns None for invalid input.
pub fn from_rfc3339(s: &str) -> Option<DateTime<Local>> {
    chrono::DateTime::parse_from_rfc3339(s)
        .ok()
        .map(|dt| dt.with_timezone(&Local))
}
```

In `crates/nexus-domain/src/shared/mod.rs`, update re-exports:
```rust
pub mod time;

pub use time::{from_epoch_millis, from_epoch_secs, from_rfc3339};
```

- [ ] **Step 4: Update UI imports**

In `src/ui/agent_list.rs`, change:
```rust
use nexus_domain::shared::time::format_duration;
```
to:
```rust
use crate::ui::shared::format::format_duration;
```

In `src/ui/detail.rs`, change:
```rust
use nexus_domain::shared::time::format_duration_short;
```
to:
```rust
use crate::ui::shared::format::format_duration_short;
```

In `src/ui/activity.rs`, change:
```rust
use nexus_domain::shared::time::format_duration;
```
to:
```rust
use crate::ui::shared::format::format_duration;
```

- [ ] **Step 5: Update testing re-exports in lib.rs**

In `src/lib.rs`, change the shared utilities re-export:
```rust
    // ── Domain: shared utilities ───────────────────────────────
    pub use nexus_domain::shared::{from_epoch_millis, from_epoch_secs, from_rfc3339};

    // ── UI: formatting ────────────────────────────────────────
    pub use crate::ui::shared::format::{format_duration, format_duration_short};
```

- [ ] **Step 6: Run tests and commit**

Run: `cargo test --workspace`
Expected: All pass.

```bash
git add src/ui/shared/format.rs src/ui/shared/mod.rs crates/nexus-domain/src/shared/ src/ui/agent_list.rs src/ui/detail.rs src/ui/activity.rs src/lib.rs
git commit -m "refactor: move format_duration out of domain crate into ui layer"
```

---

## Task 2: Quota Domain Model Cleanup

### Task 2a: Add WindowKind enum and simplify QuotaSnapshot

**Files:**
- Modify: `crates/nexus-domain/src/accounts/quota.rs`

- [ ] **Step 1: Rewrite quota.rs**

Replace `crates/nexus-domain/src/accounts/quota.rs` with:

```rust
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[non_exhaustive]
pub enum Confidence {
    #[default]
    Unavailable,
    Parsed,
    Cached,
    Exact,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[non_exhaustive]
pub enum LimitState {
    Hit,
    Available,
    #[default]
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[non_exhaustive]
pub enum WindowKind {
    FiveHour,
    Weekly,
    Monthly,
    Daily,
    LongTerm,
    Other(String),
}

impl WindowKind {
    pub fn label(&self) -> &str {
        match self {
            Self::FiveHour => "5h",
            Self::Weekly => "weekly",
            Self::Monthly => "monthly",
            Self::Daily => "daily",
            Self::LongTerm => "long-term",
            Self::Other(s) => s.as_str(),
        }
    }

    /// Parse a label string into a WindowKind, normalizing common variants.
    pub fn from_label(label: &str) -> Self {
        let normalized = label.trim().to_ascii_lowercase();
        match normalized.as_str() {
            "5h" | "5h window" | "primary" => Self::FiveHour,
            "weekly" | "7d" | "secondary" => Self::Weekly,
            "monthly" => Self::Monthly,
            "daily" => Self::Daily,
            "long-term" | "longterm" => Self::LongTerm,
            _ => Self::Other(label.to_string()),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct QuotaWindow {
    pub kind: WindowKind,
    #[serde(default)]
    pub used_pct: Option<u8>,
    #[serde(default)]
    pub reset_at: Option<DateTime<Utc>>,
    pub state: LimitState,
}

impl Default for WindowKind {
    fn default() -> Self {
        Self::Other(String::new())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct CreditsState {
    pub enabled: bool,
    #[serde(default)]
    pub balance_label: Option<String>,
    pub unlimited: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct QuotaSnapshot {
    pub captured_at: DateTime<Utc>,
    pub confidence: Confidence,
    pub windows: Vec<QuotaWindow>,
    #[serde(default)]
    pub credits: Option<CreditsState>,
    #[serde(default)]
    pub summary: String,
}

impl QuotaSnapshot {
    pub fn new(
        captured_at: DateTime<Utc>,
        confidence: Confidence,
        windows: Vec<QuotaWindow>,
        credits: Option<CreditsState>,
        summary: String,
    ) -> Self {
        Self {
            captured_at,
            confidence,
            windows,
            credits,
            summary,
        }
    }

    /// Construct from legacy per-window fields (5h + weekly).
    ///
    /// Use this at deserialization boundaries when reading old-format data.
    #[allow(clippy::too_many_arguments)]
    pub fn from_legacy(
        captured_at: DateTime<Utc>,
        confidence: Confidence,
        five_hour_state: LimitState,
        five_hour_reset_at: Option<DateTime<Utc>>,
        five_hour_used_pct: Option<u8>,
        weekly_state: LimitState,
        weekly_reset_at: Option<DateTime<Utc>>,
        weekly_used_pct: Option<u8>,
        summary: String,
    ) -> Self {
        Self::new(
            captured_at,
            confidence,
            vec![
                QuotaWindow {
                    kind: WindowKind::FiveHour,
                    used_pct: five_hour_used_pct,
                    reset_at: five_hour_reset_at,
                    state: five_hour_state,
                },
                QuotaWindow {
                    kind: WindowKind::Weekly,
                    used_pct: weekly_used_pct,
                    reset_at: weekly_reset_at,
                    state: weekly_state,
                },
            ],
            None,
            summary,
        )
    }

    /// Borrow the window list. Always returns the canonical windows.
    pub fn windows(&self) -> &[QuotaWindow] {
        &self.windows
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct QuotaCache {
    pub by_profile: BTreeMap<String, QuotaSnapshot>,
}
```

- [ ] **Step 2: Run cargo check to find all broken callers**

Run: `cargo check --workspace 2>&1 | head -80`

This will show every call site that references removed fields/methods. Fix each one in the following steps.

- [ ] **Step 3: Update callers that used `from_windows`**

All callers of `QuotaSnapshot::from_windows(...)` change to `QuotaSnapshot::new(...)`. The signature is identical except the method name.

Search: `grep -rn "from_windows" src/ tests/`

Update each occurrence.

- [ ] **Step 4: Update callers that used `legacy_fallback`**

All callers of `QuotaSnapshot::legacy_fallback(...)` change to `QuotaSnapshot::from_legacy(...)`.

Search: `grep -rn "legacy_fallback" src/ tests/`

Update each occurrence.

- [ ] **Step 5: Update callers that used `effective_windows()`**

Change `quota.effective_windows()` to `quota.windows()` (returns `&[QuotaWindow]` now).

In `src/ui/accounts/detail.rs`, change:
```rust
    for window in quota.effective_windows() {
```
to:
```rust
    for window in quota.windows() {
```

In `src/ui/accounts/list.rs`, change:
```rust
                        let windows = quota.effective_windows();
```
to:
```rust
                        let windows = quota.windows();
```

Any code that previously used `window.label` (a String) now uses `window.kind.label()` (returns `&str`). Update those references.

- [ ] **Step 6: Update callers that used `window.label`**

Search: `grep -rn "window\.label\b\|\.label =\|label:" src/ui/ src/codex_profiles/ src/claude_profiles/`

Change string field access to `window.kind.label()`. Change construction from `label: "5h".to_string()` to `kind: WindowKind::FiveHour`.

In quota constructors in `src/codex_profiles/quota.rs` and `src/claude_profiles/quota.rs`, update `QuotaWindow { label: ... }` to `QuotaWindow { kind: WindowKind::from_label(...), ... }` or direct enum variants where the label is known.

- [ ] **Step 7: Remove normalize() call**

In `src/services/accounts/quota.rs`, remove:
```rust
    cache.normalize();
```

- [ ] **Step 8: Update quota cache serialization**

The `QuotaCache` no longer has `normalize()`. The `QuotaSnapshot` struct dropped the legacy fields. Existing serialized caches that contain the old fields will be handled by serde's `#[serde(default)]` — the old fields simply won't be present in the new struct and will be ignored during deserialization. However, if old caches have `windows: []` with only legacy fields populated, those will now deserialize as empty-windowed snapshots.

To handle migration gracefully, add a serde `deserialize_with` function or a post-deserialization fixup in the cache loading path. The simplest approach: in `src/services/accounts/quota.rs` `load_quota_cache`, after loading, filter out snapshots with empty windows:

Actually, let's keep it simple — old caches with legacy fields will simply show no windows until the next refresh. This is acceptable behavior for a cache.

- [ ] **Step 9: Run tests, fix remaining compilation errors, commit**

Run: `cargo test --workspace`
Fix any remaining issues.

```bash
git add crates/nexus-domain/src/accounts/quota.rs src/ui/accounts/ src/services/accounts/quota.rs src/codex_profiles/quota.rs src/claude_profiles/quota.rs src/services/runtime/session.rs src/app/effect.rs tests/
git commit -m "refactor: simplify QuotaSnapshot, add WindowKind enum, remove legacy fields"
```

---

## Task 3: Detection Error Context

### Task 3a: Expand DetectorError with context

**Files:**
- Modify: `src/detect/error.rs`
- Modify: `src/detect/codex/sqlite.rs`
- Modify: `src/detect/codex/mod.rs`
- Modify: `src/detect/claude/mod.rs`
- Modify: `src/detect/opencode/mod.rs`
- Modify: `src/detect/opencode/query.rs`
- Modify: `src/detect/process.rs`
- Modify: `src/detect/mod.rs`

- [ ] **Step 1: Rewrite DetectorError**

Replace `src/detect/error.rs` with:

```rust
use std::path::PathBuf;

use nexus_domain::system::DetectorKind;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum DetectorError {
    #[error("{detector} database error at {}: {source}", path.display())]
    Database {
        detector: DetectorKind,
        path: PathBuf,
        source: rusqlite::Error,
    },

    #[error("{detector} I/O error ({context}): {source}")]
    Io {
        detector: DetectorKind,
        context: &'static str,
        source: std::io::Error,
    },
}

impl DetectorError {
    pub fn database(
        detector: DetectorKind,
        path: impl Into<PathBuf>,
        source: rusqlite::Error,
    ) -> Self {
        Self::Database {
            detector,
            path: path.into(),
            source,
        }
    }

    pub fn io(detector: DetectorKind, context: &'static str, source: std::io::Error) -> Self {
        Self::Io {
            detector,
            context,
            source,
        }
    }

    /// Returns `true` if this error is transient and the operation should be retried.
    pub fn is_transient(&self) -> bool {
        match self {
            DetectorError::Database { source, .. } => matches!(
                source.sqlite_error_code(),
                Some(rusqlite::ErrorCode::DatabaseBusy | rusqlite::ErrorCode::DatabaseLocked)
            ),
            DetectorError::Io { source, .. } => matches!(
                source.kind(),
                std::io::ErrorKind::Interrupted
                    | std::io::ErrorKind::WouldBlock
                    | std::io::ErrorKind::TimedOut
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn database_error_displays_context() {
        let err = DetectorError::database(
            DetectorKind::Codex,
            "/tmp/state.sqlite",
            rusqlite::Error::SqliteFailure(
                rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_BUSY),
                Some("database is locked".to_string()),
            ),
        );
        let msg = err.to_string();
        assert!(msg.contains("Codex"), "should contain detector name: {msg}");
        assert!(msg.contains("state.sqlite"), "should contain path: {msg}");
    }

    #[test]
    fn io_error_displays_context() {
        let err = DetectorError::io(
            DetectorKind::Claude,
            "reading transcript",
            std::io::Error::new(std::io::ErrorKind::NotFound, "missing"),
        );
        let msg = err.to_string();
        assert!(msg.contains("Claude"), "should contain detector name: {msg}");
        assert!(
            msg.contains("reading transcript"),
            "should contain context: {msg}"
        );
    }

    #[test]
    fn io_not_found_is_permanent() {
        let err = DetectorError::io(
            DetectorKind::Claude,
            "test",
            std::io::Error::new(std::io::ErrorKind::NotFound, "missing"),
        );
        assert!(!err.is_transient());
    }

    #[test]
    fn io_interrupted_is_transient() {
        let err = DetectorError::io(
            DetectorKind::Claude,
            "test",
            std::io::Error::new(std::io::ErrorKind::Interrupted, "interrupted"),
        );
        assert!(err.is_transient());
    }
}
```

- [ ] **Step 2: Update codex sqlite.rs**

In `src/detect/codex/sqlite.rs`, the functions use `?` with `DetectorError` via `From<rusqlite::Error>`. Since we removed the `#[from]` derive, we need to update these functions to construct errors explicitly.

Change `query_codex_threads` and `query_all_spawn_edges` to accept `detector: DetectorKind` or hardcode `DetectorKind::Codex`:

```rust
use std::path::Path;

use tracing::debug;

use crate::detect::error::DetectorError;
use nexus_domain::system::DetectorKind;
use super::types::{CodexThread, SourceInfo, SpawnEdge};

// ... parse_source_field stays the same ...

pub fn query_codex_threads(db_path: &Path) -> Result<Vec<CodexThread>, DetectorError> {
    let conn = rusqlite::Connection::open_with_flags(
        db_path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|e| DetectorError::database(DetectorKind::Codex, db_path, e))?;

    let mut stmt = conn
        .prepare(/* ... same query ... */)
        .map_err(|e| DetectorError::database(DetectorKind::Codex, db_path, e))?;

    let threads = stmt
        .query_map([], |row| {
            // ... same row mapping ...
        })
        .map_err(|e| DetectorError::database(DetectorKind::Codex, db_path, e))?
        .filter_map(|r| match r {
            Ok(t) => Some(t),
            Err(e) => {
                debug!("failed to parse codex thread row: {e}");
                None
            }
        })
        .collect();

    Ok(threads)
}
```

Apply the same pattern to `query_all_spawn_edges`.

- [ ] **Step 3: Update codex/mod.rs**

Change the `map_err` on the `spawn_blocking` JoinError (line 67):
```rust
        .map_err(|e| DetectorError::io(DetectorKind::Codex, "spawn_blocking join", std::io::Error::other(e.to_string())))?;
```

- [ ] **Step 4: Update claude/mod.rs**

All `map_err(|e| DetectorError::Io(std::io::Error::other(e.to_string())))` calls become:
```rust
.map_err(|e| DetectorError::io(DetectorKind::Claude, "spawn_blocking join", std::io::Error::other(e.to_string())))
```

- [ ] **Step 5: Update opencode/mod.rs and opencode/query.rs**

Same pattern. All `DetectorError` construction uses `DetectorError::database(DetectorKind::OpenCode, &db_path, e)` or `DetectorError::io(DetectorKind::OpenCode, "context", e)`.

For `opencode/query.rs`, the functions that take `db_path: &Path` and use `?` with rusqlite errors need explicit `map_err`. For functions that take `conn: &Connection` (no path available), pass the path as an additional parameter or use a placeholder. The simplest fix: add `db_path: &Path` parameter to the `_conn` variants, or change the return type. Alternatively, since these are called within a single `spawn_blocking` that already has the path, wrap the entire block's result.

The cleanest approach: keep `_conn` functions returning `rusqlite::Error`, and map at the call site in `opencode/mod.rs` where `db_path` is available:

```rust
// In opencode/mod.rs, inside the spawn_blocking closure:
let sessions = query_opencode_sessions_conn(&conn)
    .map_err(|e| DetectorError::database(DetectorKind::OpenCode, &db2, e))?;
```

- [ ] **Step 6: Update process.rs**

The process scanner returns `DetectorError` from its `run_process_scanner` function. Update to use `DetectorError::io(DetectorKind::Process, ...)` — but `DetectorKind` doesn't have a `Process` variant. Use the appropriate detector kind based on context, or add a general context. Check what `process.rs` actually returns:

In `src/detect/process.rs:470`, it returns `Result<(), crate::detect::error::DetectorError>`. The function is called from `src/detect/mod.rs`. Process scanning errors are I/O errors from running `ps`/`lsof`. Use a new `DetectorKind` variant or use a generic "process scanner" context. Since `DetectorKind` has `Claude`, `Codex`, `OpenCode` — process scanning is shared infrastructure. Create the error with a descriptive context string and pick the most relevant detector, or check if we need to add a variant.

Actually, looking at `DetectorKind` in `crates/nexus-domain/src/system/detector.rs`, let me check what variants it has.

Check and add a `Process` variant if needed, or use `DetectorKind::Claude` as a placeholder with the context string doing the heavy lifting.

The simplest approach: the process scanner errors should use a special context. Since `DetectorKind` is `#[non_exhaustive]` now, we can add a variant later. For now, create errors with whichever detector is relevant. In practice, the process scanner is never the direct source of DetectorError returns — it's the orchestrator in `mod.rs` that catches errors. Keep the existing pattern.

- [ ] **Step 7: Run tests and commit**

Run: `cargo test --workspace`
Expected: All pass.

```bash
git add src/detect/
git commit -m "refactor: add context to DetectorError (detector kind, path, operation)"
```

### Task 3b: Add timeout to worktree git commands

**Files:**
- Modify: `src/detect/worktree.rs`

- [ ] **Step 1: Add timeout to lookup_branch_info**

In `src/detect/worktree.rs`, change `lookup_branch_info`:

```rust
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::process::Command;

// ... WorktreeInfo, is_git_worktree, parse_worktree_list stay the same ...

const GIT_COMMAND_TIMEOUT: Duration = Duration::from_secs(5);

pub async fn lookup_branch_info(cwd: &Path) -> (Option<String>, bool) {
    let is_wt = is_git_worktree(cwd);
    let branch = match tokio::time::timeout(
        GIT_COMMAND_TIMEOUT,
        Command::new("git")
            .args(["rev-parse", "--abbrev-ref", "HEAD"])
            .current_dir(cwd)
            .output(),
    )
    .await
    {
        Ok(Ok(output)) if output.status.success() => {
            let b = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if b == "HEAD" {
                None
            } else {
                Some(b)
            }
        }
        Ok(Ok(_)) | Ok(Err(_)) => None,
        Err(_elapsed) => {
            tracing::warn!("git rev-parse timed out for {}", cwd.display());
            None
        }
    };
    (branch, is_wt)
}
```

- [ ] **Step 2: Run tests and commit**

Run: `cargo test --workspace`
Expected: All pass.

```bash
git add src/detect/worktree.rs
git commit -m "fix: add 5s timeout to git rev-parse in worktree detection"
```

### Task 3c: Add cache eviction for Claude file_state

**Files:**
- Modify: `src/detect/claude/mod.rs`

- [ ] **Step 1: Add eviction after scan**

In `src/detect/claude/mod.rs`, after the main scan loop body (after processing all transcript files and before `tokio::time::sleep`), add cache eviction:

Find the `tokio::time::sleep(poll_interval).await;` line at the end of the loop body and add before it:

```rust
        // Evict cache entries for files that no longer exist.
        file_state.retain(|path, _| path.exists());
        session_states.retain(|key, _| {
            // Keep entries whose transcript file still exists
            transcripts_dir.join(format!("{key}.jsonl")).exists()
                || project_file_cache.contains_key(&transcripts_dir.join(format!("{key}.jsonl")))
        });
```

Actually, the `file_state` keys are full `PathBuf`s to transcript files. The simplest eviction:

```rust
        // Evict file_state entries whose files no longer exist on disk.
        file_state.retain(|path, _| path.exists());
```

This runs once per poll cycle (every few seconds). `path.exists()` is cheap compared to the full scan.

- [ ] **Step 2: Run tests and commit**

Run: `cargo test --workspace`
Expected: All pass.

```bash
git add src/detect/claude/mod.rs
git commit -m "fix: evict stale entries from Claude transcript file cache"
```

---

## Task 4: Unify Agent Matching

### Task 4a: Create agent_lookup module

**Files:**
- Create: `src/app/reducer/agent_lookup.rs`
- Modify: `src/app/reducer/mod.rs`

- [ ] **Step 1: Create agent_lookup.rs**

Create `src/app/reducer/agent_lookup.rs`:

```rust
//! Unified query helpers for finding agents in the agent list.
//!
//! All functions take `&[Agent]` or `&mut [Agent]` to keep reducers stateless.

use std::path::Path;

use crate::app::agent::Agent;
use nexus_domain::agents::ids::{AgentId, AgentTool};
use nexus_domain::agents::status::PrimaryStatus;

/// Find a single agent by exact AgentId match.
pub fn find_by_id(agents: &mut [Agent], id: &AgentId) -> Option<&mut Agent> {
    agents.iter_mut().find(|a| *a.id() == *id)
}

/// Find a single agent by tool + pid (any id type).
pub fn find_by_pid(agents: &mut [Agent], tool: AgentTool, pid: u32) -> Option<&mut Agent> {
    agents
        .iter_mut()
        .find(|a| a.id().tool() == tool && a.pid() == Some(pid))
}

/// Find a process-type agent by tool + pid.
pub fn find_process_agent(agents: &mut [Agent], tool: AgentTool, pid: u32) -> Option<&mut Agent> {
    agents
        .iter_mut()
        .find(|a| a.id().tool() == tool && a.pid() == Some(pid) && a.id().is_process())
}

/// Indices of live session-type agents matching tool + pid + cwd.
pub fn session_indices_by_pid(
    agents: &[Agent],
    tool: AgentTool,
    pid: u32,
    cwd: &Path,
) -> Vec<usize> {
    agents
        .iter()
        .enumerate()
        .filter(|(_, a)| {
            a.id().tool() == tool
                && a.id().is_session()
                && a.pid() == Some(pid)
                && a.status() != PrimaryStatus::Exited
                && (a.cwd().is_none() || a.cwd() == Some(cwd))
        })
        .map(|(idx, _)| idx)
        .collect()
}

/// Indices of exited session-type agents matching tool + pid + cwd.
pub fn exited_session_indices_by_pid(
    agents: &[Agent],
    tool: AgentTool,
    pid: u32,
    cwd: &Path,
) -> Vec<usize> {
    agents
        .iter()
        .enumerate()
        .filter(|(_, a)| {
            a.id().tool() == tool
                && a.id().is_session()
                && a.pid() == Some(pid)
                && a.status() == PrimaryStatus::Exited
                && a.cwd() == Some(cwd)
        })
        .map(|(idx, _)| idx)
        .collect()
}

/// Remove all process-type Codex agents with the given pid.
pub fn remove_redundant_codex_process_agents(agents: &mut Vec<Agent>, pid: Option<u32>) {
    let Some(pid) = pid else {
        return;
    };
    agents.retain(|agent| {
        !(agent.id().tool() == AgentTool::Codex
            && agent.id().is_process()
            && agent.pid() == Some(pid))
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::agent::{AgentDetail, AgentIdent, AgentLifecycle};
    use nexus_domain::agents::status::{NormalizedState, NormalizedStatus};
    use std::collections::VecDeque;
    use std::path::PathBuf;

    fn make_agent(tool: AgentTool, pid: u32, is_session: bool) -> Agent {
        let now = chrono::Local::now();
        let id = if is_session {
            AgentId::session(tool, &format!("session-{pid}"))
        } else {
            AgentId::new(tool, pid)
        };
        Agent {
            ident: AgentIdent {
                id,
                pid: Some(pid),
                session_id: None,
                alias: None,
                cwd: Some(PathBuf::from("/tmp/test")),
                branch: None,
                is_worktree: false,
                started_at: now,
            },
            lifecycle: AgentLifecycle {
                status: PrimaryStatus::Running,
                normalized: NormalizedState::new(NormalizedStatus::Working, None),
                activity: None,
                last_activity: now,
            },
            detail: AgentDetail {
                subagents: Vec::new(),
                recent_events: VecDeque::new(),
                todos: Vec::new(),
            },
        }
    }

    #[test]
    fn find_by_id_returns_matching_agent() {
        let mut agents = vec![
            make_agent(AgentTool::Claude, 1, false),
            make_agent(AgentTool::Codex, 2, false),
        ];
        let id = AgentId::new(AgentTool::Codex, 2);
        assert!(find_by_id(&mut agents, &id).is_some());
    }

    #[test]
    fn find_by_id_returns_none_for_missing() {
        let mut agents = vec![make_agent(AgentTool::Claude, 1, false)];
        let id = AgentId::new(AgentTool::Codex, 99);
        assert!(find_by_id(&mut agents, &id).is_none());
    }

    #[test]
    fn find_process_agent_ignores_session_agents() {
        let mut agents = vec![make_agent(AgentTool::Codex, 1, true)];
        assert!(find_process_agent(&mut agents, AgentTool::Codex, 1).is_none());
    }

    #[test]
    fn session_indices_filters_by_status_and_cwd() {
        let mut agents = vec![
            make_agent(AgentTool::Codex, 1, true),
            make_agent(AgentTool::Codex, 1, true),
        ];
        agents[1].set_status(PrimaryStatus::Exited);
        let indices = session_indices_by_pid(
            &agents,
            AgentTool::Codex,
            1,
            Path::new("/tmp/test"),
        );
        assert_eq!(indices, vec![0]);
    }
}
```

- [ ] **Step 2: Register module**

In `src/app/reducer/mod.rs`, add:
```rust
pub mod agent_lookup;
```

- [ ] **Step 3: Run tests**

Run: `cargo test --workspace`
Expected: All pass including new agent_lookup tests.

- [ ] **Step 4: Commit**

```bash
git add src/app/reducer/agent_lookup.rs src/app/reducer/mod.rs
git commit -m "refactor: add unified agent_lookup module with query helpers"
```

### Task 4b: Replace inline finders in agents.rs with agent_lookup

**Files:**
- Modify: `src/app/reducer/agents.rs`

- [ ] **Step 1: Replace all inline finder functions**

In `src/app/reducer/agents.rs`:

1. Add at the top:
```rust
use super::agent_lookup;
```

2. Replace all calls to inline `find_agent_mut_by_id(agents, id)` with `agent_lookup::find_by_id(agents, id)`.

3. Replace all calls to `find_agent_mut_by_pid(agents, tool, pid)` with `agent_lookup::find_by_pid(agents, tool, pid)`.

4. Replace all calls to `find_process_agent_mut(agents, tool, pid)` with `agent_lookup::find_process_agent(agents, tool, pid)`.

5. Replace all calls to `find_session_agent_indices_by_pid(agents, tool, pid, cwd)` with `agent_lookup::session_indices_by_pid(agents, tool, pid, cwd)`.

6. Replace all calls to `find_exited_session_agent_indices_by_pid(agents, tool, pid, cwd)` with `agent_lookup::exited_session_indices_by_pid(agents, tool, pid, cwd)`.

7. Replace all calls to `remove_redundant_codex_process_agents(agents, pid)` with `agent_lookup::remove_redundant_codex_process_agents(agents, pid)`.

8. Delete the now-unused inline functions:
   - `find_agent_mut_by_id` (line 551)
   - `find_process_agent_mut` (line 599)
   - `find_session_agent_indices_by_pid` (line 605)
   - `find_exited_session_agent_indices_by_pid` (line 625)
   - `find_agent_mut_by_pid` (line 645)
   - `remove_redundant_codex_process_agents` (line 651)

- [ ] **Step 2: Run tests**

Run: `cargo test --workspace`
Expected: All pass — behavior unchanged, just delegation to the new module.

- [ ] **Step 3: Commit**

```bash
git add src/app/reducer/agents.rs
git commit -m "refactor: replace inline agent finders with agent_lookup module"
```

### Task 4c: Remove the expect() in insert_placeholder_agent

**Files:**
- Modify: `src/app/reducer/agents.rs`

- [ ] **Step 1: Restructure insert_placeholder_agent**

Replace the `insert_placeholder_agent` function (lines 555-597) with:

```rust
fn insert_placeholder_agent(
    agents: &mut Vec<Agent>,
    id: AgentId,
    pid: Option<u32>,
    now: DateTime<Local>,
) -> &mut Agent {
    if let Some(agent_idx) = agents.iter().position(|agent| *agent.id() == id) {
        return &mut agents[agent_idx];
    }

    let session_id = id.session_id().map(str::to_string);
    let normalized = if id.tool() == AgentTool::Codex {
        NormalizedState::new(NormalizedStatus::Unknown, None)
    } else {
        NormalizedState::new(NormalizedStatus::Working, None)
    };
    let idx = agents.len();
    agents.push(Agent {
        ident: AgentIdent {
            id,
            pid,
            session_id,
            alias: None,
            cwd: None,
            branch: None,
            is_worktree: false,
            started_at: now,
        },
        lifecycle: AgentLifecycle {
            status: PrimaryStatus::Running,
            normalized,
            activity: None,
            last_activity: now,
        },
        detail: AgentDetail {
            subagents: Vec::new(),
            recent_events: VecDeque::new(),
            todos: Vec::new(),
        },
    });
    &mut agents[idx]
}
```

The key change: save `idx = agents.len()` before the push, then index with `&mut agents[idx]` — this avoids `last_mut().expect(...)`.

- [ ] **Step 2: Run tests and commit**

Run: `cargo test --workspace`
Expected: All pass.

```bash
git add src/app/reducer/agents.rs
git commit -m "fix: remove expect() from insert_placeholder_agent, use indexed access"
```

### Task 4d: Move Codex-specific subagent matching to AgentTool

**Files:**
- Modify: `crates/nexus-domain/src/agents/ids.rs`
- Modify: `src/app/reducer/agents.rs`

- [ ] **Step 1: Check current AgentTool definition**

Read `crates/nexus-domain/src/agents/ids.rs` to find `AgentTool` and add the method.

- [ ] **Step 2: Add matches_subagent_name to AgentTool**

In `crates/nexus-domain/src/agents/ids.rs`, add to the `AgentTool` impl block (or create one):

```rust
impl AgentTool {
    /// Whether a subagent name matches, accounting for tool-specific conventions.
    ///
    /// Codex uses the subagent description as an alternative match target because
    /// the thread spawn metadata sometimes stores the task description rather than
    /// the agent nickname.
    pub fn matches_subagent_name(
        &self,
        subagent_name: &str,
        subagent_description: Option<&str>,
        candidate: &str,
    ) -> bool {
        subagent_name == candidate
            || (*self == AgentTool::Codex && subagent_description == Some(candidate))
    }
}
```

- [ ] **Step 3: Use it in agents.rs reduce_subagent_completed**

In `src/app/reducer/agents.rs`, replace lines 184-194:

```rust
        let matching_subagent = if id.tool() == AgentTool::Codex {
            agent.subagents_mut().iter_mut().find(|s| {
                s.name == subagent_name
                    || s.description.as_deref() == Some(subagent_name)
            })
        } else {
            agent
                .subagents_mut()
                .iter_mut()
                .find(|s| s.name == subagent_name)
        };
```

with:

```rust
        let tool = id.tool();
        let matching_subagent = agent
            .subagents_mut()
            .iter_mut()
            .find(|s| tool.matches_subagent_name(&s.name, s.description.as_deref(), subagent_name));
```

- [ ] **Step 4: Run tests and commit**

Run: `cargo test --workspace`
Expected: All pass.

```bash
git add crates/nexus-domain/src/agents/ids.rs src/app/reducer/agents.rs
git commit -m "refactor: move Codex-specific subagent matching into AgentTool method"
```

---

## Task 5: Enforce State Encapsulation

### Task 5a: Add missing getters to state types

**Files:**
- Modify: `src/app/state/overview.rs`
- Modify: `src/app/state/accounts.rs`
- Modify: `src/app/state/system.rs`
- Modify: `src/app/core.rs`

- [ ] **Step 1: Add filter getter to OverviewState**

In `src/app/state/overview.rs`, add to the impl block:
```rust
    pub fn filter(&self) -> Option<&str> {
        self.filter.as_deref()
    }
```

- [ ] **Step 2: Add missing getters to AccountsState**

In `src/app/state/accounts.rs`, add to the impl block:
```rust
    pub fn entries(&self) -> &[AccountEntry] {
        &self.entries
    }

    pub fn quota_loading(&self) -> bool {
        self.quota_loading
    }
```

- [ ] **Step 3: Add workflow getter to SystemState**

In `src/app/state/system.rs`, add to the impl block:
```rust
    pub fn workflow(&self) -> &Workflow {
        &self.workflow
    }
```

- [ ] **Step 4: Add delegating getters on AppState**

In `src/app/core.rs`, add:
```rust
    pub fn overview(&self) -> &OverviewState {
        self.root.overview()
    }

    pub fn accounts(&self) -> &AccountsState {
        self.root.accounts()
    }

    pub fn system(&self) -> &SystemState {
        self.root.system()
    }

    pub fn filter(&self) -> Option<&str> {
        self.root.overview().filter()
    }
```

- [ ] **Step 5: Run tests**

Run: `cargo test --workspace`
Expected: All pass — these are additive.

- [ ] **Step 6: Commit**

```bash
git add src/app/state/overview.rs src/app/state/accounts.rs src/app/state/system.rs src/app/core.rs
git commit -m "refactor: add read-only getters to state types for external access"
```

### Task 5b: Fix tests to use action dispatch instead of direct mutation

**Files:**
- Modify: `src/app/core_tests.rs`
- Modify: `src/app/state/mod.rs` (tests section)

- [ ] **Step 1: Fix core_tests.rs — filter tests**

In `src/app/core_tests.rs`, the test `visible_agent_indices_refresh_when_filter_changes` (line 52) directly sets `state.root.overview.filter`. Rewrite to use action dispatch:

```rust
#[test]
fn visible_agent_indices_refresh_when_filter_changes() {
    let mut state = AppState::new();
    state.handle_event(discovered(AgentTool::Claude, 1, "/tmp/claude"));
    state.handle_event(discovered(AgentTool::OpenCode, 2, "/tmp/opencode"));

    assert_eq!(state.visible_agents().len(), 2);

    // Apply filter via action dispatch
    state.start_filter();
    for ch in "claude".chars() {
        state.filter_push_char(ch);
    }
    state.confirm_filter();
    assert_eq!(state.visible_agents().len(), 1);
    assert_eq!(state.visible_agents()[0].pid(), Some(1));

    // Change filter
    state.start_filter();
    state.clear_filter_input();
    for ch in "opencode".chars() {
        state.filter_push_char(ch);
    }
    state.confirm_filter();
    assert_eq!(state.visible_agents().len(), 1);
    assert_eq!(state.visible_agents()[0].pid(), Some(2));
}
```

Note: check what methods are available on AppState for filter management. If `start_filter`, `filter_push_char`, `confirm_filter`, `clear_filter_input` don't exist, use the `handle_event` equivalent or dispatch `Action::Overview(OverviewAction::StartFiltering)` etc. Look at `src/app/overview.rs` for the available methods.

- [ ] **Step 2: Fix core_tests.rs — sort test**

Rewrite `visible_agent_indices_refresh_when_sort_changes` to dispatch `Action::Overview(OverviewAction::CycleSortMode)` instead of setting `state.root.overview.sort_mode = SortMode::Elapsed` directly.

- [ ] **Step 3: Fix core_tests.rs — invariant test**

Rewrite `overview_invariants_hold_after_filter_changes` to use action dispatch for filter changes.

- [ ] **Step 4: Fix core_tests.rs — workspace/overlay tests**

Rewrite tests that directly access `state.root.workspace.active`, `state.root.system.overlay`, `state.root.overview.filter` to use public getters instead.

For example, line 376-377:
```rust
    assert_eq!(state.root.workspace.active, Workspace::Overview);
```
becomes:
```rust
    assert_eq!(state.active_tab(), Workspace::Overview);
```

Line 389:
```rust
    assert_eq!(state.root.system.overlay, Overlay::None);
```
becomes:
```rust
    assert!(!state.show_help());
```

- [ ] **Step 5: Fix visible_agents_applies_filter test**

Line 199 sets `state.root.overview.filter = Some("review".into())`. Rewrite to use the filter action dispatch.

- [ ] **Step 6: Fix state/mod.rs tests**

In `src/app/state/mod.rs`, the tests section (line 84-319) directly accesses state fields. Since these are inside `crate::app`, they have `pub(in crate::app)` access. These tests CAN keep direct field access since they're testing state module internals. However, the tests at lines 143-149 that assert on internal field values should use getters where possible:

```rust
assert_eq!(state.workspace().active(), Workspace::Overview);
assert_eq!(*state.overview().mode(), OverviewMode::Normal);
```

Replace direct field access with getter calls. Keep `pub(in crate::app)` access for mutation in tests that specifically test state manipulation (like `accounts_entries_provide_selection_target` which pushes to `state.entries`).

- [ ] **Step 7: Fix accounts tests that directly mutate**

In `src/app/state/mod.rs`, test `accounts_entries_provide_selection_target` (line 225) directly pushes to `state.entries` and sets `state.selected_index`. Since this test is inside `crate::app`, it has access. This is fine — it's testing the state module itself. But change the assertions to use getters:

```rust
assert_eq!(
    state.selected_entry(),
    Some(&AccountEntry {
        provider: Provider::Codex,
        profile_id: "codex-main".into(),
    })
);
```

- [ ] **Step 8: Run tests and commit**

Run: `cargo test --workspace`
Expected: All pass.

```bash
git add src/app/core_tests.rs src/app/state/mod.rs
git commit -m "refactor: rewrite tests to use action dispatch and getters instead of direct state mutation"
```

### Task 5c: Final verification

- [ ] **Step 1: Run full test suite**

Run: `cargo test --workspace`
Expected: All pass.

- [ ] **Step 2: Run clippy**

Run: `cargo clippy --workspace -- -D warnings`
Expected: No warnings.

- [ ] **Step 3: Verify no remaining direct field access from UI/services**

Run: `grep -rn "\.root\.\(overview\|accounts\|system\|workspace\)\.\(filter\|mode\|selected_index\|sort_mode\|overlay\|workflow\|entries\|active\)" src/ui/ src/services/`

Expected: No matches (all external access goes through getters).

- [ ] **Step 4: Commit verification**

```bash
git add -A
git commit -m "chore: final verification — all tests pass, clippy clean"
```

---

## Summary

| Task | Description | Estimated Complexity |
|------|-------------|---------------------|
| 1a | ProfileId validation | Small |
| 1b | FromStr for TodoStatus/TodoPriority | Small |
| 1c | #[non_exhaustive] on enums | Small |
| 1d | Move EditableProfile out of domain | Small |
| 1e | Move format functions out of domain | Small |
| 2a | Quota model cleanup | Medium |
| 3a | DetectorError context | Medium |
| 3b | Worktree git timeout | Small |
| 3c | Claude cache eviction | Small |
| 4a | Create agent_lookup module | Medium |
| 4b | Replace inline finders | Small |
| 4c | Remove expect() | Small |
| 4d | Codex subagent matching in AgentTool | Small |
| 5a | Add state getters | Small |
| 5b | Fix tests for encapsulation | Medium |
| 5c | Final verification | Small |
