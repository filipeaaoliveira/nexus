mod app;
pub mod claude_profiles;
pub mod codex_profiles;
pub mod config;
mod detect;
pub mod domain;
mod event;
mod infra;
mod input;
pub mod runtime_support;
mod services;
mod ui;

pub use app::{AppState, RootState};
pub use config::{Cli, Config};
pub use event::AgentEvent;
pub use infra::config::migrate_legacy_dirs;
pub use runtime_support::run_blocking_compatible;
pub use services::runtime::run;
