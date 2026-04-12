# Open-Source Readiness Blockers Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the repo safer to publish by surfacing detector dependency failures in the Overview UI, aligning public build/install docs with locked verification, and replacing a stale internal maintenance note with accurate current guidance.

**Architecture:** Keep the change set narrow. Add a small detector-health model that can be updated from process-scanning failures and rendered in the existing Overview empty state. Leave detector behavior intact aside from surfacing failures. Treat documentation issues as explicit maintenance fixes, not refactors.

**Tech Stack:** Rust, tokio, ratatui, crossterm, Markdown docs

---

### Task 1: Overview Dependency Health Messaging

**Files:**
- Modify: `src/detect/process.rs`
- Modify: `src/event.rs`
- Modify: `src/app/mod.rs`
- Modify: `src/ui/agent_list.rs`
- Test: `tests/app_test.rs`

- [ ] **Step 1: Write the failing test**

Add a test in `tests/app_test.rs` that feeds a detector-health/degraded event into `AppState`, renders the empty Overview state through the UI helper, and asserts that the message mentions the missing dependency or degraded scanner state instead of only the generic "No agents detected" text.

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --locked tests::...` or the specific integration test target containing the new test.
Expected: FAIL because no detector-health event/rendering path exists yet.

- [ ] **Step 3: Write minimal implementation**

Add a lightweight detector-health event and app state field, emit it from process scanning when `ps` or `lsof` cannot be executed successfully, and update the empty Overview render path to prefer a degraded dependency message when health is not nominal.

- [ ] **Step 4: Run test to verify it passes**

Run the targeted test again.
Expected: PASS.

- [ ] **Step 5: Add one more regression test if needed**

Cover the generic empty-state path to ensure healthy empty states still render the normal onboarding message.

### Task 2: Public Build/Install Docs

**Files:**
- Modify: `README.md`

- [ ] **Step 1: Update source build instructions**

Change the README source-build command to `cargo build --release --locked`.

- [ ] **Step 2: Update git install instructions**

Change the README git-install command to `cargo install --git ... --locked`.

- [ ] **Step 3: Clarify validation scope**

Add a short note that CI validates locked build/test workflows and that git-install remains supported but is not a dedicated CI smoke-test path unless such a job is added later.

### Task 3: Replace Stale Codex Maintenance Note

**Files:**
- Modify: `docs/codex-recovery/current-assumptions.md`

- [ ] **Step 1: Rewrite the document**

Replace the stale assumptions with a current maintenance note that explains the present runtime layout, explicitly states that repo runtimes are keyed per canonical repo path rather than `current-repo`, and preserves only still-relevant drift risks.

- [ ] **Step 2: Remove alarming stale claims**

Delete or rewrite the old cross-repo session bleed warning so it is clearly historical if mentioned at all.

### Task 4: Verification

**Files:**
- No source changes expected unless verification exposes a regression

- [ ] **Step 1: Run targeted tests**

Run the focused Overview/app test target that covers the new health messaging.

- [ ] **Step 2: Run full verification**

Run:

```bash
cargo test --locked
cargo clippy --all-targets --all-features -- -D warnings
cargo build --release --locked
```

Expected: all pass.
