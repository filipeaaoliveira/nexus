use std::collections::BTreeMap;

use serde::de::{self, Deserializer};
use serde::Deserialize;
use serde_json::Value;

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct ThreadStatus {
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(rename = "activeFlags", default)]
    pub active_flags: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct CodexThreadSummary {
    pub id: String,
    #[serde(rename = "forkedFromId")]
    pub forked_from_id: Option<String>,
    pub preview: String,
    pub ephemeral: bool,
    #[serde(rename = "modelProvider")]
    pub model_provider: String,
    #[serde(rename = "createdAt")]
    pub created_at: i64,
    #[serde(rename = "updatedAt")]
    pub updated_at: i64,
    pub status: ThreadStatus,
    pub path: Option<String>,
    pub cwd: String,
    #[serde(rename = "cliVersion")]
    pub cli_version: String,
    pub source: SessionSource,
    #[serde(rename = "agentNickname")]
    pub agent_nickname: Option<String>,
    #[serde(rename = "agentRole")]
    pub agent_role: Option<String>,
    #[serde(rename = "gitInfo")]
    pub git_info: Option<GitInfo>,
    pub name: Option<String>,
    pub turns: Vec<Value>,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct CodexThreadDetail {
    #[serde(flatten)]
    pub summary: CodexThreadSummary,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct ThreadListResult {
    pub data: Vec<CodexThreadSummary>,
    #[serde(rename = "nextCursor", default)]
    pub next_cursor: Option<String>,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct ThreadListResponse {
    pub id: Value,
    pub result: ThreadListResult,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct ThreadReadResult {
    pub thread: CodexThreadDetail,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct ThreadReadResponse {
    pub id: Value,
    pub result: ThreadReadResult,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct ThreadStatusChangedParams {
    #[serde(rename = "threadId")]
    pub thread_id: String,
    pub status: ThreadStatus,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct ThreadStatusChangedNotification {
    pub method: String,
    pub params: ThreadStatusChangedParams,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(tag = "type")]
pub enum CodexAccount {
    #[serde(rename = "apiKey")]
    ApiKey,
    #[serde(rename = "chatgpt")]
    Chatgpt {
        email: String,
        #[serde(rename = "planType")]
        plan_type: String,
    },
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct AccountReadResult {
    #[serde(default)]
    pub account: Option<CodexAccount>,
    #[serde(rename = "requiresOpenaiAuth")]
    pub requires_openai_auth: bool,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct AccountReadResponse {
    pub id: Value,
    pub result: AccountReadResult,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionSource {
    Cli,
    Vscode,
    Exec,
    AppServer,
    Custom { custom: String },
    SubAgent {
        sub_agent: Value,
    },
    Unknown(String),
}

impl<'de> Deserialize<'de> for SessionSource {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = Value::deserialize(deserializer)?;

        match value {
            Value::String(source) => Ok(match source.as_str() {
                "cli" => SessionSource::Cli,
                "vscode" => SessionSource::Vscode,
                "exec" => SessionSource::Exec,
                "appServer" => SessionSource::AppServer,
                _ => SessionSource::Unknown(source),
            }),
            Value::Object(mut object) => {
                let custom = object.remove("custom");
                let sub_agent = object.remove("subAgent");

                if !object.is_empty() {
                    return Err(de::Error::custom("unsupported session source object"));
                }

                match (custom, sub_agent) {
                    (Some(Value::String(custom)), None) => Ok(SessionSource::Custom { custom }),
                    (None, Some(sub_agent)) => Ok(SessionSource::SubAgent { sub_agent }),
                    (None, None) => Err(de::Error::custom("unsupported session source object")),
                    _ => Err(de::Error::custom("unsupported session source object")),
                }
            }
            _ => Err(de::Error::custom("unsupported session source value")),
        }
    }
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct GitInfo {
    pub sha: Option<String>,
    pub branch: Option<String>,
    #[serde(rename = "originUrl")]
    pub origin_url: Option<String>,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct CodexRateLimitWindow {
    #[serde(rename = "usedPercent")]
    pub used_percent: u8,
    #[serde(rename = "windowDurationMins")]
    pub window_duration_mins: Option<i64>,
    #[serde(rename = "resetsAt")]
    pub resets_at: Option<i64>,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct CodexCredits {
    #[serde(rename = "hasCredits")]
    pub has_credits: bool,
    pub unlimited: bool,
    pub balance: Option<String>,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq, Default)]
pub struct CodexRateLimitSnapshot {
    #[serde(rename = "limitId")]
    pub limit_id: Option<String>,
    #[serde(rename = "limitName")]
    pub limit_name: Option<String>,
    pub primary: Option<CodexRateLimitWindow>,
    pub secondary: Option<CodexRateLimitWindow>,
    pub credits: Option<CodexCredits>,
    #[serde(rename = "planType")]
    pub plan_type: Option<String>,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq, Default)]
pub struct CodexRateLimits {
    #[serde(rename = "rateLimits")]
    pub rate_limits: CodexRateLimitSnapshot,
    #[serde(
        rename = "rateLimitsByLimitId",
        default,
        deserialize_with = "deserialize_rate_limit_map"
    )]
    pub rate_limits_by_limit_id: BTreeMap<String, CodexRateLimitSnapshot>,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct RateLimitsReadResult {
    #[serde(rename = "rateLimits")]
    pub rate_limits: CodexRateLimitSnapshot,
    #[serde(
        rename = "rateLimitsByLimitId",
        default,
        deserialize_with = "deserialize_rate_limit_map"
    )]
    pub rate_limits_by_limit_id: BTreeMap<String, CodexRateLimitSnapshot>,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct RateLimitsReadResponse {
    pub id: Value,
    pub result: RateLimitsReadResult,
}

fn deserialize_rate_limit_map<'de, D>(
    deserializer: D,
) -> Result<BTreeMap<String, CodexRateLimitSnapshot>, D::Error>
where
    D: Deserializer<'de>,
{
    Ok(Option::<BTreeMap<String, CodexRateLimitSnapshot>>::deserialize(deserializer)?
        .unwrap_or_default())
}
