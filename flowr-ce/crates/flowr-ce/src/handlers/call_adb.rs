//! call:adb handler — runs AQL against ADB (Agent Database) agent memory
//!
//! In CNCF syntax: `call: adb` with `with:` containing `query` (AQL), optional
//! `variables`, and the ADB server from `_resource` (an `adb` resource, `config.url`)
//! or `url`. ADB's HTTP API: `POST {url}/query {query, variables?}` →
//! `{success, data, error, execution_time_ms}`. `call:clawdb` is an alias (the
//! resource type ADB replaces).

use async_trait::async_trait;
use serde_json::{json, Value};
use tracing::info;

use swf_parser::{NamedTask, Task};
use swf_runtime::context::ExecutionContext;
use swf_runtime::error::{RuntimeError, Result};
use swf_runtime::traits::{TaskHandler, TaskResult};

pub struct AdbHandler {
    task_type: &'static str,
    client: reqwest::Client,
}

impl AdbHandler {
    pub fn new(task_type: &'static str) -> Self {
        Self { task_type, client: reqwest::Client::new() }
    }
}

#[async_trait]
impl TaskHandler for AdbHandler {
    fn task_type(&self) -> &str {
        self.task_type
    }

    async fn execute(
        &self,
        task: &NamedTask,
        _input: Value,
        ctx: &mut ExecutionContext,
    ) -> Result<TaskResult> {
        let with = match &task.task {
            Task::Call(c) => c.with.clone().unwrap_or(json!({})),
            _ => return Err(RuntimeError::task_failed(&task.name, "expected call task")),
        };
        let query = with.get("query").and_then(|v| v.as_str()).unwrap_or("").trim().to_string();
        if query.is_empty() {
            return Err(RuntimeError::task_failed(&task.name, "call:adb requires 'query' (AQL) in with"));
        }

        // No silent default: a missing resource is an error, not a call to localhost.
        let url = if let Some(u) = with.get("url").and_then(|v| v.as_str()) {
            u.to_string()
        } else if let Some(r) = with.get("_resource").and_then(|v| v.as_str()) {
            let storage = ctx
                .storage
                .as_ref()
                .and_then(|s| s.downcast_ref::<std::sync::Arc<dyn flowr_storage::StorageBackend>>())
                .ok_or_else(|| RuntimeError::task_failed(&task.name, "no storage to resolve the ADB resource"))?;
            resolve_adb_url(storage.as_ref(), r)
                .await
                .map_err(|e| RuntimeError::task_failed(&task.name, &e))?
        } else {
            return Err(RuntimeError::task_failed(&task.name, "call:adb requires '_resource' (an adb resource) or 'url'"));
        };

        info!(task = %task.name, url = %url, "calling ADB");
        let result = adb_query(&self.client, &url, &query, with.get("variables"))
            .await
            .map_err(|e| RuntimeError::Communication(e))?;
        Ok(TaskResult::new(result))
    }
}

/// The ADB server URL of a resource reference (`namespace/name`, or `name` in `default`).
pub async fn resolve_adb_url(storage: &dyn flowr_storage::StorageBackend, resource: &str) -> std::result::Result<String, String> {
    let (ns, name) = resource.split_once('/').unwrap_or(("default", resource));
    let r = storage
        .resources()
        .get(ns, name)
        .await
        .map_err(|e| format!("ADB resource {ns}/{name} not found: {e}"))?;
    r.config
        .get("url")
        .or_else(|| r.config.get("endpoint"))
        .and_then(|v| v.as_str())
        .map(|s| s.trim_end_matches('/').to_string())
        .ok_or_else(|| format!("ADB resource {ns}/{name} has no config.url"))
}

/// Run one AQL statement on ADB; returns the query result (`{success, data: {type, …}, metadata}`).
pub async fn adb_query(
    client: &reqwest::Client,
    url: &str,
    query: &str,
    variables: Option<&Value>,
) -> std::result::Result<Value, String> {
    let mut body = json!({ "query": query });
    if let Some(v) = variables.filter(|v| !v.is_null()) {
        body["variables"] = v.clone();
    }
    let resp = client
        .post(format!("{}/query", url.trim_end_matches('/')))
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("ADB request to {url} failed: {e}"))?;
    let status = resp.status();
    let body: Value = resp
        .json()
        .await
        .map_err(|e| format!("ADB response from {url} (HTTP {status}) is not JSON: {e}"))?;
    if body.get("success").and_then(|s| s.as_bool()) != Some(true) {
        let err = body.get("error").and_then(|e| e.as_str()).unwrap_or("unknown error");
        return Err(format!("ADB: {err}"));
    }
    Ok(body.get("data").cloned().unwrap_or(Value::Null))
}

/// Flatten a query result into table rows for consoles: records become
/// `{id, memory_type, …data}`; other results (stored, affected, …) are returned as is.
pub fn result_rows(result: &Value) -> Value {
    let data = result.get("data").unwrap_or(result);
    match data.get("records").and_then(|r| r.as_array()) {
        Some(records) => Value::Array(
            records
                .iter()
                .map(|rec| {
                    let mut row = serde_json::Map::new();
                    row.insert("id".into(), rec.get("id").cloned().unwrap_or(Value::Null));
                    row.insert("memory_type".into(), rec.get("memory_type").cloned().unwrap_or(Value::Null));
                    if let Some(fields) = rec.get("data").and_then(|d| d.as_object()) {
                        for (k, v) in fields {
                            row.insert(k.clone(), v.clone());
                        }
                    }
                    Value::Object(row)
                })
                .collect(),
        ),
        None => data.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rows_flatten_records() {
        let r = json!({"success": true, "data": {"type": "records", "count": 1, "records": [
            {"id": "a", "memory_type": "WORKING", "data": {"name": "alice", "score": 95}, "metadata": {}}
        ]}});
        assert_eq!(result_rows(&r), json!([{"id": "a", "memory_type": "WORKING", "name": "alice", "score": 95}]));
    }

    #[test]
    fn rows_pass_through_non_records() {
        let r = json!({"success": true, "data": {"type": "affected", "count": 2}});
        assert_eq!(result_rows(&r), json!({"type": "affected", "count": 2}));
    }
}
