// Re-exported intentionally: QuotaSnapshot is a shared domain concept used by
// both Codex and Claude account flows, even though its definition lives in
// codex_profiles for historical reasons.
pub use crate::codex_profiles::quota::QuotaSnapshot;
