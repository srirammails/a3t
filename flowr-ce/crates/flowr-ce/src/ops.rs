//! Runtime operations — the deployment ledger and everything that operates on it.
//!
//! Deploying used to leave nothing behind: without the EE controller, `deploy_flow`
//! resolved a placement, answered `deployed: true` and forgot it. So there was
//! nothing to list, pause or undeploy. This module is that missing record, plus
//! the operations people expect to run against it.
//!
//! - **Deployments** are registry resources (`type_ref = "deployment"`, namespace
//!   `system`), one per deployed flow or agent. Redeploying updates the record.
//! - **Pause is a hard stop.** A paused deployment refuses every request — trigger,
//!   manual run, chat, agent-to-agent — until someone resumes it. Webhooks answer
//!   with the paused response (503 by default, 404 if configured), so a caller can
//!   tell "paused" from "broken".
//! - **Runtimes** are `active`, `hibernated` or `paused`. Hibernated serves: the
//!   first request wakes it. Paused refuses everything deployed on it, without
//!   touching the deployments' own states, so resuming the runtime restores
//!   exactly what was running before — a deployment paused on its own stays paused.
//!
//! `admit` is the single admission check, consulted on every path that can start
//! work (the worker, the agent handler, trigger delivery). It reads an in-memory
//! copy of the ledger; every mutation here writes through to storage first and
//! then updates that copy, so the copy is never ahead of what is stored.

use std::collections::HashMap;
use std::sync::{Arc, OnceLock, RwLock};

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post, put},
    Json, Router,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tracing::{info, warn};

use flowr_storage::StorageBackend;
use flowr_types::canvas::{paused_status, LifecycleState, RuntimeSpec, RuntimeState};
use flowr_types::ExecutionStatus;

/// Registry namespace that holds every deployment record. One fixed namespace,
/// because a deployment's effective namespace can be anything and the ledger must
/// be loadable without guessing which namespaces exist.
const LEDGER_NS: &str = "system";
const DEPLOYMENT_TYPE: &str = "deployment";

// ============================================================================
// The record
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Deployment {
    pub id: String,
    /// `flow` or `agent`. An agent deployment is matched on the AGENT, never on
    /// its flow: agents share template flows, and pausing one agent must not
    /// pause every other agent built from the same template.
    pub kind: String,
    /// What is deployed: `ns/name` for a flow, the agent id for an agent.
    pub target: String,
    #[serde(default)]
    pub flow_ref: String,
    #[serde(default)]
    pub agent_ref: String,
    /// The canvas the placement came from (`ns/name`), or empty for an inline or
    /// default canvas.
    #[serde(default)]
    pub canvas: String,
    pub runtime: String,
    pub namespace: String,
    pub mode: String,
    /// `active` or `paused`.
    pub state: String,
    /// The canvas's `lifecycle.pausedResponse`, captured at deploy time.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub paused_response: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub paused_by: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub paused_at: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pause_reason: Option<String>,
    pub deployed_by: String,
    pub deployed_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Deployment {
    pub fn is_paused(&self) -> bool {
        self.state == "paused"
    }

    /// The registry name for a deployment of `kind`/`target`. Deterministic, so
    /// redeploying the same thing updates its record instead of adding another.
    pub fn id_for(kind: &str, target: &str) -> String {
        let clean: String = target
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '.' })
            .collect();
        format!("{kind}.{clean}")
    }
}

/// The agent id inside an `agentRef` (`ns/id` → `id`).
fn agent_id_of(agent_ref: &str) -> &str {
    agent_ref.rsplit('/').next().unwrap_or(agent_ref)
}

/// The agent a session belongs to: `<agent-id>:session-…` for a configured
/// agent's chat, or the bare agent id when a message is sent straight to it.
pub fn agent_of_session(session_id: &str) -> Option<&str> {
    let id = session_id.split(':').next().filter(|s| !s.is_empty())?;
    Some(id)
}

// ============================================================================
// The in-memory copy
// ============================================================================

#[derive(Default)]
struct Ledger {
    deployments: HashMap<String, Deployment>,
    runtimes: HashMap<String, RuntimeSpec>,
}

fn ledger() -> &'static RwLock<Ledger> {
    static L: OnceLock<RwLock<Ledger>> = OnceLock::new();
    L.get_or_init(|| RwLock::new(Ledger::default()))
}

/// Read the ledger and runtime states from storage into memory. Called at worker
/// start and whenever the console asks for the overview, which keeps the copy in
/// step with edits made through the generic registry API.
pub async fn load(storage: &dyn StorageBackend) {
    let mut deployments = HashMap::new();
    match storage.resources().list(LEDGER_NS).await {
        Ok(list) => {
            for r in list.into_iter().filter(|r| r.type_ref == DEPLOYMENT_TYPE) {
                match serde_json::from_value::<Deployment>(r.config.clone()) {
                    Ok(d) => {
                        deployments.insert(d.id.clone(), d);
                    }
                    Err(e) => warn!(deployment = %r.name, error = %e, "unreadable deployment record"),
                }
            }
        }
        Err(e) => {
            // Keep the previous copy: an unreadable table must not silently
            // un-pause everything.
            warn!(error = %e, "deployment ledger not loaded");
            return;
        }
    }
    let runtimes: HashMap<String, RuntimeSpec> = crate::canvas::collect_runtimes(storage)
        .await
        .into_iter()
        .map(|r| (r.name.clone(), r))
        .collect();
    let mut l = ledger().write().unwrap();
    l.deployments = deployments;
    l.runtimes = runtimes;
}

fn cached(id: &str) -> Option<Deployment> {
    ledger().read().unwrap().deployments.get(id).cloned()
}

// ============================================================================
// Running executions — what Terminate cancels
// ============================================================================

/// One in-flight execution, registered by `execute_workflow` for as long as it
/// runs. `stop` is raced against the executor; whoever terminates writes `why`
/// first so the run records who stopped it.
struct RunningRun {
    flow_ref: String,
    agent: Option<String>,
    stop: Arc<tokio::sync::Notify>,
    why: Arc<std::sync::Mutex<Option<String>>>,
}

fn running() -> &'static std::sync::Mutex<HashMap<uuid::Uuid, RunningRun>> {
    static R: OnceLock<std::sync::Mutex<HashMap<uuid::Uuid, RunningRun>>> = OnceLock::new();
    R.get_or_init(|| std::sync::Mutex::new(HashMap::new()))
}

/// Unregisters on drop, so a run that returns early or panics never lingers as
/// "in flight" and can never be cancelled twice.
pub struct RunGuard(uuid::Uuid);
impl Drop for RunGuard {
    fn drop(&mut self) {
        running().lock().unwrap().remove(&self.0);
    }
}

/// Register a run. Returns the stop signal to race against, the slot holding the
/// reason, and the guard that unregisters it.
pub fn register_run(
    id: uuid::Uuid,
    flow_ref: &str,
    agent: Option<&str>,
) -> (Arc<tokio::sync::Notify>, Arc<std::sync::Mutex<Option<String>>>, RunGuard) {
    let stop = Arc::new(tokio::sync::Notify::new());
    let why = Arc::new(std::sync::Mutex::new(None));
    running().lock().unwrap().insert(
        id,
        RunningRun { flow_ref: flow_ref.to_string(), agent: agent.map(str::to_string), stop: stop.clone(), why: why.clone() },
    );
    (stop, why, RunGuard(id))
}

/// Cancel every in-flight run of a deployment. `notify_one` stores a permit, so a
/// run between registering and awaiting still sees the signal.
fn cancel_runs(d: &Deployment, reason: &str) -> Vec<String> {
    let runs = running().lock().unwrap();
    let mut ids = Vec::new();
    for (id, r) in runs.iter() {
        let hit = match d.kind.as_str() {
            "agent" => r.agent.as_deref() == Some(d.target.as_str()),
            _ => r.flow_ref == d.target,
        };
        if hit {
            *r.why.lock().unwrap() = Some(reason.to_string());
            r.stop.notify_one();
            ids.push(id.to_string());
        }
    }
    ids
}

// ============================================================================
// Admission
// ============================================================================

/// Why a request was refused.
#[derive(Debug, Clone)]
pub struct Blocked {
    /// HTTP status for trigger callers: 503 or 404.
    pub status: u16,
    pub message: String,
    pub deployment: String,
}

impl std::fmt::Display for Blocked {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

/// May work for `flow_ref` (`ns/name`) and/or `agent_id` start now?
///
/// Anything with no deployment record is admitted — platform flows and things
/// run before they were ever deployed keep working exactly as before. A paused
/// deployment, or one on a paused runtime, is refused. A deployment on a
/// hibernated runtime wakes that runtime and is admitted.
pub async fn admit(
    storage: &dyn StorageBackend,
    flow_ref: Option<&str>,
    agent_id: Option<&str>,
) -> Result<(), Blocked> {
    let mut wake: Vec<String> = Vec::new();
    {
        let l = ledger().read().unwrap();
        if l.deployments.is_empty() {
            return Ok(());
        }
        for d in l.deployments.values() {
            let hit = match d.kind.as_str() {
                "agent" => agent_id.is_some_and(|a| a == d.target),
                _ => flow_ref.is_some_and(|f| f == d.target),
            };
            if !hit {
                continue;
            }
            let rt = l.runtimes.get(&d.runtime);
            let status = paused_status(d.paused_response.or(rt.and_then(|r| r.paused_response)));
            if d.is_paused() {
                return Err(Blocked {
                    status,
                    message: format!(
                        "{} '{}' is paused{} — resume it from the Runtime console",
                        d.kind,
                        d.target,
                        d.pause_reason.as_deref().map(|r| format!(" ({r})")).unwrap_or_default(),
                    ),
                    deployment: d.id.clone(),
                });
            }
            match rt.map(|r| r.state) {
                Some(RuntimeState::Paused) => {
                    return Err(Blocked {
                        status,
                        message: format!(
                            "runtime '{}' is paused, so {} '{}' is not serving — an admin must resume the runtime",
                            d.runtime, d.kind, d.target
                        ),
                        deployment: d.id.clone(),
                    });
                }
                Some(RuntimeState::Hibernated) => wake.push(d.runtime.clone()),
                _ => {}
            }
        }
    }
    for rt in wake {
        info!(runtime = %rt, "request arrived for a hibernated runtime — waking it");
        if let Err(e) = write_runtime_state(storage, &rt, RuntimeState::Active).await {
            // Serving is the point of hibernation; failing to record the wake is
            // not a reason to refuse the request.
            warn!(runtime = %rt, error = %e, "wake not recorded");
        }
    }
    Ok(())
}

// ============================================================================
// Writes — storage first, then the copy
// ============================================================================

async fn store(storage: &dyn StorageBackend, d: &Deployment) -> Result<(), String> {
    let cfg = serde_json::to_value(d).map_err(|e| e.to_string())?;
    let exists = storage.resources().exists(LEDGER_NS, &d.id).await.unwrap_or(false);
    if exists {
        storage
            .resources()
            .update(LEDGER_NS, &d.id, cfg)
            .await
            .map_err(|e| format!("failed to save deployment {}: {e}", d.id))?;
    } else {
        let r = flowr_types::Resource::new(d.id.clone(), DEPLOYMENT_TYPE, cfg)
            .with_namespace(LEDGER_NS.to_string())
            .with_description(format!("Deployment of {} {} on {}", d.kind, d.target, d.runtime));
        storage
            .resources()
            .create(r)
            .await
            .map_err(|e| format!("failed to record deployment {}: {e}", d.id))?;
    }
    ledger().write().unwrap().deployments.insert(d.id.clone(), d.clone());
    Ok(())
}

/// Where the runtime's registry row lives (its namespace), from the copy or storage.
async fn runtime_spec(storage: &dyn StorageBackend, name: &str) -> Option<RuntimeSpec> {
    if let Some(r) = ledger().read().unwrap().runtimes.get(name).cloned() {
        return Some(r);
    }
    crate::canvas::collect_runtimes(storage).await.into_iter().find(|r| r.name == name)
}

async fn write_runtime(storage: &dyn StorageBackend, spec: RuntimeSpec) -> Result<RuntimeSpec, String> {
    // Patch only our fields onto the stored config, so keys this typed view does
    // not know about survive the write.
    let mut cfg = storage
        .resources()
        .get(&spec.namespace, &spec.name)
        .await
        .map(|r| r.config)
        .map_err(|e| format!("runtime '{}' not found: {e}", spec.name))?;
    if !cfg.is_object() {
        cfg = json!({});
    }
    cfg["state"] = serde_json::to_value(spec.state).unwrap_or(json!("active"));
    match spec.paused_response {
        Some(c) => cfg["pausedResponse"] = json!(c),
        None => {
            if let Some(o) = cfg.as_object_mut() {
                o.remove("pausedResponse");
            }
        }
    }
    storage
        .resources()
        .update(&spec.namespace, &spec.name, cfg)
        .await
        .map_err(|e| format!("failed to update runtime '{}': {e}", spec.name))?;
    ledger().write().unwrap().runtimes.insert(spec.name.clone(), spec.clone());
    Ok(spec)
}

async fn write_runtime_state(
    storage: &dyn StorageBackend,
    name: &str,
    state: RuntimeState,
) -> Result<RuntimeSpec, String> {
    let mut spec = runtime_spec(storage, name)
        .await
        .ok_or_else(|| format!("runtime '{name}' not found"))?;
    spec.state = state;
    write_runtime(storage, spec).await
}

// ============================================================================
// Operations
// ============================================================================

/// Record a deployment after a successful deploy. Called by every deploy path.
///
/// A redeploy keeps the existing pause: "paused until resumed" has to survive
/// someone pressing Deploy again, or pause would be one click from meaningless.
pub async fn record_deploy(
    storage: &dyn StorageBackend,
    by: &str,
    flow_ns: &str,
    flow_name: &str,
    runtime: &str,
    namespace: &str,
    mode: &str,
) -> Result<Deployment, String> {
    let flow_ref = format!("{flow_ns}/{flow_name}");
    let stored = crate::canvas::stored_canvases(storage)
        .await
        .into_iter()
        .find(|(_, _, s)| s.flow_ref == flow_ref);
    let spec = crate::canvas::resolve_canvas(storage, flow_ns, flow_name).await.ok();
    let agent_ref = stored.as_ref().map(|(_, _, s)| s.agent_ref.clone()).unwrap_or_default();
    let (kind, target) = if agent_ref.is_empty() {
        ("flow", flow_ref.clone())
    } else {
        ("agent", agent_id_of(&agent_ref).to_string())
    };
    let id = Deployment::id_for(kind, &target);
    let now = Utc::now();
    let previous = cached(&id);
    let starts_paused = spec.as_ref().is_some_and(|s| s.lifecycle.state == LifecycleState::Paused);
    let d = Deployment {
        id: id.clone(),
        kind: kind.to_string(),
        target,
        flow_ref,
        agent_ref,
        canvas: stored.as_ref().map(|(n, c, _)| format!("{n}/{c}")).unwrap_or_default(),
        runtime: runtime.to_string(),
        namespace: namespace.to_string(),
        mode: mode.to_string(),
        state: match &previous {
            Some(p) => p.state.clone(),
            None if starts_paused => "paused".to_string(),
            None => "active".to_string(),
        },
        paused_response: spec.as_ref().and_then(|s| s.lifecycle.paused_response),
        paused_by: previous.as_ref().and_then(|p| p.paused_by.clone()),
        paused_at: previous.as_ref().and_then(|p| p.paused_at),
        pause_reason: previous
            .as_ref()
            .and_then(|p| p.pause_reason.clone())
            .or_else(|| starts_paused.then(|| "canvas lifecycle.state is paused".to_string())),
        deployed_by: by.to_string(),
        deployed_at: previous.as_ref().map(|p| p.deployed_at).unwrap_or(now),
        updated_at: now,
    };
    store(storage, &d).await?;
    info!(deployment = %id, runtime = %runtime, by = %by, "deployment recorded");
    Ok(d)
}

/// Find a deployment by id, or by what it deploys (`ns/name` or an agent id) —
/// people and models name the thing, not the record.
pub fn resolve(id_or_target: &str) -> Option<Deployment> {
    let l = ledger().read().unwrap();
    l.deployments
        .get(id_or_target)
        .or_else(|| l.deployments.values().find(|d| d.target == id_or_target || d.flow_ref == id_or_target))
        .cloned()
}

fn not_found(id: &str) -> String {
    format!("no deployment '{id}' — list_deployments shows what is deployed")
}

pub async fn pause(storage: &dyn StorageBackend, id: &str, by: &str, reason: Option<&str>) -> Result<Value, String> {
    let mut d = resolve(id).ok_or_else(|| not_found(id))?;
    if d.is_paused() {
        return Ok(json!({ "paused": true, "alreadyPaused": true, "deployment": d }));
    }
    d.state = "paused".into();
    d.paused_by = Some(by.to_string());
    d.paused_at = Some(Utc::now());
    d.pause_reason = reason.filter(|r| !r.is_empty()).map(str::to_string);
    d.updated_at = Utc::now();
    store(storage, &d).await?;
    let status = effective_paused_status(&d);
    info!(deployment = %d.id, by = %by, "deployment paused");
    Ok(json!({
        "paused": true,
        "deployment": d,
        "callersGet": status,
        "note": "Every new request is refused until resumed. Runs already in progress finish.",
    }))
}

pub async fn resume(storage: &dyn StorageBackend, id: &str, by: &str) -> Result<Value, String> {
    let mut d = resolve(id).ok_or_else(|| not_found(id))?;
    if !d.is_paused() {
        return Ok(json!({ "resumed": true, "wasActive": true, "deployment": d }));
    }
    d.state = "active".into();
    d.paused_by = None;
    d.paused_at = None;
    d.pause_reason = None;
    d.updated_at = Utc::now();
    store(storage, &d).await?;
    info!(deployment = %d.id, by = %by, "deployment resumed");
    let rt_paused = ledger()
        .read()
        .unwrap()
        .runtimes
        .get(&d.runtime)
        .is_some_and(|r| r.state == RuntimeState::Paused);
    Ok(json!({
        "resumed": true,
        "deployment": d,
        "note": if rt_paused { "Resumed, but its runtime is paused — it serves once an admin resumes the runtime." } else { "Serving again." },
    }))
}

/// Re-resolve placement from the (possibly edited) canvas and refresh the record.
/// This is "edit the canvas, then redeploy" as one action.
pub async fn restart(storage: &dyn StorageBackend, id: &str, by: &str) -> Result<Value, String> {
    let d = resolve(id).ok_or_else(|| not_found(id))?;
    if d.is_paused() {
        return Err(format!("{} '{}' is paused — resume it before restarting", d.kind, d.target));
    }
    let (fns, fname) = d
        .flow_ref
        .split_once('/')
        .ok_or_else(|| format!("deployment {} has no flow to re-place", d.id))?;
    let (rt, eff_ns, mode) = crate::canvas::placement_target(storage, fns, fname, None)
        .await
        .ok_or_else(|| "no runtime matches the canvas placement any more — edit the canvas or add a runtime".to_string())?;
    let moved = rt.name != d.runtime;
    let fresh = record_deploy(storage, by, fns, fname, &rt.name, &eff_ns, &mode).await?;
    Ok(json!({
        "restarted": true,
        "deployment": fresh,
        "movedFrom": if moved { Some(d.runtime) } else { None },
        "note": "Placement re-resolved from the current canvas. In CE placement is logical — there is no process to recycle; the EE controller rolls the pods.",
    }))
}

pub async fn undeploy(storage: &dyn StorageBackend, id: &str, by: &str) -> Result<Value, String> {
    let d = resolve(id).ok_or_else(|| not_found(id))?;
    storage
        .resources()
        .delete(LEDGER_NS, &d.id)
        .await
        .map_err(|e| format!("failed to remove deployment {}: {e}", d.id))?;
    ledger().write().unwrap().deployments.remove(&d.id);
    info!(deployment = %d.id, by = %by, "undeployed");
    // EE: the deploy route pushed a Canvas CR named after the flow; remove it so
    // the controller garbage-collects what it created. Best effort, and reported —
    // the ledger entry is gone either way, and saying so is better than hiding a
    // CR that outlived its deployment.
    let controller = match std::env::var("A3T_CONTROLLER_URL").ok().filter(|u| !u.is_empty()) {
        None => Value::Null,
        Some(url) => {
            // The CR path is `ns/name`, exactly the flow ref.
            let mut req = reqwest::Client::new()
                .delete(format!("{}/v1/canvas/{}", url.trim_end_matches('/'), d.flow_ref))
                .timeout(std::time::Duration::from_secs(5));
            if let Ok(tok) = std::env::var("A3T_CONTROLLER_TOKEN") {
                if !tok.is_empty() {
                    req = req.bearer_auth(tok);
                }
            }
            match req.send().await {
                Ok(r) if r.status().is_success() || r.status() == reqwest::StatusCode::NOT_FOUND => json!({ "removed": true }),
                Ok(r) => json!({ "removed": false, "error": format!("controller returned {}", r.status()) }),
                Err(e) => json!({ "removed": false, "error": e.to_string() }),
            }
        }
    };
    Ok(json!({
        "undeployed": true,
        "deployment": d.id,
        "controller": controller,
        "note": "The deployment record is gone; the flow and its canvas are kept, so it can be deployed again.",
    }))
}

/// Undeploy NOW: cancel the deployment's in-flight runs, then remove it.
pub async fn terminate(storage: &dyn StorageBackend, id: &str, by: &str) -> Result<Value, String> {
    let d = resolve(id).ok_or_else(|| not_found(id))?;
    let cancelled = cancel_runs(&d, &format!("terminated by {by}"));
    let mut out = undeploy(storage, &d.id, by).await?;
    info!(deployment = %d.id, by = %by, cancelled = cancelled.len(), "terminated");
    out["terminated"] = json!(true);
    out["cancelledRuns"] = json!(cancelled);
    out["note"] = json!(format!(
        "Stopped {} run(s) in progress and removed the deployment. The flow and its canvas are kept.",
        cancelled.len()
    ));
    Ok(out)
}

/// Where a deployment would land among `runtimes` — the same rule as
/// `canvas::placement_target`, over a list we choose (so the runtime being
/// terminated can be left out of it).
async fn place_among(
    storage: &dyn StorageBackend,
    d: &Deployment,
    runtimes: &[RuntimeSpec],
) -> Option<(RuntimeSpec, String, String)> {
    let (fns, fname) = d.flow_ref.split_once('/')?;
    let spec = crate::canvas::resolve_canvas(storage, fns, fname).await.ok()?;
    let placed = flowr_types::canvas::select_runtime(&spec.placement, runtimes)?.clone();
    let ns = if spec.placement.namespace.is_empty() { placed.namespace.clone() } else { spec.placement.namespace.clone() };
    Some((placed, ns, d.mode.clone()))
}

/// What terminating a runtime does to one deployment on it.
///
/// Terminating a runtime is killing its container, not withdrawing what was
/// deployed: deployments are the desired state, so each is rescheduled the way a
/// k8s controller replaces a pod. An ISOLATED deployment owned that container, so
/// it gets a fresh one and loses whatever was only in memory — that is the
/// warning. A shared one moves to another runtime its canvas matches; if none
/// does, the runtime is recreated for it.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Reschedule {
    deployment: String,
    kind: String,
    target: String,
    mode: String,
    session: String,
    /// "move" (to `to`) or "recreate" (a fresh runtime with the old spec).
    action: String,
    to: String,
    runs_in_flight: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    warning: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    note: Option<String>,
}

fn in_flight_for(d: &Deployment) -> usize {
    running()
        .lock()
        .unwrap()
        .values()
        .filter(|r| match d.kind.as_str() {
            "agent" => r.agent.as_deref() == Some(d.target.as_str()),
            _ => r.flow_ref == d.target,
        })
        .count()
}

async fn reschedule_plan(storage: &dyn StorageBackend, name: &str) -> (Vec<(Deployment, Reschedule)>, Vec<String>) {
    let (deps, others): (Vec<Deployment>, Vec<RuntimeSpec>) = {
        let l = ledger().read().unwrap();
        (
            l.deployments.values().filter(|d| d.runtime == name).cloned().collect(),
            l.runtimes.values().filter(|r| r.name != name).cloned().collect(),
        )
    };
    let mut plan = Vec::new();
    for d in deps {
        let session = match d.flow_ref.split_once('/') {
            Some((fns, fname)) => crate::canvas::resolve_canvas(storage, fns, fname)
                .await
                .map(|s| format!("{:?}", s.lifecycle.session).to_lowercase())
                .unwrap_or_else(|_| "ephemeral".into()),
            None => "ephemeral".into(),
        };
        let runs = in_flight_for(&d);
        let isolated = d.mode == "isolated";
        let elsewhere = if isolated { None } else { place_among(storage, &d, &others).await };
        let (action, to) = match &elsewhere {
            Some((rt, ..)) => ("move", rt.name.clone()),
            None => ("recreate", name.to_string()),
        };
        let warning = isolated.then(|| format!(
            "Isolated — this runtime is its own container. It is replaced by a fresh one; {} in-memory state is lost.",
            if runs > 0 { format!("{runs} run(s) in progress are cancelled and") } else { "any".to_string() }
        ));
        let note = match (isolated, session.as_str(), runs) {
            (true, ..) => None,
            (false, "persistent", _) => Some("Persistent sessions resume from their last checkpoint.".to_string()),
            (false, _, n) if n > 0 => Some(format!("{n} run(s) in progress are cancelled; new requests go to {to}.")),
            _ => None,
        };
        plan.push((d.clone(), Reschedule {
            deployment: d.id.clone(), kind: d.kind.clone(), target: d.target.clone(), mode: d.mode.clone(), session,
            action: action.into(), to, runs_in_flight: runs, warning, note,
        }));
    }
    let pinned: Vec<String> = crate::canvas::stored_canvases(storage)
        .await
        .into_iter()
        .filter(|(_, _, s)| s.placement.runtime.as_deref() == Some(name))
        .map(|(ns, n, _)| format!("{ns}/{n}"))
        .collect();
    (plan, pinned)
}

/// Terminate a runtime: kill its container (cancel everything running on it),
/// remove it, and reschedule what was deployed there.
///
/// Without `confirm` equal to the runtime's name this changes nothing and returns
/// the plan — where each deployment goes, which are isolated (the warning), and
/// how many runs would be cancelled.
pub async fn terminate_runtime(
    storage: &dyn StorageBackend,
    name: &str,
    by: &str,
    confirm: Option<&str>,
) -> Result<Value, String> {
    load(storage).await;
    let old = runtime_spec(storage, name)
        .await
        .ok_or_else(|| format!("runtime '{name}' not found"))?;
    let (plan, pinned) = reschedule_plan(storage, name).await;
    let recreate = plan.iter().any(|(_, r)| r.action == "recreate");
    let warnings: Vec<&String> = plan.iter().filter_map(|(_, r)| r.warning.as_ref()).collect();
    let summary = json!({
        "runtime": name,
        "plan": plan.iter().map(|(_, r)| r).collect::<Vec<_>>(),
        "recreatesRuntime": recreate,
        "warnings": warnings,
        "runsInFlight": plan.iter().map(|(_, r)| r.runs_in_flight).sum::<usize>(),
        "pinnedCanvases": pinned,
    });
    if confirm != Some(name) {
        return Ok(json!({
            "terminated": false,
            "preview": true,
            "impact": summary,
            "note": format!("Nothing changed. To terminate, call again with confirm = \"{name}\"."),
        }));
    }

    // 1. Kill the container: cancel every run on it.
    let mut cancelled = 0;
    for (d, _) in &plan {
        cancelled += cancel_runs(d, &format!("runtime {name} terminated by {by}")).len();
    }
    // 2. Remove the runtime.
    storage
        .resources()
        .delete(&old.namespace, name)
        .await
        .map_err(|e| format!("removing runtime '{name}' failed: {e} — nothing was rescheduled"))?;
    ledger().write().unwrap().runtimes.remove(name);
    // 3. Recreate it when something can only live there (isolated, or no other
    //    runtime matches): same name, labels and namespace — so pins still hold —
    //    as a fresh, active instance.
    if recreate {
        let labels: serde_json::Map<String, Value> =
            old.labels.iter().map(|(k, v)| (k.clone(), json!(v))).collect();
        crate::canvas::op_create_runtime(storage, &old.namespace, name, &labels, old.url.as_deref())
            .await
            .map_err(|e| format!("runtime '{name}' was removed but recreating it failed: {e}"))?;
        load(storage).await;
    }
    // 4. Reschedule each deployment onto its new home. record_deploy keeps a
    //    pause: a paused agent is still paused after the move.
    let mut moved = Vec::new();
    for (d, r) in &plan {
        let rt = if r.action == "move" { r.to.clone() } else { name.to_string() };
        let target_ns = match runtime_spec(storage, &rt).await {
            Some(spec) => place_among(storage, d, std::slice::from_ref(&spec)).await.map(|(_, ns, _)| ns).unwrap_or(d.namespace.clone()),
            None => d.namespace.clone(),
        };
        if let Some((fns, fname)) = d.flow_ref.split_once('/') {
            match record_deploy(storage, by, fns, fname, &rt, &target_ns, &d.mode).await {
                Ok(_) => moved.push(json!({ "deployment": d.id, "runtime": rt, "action": r.action })),
                Err(e) => moved.push(json!({ "deployment": d.id, "error": e })),
            }
        }
    }
    info!(runtime = %name, by = %by, cancelled, rescheduled = moved.len(), recreated = recreate, "runtime terminated");
    let n_moved = plan.iter().filter(|(_, r)| r.action == "move").count();
    let note = match (plan.is_empty(), recreate, n_moved) {
        (true, ..) => format!("Runtime {name} removed — nothing was deployed on it."),
        (false, true, 0) => format!("Runtime {name} was terminated and recreated fresh for what can only run there."),
        (false, true, m) => format!("Runtime {name} was terminated and recreated fresh for what can only run there; {m} deployment(s) moved to other runtimes."),
        (false, false, _) => format!("Runtime {name} removed; everything on it moved to other runtimes."),
    };
    Ok(json!({
        "terminated": true,
        "runtime": name,
        "cancelledRuns": cancelled,
        "rescheduled": moved,
        "recreated": recreate,
        "note": note,
    }))
}

pub async fn set_runtime_state(
    storage: &dyn StorageBackend,
    name: &str,
    state: RuntimeState,
    by: &str,
) -> Result<Value, String> {
    let spec = write_runtime_state(storage, name, state).await?;
    let affected: Vec<String> = ledger()
        .read()
        .unwrap()
        .deployments
        .values()
        .filter(|d| d.runtime == name)
        .map(|d| d.id.clone())
        .collect();
    info!(runtime = %name, state = ?state, by = %by, "runtime state changed");
    let note = match state {
        RuntimeState::Paused => "Everything deployed here refuses requests until an admin resumes the runtime. Deployments keep their own states.",
        RuntimeState::Hibernated => "Asleep. The first request for anything deployed here wakes it.",
        RuntimeState::Active => "Serving. Deployments paused on their own stay paused.",
    };
    Ok(json!({ "runtime": spec, "deployments": affected, "note": note }))
}

pub async fn set_paused_response(storage: &dyn StorageBackend, name: &str, code: Option<u16>) -> Result<Value, String> {
    if let Some(c) = code {
        if c != 503 && c != 404 {
            return Err("pausedResponse must be 503 or 404".to_string());
        }
    }
    let mut spec = runtime_spec(storage, name)
        .await
        .ok_or_else(|| format!("runtime '{name}' not found"))?;
    spec.paused_response = code;
    let spec = write_runtime(storage, spec).await?;
    Ok(json!({ "runtime": spec }))
}

fn effective_paused_status(d: &Deployment) -> u16 {
    let rt = ledger().read().unwrap().runtimes.get(&d.runtime).and_then(|r| r.paused_response);
    paused_status(d.paused_response.or(rt))
}

// ============================================================================
// Policy — one builder for every path (agent loop, direct MCP, HTTP)
// ============================================================================

/// The policy request for an operations tool, or `None` for reads and unknown
/// tools. Returns the request plus a decision id for the session ledger, keyed on
/// the target so approving one pause does not cover another.
pub fn ops_request(
    principal: &flowr_policy::Principal,
    tool: &str,
    args: &Value,
) -> Option<(flowr_policy::ExecutionRequest, &'static str, String)> {
    let (action, act_str, rt_name, ns, mode, did) = match tool {
        "pause_deployment" | "resume_deployment" | "undeploy" | "restart_deployment" | "terminate_deployment" => {
            let id = args.get("deployment").and_then(|v| v.as_str())?;
            let d = resolve(id)?;
            let (a, s) = match tool {
                "pause_deployment" => (flowr_policy::actions::flow_pause(), flowr_policy::actions::FLOW_PAUSE),
                "resume_deployment" => (flowr_policy::actions::flow_resume(), flowr_policy::actions::FLOW_RESUME),
                "undeploy" => (flowr_policy::actions::flow_undeploy(), flowr_policy::actions::FLOW_UNDEPLOY),
                "terminate_deployment" => (flowr_policy::actions::flow_terminate(), flowr_policy::actions::FLOW_TERMINATE),
                // A restart re-places the deployment, so it asks the deploy question.
                _ => (flowr_policy::actions::flow_deploy(), flowr_policy::actions::FLOW_DEPLOY),
            };
            (a, s, d.runtime.clone(), d.namespace.clone(), d.mode.clone(), format!("{}/{}", d.runtime, d.id))
        }
        // A preview (no matching confirm) changes nothing, so it is not a decision.
        "terminate_runtime" if args.get("confirm").and_then(|v| v.as_str()) != args.get("runtime").and_then(|v| v.as_str()) => return None,
        "set_runtime_state" | "set_paused_response" | "terminate_runtime" => {
            let name = args.get("runtime").and_then(|v| v.as_str())?;
            let rt = ledger().read().unwrap().runtimes.get(name).cloned();
            let ns = rt.as_ref().map(|r| r.namespace.clone()).unwrap_or_else(|| "default".into());
            let (a, s) = match (tool, args.get("state").and_then(|v| v.as_str())) {
                ("terminate_runtime", _) => (flowr_policy::actions::runtime_terminate(), flowr_policy::actions::RUNTIME_TERMINATE),
                ("set_runtime_state", Some("hibernated")) => (flowr_policy::actions::runtime_hibernate(), flowr_policy::actions::RUNTIME_HIBERNATE),
                ("set_runtime_state", Some("active")) => (flowr_policy::actions::runtime_resume(), flowr_policy::actions::RUNTIME_RESUME),
                // Pausing, and changing what callers see while paused, are both
                // decisions about refusing traffic on shared infrastructure.
                _ => (flowr_policy::actions::runtime_pause(), flowr_policy::actions::RUNTIME_PAUSE),
            };
            (a, s, name.to_string(), ns, "shared".to_string(), format!("{name}/{tool}"))
        }
        _ => return None,
    };
    let labels: Vec<(String, String)> = ledger()
        .read()
        .unwrap()
        .runtimes
        .get(&rt_name)
        .map(|r| r.labels.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
        .unwrap_or_default();
    let req = flowr_policy::ExecutionRequest {
        principal: principal.clone(),
        action,
        resource: flowr_policy::Resource::runtime(rt_name, ns, mode, labels),
        context: Default::default(),
        attributes: Default::default(),
        params: args.clone(),
    };
    Some((req, act_str, did))
}

/// Run an operations tool. Authorization has already happened — the caller (agent
/// loop, MCP interceptor or HTTP route) checked `ops_request` first.
pub async fn dispatch(storage: &dyn StorageBackend, by: &str, tool: &str, args: &Value) -> Option<Result<Value, String>> {
    let s = |k: &str| args.get(k).and_then(|v| v.as_str()).unwrap_or_default().to_string();
    let r = match tool {
        "list_deployments" => Ok(overview(storage).await),
        "probe_deployment" => probe(storage, &s("deployment")).await,
        "pause_deployment" => pause(storage, &s("deployment"), by, args.get("reason").and_then(|v| v.as_str())).await,
        "resume_deployment" => resume(storage, &s("deployment"), by).await,
        "restart_deployment" => restart(storage, &s("deployment"), by).await,
        "undeploy" => undeploy(storage, &s("deployment"), by).await,
        "terminate_deployment" => terminate(storage, &s("deployment"), by).await,
        "terminate_runtime" => {
            let confirm = args.get("confirm").and_then(|v| v.as_str());
            terminate_runtime(storage, &s("runtime"), by, confirm).await
        }
        "set_runtime_state" => {
            let state = match s("state").as_str() {
                "active" => RuntimeState::Active,
                "hibernated" => RuntimeState::Hibernated,
                "paused" => RuntimeState::Paused,
                other => return Some(Err(format!("state must be active, hibernated or paused, got '{other}'"))),
            };
            set_runtime_state(storage, &s("runtime"), state, by).await
        }
        "set_paused_response" => {
            let code = args.get("status").and_then(|v| v.as_u64()).map(|c| c as u16);
            set_paused_response(storage, &s("runtime"), code).await
        }
        _ => return None,
    };
    Some(r)
}

pub const OPS_TOOLS: &[&str] = &[
    "list_deployments",
    "probe_deployment",
    "pause_deployment",
    "resume_deployment",
    "restart_deployment",
    "undeploy",
    "terminate_deployment",
    "terminate_runtime",
    "set_runtime_state",
    "set_paused_response",
];

// ============================================================================
// Views — overview, probe, metrics
// ============================================================================

#[derive(Default, Serialize)]
#[serde(rename_all = "camelCase")]
struct Stats {
    /// Runs in the sampled window (the most recent executions, up to 200).
    runs: usize,
    in_flight: usize,
    failed: usize,
    /// Requests refused because of a pause — not failures of the flow, so they
    /// are kept out of `failed` and the error rate.
    refused: usize,
    completed: usize,
    runs_last_hour: usize,
    error_rate: f64,
    p50_ms: Option<u64>,
    p95_ms: Option<u64>,
    last_run_at: Option<DateTime<Utc>>,
    last_error: Option<String>,
}

fn stats_of(runs: &[&flowr_types::ExecutionState]) -> Stats {
    let hour_ago = Utc::now() - chrono::Duration::hours(1);
    let mut s = Stats { runs: runs.len(), ..Default::default() };
    let mut durs: Vec<u64> = Vec::new();
    for e in runs {
        match e.status {
            ExecutionStatus::Queued | ExecutionStatus::Running | ExecutionStatus::Suspended => s.in_flight += 1,
            ExecutionStatus::Failed if is_refusal(e) => s.refused += 1,
            ExecutionStatus::Failed => s.failed += 1,
            ExecutionStatus::Completed => s.completed += 1,
            ExecutionStatus::Cancelled => {}
        }
        if e.started_at > hour_ago {
            s.runs_last_hour += 1;
        }
        if let Some(d) = e.duration_us {
            if e.status == ExecutionStatus::Completed {
                durs.push(d / 1000);
            }
        }
        if s.last_run_at.map_or(true, |t| e.started_at > t) {
            s.last_run_at = Some(e.started_at);
        }
    }
    if let Some(e) = runs
        .iter()
        .filter(|e| e.status == ExecutionStatus::Failed && !is_refusal(e))
        .max_by_key(|e| e.started_at)
    {
        s.last_error = e.error.clone();
    }
    let finished = s.failed + s.completed;
    if finished > 0 {
        s.error_rate = s.failed as f64 / finished as f64;
    }
    durs.sort_unstable();
    let pct = |p: f64| durs.get(((durs.len() as f64 - 1.0) * p).round() as usize).copied();
    if !durs.is_empty() {
        s.p50_ms = pct(0.5);
        s.p95_ms = pct(0.95);
    }
    s
}

fn is_refusal(e: &flowr_types::ExecutionState) -> bool {
    e.error.as_deref().is_some_and(|m| m.starts_with("refused: "))
}

/// The runs that belong to a deployment, out of a shared sample.
fn runs_for<'a>(d: &Deployment, flow_runs: &'a [flowr_types::ExecutionState], recent: &'a [flowr_types::ExecutionState]) -> Vec<&'a flowr_types::ExecutionState> {
    if d.kind == "agent" {
        recent
            .iter()
            .filter(|e| {
                e.input
                    .get("session_id")
                    .and_then(|v| v.as_str())
                    .and_then(agent_of_session)
                    .is_some_and(|a| a == d.target)
            })
            .collect()
    } else {
        flow_runs.iter().collect()
    }
}

async fn flow_runs(storage: &dyn StorageBackend, d: &Deployment, limit: usize) -> Vec<flowr_types::ExecutionState> {
    if d.kind == "agent" {
        return Vec::new();
    }
    let Some((ns, name)) = d.flow_ref.split_once('/') else { return Vec::new() };
    storage.executions().list_by_workflow(ns, name, limit).await.unwrap_or_default()
}

/// What a request for this deployment would get right now.
fn serving_state(d: &Deployment, rt: Option<&RuntimeSpec>) -> &'static str {
    if d.is_paused() {
        "paused"
    } else if rt.is_none() {
        "unplaced"
    } else {
        match rt.map(|r| r.state) {
            Some(RuntimeState::Paused) => "runtime-paused",
            Some(RuntimeState::Hibernated) => "hibernated",
            _ => "serving",
        }
    }
}

/// Every runtime with what is deployed on it, live activity and host metrics.
pub async fn overview(storage: &dyn StorageBackend) -> Value {
    load(storage).await;
    let (deployments, runtimes) = {
        let l = ledger().read().unwrap();
        (l.deployments.values().cloned().collect::<Vec<_>>(), l.runtimes.clone())
    };
    let need_recent = deployments.iter().any(|d| d.kind == "agent");
    let recent = if need_recent {
        storage.executions().list_recent(500).await.unwrap_or_default()
    } else {
        Vec::new()
    };

    let mut rows: Vec<(String, Value)> = Vec::new();
    for d in &deployments {
        let fr = flow_runs(storage, d, 200).await;
        let runs = runs_for(d, &fr, &recent);
        let rt = runtimes.get(&d.runtime);
        rows.push((
            d.runtime.clone(),
            json!({
                "deployment": d,
                "serving": serving_state(d, rt),
                "callersGetWhenPaused": paused_status(d.paused_response.or(rt.and_then(|r| r.paused_response))),
                "stats": stats_of(&runs),
            }),
        ));
    }

    let mut names: Vec<&String> = runtimes.keys().collect();
    names.sort();
    let host = host_metrics();
    let rts: Vec<Value> = names
        .into_iter()
        .map(|n| {
            let r = &runtimes[n];
            let deps: Vec<&Value> = rows.iter().filter(|(rt, _)| rt == n).map(|(_, v)| v).collect();
            json!({
                "runtime": r,
                "state": r.state,
                "pausedResponse": paused_status(r.paused_response),
                "deployments": deps,
                "host": host,
            })
        })
        .collect();
    // Deployments whose runtime was deleted after they were placed.
    let orphans: Vec<&Value> = rows
        .iter()
        .filter(|(rt, _)| !runtimes.contains_key(rt))
        .map(|(_, v)| v)
        .collect();

    json!({
        "runtimes": rts,
        "unplaced": orphans,
        "host": host,
        // "none" | "reachable" | "unreachable". Configured is not the same as
        // working: on a cluster where the controller is not running, claiming
        // "EE orchestration" would be the console lying about isolation.
        "controller": controller_status().await,
    })
}

/// Whether the EE controller answers, cached for 30s so the console's polling
/// does not turn into a health-check flood.
async fn controller_status() -> &'static str {
    static CACHE: OnceLock<std::sync::Mutex<Option<(std::time::Instant, &'static str)>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| std::sync::Mutex::new(None));
    if let Some((t, v)) = *cache.lock().unwrap() {
        if t.elapsed() < std::time::Duration::from_secs(30) {
            return v;
        }
    }
    let v = match std::env::var("A3T_CONTROLLER_URL").ok().filter(|u| !u.is_empty()) {
        None => "none",
        Some(url) => {
            let mut req = reqwest::Client::new()
                .get(format!("{}/v1/runtimes", url.trim_end_matches('/')))
                .timeout(std::time::Duration::from_millis(1500));
            if let Ok(tok) = std::env::var("A3T_CONTROLLER_TOKEN") {
                if !tok.is_empty() {
                    req = req.bearer_auth(tok);
                }
            }
            match req.send().await {
                Ok(r) if r.status().is_success() => "reachable",
                _ => "unreachable",
            }
        }
    };
    *cache.lock().unwrap() = Some((std::time::Instant::now(), v));
    v
}

/// A live look at one deployment: is it serving, what it has been doing, which
/// triggers feed it, and whether what it depends on still resolves.
pub async fn probe(storage: &dyn StorageBackend, id: &str) -> Result<Value, String> {
    load(storage).await;
    let d = resolve(id).ok_or_else(|| not_found(id))?;
    let rt = ledger().read().unwrap().runtimes.get(&d.runtime).cloned();

    let fr = flow_runs(storage, &d, 200).await;
    let recent = if d.kind == "agent" {
        storage.executions().list_recent(500).await.unwrap_or_default()
    } else {
        Vec::new()
    };
    let mut runs = runs_for(&d, &fr, &recent);
    let stats = stats_of(&runs);
    runs.sort_by(|a, b| b.started_at.cmp(&a.started_at));
    let last: Vec<Value> = runs
        .iter()
        .take(20)
        .map(|e| {
            json!({
                "instanceId": e.instance_id,
                "flow": format!("{}/{}", e.namespace, e.name),
                "status": e.status,
                "startedAt": e.started_at,
                "durationMs": e.duration_us.map(|d| d / 1000),
                "currentStep": e.current_step,
                "error": e.error,
            })
        })
        .collect();

    // Triggers that feed this deployment: workflow targets for a flow; for an
    // agent, triggers named after it (the convention delete_agent relies on) or
    // targeting it directly.
    let triggers: Vec<Value> = flowr_ce_agents::trigger_manager()
        .list()
        .await
        .into_iter()
        .filter(|t| {
            t.targets.iter().any(|tg| match tg {
                flowr_ce_agents::TriggerTarget::Workflow { namespace, name, .. } => {
                    d.kind == "flow" && format!("{namespace}/{name}") == d.target
                }
                flowr_ce_agents::TriggerTarget::Agent { agent_id } => d.kind == "agent" && *agent_id == d.target,
                _ => false,
            }) || (d.kind == "agent" && t.name.starts_with(&d.target))
        })
        .map(|t| serde_json::to_value(&t).unwrap_or(json!({})))
        .collect();

    // Health: does what this deployment depends on still resolve?
    let mut checks = Vec::new();
    if let Some((ns, name)) = d.flow_ref.split_once('/') {
        let ok = storage.workflows().get(ns, name).await.is_ok();
        checks.push(json!({ "check": "flow exists", "target": d.flow_ref, "ok": ok }));
        let canvas_ok = crate::canvas::resolve_canvas(storage, ns, name).await.is_ok();
        checks.push(json!({ "check": "canvas resolves", "target": d.canvas, "ok": canvas_ok }));
        let placed = crate::canvas::placement_target(storage, ns, name, None).await;
        checks.push(json!({
            "check": "placement still matches",
            "ok": placed.as_ref().is_some_and(|(r, ..)| r.name == d.runtime),
            "detail": placed.map(|(r, ..)| format!("canvas now resolves to {}", r.name)).unwrap_or_else(|| "no runtime matches".into()),
        }));
    }
    checks.push(json!({ "check": "runtime registered", "target": d.runtime, "ok": rt.is_some() }));

    Ok(json!({
        "deployment": d,
        "serving": serving_state(&d, rt.as_ref()),
        "runtime": rt,
        "callersGetWhenPaused": effective_paused_status(&d),
        "stats": stats,
        "recentRuns": last,
        "triggers": triggers,
        "checks": checks,
        "host": host_metrics(),
    }))
}

/// Resource usage of the process hosting the runtimes, from cgroup v2 and the
/// data volume. In CE every runtime is a placement label inside this one pod, so
/// these figures are SHARED — they are labelled as such and never apportioned
/// into per-runtime numbers that would be invented.
pub fn host_metrics() -> Value {
    let read = |f: &str| std::fs::read_to_string(format!("/sys/fs/cgroup/{f}")).ok();
    let mem_used = read("memory.current").and_then(|s| s.trim().parse::<u64>().ok());
    let mem_limit = read("memory.max").and_then(|s| s.trim().parse::<u64>().ok());
    let cpu_usage_us = read("cpu.stat").and_then(|s| {
        s.lines()
            .find_map(|l| l.strip_prefix("usage_usec ").and_then(|v| v.trim().parse::<u64>().ok()))
    });
    let cpu_limit_cores = read("cpu.max").and_then(|s| {
        let mut it = s.split_whitespace();
        let quota = it.next()?.parse::<f64>().ok()?;
        let period = it.next()?.parse::<f64>().ok()?;
        (period > 0.0).then(|| quota / period)
    });

    // CPU % needs two samples; keep the previous one and report the rate since.
    static LAST: OnceLock<std::sync::Mutex<Option<(std::time::Instant, u64)>>> = OnceLock::new();
    let cpu_cores_used = cpu_usage_us.and_then(|now_us| {
        let mut last = LAST.get_or_init(|| std::sync::Mutex::new(None)).lock().unwrap();
        let now = std::time::Instant::now();
        let rate = last.and_then(|(t, us)| {
            let el = now.duration_since(t).as_micros() as f64;
            (el > 100_000.0 && now_us >= us).then(|| (now_us - us) as f64 / el)
        });
        *last = Some((now, now_us));
        rate
    });

    let data_dir = std::env::var("CLAWDFLOW_DATA_DIR").unwrap_or_else(|_| "/".to_string());
    let disk = disk_usage(&data_dir);

    json!({
        "scope": "shared-host",
        "host": std::env::var("HOSTNAME").unwrap_or_default(),
        "note": "CE runs every runtime inside this one process — these figures are shared by all of them.",
        "memory": { "usedBytes": mem_used, "limitBytes": mem_limit },
        "cpu": { "coresUsed": cpu_cores_used, "limitCores": cpu_limit_cores },
        "disk": disk.map(|(total, free)| json!({ "path": data_dir, "totalBytes": total, "freeBytes": free, "usedBytes": total.saturating_sub(free) })),
    })
}

fn disk_usage(path: &str) -> Option<(u64, u64)> {
    let c = std::ffi::CString::new(path).ok()?;
    let mut st: libc::statvfs = unsafe { std::mem::zeroed() };
    // SAFETY: `c` is a valid NUL-terminated path and `st` a properly sized out-param.
    let rc = unsafe { libc::statvfs(c.as_ptr(), &mut st) };
    if rc != 0 {
        return None;
    }
    let frsize = st.f_frsize as u64;
    Some((st.f_blocks as u64 * frsize, st.f_bavail as u64 * frsize))
}

// ============================================================================
// HTTP — the Runtime console's API
// ============================================================================

struct OpsState {
    storage: Arc<dyn StorageBackend>,
}

pub fn ops_router(storage: Arc<dyn StorageBackend>) -> Router {
    Router::new()
        .route("/overview", get(http_overview))
        .route("/deployments/:id/probe", get(http_probe))
        .route("/deployments/:id/:action", post(http_deployment_action))
        .route("/runtimes/:name/:action", get(http_runtime_preview).post(http_runtime_action))
        .route("/runtimes/:name/paused-response", put(http_paused_response))
        .with_state(Arc::new(OpsState { storage }))
}

async fn http_overview(State(s): State<Arc<OpsState>>) -> impl IntoResponse {
    Json(overview(&*s.storage).await)
}

async fn http_probe(State(s): State<Arc<OpsState>>, Path(id): Path<String>) -> impl IntoResponse {
    match probe(&*s.storage, &id).await {
        Ok(v) => Json(v).into_response(),
        Err(e) => (StatusCode::NOT_FOUND, Json(json!({ "error": e }))).into_response(),
    }
}

/// Authorize and run a write from the console. There is no approver on an HTTP
/// call, so an Ask degrades to a denial — the same rule as the deploy routes.
async fn http_run(
    s: &OpsState,
    principal: Option<axum::Extension<flowr_policy::Principal>>,
    tool: &str,
    args: Value,
) -> axum::response::Response {
    let Some(axum::Extension(principal)) = principal else {
        return (StatusCode::UNAUTHORIZED, Json(json!({ "error": "authentication required" }))).into_response();
    };
    if let Some((req, ..)) = ops_request(&principal, tool, &args) {
        let decision = flowr_policy::policy_hook().check(&req).await;
        flowr_policy::audit::record(&req, &decision);
        if let flowr_policy::PolicyDecision::Deny { reason, policy_ids } = decision.non_interactive() {
            return (
                StatusCode::FORBIDDEN,
                Json(json!({ "error": "forbidden", "reason": reason, "policies": policy_ids })),
            )
                .into_response();
        }
    }
    match dispatch(&*s.storage, &principal.id(), tool, &args).await {
        Some(Ok(v)) => Json(v).into_response(),
        Some(Err(e)) => (StatusCode::UNPROCESSABLE_ENTITY, Json(json!({ "error": e }))).into_response(),
        None => (StatusCode::NOT_FOUND, Json(json!({ "error": format!("unknown operation {tool}") }))).into_response(),
    }
}

async fn http_deployment_action(
    State(s): State<Arc<OpsState>>,
    principal: Option<axum::Extension<flowr_policy::Principal>>,
    Path((id, action)): Path<(String, String)>,
    body: Option<Json<Value>>,
) -> axum::response::Response {
    let tool = match action.as_str() {
        "pause" => "pause_deployment",
        "resume" => "resume_deployment",
        "restart" => "restart_deployment",
        "undeploy" => "undeploy",
        "terminate" => "terminate_deployment",
        other => return (StatusCode::NOT_FOUND, Json(json!({ "error": format!("unknown action {other}") }))).into_response(),
    };
    let mut args = body.map(|Json(b)| b).unwrap_or_else(|| json!({}));
    if !args.is_object() {
        args = json!({});
    }
    args["deployment"] = json!(id);
    http_run(&s, principal, tool, args).await
}

/// `GET /ops/runtimes/:name/terminate` — what terminating would remove.
async fn http_runtime_preview(
    State(s): State<Arc<OpsState>>,
    Path((name, action)): Path<(String, String)>,
) -> axum::response::Response {
    if action != "terminate" {
        return (StatusCode::NOT_FOUND, Json(json!({ "error": format!("unknown action {action}") }))).into_response();
    }
    match terminate_runtime(&*s.storage, &name, "", None).await {
        Ok(v) => Json(v).into_response(),
        Err(e) => (StatusCode::NOT_FOUND, Json(json!({ "error": e }))).into_response(),
    }
}

async fn http_runtime_action(
    State(s): State<Arc<OpsState>>,
    principal: Option<axum::Extension<flowr_policy::Principal>>,
    Path((name, action)): Path<(String, String)>,
    body: Option<Json<Value>>,
) -> axum::response::Response {
    if action == "terminate" {
        let confirm = body.and_then(|Json(b)| b.get("confirm").cloned()).unwrap_or(Value::Null);
        return http_run(&s, principal, "terminate_runtime", json!({ "runtime": name, "confirm": confirm })).await;
    }
    let state = match action.as_str() {
        "hibernate" => "hibernated",
        "pause" => "paused",
        "resume" => "active",
        other => return (StatusCode::NOT_FOUND, Json(json!({ "error": format!("unknown action {other}") }))).into_response(),
    };
    http_run(&s, principal, "set_runtime_state", json!({ "runtime": name, "state": state })).await
}

async fn http_paused_response(
    State(s): State<Arc<OpsState>>,
    principal: Option<axum::Extension<flowr_policy::Principal>>,
    Path(name): Path<String>,
    Json(body): Json<Value>,
) -> axum::response::Response {
    http_run(&s, principal, "set_paused_response", json!({ "runtime": name, "status": body.get("status") })).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_deterministic_and_registry_safe() {
        assert_eq!(Deployment::id_for("flow", "a3t/orders"), "flow.a3t.orders");
        assert_eq!(Deployment::id_for("agent", "agent-12ab"), "agent.agent-12ab");
    }

    #[test]
    fn agent_comes_from_the_session_prefix() {
        assert_eq!(agent_of_session("agent-1:session-9"), Some("agent-1"));
        assert_eq!(agent_of_session("agent-1"), Some("agent-1"));
        assert_eq!(agent_of_session(""), None);
        assert_eq!(agent_id_of("a3t/agent-1"), "agent-1");
    }

    #[test]
    fn paused_response_only_ever_503_or_404() {
        assert_eq!(paused_status(None), 503);
        assert_eq!(paused_status(Some(404)), 404);
        assert_eq!(paused_status(Some(200)), 503);
    }
}
