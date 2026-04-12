# Architecture And Release Docs Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a deep architecture documentation set for how `nexus` works internally and document a lightweight public release process with tagging and release-notes discipline.

**Architecture:** Create a dedicated `docs/architecture/` documentation tree with one index and several subsystem deep-dives that follow the actual code boundaries: runtime/data flow, detection, app state/events, UI/interaction, and profile management. Add a separate release-process doc under `docs/` and link both the architecture and release docs from the existing README and CONTRIBUTING guides.

**Tech Stack:** Markdown documentation, existing Rust source layout, GitHub-oriented release workflow guidance

---

### Task 1: Architecture Doc Set

**Files:**
- Create: `docs/architecture/README.md`
- Create: `docs/architecture/runtime-and-data-flow.md`
- Create: `docs/architecture/detection.md`
- Create: `docs/architecture/app-state-and-events.md`
- Create: `docs/architecture/ui-and-interaction.md`
- Create: `docs/architecture/codex-and-claude-profiles.md`

- [ ] **Step 1: Draft the architecture index**

Write the index doc with the system map, subsystem overview, glossary, and reading order.

- [ ] **Step 2: Draft runtime/data-flow documentation**

Explain startup, tracing, terminal lifecycle, detector spawning, quota polling, event flow, and shutdown/degraded behavior.

- [ ] **Step 3: Draft detection documentation**

Explain process scanning, provider-specific detectors, helper modules, event emission, and schema drift risk.

- [ ] **Step 4: Draft app-state/events documentation**

Explain `AppState`, event families, Overview vs Accounts state, selection/filter/sort/cache behavior, and state mutation boundaries.

- [ ] **Step 5: Draft UI/interaction documentation**

Explain rendering composition, input mapping, overlays, empty states, terminal behavior constraints, and contributor guardrails.

- [ ] **Step 6: Draft profile-management documentation**

Explain Codex/Claude auth import, runtime materialization, quota/token flows, persistence boundaries, and safety assumptions.

### Task 2: Release Process Doc

**Files:**
- Create: `docs/releasing.md`

- [ ] **Step 1: Write the release-process document**

Document a minimal release discipline: update changelog, cut annotated tag, create GitHub release notes from changelog, verify locked build/test/clippy/release build before release, and keep README install guidance aligned with tagged releases.

### Task 3: Linkage

**Files:**
- Modify: `README.md`
- Modify: `CONTRIBUTING.md`

- [ ] **Step 1: Link architecture docs from README**

Add a short “Architecture” or “Internals” section pointing readers to `docs/architecture/README.md`.

- [ ] **Step 2: Link release-process doc from README or contributing docs**

Add a pointer for maintainers/contributors to the release workflow doc.

- [ ] **Step 3: Link architecture and release docs from CONTRIBUTING**

Add clear references for contributors who need deeper context than the quick contribution guide.

### Task 4: Sanity Pass

**Files:**
- No new files expected

- [ ] **Step 1: Read the new docs in order**

Open each new/modified doc and check for broken structure, contradictions, or misplaced claims.

- [ ] **Step 2: Confirm terminology matches code**

Verify that major terms used in the docs map cleanly to current module names and behavior.
