//! Canvas deploy path (CE, P1) — resolve a flow's effective canvas, pick the
//! runtime it maps to, and report the placement. See `.ai/A3T_CANVAS_SPEC.md`.
//!
//! Runtimes and canvases are ordinary registry resources
//! (`type_ref = "runtime"` / `"canvas"`), so CRUD is the existing registry. This
//! module adds only the *resolution* + *placement* logic:
//! - `GET  /canvas/runtimes`                    — list runtime pools
//! - `GET  /canvas/flows/:ns/:name/placement`   — dry-run: resolved canvas + chosen runtime
//! - `POST /canvas/flows/:ns/:name/deploy`      — CE logical placement (no k8s yet)
//!
//! In CE this is a *logical* placement: it validates the canvas and resolves the
//! target runtime (label mapping), but does not orchestrate containers — that is
//! the EE controller (P3). Placement is Cedar-gated in P2.

use std::sync::Arc;

use axum::{
    extract::{Path, State},
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use serde_json::json;

use flowr_storage::StorageBackend;
use flowr_types::canvas::{select_runtime, CanvasAttachment, CanvasSpec, RuntimeSpec};

pub struct CanvasState {
    pub storage: Arc<dyn StorageBackend>,
}

pub fn canvas_router(storage: Arc<dyn StorageBackend>) -> Router {
    let state = Arc::new(CanvasState { storage });
    Router::new()
        .route("/runtimes", get(list_runtimes))
        .route("/schema", get(canvas_schema))
        .route("/deployments", get(list_deployments))
        .route("/flows/:ns/:name/placement", get(preview_placement))
        .route("/flows/:ns/:name/deploy", post(deploy_flow))
        .route("/canvases/:ns/:name/deploy", post(deploy_canvas_resource))
        .with_state(state)
}

/// Collect all registered runtime pools (`type_ref == "runtime"`) as typed specs.
async fn collect_runtimes(storage: &dyn StorageBackend) -> Vec<RuntimeSpec> {
    let mut out = Vec::new();
    for ns in &["system", "a3t", "default"] {
        if let Ok(resources) = storage.resources().list(ns).await {
            for r in resources {
                if r.type_ref != "runtime" {
                    continue;
                }
                let mut spec: RuntimeSpec = serde_json::from_value(r.config.clone()).unwrap_or_default();
                // The resource identity is authoritative over the config body.
                spec.name = r.name.clone();
                spec.namespace = r.namespace.clone();
                out.push(spec);
            }
        }
    }
    out
}

/// Find a standalone canvas resource (`type_ref == "canvas"`) whose `flowRef`
/// targets `flow_ref`. Scans the usual namespaces; returns the first match.
async fn find_canvas_by_flow_ref(storage: &dyn StorageBackend, flow_ref: &str) -> Option<CanvasSpec> {
    for ns in &["a3t", "default", "system"] {
        if let Ok(resources) = storage.resources().list(ns).await {
            for r in resources {
                if r.type_ref != "canvas" {
                    continue;
                }
                if let Ok(spec) = serde_json::from_value::<CanvasSpec>(r.config.clone()) {
                    if spec.flow_ref == flow_ref {
                        return Some(spec);
                    }
                }
            }
        }
    }
    None
}

/// Resolve a flow's effective canvas: the inline block, the referenced canvas
/// resource, a standalone canvas that targets this flow, or a default canvas.
async fn resolve_canvas(storage: &dyn StorageBackend, ns: &str, name: &str) -> Result<CanvasSpec, String> {
    let wf = storage
        .workflows()
        .get(ns, name)
        .await
        .map_err(|e| format!("flow '{ns}/{name}' not found: {e}"))?;
    let flow_ref = format!("{ns}/{name}");

    let attachment = wf
        .metadata
        .metadata
        .as_ref()
        .and_then(|m| m.get("a3t"))
        .and_then(|a| a.get("canvas"))
        .cloned();

    let mut spec = match attachment {
        // No inline/ref on the flow — the canvas owns the flowRef, so look for a
        // standalone canvas resource whose flowRef targets this flow (the
        // K8s-native direction). Fall back to a default canvas if none exists.
        None => find_canvas_by_flow_ref(storage, &flow_ref).await.unwrap_or_default(),
        Some(v) => {
            let att: CanvasAttachment =
                serde_json::from_value(v).map_err(|e| format!("invalid metadata.a3t.canvas: {e}"))?;
            match att {
                CanvasAttachment::Inline { inline } => *inline,
                CanvasAttachment::Ref { r#ref: canvas_ref } => {
                    let (cns, cname) = canvas_ref
                        .split_once('/')
                        .ok_or_else(|| format!("canvas ref must be 'namespace/name', got '{canvas_ref}'"))?;
                    let res = storage
                        .resources()
                        .get(cns, cname)
                        .await
                        .map_err(|e| format!("canvas resource '{canvas_ref}' not found: {e}"))?;
                    if res.type_ref != "canvas" {
                        return Err(format!("resource '{canvas_ref}' is type '{}', not 'canvas'", res.type_ref));
                    }
                    serde_json::from_value(res.config)
                        .map_err(|e| format!("invalid canvas resource '{canvas_ref}': {e}"))?
                }
            }
        }
    };
    if spec.flow_ref.is_empty() {
        spec.flow_ref = flow_ref;
    }
    Ok(spec)
}

/// Resolve canvas + pick runtime, returning a JSON placement (shared by preview
/// and deploy). `Err` is a ready-to-return HTTP response.
async fn compute_placement(
    storage: &dyn StorageBackend,
    ns: &str,
    name: &str,
) -> Result<serde_json::Value, axum::response::Response> {
    let spec = resolve_canvas(storage, ns, name)
        .await
        .map_err(|e| (axum::http::StatusCode::BAD_REQUEST, Json(json!({ "error": e }))).into_response())?;

    let errs = spec.validate();
    if !errs.is_empty() {
        return Err((
            axum::http::StatusCode::UNPROCESSABLE_ENTITY,
            Json(json!({ "error": "invalid canvas", "problems": errs })),
        )
            .into_response());
    }

    let runtimes = collect_runtimes(storage).await;
    let placed = select_runtime(&spec.placement, &runtimes);
    let effective_ns = if !spec.placement.namespace.is_empty() {
        spec.placement.namespace.clone()
    } else {
        placed.map(|r| r.namespace.clone()).unwrap_or_else(|| ns.to_string())
    };

    Ok(json!({
        "flow": spec.flow_ref,
        "mode": spec.placement.mode,
        "namespace": effective_ns,
        "selector": spec.placement.runtime_selector,
        "placedRuntime": placed.map(|r| r.name.clone()),
        "matched": placed.is_some(),
        "availableRuntimes": runtimes.iter().map(|r| &r.name).collect::<Vec<_>>(),
        "scaling": spec.lifecycle.scaling,
    }))
}

async fn list_runtimes(State(state): State<Arc<CanvasState>>) -> impl IntoResponse {
    let runtimes = collect_runtimes(&*state.storage).await;
    Json(json!({ "count": runtimes.len(), "runtimes": runtimes }))
}

/// The published CanvasSpec JSON Schema — the single source of truth the editor
/// scaffolds the full spec from, and the reference for hand-authoring/verification.
async fn canvas_schema() -> impl IntoResponse {
    Json(flowr_types::canvas::canvas_json_schema())
}

/// Live deployment status from the A3T controller (EE). CE has no controller, so
/// this returns an empty list with `controller: false` — the UI shows canvases as
/// registry-only (undeployed) in that case.
async fn list_deployments(State(_state): State<Arc<CanvasState>>) -> impl IntoResponse {
    let url = match std::env::var("A3T_CONTROLLER_URL").ok().filter(|u| !u.is_empty()) {
        Some(u) => u,
        None => return Json(json!({ "controller": false, "canvases": [] })).into_response(),
    };
    let mut req = reqwest::Client::new().get(format!("{}/v1/canvas", url.trim_end_matches('/')));
    if let Ok(tok) = std::env::var("A3T_CONTROLLER_TOKEN") {
        if !tok.is_empty() {
            req = req.bearer_auth(tok);
        }
    }
    match req.send().await {
        Ok(resp) => match resp.json::<serde_json::Value>().await {
            Ok(v) => Json(json!({ "controller": true, "canvases": v.get("canvases").cloned().unwrap_or(json!([])) })).into_response(),
            Err(e) => (axum::http::StatusCode::BAD_GATEWAY, Json(json!({ "controller": true, "error": e.to_string() }))).into_response(),
        },
        Err(e) => (axum::http::StatusCode::BAD_GATEWAY, Json(json!({ "controller": true, "error": e.to_string() }))).into_response(),
    }
}

async fn preview_placement(
    State(state): State<Arc<CanvasState>>,
    Path((ns, name)): Path<(String, String)>,
) -> impl IntoResponse {
    match compute_placement(&*state.storage, &ns, &name).await {
        Ok(p) => Json(p).into_response(),
        Err(resp) => resp,
    }
}

async fn deploy_flow(
    State(state): State<Arc<CanvasState>>,
    Path((ns, name)): Path<(String, String)>,
) -> impl IntoResponse {
    match compute_placement(&*state.storage, &ns, &name).await {
        Ok(mut p) => {
            let matched = p.get("matched").and_then(|v| v.as_bool()).unwrap_or(false);
            if !matched {
                return (
                    axum::http::StatusCode::UNPROCESSABLE_ENTITY,
                    Json(json!({
                        "deployed": false,
                        "error": "no runtime matches the canvas placement selector",
                        "placement": p,
                    })),
                )
                    .into_response();
            }

            // EE: hand the canvas to the A3T controller, which owns the k8s client.
            // flowr itself stays k8s-free — this is an ordinary outbound HTTP call.
            // CE never sets A3T_CONTROLLER_URL, so it falls through to logical placement.
            let controller_url = std::env::var("A3T_CONTROLLER_URL").ok().filter(|u| !u.is_empty());
            if let Some(url) = controller_url {
                match push_to_controller(&url, &*state.storage, &ns, &name).await {
                    Ok(applied) => {
                        if let Some(obj) = p.as_object_mut() {
                            obj.insert("deployed".into(), json!(true));
                            obj.insert("edition".into(), json!("ee"));
                            obj.insert("controller".into(), applied);
                        }
                        tracing::info!(flow = %format!("{ns}/{name}"), "canvas pushed to controller (EE)");
                        return Json(p).into_response();
                    }
                    Err(e) => {
                        return (
                            axum::http::StatusCode::BAD_GATEWAY,
                            Json(json!({ "deployed": false, "error": format!("controller push failed: {e}"), "placement": p })),
                        )
                            .into_response();
                    }
                }
            }

            // CE (or EE without a controller): logical placement only.
            if let Some(obj) = p.as_object_mut() {
                obj.insert("deployed".into(), json!(true));
                obj.insert("edition".into(), json!("ce"));
                obj.insert(
                    "note".into(),
                    json!("CE logical placement — flow mapped to runtime; EE controller performs container orchestration"),
                );
            }
            tracing::info!(flow = %format!("{ns}/{name}"), "canvas placement resolved (CE)");
            Json(p).into_response()
        }
        Err(resp) => resp,
    }
}

/// Push a canvas spec to the A3T controller's HTTP API (`POST /v1/canvas`) under
/// the CR name `name`. The controller applies the `Canvas` CR → Sandbox; flowr
/// holds no k8s client.
async fn push_spec_to_controller(
    url: &str,
    ns: &str,
    name: &str,
    spec: &serde_json::Value,
) -> Result<serde_json::Value, String> {
    let body = json!({ "namespace": ns, "name": name, "spec": spec });
    let mut req = reqwest::Client::new()
        .post(format!("{}/v1/canvas", url.trim_end_matches('/')))
        .json(&body);
    if let Ok(tok) = std::env::var("A3T_CONTROLLER_TOKEN") {
        if !tok.is_empty() {
            req = req.bearer_auth(tok);
        }
    }
    let resp = req.send().await.map_err(|e| e.to_string())?;
    let status = resp.status();
    let v: serde_json::Value = resp.json().await.unwrap_or_else(|_| json!({}));
    if !status.is_success() {
        return Err(format!("controller returned {status}: {v}"));
    }
    Ok(v)
}

/// Deploy-by-flow: resolve the flow's effective canvas, push under the flow name.
async fn push_to_controller(
    url: &str,
    storage: &dyn StorageBackend,
    ns: &str,
    name: &str,
) -> Result<serde_json::Value, String> {
    let spec = resolve_canvas(storage, ns, name).await?;
    let spec_val = serde_json::to_value(spec).map_err(|e| e.to_string())?;
    push_spec_to_controller(url, ns, name, &spec_val).await
}

/// Deploy a specific **canvas resource** by name (an agent's bound canvas). The
/// Canvas CR is named after the resource, so each agent gets its own CR/Sandbox
/// even when several agents share a target flow. Presets (no flowRef) are refused.
async fn deploy_canvas_resource(
    State(state): State<Arc<CanvasState>>,
    Path((ns, name)): Path<(String, String)>,
) -> impl IntoResponse {
    let controller_url = std::env::var("A3T_CONTROLLER_URL").ok().filter(|u| !u.is_empty());
    let Some(url) = controller_url else {
        return (axum::http::StatusCode::NOT_IMPLEMENTED, Json(json!({ "deployed": false, "error": "controller not configured (EE only)" }))).into_response();
    };
    let res = match state.storage.resources().get(&ns, &name).await {
        Ok(r) => r,
        Err(e) => return (axum::http::StatusCode::NOT_FOUND, Json(json!({ "error": format!("canvas '{ns}/{name}' not found: {e}") }))).into_response(),
    };
    if res.type_ref != "canvas" {
        return (axum::http::StatusCode::BAD_REQUEST, Json(json!({ "error": format!("resource '{ns}/{name}' is type '{}', not 'canvas'", res.type_ref) }))).into_response();
    }
    let spec: CanvasSpec = match serde_json::from_value(res.config.clone()) {
        Ok(s) => s,
        Err(e) => return (axum::http::StatusCode::UNPROCESSABLE_ENTITY, Json(json!({ "error": format!("invalid canvas '{ns}/{name}': {e}") }))).into_response(),
    };
    if spec.is_preset() {
        return (axum::http::StatusCode::UNPROCESSABLE_ENTITY, Json(json!({ "deployed": false, "error": "canvas is a preset (no flowRef) — clone + bind it to an agent before deploying" }))).into_response();
    }
    let errs = spec.validate();
    if !errs.is_empty() {
        return (axum::http::StatusCode::UNPROCESSABLE_ENTITY, Json(json!({ "deployed": false, "error": "invalid canvas", "problems": errs }))).into_response();
    }
    match push_spec_to_controller(&url, &ns, &name, &res.config).await {
        Ok(applied) => {
            tracing::info!(canvas = %format!("{ns}/{name}"), flow = %spec.flow_ref, "canvas resource deployed to controller (EE)");
            Json(json!({ "deployed": true, "edition": "ee", "canvas": format!("{ns}/{name}"), "flowRef": spec.flow_ref, "agentRef": spec.agent_ref, "controller": applied })).into_response()
        }
        Err(e) => (axum::http::StatusCode::BAD_GATEWAY, Json(json!({ "deployed": false, "error": format!("controller push failed: {e}") }))).into_response(),
    }
}
