//! `adb_topic` triggers (ADB stage F, F5): run a trigger's targets once per message
//! published to an ADB topic.
//!
//! A supervisor looks at the triggers every few seconds and keeps one reader task
//! per enabled `adb_topic` trigger. A reader subscribes on first run (the
//! subscription id is saved in the trigger, so a restart resumes where it acked),
//! long-polls ADB's `next_events`, delivers each message to the targets, and acks the
//! batch once the targets were started — at-least-once: a crash between delivery and
//! ack delivers those messages again. Every FlowR instance with the trigger holds its
//! own subscription, so each instance sees every message, in the same order.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use flowr_ce_agents::triggers::types::{Trigger, TriggerConfig};
use flowr_storage::StorageBackend;
use serde_json::{json, Value};
use tokio::task::JoinHandle;
use tracing::{info, warn};

use crate::handlers::call_adb::{adb_query, resolve_adb, AdbEndpoint};

/// How often the supervisor reconciles reader tasks with the triggers.
const RECONCILE: Duration = Duration::from_secs(3);
/// `next_events` long-poll (ADB caps it at 30 s).
const WAIT_SECONDS: u64 = 20;
const RETRY: Duration = Duration::from_secs(2);

/// Start the supervisor (once, at startup, after triggers are restored).
pub fn spawn(storage: Arc<dyn StorageBackend>) -> JoinHandle<()> {
    tokio::spawn(async move {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(WAIT_SECONDS + 15))
            .build()
            .unwrap_or_default();
        // trigger id → (reader task, the config it was started with)
        let mut readers: HashMap<String, (JoinHandle<()>, String)> = HashMap::new();
        loop {
            let wanted: HashMap<String, Trigger> = flowr_ce_agents::trigger_manager()
                .list()
                .await
                .into_iter()
                .filter(|t| t.enabled && matches!(t.config, TriggerConfig::AdbTopic { .. }))
                .map(|t| (t.id.clone(), t))
                .collect();
            // Stop readers whose trigger went away, was paused or changed.
            readers.retain(|id, (task, key)| {
                let keep = !task.is_finished() && wanted.get(id).map(reader_key).as_deref() == Some(key.as_str());
                if !keep {
                    task.abort();
                }
                keep
            });
            for (id, t) in wanted {
                if !readers.contains_key(&id) {
                    let key = reader_key(&t);
                    let task = tokio::spawn(read_topic(Arc::clone(&storage), client.clone(), id.clone()));
                    readers.insert(id, (task, key));
                }
            }
            tokio::time::sleep(RECONCILE).await;
        }
    })
}

/// What a reader depends on (resource, topic, filter); not the subscription id,
/// which the reader sets itself.
fn reader_key(t: &Trigger) -> String {
    match &t.config {
        TriggerConfig::AdbTopic { resource, topic, filter, .. } => json!([resource, topic, filter]).to_string(),
        _ => String::new(),
    }
}

/// A double-quoted AQL string literal.
fn aql_str(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
}

/// An AQL literal for a filter value.
fn literal(v: &Value) -> String {
    match v {
        Value::String(s) => aql_str(s),
        Value::Null => "null".into(),
        other => other.to_string(),
    }
}

fn subscribe_aql(topic: &str, filter: &Option<Value>) -> String {
    let mut q = format!("SUBSCRIBE TOPIC {}", aql_str(topic));
    let conds: Vec<String> = filter
        .as_ref()
        .and_then(|f| f.as_object())
        .map(|m| m.iter().map(|(k, v)| format!("{k} = {}", literal(v))).collect())
        .unwrap_or_default();
    if !conds.is_empty() {
        q.push_str(&format!(" WHERE {}", conds.join(" AND ")));
    }
    q
}

/// Call one of ADB's MCP tools (JSON over HTTP) and parse its JSON text result.
async fn mcp_tool(client: &reqwest::Client, ep: &AdbEndpoint, tool: &str, args: Value) -> Result<Value, String> {
    let body = json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": { "name": tool, "arguments": args } });
    let resp: Value = client
        .post(format!("{}/mcp", ep.http))
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("ADB {tool} request failed: {e}"))?
        .json()
        .await
        .map_err(|e| format!("ADB {tool} response: {e}"))?;
    let text = resp.pointer("/result/content/0/text").and_then(|t| t.as_str()).unwrap_or_default();
    if resp.pointer("/result/isError").and_then(|v| v.as_bool()) == Some(true) || resp.get("error").is_some() {
        return Err(if text.is_empty() { resp.to_string() } else { text.to_string() });
    }
    serde_json::from_str(text).map_err(|e| format!("ADB {tool}: {e}: {text}"))
}

/// ADB's `next_events` MCP tool: `{deliveries, next, tip}`.
async fn next_events(client: &reqwest::Client, ep: &AdbEndpoint, sub: &str, from: Option<u64>) -> Result<Value, String> {
    let mut args = json!({ "subscription": sub, "max": 50, "wait_seconds": WAIT_SECONDS });
    if let Some(f) = from {
        args["from"] = json!(f);
    }
    mcp_tool(client, ep, "next_events", args).await
}

/// Publish `fields` to `topic` on the ADB resource `resource` (MCP `publish`).
pub async fn publish(storage: &dyn StorageBackend, resource: &str, topic: &str, fields: Value) -> Result<Value, String> {
    let ep = resolve_adb(storage, resource).await?;
    mcp_tool(&reqwest::Client::new(), &ep, "publish", json!({ "topic": topic, "fields": fields })).await
}

/// A finished run as the reply a deployed canvas publishes to `_reply_topic`
/// (ADB MVP W2b): the caller's `request_id`, the run's status, its output (as
/// `output_json`) and error.
pub fn reply_fields(input: &Value, instance_id: &str, result: Result<&flowr_types::ExecutionState, &str>) -> Value {
    let mut r = json!({ "instance_id": instance_id });
    if let Some(id) = input.get("request_id") {
        r["request_id"] = id.clone();
    }
    if let Some(c) = std::env::var("A3T_CANVAS").ok().filter(|c| !c.is_empty()) {
        r["canvas"] = json!(c);
    }
    match result {
        Ok(state) => {
            r["status"] = serde_json::to_value(&state.status).unwrap_or(Value::Null);
            // As JSON text: ADB topic messages hold scalars (AQL literals).
            r["output_json"] = json!(state.output.as_ref().map(|o| o.to_string()).unwrap_or_else(|| "null".into()));
            if let Some(e) = &state.error {
                r["error"] = json!(e);
            }
        }
        Err(e) => {
            r["status"] = json!("failed");
            r["error"] = json!(e);
        }
    }
    r
}

/// One trigger's reader: subscribe if needed, then deliver and ack, forever.
async fn read_topic(storage: Arc<dyn StorageBackend>, client: reqwest::Client, trigger_id: String) {
    let agents = crate::TriggerAgentAdapter;
    let mut from: Option<u64> = None;
    loop {
        let Some(t) = flowr_ce_agents::trigger_manager().get(&trigger_id).await else { return };
        let TriggerConfig::AdbTopic { resource, topic, filter, subscription, .. } = t.config else { return };
        let ep = match resolve_adb(storage.as_ref(), &resource).await {
            Ok(e) => e,
            Err(e) => {
                warn!(trigger = %trigger_id, error = %e, "adb_topic: resource unavailable; retrying");
                tokio::time::sleep(RETRY * 5).await;
                continue;
            }
        };
        let sub = match subscription {
            Some(s) => s,
            None => match adb_query(&ep, &subscribe_aql(&topic, &filter), None).await {
                Ok(r) => {
                    let Some(id) = r.pointer("/data/subscription").and_then(|v| v.as_str()).map(String::from) else {
                        warn!(trigger = %trigger_id, result = %r, "adb_topic: SUBSCRIBE returned no id");
                        tokio::time::sleep(RETRY).await;
                        continue;
                    };
                    flowr_ce_agents::trigger_manager().set_topic_subscription(&trigger_id, Some(id.clone())).await;
                    crate::persist_triggers().await;
                    info!(trigger = %trigger_id, topic = %topic, subscription = %id, "adb_topic: subscribed");
                    from = None;
                    id
                }
                Err(e) => {
                    warn!(trigger = %trigger_id, error = %e, "adb_topic: subscribe failed; retrying");
                    tokio::time::sleep(RETRY).await;
                    continue;
                }
            },
        };
        let batch = match next_events(&client, &ep, &sub, from).await {
            Ok(b) => b,
            Err(e) => {
                // A lost subscription (ADB reset) or an expired cursor: subscribe again
                // from now; anything else: retry.
                if e.contains("unknown subscription") || e.contains("cursor_expired") {
                    warn!(trigger = %trigger_id, error = %e, "adb_topic: subscription lost; subscribing again from now");
                    flowr_ce_agents::trigger_manager().set_topic_subscription(&trigger_id, None).await;
                    crate::persist_triggers().await;
                } else {
                    warn!(trigger = %trigger_id, error = %e, "adb_topic: read failed; retrying");
                    tokio::time::sleep(RETRY).await;
                }
                continue;
            }
        };
        let deliveries = batch.get("deliveries").and_then(|d| d.as_array()).cloned().unwrap_or_default();
        let mut last = None;
        for d in &deliveries {
            let offset = d.get("offset").and_then(|o| o.as_u64()).unwrap_or(0);
            for change in d.get("changes").and_then(|c| c.as_array()).into_iter().flatten() {
                if change.get("op").and_then(|o| o.as_str()) != Some("publish") {
                    continue;
                }
                let mut message = change.get("value").cloned().unwrap_or(Value::Null);
                // A request that wants its result back: the run publishes it to
                // `_reply_topic` on this same ADB.
                if let Some(m) = message.as_object_mut().filter(|m| m.contains_key("_reply_topic")) {
                    m.insert("_adb_resource".into(), json!(resource));
                }
                // Trigger runs have no caller: targets run as the runtime service.
                let flows = crate::TriggerWorkflowAdapter { principal: None, blocked: Default::default() };
                let result = flowr_ce_agents::trigger_manager()
                    .deliver_topic_message(&trigger_id, offset, message, &agents, &flows)
                    .await;
                if result.is_none() {
                    return; // paused or deleted meanwhile; the supervisor restarts us if needed
                }
            }
            last = Some(offset);
        }
        if let Some(offset) = last {
            let ack = format!("ACK {} OFFSET {offset}", aql_str(&sub));
            if let Err(e) = adb_query(&ep, &ack, None).await {
                warn!(trigger = %trigger_id, error = %e, "adb_topic: ack failed; those messages may be delivered again");
            }
        }
        from = batch.get("next").and_then(|n| n.as_u64()).or(from);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subscribe_statement_quotes_and_filters() {
        assert_eq!(subscribe_aql("orders", &None), r#"SUBSCRIBE TOPIC "orders""#);
        let f = Some(json!({ "status": "paid", "n": 2 }));
        let q = subscribe_aql(r#"a"b"#, &f);
        assert!(q.starts_with(r#"SUBSCRIBE TOPIC "a\"b" WHERE "#), "{q}");
        assert!(q.contains(r#"status = "paid""#) && q.contains("n = 2"), "{q}");
    }
}
