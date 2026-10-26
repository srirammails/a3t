//! call:adb handler — runs AQL against ADB (Agent Database) agent memory
//!
//! In CNCF syntax: `call: adb` with `with:` containing `query` (AQL), optional
//! `variables`, and the ADB server from `_resource` (an `adb` resource: `config.url`,
//! optional `config.flight_url`) or `url`. `call:adb` talks to ADB over **Arrow
//! Flight** (ADB stage G): AQL is `do_action("aql")` with `POST /query`'s JSON in and
//! out; this module is also the Flight client for ADB platform tables. ADB over MCP
//! stays JSON over HTTP. `call:clawdb` is an alias (the resource type ADB replaces).

use async_trait::async_trait;
use serde_json::{json, Value};
use tracing::info;

use swf_parser::{NamedTask, Task};
use swf_runtime::context::ExecutionContext;
use swf_runtime::error::{RuntimeError, Result};
use swf_runtime::traits::{TaskHandler, TaskResult};

pub struct AdbHandler {
    task_type: &'static str,
}

impl AdbHandler {
    pub fn new(task_type: &'static str) -> Self {
        Self { task_type }
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
        let ep = if let Some(u) = with.get("url").and_then(|v| v.as_str()) {
            adb_endpoint(u, with.get("flight_url").and_then(|v| v.as_str()))
        } else if let Some(r) = with.get("_resource").and_then(|v| v.as_str()) {
            let storage = ctx
                .storage
                .as_ref()
                .and_then(|s| s.downcast_ref::<std::sync::Arc<dyn flowr_storage::StorageBackend>>())
                .ok_or_else(|| RuntimeError::task_failed(&task.name, "no storage to resolve the ADB resource"))?;
            resolve_adb(storage.as_ref(), r)
                .await
                .map_err(|e| RuntimeError::task_failed(&task.name, &e))?
        } else {
            return Err(RuntimeError::task_failed(&task.name, "call:adb requires '_resource' (an adb resource) or 'url'"));
        };

        info!(task = %task.name, flight = %ep.flight, "calling ADB");
        let result = adb_query(&ep, &query, with.get("variables"))
            .await
            .map_err(|e| RuntimeError::Communication(e))?;
        Ok(TaskResult::new(result))
    }
}

/// Where an ADB server listens: HTTP (`/query`, MCP) and Arrow Flight, FlowR's native
/// wire for `call:adb` (ADB stage G).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdbEndpoint {
    pub http: String,
    pub flight: String,
}

/// ADB's default Flight port (`adb serve-http --flight-port`).
pub const ADB_FLIGHT_PORT: u16 = 3001;

/// The endpoint of an ADB at `url`: Flight at `flight_url`, else the same host on
/// ADB's default Flight port.
pub fn adb_endpoint(url: &str, flight_url: Option<&str>) -> AdbEndpoint {
    let http = url.trim_end_matches('/').to_string();
    let flight = match flight_url {
        Some(f) => f.trim_end_matches('/').to_string(),
        None => match reqwest::Url::parse(&http) {
            Ok(mut u) => {
                let _ = u.set_port(Some(ADB_FLIGHT_PORT));
                u.set_path("");
                u.to_string().trim_end_matches('/').to_string()
            }
            Err(_) => http.clone(),
        },
    };
    AdbEndpoint { http, flight }
}

/// The endpoint of an ADB resource (`namespace/name`, or `name` in `default`):
/// `config.url` (or `endpoint`) and optional `config.flight_url`.
pub async fn resolve_adb(storage: &dyn flowr_storage::StorageBackend, resource: &str) -> std::result::Result<AdbEndpoint, String> {
    let (ns, name) = resource.split_once('/').unwrap_or(("default", resource));
    let r = storage
        .resources()
        .get(ns, name)
        .await
        .map_err(|e| format!("ADB resource {ns}/{name} not found: {e}"))?;
    let url = r
        .config
        .get("url")
        .or_else(|| r.config.get("endpoint"))
        .and_then(|v| v.as_str())
        .ok_or_else(|| format!("ADB resource {ns}/{name} has no config.url"))?;
    Ok(adb_endpoint(url, r.config.get("flight_url").and_then(|v| v.as_str())))
}

/// A Flight client for `flight` (channels are cached and reconnect lazily).
pub fn flight_client(flight: &str) -> std::result::Result<arrow_flight::flight_service_client::FlightServiceClient<tonic::transport::Channel>, String> {
    use std::collections::HashMap;
    use std::sync::{Mutex, OnceLock};
    static CHANNELS: OnceLock<Mutex<HashMap<String, tonic::transport::Channel>>> = OnceLock::new();
    let mut cache = CHANNELS.get_or_init(Default::default).lock().unwrap();
    let channel = match cache.get(flight) {
        Some(c) => c.clone(),
        None => {
            let c = tonic::transport::Channel::from_shared(flight.to_string())
                .map_err(|e| format!("bad ADB Flight URL {flight}: {e}"))?
                .connect_timeout(std::time::Duration::from_secs(5))
                .connect_lazy();
            cache.insert(flight.to_string(), c.clone());
            c
        }
    };
    Ok(arrow_flight::flight_service_client::FlightServiceClient::new(channel)
        .max_decoding_message_size(64 << 20)
        .max_encoding_message_size(64 << 20))
}

/// A Flight `do_action` with a JSON body; the answer's JSON.
pub async fn flight_action(ep: &AdbEndpoint, kind: &str, body: &Value) -> std::result::Result<Value, String> {
    use futures::StreamExt;
    let mut client = flight_client(&ep.flight)?;
    let action = arrow_flight::Action { r#type: kind.to_string(), body: body.to_string().into() };
    let mut stream = client
        .do_action(action)
        .await
        .map_err(|e| flight_err(ep, e))?
        .into_inner();
    let r = stream
        .next()
        .await
        .ok_or_else(|| format!("ADB {kind}: no answer"))?
        .map_err(|e| flight_err(ep, e))?;
    serde_json::from_slice(&r.body).map_err(|e| format!("ADB {kind}: answer is not JSON: {e}"))
}

fn flight_err(ep: &AdbEndpoint, e: tonic::Status) -> String {
    match e.code() {
        tonic::Code::Unavailable | tonic::Code::Unknown | tonic::Code::DeadlineExceeded => {
            format!("ADB request to {} failed: {}", ep.flight, e.message())
        }
        _ => format!("ADB: {}", e.message()),
    }
}

/// Rows of an ADB platform table (`do_get`).
pub async fn flight_get(ep: &AdbEndpoint, table: &str, filter: Option<&str>, limit: usize) -> std::result::Result<Vec<arrow_array::RecordBatch>, String> {
    use futures::TryStreamExt;
    let mut client = flight_client(&ep.flight)?;
    let ticket = json!({ "table": table, "filter": filter, "limit": limit });
    let stream = client
        .do_get(arrow_flight::Ticket { ticket: ticket.to_string().into() })
        .await
        .map_err(|e| flight_err(ep, e))?
        .into_inner()
        .map_err(|e| arrow_flight::error::FlightError::Tonic(Box::new(e)));
    arrow_flight::decode::FlightRecordBatchStream::new_from_flight_data(stream)
        .try_collect()
        .await
        .map_err(|e| format!("ADB query {table}: {e}"))
}

/// Insert a batch into an ADB platform table (`do_put`) — or, with `replace`, make
/// it the whole table (columns included); the answer's metadata.
pub async fn flight_put(ep: &AdbEndpoint, table: &str, batch: arrow_array::RecordBatch, replace: bool) -> std::result::Result<Value, String> {
    use futures::StreamExt;
    let mut client = flight_client(&ep.flight)?;
    let mut path = vec![table.to_string()];
    if replace {
        path.push("replace".into());
    }
    let data = arrow_flight::encode::FlightDataEncoderBuilder::new()
        .with_flight_descriptor(Some(arrow_flight::FlightDescriptor::new_path(path)))
        .build(futures::stream::iter(vec![Ok(batch)]))
        .filter_map(|r| async move { r.ok() });
    let mut results = client.do_put(data).await.map_err(|e| flight_err(ep, e))?.into_inner();
    let r = results
        .next()
        .await
        .ok_or_else(|| format!("ADB insert {table}: no answer"))?
        .map_err(|e| flight_err(ep, e))?;
    serde_json::from_slice(&r.app_metadata).map_err(|e| format!("ADB insert {table}: {e}"))
}

/// Run one AQL statement on ADB over Flight (`do_action("aql")`); returns the query
/// result (`{success, data: {type, …}, metadata}`), as `POST /query` did.
pub async fn adb_query(ep: &AdbEndpoint, query: &str, variables: Option<&Value>) -> std::result::Result<Value, String> {
    let mut body = json!({ "query": query });
    if let Some(v) = variables.filter(|v| !v.is_null()) {
        body["variables"] = v.clone();
    }
    let answer = flight_action(ep, "aql", &body).await?;
    if answer.get("success").and_then(|s| s.as_bool()) != Some(true) {
        let err = answer.get("error").and_then(|e| e.as_str()).unwrap_or("unknown error");
        return Err(format!("ADB: {err}"));
    }
    Ok(answer.get("data").cloned().unwrap_or(Value::Null))
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
    fn flight_endpoint_defaults_to_the_same_host() {
        assert_eq!(adb_endpoint("http://adb.a3t.svc.cluster.local:3000", None).flight, "http://adb.a3t.svc.cluster.local:3001");
        assert_eq!(adb_endpoint("http://adb:3000/", None).http, "http://adb:3000");
        assert_eq!(adb_endpoint("http://adb", None).flight, "http://adb:3001");
        assert_eq!(adb_endpoint("http://x:1", Some("http://y:9/")).flight, "http://y:9");
    }

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
