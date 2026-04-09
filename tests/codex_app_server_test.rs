use anyhow::{anyhow, Result};
use nexus::codex_app_server::client::{CodexAppServerClient, CodexControlPlane};
use nexus::codex_app_server::client::collect_thread_list_pages;
use nexus::codex_app_server::types::{
    AccountReadResponse, AccountReadResult, RateLimitsReadResponse, SessionSource,
    ThreadListResponse, ThreadReadResponse, ThreadStatusChangedNotification, ThreadListResult,
};
use std::path::PathBuf;

#[test]
fn decodes_thread_list_response() {
    let response: ThreadListResponse = serde_json::from_str(
        r#"{
            "id": 20,
            "result": {
                "data": [
                    {
                        "id": "thr_a",
                        "forkedFromId": null,
                        "preview": "Create a TUI",
                        "ephemeral": false,
                        "modelProvider": "openai",
                        "createdAt": 1730831111,
                        "updatedAt": 1730831111,
                        "status": { "type": "notLoaded" },
                        "path": "/tmp/session.jsonl",
                        "cwd": "/tmp/project",
                        "cliVersion": "0.1.0",
                        "source": "cli",
                        "agentNickname": "Atlas",
                        "agentRole": "explorer",
                        "gitInfo": null,
                        "name": null,
                        "turns": []
                    }
                ],
                "nextCursor": "opaque-token-or-null"
            }
        }"#,
    )
    .expect("thread/list response");

    assert_eq!(response.result.data.len(), 1);
    assert_eq!(response.result.data[0].id, "thr_a");
    assert_eq!(response.result.data[0].status.kind, "notLoaded");
}

#[test]
fn rejects_thread_list_payloads_missing_required_thread_fields() {
    let error = serde_json::from_str::<ThreadListResponse>(
        r#"{
            "id": 20,
            "result": {
                "data": [
                    {
                        "id": "thr_a",
                        "preview": "Create a TUI",
                        "modelProvider": "openai",
                        "createdAt": 1730831111,
                        "updatedAt": 1730831111,
                        "status": { "type": "notLoaded" },
                        "turns": []
                    }
                ],
                "nextCursor": null
            }
        }"#,
    )
    .expect_err("missing required thread fields should fail");

    let error_text = error.to_string();
    assert!(
        error_text.contains("ephemeral")
            || error_text.contains("cwd")
            || error_text.contains("cliVersion")
            || error_text.contains("source"),
        "unexpected error: {error_text}"
    );
}

#[test]
fn rejects_thread_list_payloads_missing_turns() {
    let error = serde_json::from_str::<ThreadListResponse>(
        r#"{
            "id": 20,
            "result": {
                "data": [
                    {
                        "id": "thr_a",
                        "forkedFromId": null,
                        "preview": "Create a TUI",
                        "ephemeral": false,
                        "modelProvider": "openai",
                        "createdAt": 1730831111,
                        "updatedAt": 1730831111,
                        "status": { "type": "notLoaded" },
                        "path": "/tmp/session.jsonl",
                        "cwd": "/tmp/project",
                        "cliVersion": "0.1.0",
                        "source": "cli",
                        "agentNickname": "Atlas",
                        "agentRole": "explorer",
                        "gitInfo": null,
                        "name": null
                    }
                ],
                "nextCursor": null
            }
        }"#,
    )
    .expect_err("missing thread turns should fail");

    assert!(error.to_string().contains("turns"));
}

#[test]
fn decodes_thread_status_changed_notification() {
    let notification: ThreadStatusChangedNotification = serde_json::from_str(
        r#"{
            "method": "thread/status/changed",
            "params": {
                "threadId": "thr_123",
                "status": { "type": "active", "activeFlags": [] }
            }
        }"#,
    )
    .expect("thread/status/changed notification");

    assert_eq!(notification.params.thread_id, "thr_123");
    assert_eq!(notification.params.status.kind, "active");
}

#[test]
fn preserves_unknown_thread_source_values() {
    let response: ThreadListResponse = serde_json::from_str(
        r#"{
            "id": 20,
            "result": {
                "data": [
                    {
                        "id": "thr_a",
                        "forkedFromId": null,
                        "preview": "Create a TUI",
                        "ephemeral": false,
                        "modelProvider": "openai",
                        "createdAt": 1730831111,
                        "updatedAt": 1730831111,
                        "status": { "type": "notLoaded" },
                        "path": "/tmp/session.jsonl",
                        "cwd": "/tmp/project",
                        "cliVersion": "0.1.0",
                        "source": "futureSource",
                        "agentNickname": null,
                        "agentRole": null,
                        "gitInfo": null,
                        "name": null,
                        "turns": []
                    }
                ],
                "nextCursor": null
            }
        }"#,
    )
    .expect("thread/list response with unknown source");

    assert_eq!(
        response.result.data[0].source,
        SessionSource::Unknown("futureSource".to_string())
    );
}

#[test]
fn decodes_custom_thread_source_object() {
    let response: ThreadListResponse = serde_json::from_str(
        r#"{
            "id": 20,
            "result": {
                "data": [
                    {
                        "id": "thr_a",
                        "forkedFromId": null,
                        "preview": "Create a TUI",
                        "ephemeral": false,
                        "modelProvider": "openai",
                        "createdAt": 1730831111,
                        "updatedAt": 1730831111,
                        "status": { "type": "notLoaded" },
                        "path": "/tmp/session.jsonl",
                        "cwd": "/tmp/project",
                        "cliVersion": "0.1.0",
                        "source": { "custom": "myClient" },
                        "agentNickname": null,
                        "agentRole": null,
                        "gitInfo": null,
                        "name": null,
                        "turns": []
                    }
                ],
                "nextCursor": null
            }
        }"#,
    )
    .expect("thread/list response with custom source");

    assert_eq!(
        response.result.data[0].source,
        SessionSource::Custom {
            custom: "myClient".to_string()
        }
    );
}

#[test]
fn rejects_invalid_thread_source_object_shapes() {
    let error = serde_json::from_str::<ThreadListResponse>(
        r#"{
            "id": 20,
            "result": {
                "data": [
                    {
                        "id": "thr_a",
                        "forkedFromId": null,
                        "preview": "Create a TUI",
                        "ephemeral": false,
                        "modelProvider": "openai",
                        "createdAt": 1730831111,
                        "updatedAt": 1730831111,
                        "status": { "type": "notLoaded" },
                        "path": "/tmp/session.jsonl",
                        "cwd": "/tmp/project",
                        "cliVersion": "0.1.0",
                        "source": { "custom": "myClient", "subAgent": {} },
                        "agentNickname": null,
                        "agentRole": null,
                        "gitInfo": null,
                        "name": null,
                        "turns": []
                    }
                ],
                "nextCursor": null
            }
        }"#,
    )
    .expect_err("invalid source object should fail");

    assert!(error.to_string().contains("session source"));
}

#[test]
fn decodes_account_rate_limits_response() {
    let response: RateLimitsReadResponse = serde_json::from_str(
        r#"{
            "id": 7,
            "result": {
                "rateLimits": {
                    "limitId": null,
                    "limitName": null,
                    "primary": {
                        "usedPercent": 25,
                        "windowDurationMins": 15,
                        "resetsAt": 1730947200
                    },
                    "secondary": null,
                    "credits": {
                        "hasCredits": true,
                        "balance": "$12.50",
                        "unlimited": false
                    },
                    "planType": "pro"
                },
                "rateLimitsByLimitId": {
                    "burst": {
                        "limitId": "burst",
                        "limitName": "15m",
                        "primary": {
                            "usedPercent": 25,
                            "windowDurationMins": 15,
                            "resetsAt": 1730947200
                        },
                        "secondary": null,
                        "credits": null,
                        "planType": "pro"
                    }
                }
            }
        }"#,
    )
    .expect("account/rateLimits/read response");

    assert_eq!(response.result.rate_limits.primary.as_ref().unwrap().used_percent, 25);
    assert_eq!(
        response.result.rate_limits.primary.as_ref().unwrap().window_duration_mins,
        Some(15)
    );
    assert!(response.result.rate_limits.secondary.is_none());
    assert_eq!(
        response
            .result
            .rate_limits
            .credits
            .as_ref()
            .and_then(|credits| credits.balance.as_deref()),
        Some("$12.50")
    );
    assert_eq!(
        response
            .result
            .rate_limits_by_limit_id
            .get("burst")
            .and_then(|snapshot| snapshot.limit_name.as_deref()),
        Some("15m")
    );
}

#[test]
fn decodes_account_rate_limits_response_with_null_limit_map() {
    let response: RateLimitsReadResponse = serde_json::from_str(
        r#"{
            "id": 7,
            "result": {
                "rateLimits": {
                    "limitId": null,
                    "limitName": null,
                    "primary": {
                        "usedPercent": 25,
                        "windowDurationMins": 15,
                        "resetsAt": 1730947200
                    },
                    "secondary": null,
                    "credits": null,
                    "planType": "pro"
                },
                "rateLimitsByLimitId": null
            }
        }"#,
    )
    .expect("account/rateLimits/read response with null limit map");

    assert!(response.result.rate_limits_by_limit_id.is_empty());
}

#[test]
fn decodes_account_read_response() {
    let response: AccountReadResponse = serde_json::from_str(
        r#"{
            "id": 1,
            "result": {
                "account": {
                    "type": "chatgpt",
                    "email": "user@example.com",
                    "planType": "pro"
                },
                "requiresOpenaiAuth": true
            }
        }"#,
    )
    .expect("account/read response");

    let account = response.result.account.expect("account");
    assert_eq!(
        account,
        nexus::codex_app_server::types::CodexAccount::Chatgpt {
            email: "user@example.com".to_string(),
            plan_type: "pro".to_string()
        }
    );
    assert!(response.result.requires_openai_auth);
}

#[test]
fn rejects_account_read_response_missing_requires_openai_auth() {
    let error = serde_json::from_str::<AccountReadResponse>(
        r#"{
            "id": 1,
            "result": {
                "account": {
                    "type": "apiKey"
                }
            }
        }"#,
    )
    .expect_err("missing requiresOpenaiAuth should fail");

    assert!(error.to_string().contains("requiresOpenaiAuth"));
}

#[test]
fn decodes_api_key_account_read_response() {
    let response: AccountReadResponse = serde_json::from_str(
        r#"{
            "id": 1,
            "result": {
                "account": {
                    "type": "apiKey"
                },
                "requiresOpenaiAuth": false
            }
        }"#,
    )
    .expect("account/read api key response");

    let account = response.result.account.expect("account");
    assert_eq!(account, nexus::codex_app_server::types::CodexAccount::ApiKey);
    assert!(!response.result.requires_openai_auth);
}

#[test]
fn rejects_malformed_chatgpt_account_read_response() {
    let error = serde_json::from_str::<AccountReadResponse>(
        r#"{
            "id": 1,
            "result": {
                "account": {
                    "type": "chatgpt",
                    "email": "user@example.com"
                },
                "requiresOpenaiAuth": true
            }
        }"#,
    )
    .expect_err("missing chatgpt planType should fail");

    assert!(error.to_string().contains("planType"));
}

#[test]
fn account_read_result_preserves_auth_requirement_without_account() {
    let result: AccountReadResult = serde_json::from_str(
        r#"{
            "account": null,
            "requiresOpenaiAuth": true
        }"#,
    )
    .expect("account/read result");

    assert!(result.account.is_none());
    assert!(result.requires_openai_auth);
}

#[test]
fn decodes_thread_read_response() {
    let response: ThreadReadResponse = serde_json::from_str(
        r#"{
            "id": 22,
            "result": {
                "thread": {
                    "id": "thr_123",
                    "forkedFromId": null,
                    "preview": "Create a TUI",
                    "ephemeral": false,
                    "modelProvider": "openai",
                    "createdAt": 1730831111,
                    "updatedAt": 1730832222,
                    "status": { "type": "notLoaded" },
                    "path": "/tmp/session.jsonl",
                    "cwd": "/tmp/project",
                    "cliVersion": "0.1.0",
                    "source": "cli",
                    "agentNickname": null,
                    "agentRole": null,
                    "gitInfo": null,
                    "name": null,
                    "turns": []
                }
            }
        }"#,
    )
    .expect("thread/read response");

    assert_eq!(response.result.thread.summary.id, "thr_123");
    assert_eq!(response.result.thread.summary.turns.len(), 0);
}

#[test]
fn rejects_thread_read_response_missing_turns() {
    let error = serde_json::from_str::<ThreadReadResponse>(
        r#"{
            "id": 22,
            "result": {
                "thread": {
                    "id": "thr_123",
                    "forkedFromId": null,
                    "preview": "Create a TUI",
                    "ephemeral": false,
                    "modelProvider": "openai",
                    "createdAt": 1730831111,
                    "updatedAt": 1730832222,
                    "status": { "type": "notLoaded" },
                    "path": "/tmp/session.jsonl",
                    "cwd": "/tmp/project",
                    "cliVersion": "0.1.0",
                    "source": "cli",
                    "agentNickname": null,
                    "agentRole": null,
                    "gitInfo": null,
                    "name": null
                }
            }
        }"#,
    )
    .expect_err("missing thread/read turns should fail");

    assert!(error.to_string().contains("turns"));
}

#[test]
fn collects_all_thread_list_pages() {
    let pages = vec![
        ThreadListResult {
            data: vec![serde_json::from_value(serde_json::json!({
                "id": "thr_a",
                "forkedFromId": null,
                "preview": "First",
                "ephemeral": false,
                "modelProvider": "openai",
                "createdAt": 1,
                "updatedAt": 1,
                "status": { "type": "notLoaded" },
                "path": "/tmp/a.jsonl",
                "cwd": "/tmp/project",
                "cliVersion": "0.1.0",
                "source": "cli",
                "agentNickname": null,
                "agentRole": null,
                "gitInfo": null,
                "name": null,
                "turns": []
            }))
            .unwrap()],
            next_cursor: Some("page-2".to_string()),
        },
        ThreadListResult {
            data: vec![serde_json::from_value(serde_json::json!({
                "id": "thr_b",
                "forkedFromId": null,
                "preview": "Second",
                "ephemeral": false,
                "modelProvider": "openai",
                "createdAt": 2,
                "updatedAt": 2,
                "status": { "type": "notLoaded" },
                "path": "/tmp/b.jsonl",
                "cwd": "/tmp/project",
                "cliVersion": "0.1.0",
                "source": "cli",
                "agentNickname": null,
                "agentRole": null,
                "gitInfo": null,
                "name": null,
                "turns": []
            }))
            .unwrap()],
            next_cursor: None,
        },
    ];
    let mut seen_cursors = Vec::new();
    let mut page_iter = pages.into_iter();

    let threads = collect_thread_list_pages(|cursor| -> Result<ThreadListResult> {
        seen_cursors.push(cursor);
        Ok(page_iter.next().expect("next page"))
    })
    .expect("paginated threads");

    assert_eq!(seen_cursors, vec![None, Some("page-2".to_string())]);
    assert_eq!(threads.len(), 2);
    assert_eq!(threads[0].id, "thr_a");
    assert_eq!(threads[1].id, "thr_b");
}

#[test]
fn rejects_repeated_thread_list_cursor() {
    let mut calls = 0;

    let error = collect_thread_list_pages(|cursor| -> Result<ThreadListResult> {
        calls += 1;
        Ok(ThreadListResult {
            data: Vec::new(),
            next_cursor: match cursor {
                None => Some("page-2".to_string()),
                Some(existing) => Some(existing),
            },
        })
    })
    .expect_err("repeated cursor should fail");

    assert!(error.to_string().contains("cursor"));
    assert_eq!(calls, 2);
}

fn codex_bin() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("CODEX_BIN").map(PathBuf::from) {
        return Some(path);
    }

    std::env::var_os("PATH").and_then(|path| {
        std::env::split_paths(&path)
            .map(|dir| dir.join("codex"))
            .find(|candidate| candidate.is_file())
    })
}

#[test]
#[ignore = "requires CODEX_LIVE_SMOKE=1 and a local codex install"]
fn live_codex_app_server_smoke_test() -> Result<()> {
    if std::env::var("CODEX_LIVE_SMOKE").as_deref() != Ok("1") {
        return Ok(());
    }

    let codex_bin =
        codex_bin().ok_or_else(|| anyhow!("CODEX_LIVE_SMOKE=1 but no codex binary was found via CODEX_BIN or PATH"))?;

    let mut client = CodexAppServerClient::spawn(&codex_bin)?;

    let account = client.read_account()?;
    let threads = client.list_threads()?;

    assert!(
        account.account.is_some() || account.requires_openai_auth,
        "account/read returned neither account data nor requiresOpenaiAuth"
    );

    if account.account.is_some() {
        let _rate_limits = client.read_rate_limits()?;
    }

    if let Some(thread) = threads.first() {
        let thread_detail = client.read_thread(&thread.id)?;
        assert_eq!(thread_detail.summary.id, thread.id);
    }

    Ok(())
}
