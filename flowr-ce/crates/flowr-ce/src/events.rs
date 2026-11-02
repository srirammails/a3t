//! Events into a project (limitations L29, L37): a CloudEvent sent by `emit`,
//! the `send_event` tool or `POST /events/:namespace` wakes the runs whose
//! `listen` it completes, and starts the flows of the project's event triggers
//! for its type. Both are at most once per event id: the store drops an id it
//! has delivered, and a trigger's run is started with the id as its dedupe key.

use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::{Extension, Json};
use serde_json::{json, Value};
use std::sync::Arc;

use flowr_policy::Principal;
use flowr_storage::StorageBackend;

/// Deliver `event` in `namespace`: `{id, woke, started}`.
pub(crate) async fn publish(storage: &dyn StorageBackend, namespace: &str, event: Value, principal: Option<Principal>) -> Result<Value, String> {
    let engine = storage.engine().ok_or("events need the engine store (FLOWR_STORE=lance has none)")?;
    let woke = engine.deliver_event(namespace, event.clone()).await.map_err(|e| e.to_string())?;
    let workflows = crate::TriggerWorkflowAdapter { principal, blocked: Default::default() };
    let started: Vec<String> = flowr_ce_agents::trigger_manager()
        .deliver_cloud_event(namespace, &event, &crate::TriggerAgentAdapter, &workflows)
        .await
        .into_iter()
        .flat_map(|r| r.delivered_to)
        .collect();
    Ok(json!({ "id": event["id"], "woke": woke, "started": started }))
}

/// Let the `send_event` tool (in `flowr-mcp-core`) deliver to triggers too.
pub(crate) fn install_sink() {
    flowr_mcp_core::runs::set_event_sink(Arc::new(|namespace, event, principal| {
        Box::pin(async move {
            let storage = crate::GLOBAL_STORAGE.get().ok_or("storage not initialised")?.clone();
            publish(storage.as_ref(), &namespace, event, Some(principal)).await
        })
    }));
}

/// `POST /events/:namespace` — the CloudEvents HTTP binding: a structured
/// event (JSON body with `type`, …) or a binary one (`ce-type`, `ce-source`,
/// … headers, the body as `data`). Needs Run access to the project.
pub(crate) async fn post_event(
    State(state): State<Arc<crate::AppState>>,
    Path(namespace): Path<String>,
    principal: Option<Extension<Principal>>,
    headers: HeaderMap,
    body: Option<Json<Value>>,
) -> axum::response::Response {
    let guard = flowr_policy::namespace::namespace_guard();
    let principal = principal.map(|Extension(p)| p);
    if guard.enforcing() {
        let Some(p) = &principal else {
            return (StatusCode::UNAUTHORIZED, Json(json!({ "error": "authentication required" }))).into_response();
        };
        if guard.access(p, &namespace).await < flowr_policy::namespace::NsAccess::Run {
            return (StatusCode::FORBIDDEN, Json(json!({ "error": format!("this needs run access to project '{namespace}'") }))).into_response();
        }
    }
    let body = body.map(|Json(v)| v).unwrap_or(Value::Null);
    let mut given = serde_json::Map::new();
    for (k, v) in headers.iter() {
        if let (Some(attr), Ok(v)) = (k.as_str().strip_prefix("ce-"), v.to_str()) {
            given.insert(attr.to_string(), json!(v));
        }
    }
    let given = if given.contains_key("type") {
        given.insert("data".into(), body);
        Value::Object(given)
    } else {
        body
    };
    if given.get("type").and_then(|t| t.as_str()).is_none_or(str::is_empty) {
        return (StatusCode::BAD_REQUEST, Json(json!({ "error": "a CloudEvent needs a type" }))).into_response();
    }
    let who = principal.as_ref().map(|p| p.id().to_string()).unwrap_or_else(|| "anonymous".into());
    let event = flowr_storage::engine::cloud_event(&given, &format!("/a3t/{who}"));
    let principal = principal.unwrap_or_else(|| Principal::service("trigger"));
    match publish(state.storage.as_ref(), &namespace, event, Some(principal)).await {
        Ok(v) => (StatusCode::ACCEPTED, Json(v)).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": e }))).into_response(),
    }
}
