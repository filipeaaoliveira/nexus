use nexus::domain::accounts::model::{
    AccountEntry, AuthMetadata, ClaudeProfile, ClaudeProfileMetadata, CodexProfile, ProfileMetadata,
};
use nexus::domain::accounts::provider::{Provider, ProviderFilter};
use nexus::domain::accounts::quota::{
    Confidence, LimitState, QuotaCache, QuotaSnapshot, QuotaWindow,
};
use nexus::domain::agents::activity::{
    Activity, ActivityEvent, ActivityEventKind, Subagent, TodoItem, TodoPriority, TodoStatus,
};
use nexus::domain::agents::ids::{AgentId, AgentIdentity, AgentTool};
use nexus::domain::agents::model::{Agent, SortMode};
use nexus::domain::agents::status::{
    NormalizedReason, NormalizedState, NormalizedStatus, PrimaryStatus,
};
use nexus::domain::system::health::CodexBackendHealth;
use nexus::domain::system::notification::DetectorNotification;

#[test]
fn domain_modules_export_expected_paths() {
    let _ = std::any::TypeId::of::<AgentTool>();
    let _ = std::any::TypeId::of::<AgentIdentity>();
    let _ = std::any::TypeId::of::<AgentId>();
    let _ = std::any::TypeId::of::<PrimaryStatus>();
    let _ = std::any::TypeId::of::<NormalizedStatus>();
    let _ = std::any::TypeId::of::<NormalizedReason>();
    let _ = std::any::TypeId::of::<NormalizedState>();
    let _ = std::any::TypeId::of::<Activity>();
    let _ = std::any::TypeId::of::<ActivityEventKind>();
    let _ = std::any::TypeId::of::<ActivityEvent>();
    let _ = std::any::TypeId::of::<Subagent>();
    let _ = std::any::TypeId::of::<TodoStatus>();
    let _ = std::any::TypeId::of::<TodoPriority>();
    let _ = std::any::TypeId::of::<TodoItem>();
    let _ = std::any::TypeId::of::<Agent>();
    let _ = std::any::TypeId::of::<SortMode>();
    let _ = std::any::TypeId::of::<Provider>();
    let _ = std::any::TypeId::of::<ProviderFilter>();
    let _ = std::any::TypeId::of::<AccountEntry>();
    let _ = std::any::TypeId::of::<AuthMetadata>();
    let _ = std::any::TypeId::of::<CodexProfile>();
    let _ = std::any::TypeId::of::<ProfileMetadata>();
    let _ = std::any::TypeId::of::<ClaudeProfile>();
    let _ = std::any::TypeId::of::<ClaudeProfileMetadata>();
    let _ = std::any::TypeId::of::<QuotaWindow>();
    let _ = std::any::TypeId::of::<QuotaSnapshot>();
    let _ = std::any::TypeId::of::<QuotaCache>();
    let _ = std::any::TypeId::of::<Confidence>();
    let _ = std::any::TypeId::of::<LimitState>();
    let _ = std::any::TypeId::of::<CodexBackendHealth>();
    let _ = std::any::TypeId::of::<DetectorNotification>();
}
