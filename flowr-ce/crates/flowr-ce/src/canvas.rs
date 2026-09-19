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
use flowr_types::canvas::{
    apply_patch, bind_to_flow, select_runtime, CanvasAttachment, CanvasSpec, RuntimeSpec,
};

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
        // Presets: the deployment shapes a user picks from, reviews, and either
        // reuses or clones. A preset deploys nothing until it is bound.
        .route("/presets", get(list_presets))
        .route("/presets/:ns/:name/clone", post(clone_preset))
        .route("/canvases/:ns/:name/bind", post(bind_canvas))
        .with_state(state)
}

/// Authorize placing a flow into a runtime pool.
///
/// Deployment had NO gate at all: `deploy_flow` took no principal and never
/// reached the policy hook, so anyone who could call the endpoint could place any
/// flow into any pool, production included. This is that choke point.
///
/// The resource is the TARGET, not the flow — the decision is which pool and
/// namespace work may be placed into, so the pool's labels ride as attributes and
/// a policy can say "prod tier needs Admin" without naming flows.
///
/// `Ask` is honoured the same way it is everywhere else: an agent acting for
/// someone who WOULD be allowed produces a question rather than a refusal, which
/// is what lets a model propose a production deploy and a person confirm it. A
/// request with no interactive approver degrades to a denial via
/// `non_interactive()`.
async fn authorize_deploy(
    principal: &flowr_policy::Principal,
    runtime: Option<&RuntimeSpec>,
    namespace: &str,
    mode: &str,
) -> Result<(), axum::response::Response> {
    // An unmatched selector has no pool to authorize against. Deploying nowhere
    // is not a privilege, and the caller already reports `matched: false`, so let
    // it through to that clearer error rather than denying on a missing resource.
    let Some(rt) = runtime else { return Ok(()) };

    let resource = flowr_policy::Resource::runtime(
        rt.name.clone(),
        namespace.to_string(),
        mode.to_string(),
        rt.labels.iter().map(|(k, v)| (k.clone(), v.clone())),
    );
    let req = flowr_policy::ExecutionRequest {
        principal: principal.clone(),
        action: flowr_policy::actions::flow_deploy(),
        resource,
        context: Default::default(),
        attributes: Default::default(),
        params: serde_json::Value::Null,
    };
    let decision = flowr_policy::policy_hook().check(&req).await;
    flowr_policy::audit::record(&req, &decision);

    match decision.non_interactive() {
        flowr_policy::PolicyDecision::Allow => Ok(()),
        flowr_policy::PolicyDecision::Deny { reason, policy_ids } => Err((
            axum::http::StatusCode::FORBIDDEN,
            Json(json!({
                "deployed": false,
                "error": "forbidden",
                "reason": reason,
                "policies": policy_ids,
                "runtime": rt.name,
                "namespace": namespace,
                "mode": mode,
            })),
        )
            .into_response()),
        flowr_policy::PolicyDecision::Ask { .. } => unreachable!(
            "non_interactive() collapses Ask into Deny"
        ),
    }
}

/// Read every stored canvas as a typed spec, paired with its registry identity.
async fn stored_canvases(storage: &dyn StorageBackend) -> Vec<(String, String, CanvasSpec)> {
    let mut out = Vec::new();
    for ns in ["a3t", "default", "system"] {
        let Ok(list) = storage.resources().list(ns).await else { continue };
        for r in list.into_iter().filter(|r| r.type_ref == "canvas") {
            if let Ok(spec) = serde_json::from_value::<CanvasSpec>(r.config.clone()) {
                out.push((r.namespace.clone(), r.name.clone(), spec));
            }
        }
    }
    out
}

/// A preset rendered for review: enough to DECIDE on, not the whole spec.
///
/// "prod-ha" says nothing about whether it is isolated or how far it scales, so
/// the summary carries exactly the facts someone weighs when choosing — mode,
/// where it lands, how far it scales, what happens on failure, and whether state
/// survives between triggers.
fn preset_summary(ns: &str, name: &str, spec: &CanvasSpec) -> serde_json::Value {
    json!({
        "namespace": ns,
        "name": name,
        "description": spec.description,
        "mode": spec.placement.mode,
        "targetNamespace": spec.placement.namespace,
        "selector": spec.placement.runtime_selector,
        "tolerations": spec.placement.tolerations,
        "scaling": spec.lifecycle.scaling,
        "restart": spec.lifecycle.restart,
        "session": spec.lifecycle.session,
        "dependsOn": spec.health.depends_on,
    })
}

/// `GET /canvas/presets` — the deployment shapes available to choose from.
///
/// A preset is a canvas with no `flowRef`. Bound canvases are excluded: they
/// belong to a flow already and are not something to pick from a menu.
async fn list_presets(State(state): State<Arc<CanvasState>>) -> impl IntoResponse {
    let presets: Vec<serde_json::Value> = stored_canvases(&*state.storage)
        .await
        .into_iter()
        .filter(|(_, _, spec)| spec.is_preset())
        .map(|(ns, name, spec)| preset_summary(&ns, &name, &spec))
        .collect();
    Json(json!({ "count": presets.len(), "presets": presets }))
}

/// `POST /canvas/presets/:ns/:name/clone` — copy a preset, with changes.
///
/// Body: `{ "name": "...", "namespace": "...", "patch": { … } }`.
///
/// The source preset is read and never written: cloning is how one team adapts a
/// shared shape without altering anyone else's. The patch is validated through
/// `apply_patch`, so a clone that would not deploy is refused here rather than
/// discovered later.
async fn clone_preset(
    State(state): State<Arc<CanvasState>>,
    Path((ns, name)): Path<(String, String)>,
    Json(body): Json<serde_json::Value>,
) -> impl IntoResponse {
    let Some(new_name) = body.get("name").and_then(|v| v.as_str()) else {
        return (axum::http::StatusCode::BAD_REQUEST,
            Json(json!({ "error": "name is required for the clone" }))).into_response();
    };
    let new_ns = body.get("namespace").and_then(|v| v.as_str()).unwrap_or(&ns).to_string();

    let src = match state.storage.resources().get(&ns, &name).await {
        Ok(r) if r.type_ref == "canvas" => r,
        Ok(r) => return (axum::http::StatusCode::BAD_REQUEST,
            Json(json!({ "error": format!("'{ns}/{name}' is type '{}', not 'canvas'", r.type_ref) }))).into_response(),
        Err(e) => return (axum::http::StatusCode::NOT_FOUND,
            Json(json!({ "error": format!("preset '{ns}/{name}' not found: {e}") }))).into_response(),
    };
    let base: CanvasSpec = match serde_json::from_value(src.config.clone()) {
        Ok(c) => c,
        Err(e) => return (axum::http::StatusCode::UNPROCESSABLE_ENTITY,
            Json(json!({ "error": format!("stored canvas is not a valid spec: {e}") }))).into_response(),
    };

    let patch = body.get("patch").cloned().unwrap_or_else(|| json!({}));
    let cloned = match apply_patch(&base, &patch) {
        Ok(c) => c,
        Err(problems) => return (axum::http::StatusCode::UNPROCESSABLE_ENTITY,
            Json(json!({ "error": "patch produced an invalid canvas", "problems": problems }))).into_response(),
    };

    let resource = flowr_types::Resource::new(
        new_name.to_string(),
        "canvas",
        serde_json::to_value(&cloned).unwrap_or(json!({})),
    )
    .with_namespace(new_ns.clone())
    .with_description(cloned.description.clone());
    match state.storage.resources().create(resource).await {
        Ok(_) => Json(json!({
            "cloned": true,
            "from": format!("{ns}/{name}"),
            "canvas": format!("{new_ns}/{new_name}"),
            "spec": cloned,
            "note": "Not deployed. Bind it to a flow, then deploy.",
        }))
        .into_response(),
        Err(e) => (axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": format!("failed to store clone: {e}") }))).into_response(),
    }
}

/// `POST /canvas/canvases/:ns/:name/bind` — attach a canvas to a flow.
///
/// Body: `{ "flow": "namespace/name" }`.
///
/// This is the step that makes a canvas deployable, so the reference has to be
/// well-formed. Binding still places nothing — deploy does.
async fn bind_canvas(
    State(state): State<Arc<CanvasState>>,
    Path((ns, name)): Path<(String, String)>,
    Json(body): Json<serde_json::Value>,
) -> impl IntoResponse {
    let Some(flow) = body.get("flow").and_then(|v| v.as_str()) else {
        return (axum::http::StatusCode::BAD_REQUEST,
            Json(json!({ "error": "flow is required, as 'namespace/name'" }))).into_response();
    };
    let res = match state.storage.resources().get(&ns, &name).await {
        Ok(r) if r.type_ref == "canvas" => r,
        Ok(r) => return (axum::http::StatusCode::BAD_REQUEST,
            Json(json!({ "error": format!("'{ns}/{name}' is type '{}', not 'canvas'", r.type_ref) }))).into_response(),
        Err(e) => return (axum::http::StatusCode::NOT_FOUND,
            Json(json!({ "error": format!("canvas '{ns}/{name}' not found: {e}") }))).into_response(),
    };
    let spec: CanvasSpec = match serde_json::from_value(res.config.clone()) {
        Ok(c) => c,
        Err(e) => return (axum::http::StatusCode::UNPROCESSABLE_ENTITY,
            Json(json!({ "error": format!("stored canvas is not a valid spec: {e}") }))).into_response(),
    };
    let bound = match bind_to_flow(&spec, flow) {
        Ok(b) => b,
        Err(problems) => return (axum::http::StatusCode::UNPROCESSABLE_ENTITY,
            Json(json!({ "error": "cannot bind", "problems": problems }))).into_response(),
    };
    let cfg = serde_json::to_value(&bound).unwrap_or(json!({}));
    match state.storage.resources().update(&ns, &name, cfg).await {
        Ok(_) => Json(json!({
            "bound": true,
            "canvas": format!("{ns}/{name}"),
            "flow": flow,
            "note": "Not deployed. Preview the placement, then deploy.",
        }))
        .into_response(),
        Err(e) => (axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": format!("failed to bind: {e}") }))).into_response(),
    }
}

// ── Tool-facing operations ──
//
// Plain functions over storage, called by both the HTTP routes and the MCP
// tools. Kept here beside the handlers so the two cannot answer differently:
// a preview shown in chat must be the preview the deploy then acts on.

/// Every preset available to choose from, as review summaries.
pub(crate) async fn op_list_presets(storage: &dyn StorageBackend) -> serde_json::Value {
    let presets: Vec<serde_json::Value> = stored_canvases(storage)
        .await
        .into_iter()
        .filter(|(_, _, spec)| spec.is_preset())
        .map(|(ns, name, spec)| preset_summary(&ns, &name, &spec))
        .collect();
    json!({ "count": presets.len(), "presets": presets })
}

/// Every runtime pool, with the labels placement selects on.
pub(crate) async fn op_list_runtimes(storage: &dyn StorageBackend) -> serde_json::Value {
    let runtimes = collect_runtimes(storage).await;
    json!({ "count": runtimes.len(), "runtimes": runtimes })
}

/// One canvas, preset or bound, as stored.
pub(crate) async fn op_get_canvas(
    storage: &dyn StorageBackend,
    ns: &str,
    name: &str,
) -> Result<serde_json::Value, String> {
    let res = storage
        .resources()
        .get(ns, name)
        .await
        .map_err(|e| format!("canvas '{ns}/{name}' not found: {e}"))?;
    if res.type_ref != "canvas" {
        return Err(format!("'{ns}/{name}' is type '{}', not 'canvas'", res.type_ref));
    }
    let spec: CanvasSpec = serde_json::from_value(res.config.clone())
        .map_err(|e| format!("stored canvas is not a valid spec: {e}"))?;
    Ok(json!({
        "namespace": ns,
        "name": name,
        "isPreset": spec.is_preset(),
        "summary": preset_summary(ns, name, &spec),
        "spec": spec,
    }))
}

/// Copy a preset, optionally with changes. Never mutates the source.
pub(crate) async fn op_clone_preset(
    storage: &dyn StorageBackend,
    ns: &str,
    name: &str,
    new_ns: &str,
    new_name: &str,
    patch: &serde_json::Value,
) -> Result<serde_json::Value, String> {
    let src = storage
        .resources()
        .get(ns, name)
        .await
        .map_err(|e| format!("preset '{ns}/{name}' not found: {e}"))?;
    if src.type_ref != "canvas" {
        return Err(format!("'{ns}/{name}' is type '{}', not 'canvas'", src.type_ref));
    }
    let base: CanvasSpec = serde_json::from_value(src.config.clone())
        .map_err(|e| format!("stored canvas is not a valid spec: {e}"))?;
    let cloned = apply_patch(&base, patch)
        .map_err(|problems| format!("patch produced an invalid canvas: {}", problems.join("; ")))?;

    let resource = flowr_types::Resource::new(
        new_name.to_string(),
        "canvas",
        serde_json::to_value(&cloned).unwrap_or(json!({})),
    )
    .with_namespace(new_ns.to_string())
    .with_description(cloned.description.clone());
    storage
        .resources()
        .create(resource)
        .await
        .map_err(|e| format!("failed to store clone: {e}"))?;
    Ok(json!({
        "cloned": true,
        "from": format!("{ns}/{name}"),
        "canvas": format!("{new_ns}/{new_name}"),
        "summary": preset_summary(new_ns, new_name, &cloned),
        "note": "Nothing is deployed yet. Bind it to a flow, then deploy.",
    }))
}

/// Attach a canvas to a flow — the step that makes it deployable.
pub(crate) async fn op_bind_canvas(
    storage: &dyn StorageBackend,
    ns: &str,
    name: &str,
    flow: &str,
) -> Result<serde_json::Value, String> {
    let res = storage
        .resources()
        .get(ns, name)
        .await
        .map_err(|e| format!("canvas '{ns}/{name}' not found: {e}"))?;
    if res.type_ref != "canvas" {
        return Err(format!("'{ns}/{name}' is type '{}', not 'canvas'", res.type_ref));
    }
    let spec: CanvasSpec = serde_json::from_value(res.config.clone())
        .map_err(|e| format!("stored canvas is not a valid spec: {e}"))?;
    let bound = bind_to_flow(&spec, flow)
        .map_err(|problems| format!("cannot bind: {}", problems.join("; ")))?;
    let cfg = serde_json::to_value(&bound).unwrap_or(json!({}));
    storage
        .resources()
        .update(ns, name, cfg)
        .await
        .map_err(|e| format!("failed to bind: {e}"))?;
    Ok(json!({
        "bound": true,
        "canvas": format!("{ns}/{name}"),
        "flow": flow,
        "note": "Nothing is deployed yet. Preview the placement, then deploy.",
    }))
}

/// Deploy a flow into the runtime its canvas selects.
///
/// Carries NO authorization of its own: the caller gates first
/// (`gate_placement` for HTTP, the `flow:deploy` check in the agent loop for the
/// tool path), because only the caller knows whether an approval can be put to a
/// human. Two gates in two places is what refused a call the user had just
/// granted, earlier in this work; one gate, at the layer that can prompt.
pub(crate) async fn op_deploy_flow(
    storage: &dyn StorageBackend,
    ns: &str,
    name: &str,
    runtime: Option<&str>,
) -> Result<serde_json::Value, String> {
    // Resolve against the requested runtime first; only if it actually matches
    // is the pin written to the canvas. A deploy that fails to place leaves the
    // stored configuration exactly as it was.
    let mut p = compute_placement_with(storage, ns, name, runtime).await?;
    if let Some(rt) = runtime.filter(|r| !r.is_empty()) {
        if p.get("matched").and_then(|v| v.as_bool()).unwrap_or(false) {
            pin_runtime(storage, &format!("{ns}/{name}"), rt).await?;
        }
    }
    if !p.get("matched").and_then(|v| v.as_bool()).unwrap_or(false) {
        return Err(format!(
            "no runtime matches this flow's placement selector — available: {}",
            p.get("availableRuntimes").map(|v| v.to_string()).unwrap_or_default()
        ));
    }
    if let Some(obj) = p.as_object_mut() {
        obj.insert("deployed".to_string(), json!(true));
        // Say plainly what happened. Placement in CE is logical: the flow is
        // mapped to a pool, not moved into a separate sandbox. Calling that
        // "deployed" without qualification is how people come to believe they
        // have isolation they do not have.
        obj.insert(
            "note".to_string(),
            json!("Placement resolved and recorded. In CE this is a logical mapping; \
isolation requires the EE controller."),
        );
    }
    tracing::info!(flow = %format!("{ns}/{name}"), "flow deployed via tool");
    Ok(p)
}

/// Record a runtime pin on the canvas bound to `flow_ref`.
async fn pin_runtime(storage: &dyn StorageBackend, flow_ref: &str, runtime: &str) -> Result<(), String> {
    let (cns, cname, spec) = stored_canvases(storage)
        .await
        .into_iter()
        .find(|(_, _, s)| s.flow_ref == flow_ref)
        .ok_or_else(|| format!("no canvas is bound to {flow_ref} — bind one before choosing a runtime"))?;
    let pinned = apply_patch(&spec, &json!({ "placement": { "runtime": runtime } }))
        .map_err(|p| format!("cannot pin runtime: {}", p.join("; ")))?;
    storage
        .resources()
        .update(&cns, &cname, serde_json::to_value(&pinned).unwrap_or(json!({})))
        .await
        .map_err(|e| format!("failed to record the runtime on the canvas: {e}"))?;
    Ok(())
}

/// Change a canvas the caller owns — never a preset.
///
/// A preset is shared; editing it in place would change the deployment shape of
/// every flow that later reuses it, which is precisely what cloning exists to
/// prevent. So presets are refused here with a pointer to `clone_canvas`.
pub(crate) async fn op_update_canvas(
    storage: &dyn StorageBackend,
    ns: &str,
    name: &str,
    patch: &serde_json::Value,
) -> Result<serde_json::Value, String> {
    let res = storage
        .resources()
        .get(ns, name)
        .await
        .map_err(|e| format!("canvas '{ns}/{name}' not found: {e}"))?;
    if res.type_ref != "canvas" {
        return Err(format!("'{ns}/{name}' is type '{}', not 'canvas'", res.type_ref));
    }
    let spec: CanvasSpec = serde_json::from_value(res.config.clone())
        .map_err(|e| format!("stored canvas is not a valid spec: {e}"))?;
    if spec.is_preset() {
        return Err(format!(
            "'{ns}/{name}' is a shared preset and is not edited in place — clone it with clone_canvas and change the copy"
        ));
    }
    // Two shapes: a `patch` merged over the current spec (what the agent sends),
    // or a whole `spec` from the editor. The editor case REPLACES rather than
    // merges — otherwise deleting a key in the editor would silently keep it.
    let updated = match patch.get("spec").filter(|v| v.is_object()) {
        Some(full) => {
            let replaced: CanvasSpec = serde_json::from_value(full.clone())
                .map_err(|e| format!("not a valid canvas spec: {e}"))?;
            let errs = replaced.validate();
            if !errs.is_empty() {
                return Err(format!("invalid canvas: {}", errs.join("; ")));
            }
            replaced
        }
        None => apply_patch(&spec, patch.get("patch").unwrap_or(patch))
            .map_err(|p| format!("patch produced an invalid canvas: {}", p.join("; ")))?,
    };
    storage
        .resources()
        .update(ns, name, serde_json::to_value(&updated).unwrap_or(json!({})))
        .await
        .map_err(|e| format!("failed to update canvas: {e}"))?;
    Ok(json!({
        "updated": true,
        "canvas": format!("{ns}/{name}"),
        "summary": preset_summary(ns, name, &updated),
        "note": "Configuration only — nothing is redeployed until deploy_flow.",
    }))
}

/// Register a new runtime pool.
///
/// Authorization is the caller's (registry governance: `resource:create`), done
/// where an approval can be put to a person. This only validates and stores.
///
/// Honest about what it creates: a runtime here is a REGISTERED pool — labels and
/// an optional endpoint that placement can select. It does not start a process
/// or a sandbox. Saying "created a runtime" without that qualification is how
/// people come to believe they have isolation they do not.
pub(crate) async fn op_create_runtime(
    storage: &dyn StorageBackend,
    ns: &str,
    name: &str,
    labels: &serde_json::Map<String, serde_json::Value>,
    url: Option<&str>,
) -> Result<serde_json::Value, String> {
    let valid = |s: &str| !s.is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
    if !valid(name) {
        return Err(format!("runtime name '{name}' must be letters, digits, '-' or '_'"));
    }
    if storage.resources().get(ns, name).await.is_ok() {
        return Err(format!("'{ns}/{name}' already exists"));
    }
    let mut lbl = std::collections::BTreeMap::new();
    for (k, v) in labels {
        let Some(v) = v.as_str() else {
            return Err(format!("label '{k}' must be a string"));
        };
        lbl.insert(k.clone(), v.to_string());
    }
    let spec = RuntimeSpec {
        name: name.to_string(),
        namespace: ns.to_string(),
        labels: lbl,
        url: url.filter(|u| !u.is_empty()).map(str::to_string),
        default: false,
    };
    let resource = flowr_types::Resource::new(
        name.to_string(),
        "runtime",
        serde_json::to_value(&spec).unwrap_or(json!({})),
    )
    .with_namespace(ns.to_string())
    .with_description(format!("Runtime pool {name}"));
    storage
        .resources()
        .create(resource)
        .await
        .map_err(|e| format!("failed to register runtime: {e}"))?;
    Ok(json!({
        "created": true,
        "runtime": spec,
        "note": "Registered as a placement target. This does not start a process or a sandbox.",
    }))
}

/// The `flow:deploy` request for deploying `ns/name` (onto `runtime`, if chosen).
///
/// ONE builder for every path that can deploy — the agent loop, direct MCP calls
/// and the HTTP routes — so they cannot ask different questions. Returns `None`
/// when nothing matches, which is not a privilege and is reported by the deploy
/// itself as "no runtime matches".
pub(crate) async fn deploy_request(
    storage: &dyn StorageBackend,
    principal: &flowr_policy::Principal,
    ns: &str,
    name: &str,
    runtime: Option<&str>,
    params: &serde_json::Value,
) -> Option<(flowr_policy::ExecutionRequest, RuntimeSpec, String, String)> {
    let (rt, eff_ns, mode) = placement_target(storage, ns, name, runtime).await?;
    let resource = flowr_policy::Resource::runtime(
        rt.name.clone(),
        eff_ns.clone(),
        mode.clone(),
        rt.labels.iter().map(|(k, v)| (k.clone(), v.clone())),
    );
    let req = flowr_policy::ExecutionRequest {
        principal: principal.clone(),
        action: flowr_policy::actions::flow_deploy(),
        resource,
        context: Default::default(),
        attributes: Default::default(),
        params: params.clone(),
    };
    Some((req, rt, eff_ns, mode))
}

/// The `resource:create` request for registering runtime `name`.
pub(crate) fn runtime_create_request(
    principal: &flowr_policy::Principal,
    name: &str,
    params: &serde_json::Value,
) -> flowr_policy::ExecutionRequest {
    flowr_policy::ExecutionRequest {
        principal: principal.clone(),
        action: flowr_policy::actions::resource_create(),
        resource: flowr_policy::Resource::registry("runtime", name),
        context: Default::default(),
        attributes: Default::default(),
        params: params.clone(),
    }
}

/// The runtime a flow's canvas would place it into, for the deployment gate.
///
/// Returns the pool, the effective namespace and the mode — exactly what the
/// `flow:deploy` decision is made against, so the gate authorizes the placement
/// that would actually happen rather than an approximation of it.
pub(crate) async fn placement_target(
    storage: &dyn StorageBackend,
    ns: &str,
    name: &str,
    runtime_override: Option<&str>,
) -> Option<(RuntimeSpec, String, String)> {
    let mut spec = resolve_canvas(storage, ns, name).await.ok()?;
    if let Some(rt) = runtime_override.filter(|r| !r.is_empty()) {
        spec.placement.runtime = Some(rt.to_string());
    }
    let runtimes = collect_runtimes(storage).await;
    let placed = select_runtime(&spec.placement, &runtimes)?.clone();
    let effective_ns = if !spec.placement.namespace.is_empty() {
        spec.placement.namespace.clone()
    } else {
        placed.namespace.clone()
    };
    let mode = match spec.placement.mode {
        flowr_types::canvas::PlacementMode::Isolated => "isolated",
        flowr_types::canvas::PlacementMode::Shared => "shared",
    };
    Some((placed, effective_ns, mode.to_string()))
}

/// Resolve where a flow would land and authorize that placement.
///
/// Shared by both deploy routes so they cannot drift apart — one of them being
/// gated and the other not is exactly how the original hole would come back.
async fn gate_placement(
    storage: &dyn StorageBackend,
    principal: &flowr_policy::Principal,
    ns: &str,
    name: &str,
) -> Result<(), axum::response::Response> {
    let spec = match resolve_canvas(storage, ns, name).await {
        Ok(s) => s,
        // A canvas that will not resolve cannot be deployed either; let the
        // deploy path report that in its own words rather than as a denial.
        Err(_) => return Ok(()),
    };
    let runtimes = collect_runtimes(storage).await;
    let placed = select_runtime(&spec.placement, &runtimes);
    let effective_ns = if !spec.placement.namespace.is_empty() {
        spec.placement.namespace.clone()
    } else {
        placed.map(|r| r.namespace.clone()).unwrap_or_else(|| ns.to_string())
    };
    let mode = match spec.placement.mode {
        flowr_types::canvas::PlacementMode::Isolated => "isolated",
        flowr_types::canvas::PlacementMode::Shared => "shared",
    };
    authorize_deploy(principal, placed, &effective_ns, mode).await
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
/// Where a flow would land, as plain data.
///
/// Deliberately not axum-coupled: the MCP tool path needs the same answer as the
/// HTTP route, and duplicating the resolution is how the two drift apart.
pub(crate) async fn compute_placement(
    storage: &dyn StorageBackend,
    ns: &str,
    name: &str,
) -> Result<serde_json::Value, String> {
    compute_placement_with(storage, ns, name, None).await
}

/// Placement with an optional runtime pin applied on top of the stored canvas.
///
/// The override is TRANSIENT: previewing "what if I put it on rt-x" must not
/// rewrite the canvas. `op_deploy_flow` persists the pin only once the deploy
/// has been authorized.
pub(crate) async fn compute_placement_with(
    storage: &dyn StorageBackend,
    ns: &str,
    name: &str,
    runtime_override: Option<&str>,
) -> Result<serde_json::Value, String> {
    let mut spec = resolve_canvas(storage, ns, name).await?;
    if let Some(rt) = runtime_override.filter(|r| !r.is_empty()) {
        spec.placement.runtime = Some(rt.to_string());
    }

    let errs = spec.validate();
    if !errs.is_empty() {
        return Err(format!("invalid canvas: {}", errs.join("; ")));
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
        Err(e) => (axum::http::StatusCode::UNPROCESSABLE_ENTITY, Json(json!({ "error": e }))).into_response(),
    }
}

async fn deploy_flow(
    State(state): State<Arc<CanvasState>>,
    principal: Option<axum::Extension<flowr_policy::Principal>>,
    Path((ns, name)): Path<(String, String)>,
) -> impl IntoResponse {
    // Deploying places workloads, so it is never anonymous. Refusing outright
    // rather than falling back to a service identity is the same rule the chat
    // route follows: defaulting a privileged action to a shared identity is the
    // identity-collapse trap.
    let Some(axum::Extension(principal)) = principal else {
        return (
            axum::http::StatusCode::UNAUTHORIZED,
            Json(json!({ "deployed": false, "error": "authentication required" })),
        )
            .into_response();
    };

    // Authorize against the pool this would actually land in, before anything is
    // pushed anywhere.
    if let Err(denied) = gate_placement(&*state.storage, &principal, &ns, &name).await {
        return denied;
    }

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
        Err(e) => (axum::http::StatusCode::UNPROCESSABLE_ENTITY, Json(json!({ "error": e }))).into_response(),
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
    principal: Option<axum::Extension<flowr_policy::Principal>>,
    Path((ns, name)): Path<(String, String)>,
) -> impl IntoResponse {
    let Some(axum::Extension(principal)) = principal else {
        return (
            axum::http::StatusCode::UNAUTHORIZED,
            Json(json!({ "deployed": false, "error": "authentication required" })),
        )
            .into_response();
    };
    if let Err(denied) = gate_placement(&*state.storage, &principal, &ns, &name).await {
        return denied;
    }
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
