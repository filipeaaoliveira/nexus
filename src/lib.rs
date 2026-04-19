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
    // app / event
    pub use crate::app::{AppState, RootState};
    pub use crate::event::AgentEvent;

    // domain::accounts
    pub use nexus_domain::accounts::{
        Confidence, CreditsState, LimitState, QuotaCache, QuotaSnapshot, QuotaWindow,
    };

    // domain::agents — shared vocabulary only
    pub use nexus_domain::agents::{
        Activity, AgentId, AgentTool, NormalizedReason, NormalizedState, NormalizedStatus,
        PrimaryStatus, SortMode, Subagent, TodoItem, TodoPriority, TodoStatus,
    };

    // app::agent — the stateful composite
    pub use crate::app::agent::{
        ActivityEvent, ActivityEventKind, Agent, AgentDetail, AgentIdent, AgentLifecycle,
    };

    // domain::shared
    pub use nexus_domain::shared::{
        format_duration, format_duration_short, from_epoch_millis, from_epoch_secs, from_rfc3339,
    };

    // domain::system
    pub use nexus_domain::system::{CodexBackendHealth, DetectorKind, DetectorNotification};

    // claude_profiles
    pub use crate::claude_profiles::profile::{
        load_claude_profile_metadata, ClaudeProfileMetadata,
    };

    // codex_profiles
    pub use crate::codex_profiles::profile::{
        extract_auth_metadata, load_profile_metadata, save_profile_metadata, ProfileMetadata,
        StoredCodexProfile,
    };
    pub use crate::codex_profiles::quota::{parse_legacy_status_output, parse_quota_from_sessions};
    pub use crate::codex_profiles::runtime::{
        canonical_repo_root, launch_codex, repo_runtime_key, require_codex_binary, RuntimePlan,
    };

    // detect
    pub use crate::detect::claude::{parse_project_session, parse_transcript};
    pub use crate::detect::codex::{parse_rollout, parse_session_index, parse_source_field};
    pub use crate::detect::opencode::{parse_message_data, parse_part_data};
    pub use crate::detect::process::{parse_lsof_cwds_multi, parse_ps_output};
    pub use crate::detect::worktree::parse_worktree_list;
}
