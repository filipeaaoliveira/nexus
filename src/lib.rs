#![deny(unused_must_use)]

mod app;
pub mod claude_profiles;
pub mod codex_profiles;
pub mod config;
#[doc(hidden)]
pub mod detect;
pub mod domain;
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
    pub use crate::app::{AppState, RootState};
    pub use crate::event::AgentEvent;
}
