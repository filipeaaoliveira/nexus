use std::collections::BTreeSet;
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::path::Path;

use anyhow::{anyhow, Context, Result};
use serde::de::DeserializeOwned;
use serde_json::{json, Value};

use crate::codex_app_server::types::{
    AccountReadResponse, AccountReadResult, CodexRateLimits, CodexThreadDetail,
    CodexThreadSummary, RateLimitsReadResponse, ThreadListResponse, ThreadListResult,
    ThreadReadResponse,
};

pub trait CodexControlPlane {
    fn list_threads(&mut self) -> Result<Vec<CodexThreadSummary>>;
    fn read_thread(&mut self, thread_id: &str) -> Result<CodexThreadDetail>;
    fn read_account(&mut self) -> Result<AccountReadResult>;
    fn read_rate_limits(&mut self) -> Result<CodexRateLimits>;
}

pub struct CodexAppServerClient {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    next_id: u64,
}

impl CodexAppServerClient {
    pub fn spawn(codex_bin: &Path) -> Result<Self> {
        let mut child = Command::new(codex_bin)
            .arg("app-server")
            .arg("--listen")
            .arg("stdio://")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .with_context(|| format!("failed to spawn {}", codex_bin.display()))?;

        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| anyhow!("failed to capture app-server stdin"))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| anyhow!("failed to capture app-server stdout"))?;

        let mut client = Self {
            child,
            stdin,
            stdout: BufReader::new(stdout),
            next_id: 1,
        };
        client.initialize()?;
        Ok(client)
    }

    fn initialize(&mut self) -> Result<()> {
        let _: Value = self.request(
            "initialize",
            json!({
                "clientInfo": {
                    "name": "nexus",
                    "title": "Nexus",
                    "version": env!("CARGO_PKG_VERSION"),
                }
            }),
        )?;
        self.notify("initialized", None)?;
        Ok(())
    }

    fn notify(&mut self, method: &str, params: Option<Value>) -> Result<()> {
        let message = notification_message(method, params);
        self.write_message(&message)
    }

    fn request<T: DeserializeOwned>(&mut self, method: &str, params: Value) -> Result<T> {
        let id = self.next_id;
        self.next_id += 1;

        let message = request_message(id, method, params);
        self.write_message(&message)?;

        let mut line = String::new();
        loop {
            line.clear();
            let read = self
                .stdout
                .read_line(&mut line)
                .context("failed to read app-server response")?;
            if read == 0 {
                return Err(anyhow!("app-server closed stdout"));
            }

            let value: Value = serde_json::from_str(line.trim())
                .context("failed to decode app-server JSON-RPC message")?;

            match value.get("id") {
                Some(response_id) if response_id == &json!(id) => {
                    if let Some(error) = value.get("error") {
                        return Err(anyhow!("app-server error: {error}"));
                    }
                    return serde_json::from_value(value)
                        .context("failed to decode typed app-server response");
                }
                _ => continue,
            }
        }
    }

    fn write_message(&mut self, message: &Value) -> Result<()> {
        serde_json::to_writer(&mut self.stdin, message)
            .context("failed to encode app-server request")?;
        self.stdin
            .write_all(b"\n")
            .context("failed to write app-server newline")?;
        self.stdin.flush().context("failed to flush app-server stdin")
    }

    fn request_thread_page(&mut self, cursor: Option<String>) -> Result<ThreadListResult> {
        let response: ThreadListResponse = self.request(
            "thread/list",
            json!({
                "cursor": cursor,
                "limit": 100,
            }),
        )?;
        Ok(response.result)
    }
}

pub fn collect_thread_list_pages<F>(mut fetch_page: F) -> Result<Vec<CodexThreadSummary>>
where
    F: FnMut(Option<String>) -> Result<ThreadListResult>,
{
    let mut cursor: Option<String> = None;
    let mut all_threads = Vec::new();
    let mut seen_cursors = BTreeSet::new();

    loop {
        if let Some(current_cursor) = cursor.as_ref() {
            if !seen_cursors.insert(current_cursor.clone()) {
                return Err(anyhow!(
                    "app-server thread/list returned a repeated cursor: {current_cursor}"
                ));
            }
        }
        let result = fetch_page(cursor.clone())?;
        all_threads.extend(result.data);
        cursor = result.next_cursor;
        if cursor.is_none() {
            break;
        }
    }

    Ok(all_threads)
}

fn notification_message(method: &str, params: Option<Value>) -> Value {
    match params {
        Some(params) => json!({
            "method": method,
            "params": params,
        }),
        None => json!({
            "method": method,
        }),
    }
}

fn request_message(id: u64, method: &str, params: Value) -> Value {
    json!({
        "id": id,
        "method": method,
        "params": params,
    })
}

impl CodexControlPlane for CodexAppServerClient {
    fn list_threads(&mut self) -> Result<Vec<CodexThreadSummary>> {
        collect_thread_list_pages(|cursor| self.request_thread_page(cursor))
    }

    fn read_thread(&mut self, thread_id: &str) -> Result<CodexThreadDetail> {
        let response: ThreadReadResponse = self.request(
            "thread/read",
            json!({
                "threadId": thread_id,
                "includeTurns": true,
            }),
        )?;
        Ok(response.result.thread)
    }

    fn read_account(&mut self) -> Result<AccountReadResult> {
        let response: AccountReadResponse = self.request(
            "account/read",
            json!({
                "refreshToken": false,
            }),
        )?;
        Ok(response.result)
    }

    fn read_rate_limits(&mut self) -> Result<CodexRateLimits> {
        let response: RateLimitsReadResponse =
            self.request("account/rateLimits/read", json!({}))?;
        Ok(CodexRateLimits {
            rate_limits: response.result.rate_limits,
            rate_limits_by_limit_id: response.result.rate_limits_by_limit_id,
        })
    }
}

impl Drop for CodexAppServerClient {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{notification_message, request_message};

    #[test]
    fn initialized_notification_omits_params() {
        assert_eq!(notification_message("initialized", None), json!({ "method": "initialized" }));
    }

    #[test]
    fn request_message_includes_id_method_and_params() {
        assert_eq!(
            request_message(7, "account/read", json!({ "refreshToken": false })),
            json!({
                "id": 7,
                "method": "account/read",
                "params": {
                    "refreshToken": false
                }
            })
        );
    }
}
