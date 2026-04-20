#![forbid(unsafe_code)]
#![deny(unused_must_use)]

mod app;
mod claude_profiles;
mod codex_profiles;
pub mod config;
mod detect;
mod event;
mod infra;
mod input;
pub mod runtime_support;
mod services;
mod ui;

pub use config::{Cli, Config};
pub use infra::config::migrate_legacy_dirs;
pub use runtime_support::run_blocking_compatible;
pub use services::runtime::run;

#[doc(hidden)]
pub mod testing {
    //! Curated re-exports for integration tests.
    //!
    //! Grouped by domain. Only types needed by `tests/*.rs` are exposed.

    // ── App state & events ─────────────────────────────────────
    pub use crate::app::{AppState, RootState};
    pub use crate::event::AgentEvent;

    // ── Domain: accounts ───────────────────────────────────────
    pub use nexus_domain::accounts::{
        Confidence, CreditsState, LimitState, QuotaCache, QuotaSnapshot, QuotaWindow,
    };

    // ── Domain: agents ─────────────────────────────────────────
    pub use nexus_domain::agents::{
        Activity, AgentId, AgentTool, NormalizedReason, NormalizedState, NormalizedStatus,
        PrimaryStatus, SortMode, Subagent, TodoItem, TodoPriority, TodoStatus,
    };

    // ── App: agent composite ───────────────────────────────────
    pub use crate::app::agent::{
        ActivityEvent, ActivityEventKind, Agent, AgentDetail, AgentIdent, AgentLifecycle,
    };

    // ── Domain: shared utilities ───────────────────────────────
    pub use nexus_domain::shared::{
        format_duration, format_duration_short, from_epoch_millis, from_epoch_secs, from_rfc3339,
    };

    // ── Domain: system ─────────────────────────────────────────
    pub use nexus_domain::system::{CodexBackendHealth, DetectorKind, DetectorNotification};

    // ── Services: profile storage ──────────────────────────────
    pub use crate::services::accounts::quota::{load_quota_cache, save_quota_cache};
    pub use crate::services::accounts::storage::{ProfileStoreError, StoreResult};

    // ── Profiles: Claude ───────────────────────────────────────
    pub use crate::claude_profiles::profile::{
        load_claude_profile_metadata, ClaudeProfileMetadata,
    };

    // ── Profiles: Codex ────────────────────────────────────────
    pub use crate::codex_profiles::profile::{
        extract_auth_metadata, load_profile_metadata, save_profile_metadata, ProfileMetadata,
        StoredCodexProfile,
    };
    pub use crate::codex_profiles::quota::{parse_legacy_status_output, parse_quota_from_sessions};
    pub use crate::codex_profiles::runtime::{
        canonical_repo_root, launch_codex, repo_runtime_key, require_codex_binary, RuntimePlan,
    };

    // ── Detectors: parsers ─────────────────────────────────────
    pub use crate::detect::claude::{parse_project_session, parse_transcript};
    pub use crate::detect::codex::{parse_rollout, parse_session_index, parse_source_field};
    pub use crate::detect::opencode::{parse_message_data, parse_part_data};
    pub use crate::detect::process::{parse_lsof_cwds_multi, parse_ps_output};
    pub use crate::detect::worktree::parse_worktree_list;
}
