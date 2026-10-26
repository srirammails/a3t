//! Reconcile logic for Canvas + Runtime CRs. Both turn into an Agent Sandbox
//! `Sandbox` (agents.x-k8s.io/v1beta1) via [`apply_sandbox`] — the single seam
//! where A3T orchestrates the sandbox substrate (isolation + hibernate/resume).
//!
//! - **Runtime** → a shared `Sandbox` pool (`rt-<name>`) many flows load into.
//!   `spec.state` → the Sandbox `operatingMode` (Running | Suspended = hibernated).
//! - **Canvas**, `mode: isolated` → its OWN `Sandbox` (`cv-<name>`) per flow,
//!   inheriting the target runtime's image + isolation settings; `lifecycle.state`
//!   → operatingMode, `lifecycle.session` → the flow's `A3T_SESSION_MODE`.
//! - **Canvas**, `mode: shared` → resolve placement onto a Runtime and write status
//!   (the flow physically loads into that pool via the deferred dispatch plane).
//!
//! Created Sandboxes carry an owner reference to their CR, so they are
//! garbage-collected when the CR is deleted. Applied via server-side apply.

use std::collections::BTreeMap;
use std::sync::Arc;

use k8s_openapi::apimachinery::pkg::apis::meta::v1::OwnerReference;
use kube::api::{DynamicObject, GroupVersionKind, Patch, PatchParams};
use kube::core::ApiResource;
use kube::{Api, Client, Resource, ResourceExt};
use tracing::info;

use crate::crd::{
    select_runtime, validate_canvas, Canvas, CanvasStatus, LifecycleState, PlacementMode, Runtime,
    RuntimeStatus, SessionMode,
};

pub struct ReconcileContext {
    pub client: Client,
}

#[derive(thiserror::Error, Debug)]
pub enum ReconcileError {
    #[error("kube error: {0}")]
    Kube(#[from] kube::Error),
    #[error("{0}")]
    Msg(String),
}

const DEFAULT_IMAGE: &str = "ghcr.io/srirammails/flowr:latest";
const PORT: i32 = 3002;

/// The runtime image when a Runtime CR names none: `A3T_RUNTIME_IMAGE`, else
/// [`DEFAULT_IMAGE`].
fn default_image() -> String {
    std::env::var("A3T_RUNTIME_IMAGE").ok().filter(|i| !i.is_empty()).unwrap_or_else(|| DEFAULT_IMAGE.to_string())
}

/// Env every runtime pod gets from the controller's own `A3T_RUNTIME_ENV_<NAME>`
/// variables, as `<NAME>` — how runtimes learn where ADB is (ADB MVP W2b), e.g.
/// `A3T_RUNTIME_ENV_FLOWR_REGISTRY_ADB=http://adb.a3t.svc.cluster.local:3000`.
/// Secret env for runtime pods from the controller's
/// `A3T_RUNTIME_SECRET_ENV_<NAME>=<secret>:<key>` variables: `<NAME>` from that
/// Secret (in the sandbox's namespace) by `secretKeyRef`, so the value never sits
/// in a Sandbox spec — e.g. `A3T_RUNTIME_SECRET_ENV_ADB_AUTH_TOKEN=adb-auth:token`.
/// Optional: a missing Secret leaves the variable unset instead of failing the pod.
fn runtime_secret_env() -> Vec<serde_json::Value> {
    let mut env: Vec<(String, String, String)> = std::env::vars()
        .filter_map(|(k, v)| {
            let name = k.strip_prefix("A3T_RUNTIME_SECRET_ENV_").filter(|n| !n.is_empty())?;
            let (secret, key) = v.split_once(':')?;
            Some((name.to_string(), secret.to_string(), key.to_string()))
        })
        .collect();
    env.sort();
    env.into_iter()
        .map(|(name, secret, key)| serde_json::json!({ "name": name, "valueFrom": { "secretKeyRef": { "name": secret, "key": key, "optional": true } } }))
        .collect()
}

fn runtime_env() -> Vec<(String, String)> {
    let mut env: Vec<(String, String)> = std::env::vars()
        .filter_map(|(k, v)| k.strip_prefix("A3T_RUNTIME_ENV_").filter(|n| !n.is_empty()).map(|n| (n.to_string(), v)))
        .collect();
    env.sort();
    env
}

// ────────────────────────────── Runtime ──────────────────────────────

/// Runtime reconcile: emit Deployment + Service + HPA (+ base IngressRoute).
pub async fn reconcile_runtime(
    runtime: Arc<Runtime>,
    ctx: Arc<ReconcileContext>,
) -> Result<(), ReconcileError> {
    let cr_ns = runtime.namespace().unwrap_or_else(|| "default".into());
    let ns = if runtime.spec.namespace.is_empty() { cr_ns.clone() } else { runtime.spec.namespace.clone() };
    let name = runtime.name_any();
    let sb_name = format!("rt-{name}");
    let owner = runtime.controller_owner_ref(&()).into_iter().collect::<Vec<_>>();
    let labels = runtime_labels(&name, &runtime.spec.labels);
    let image = if runtime.spec.image.is_empty() { default_image() } else { runtime.spec.image.clone() };

    // A3T orchestrates Agent Sandbox: a Runtime → a `Sandbox` (isolated, hibernatable).
    // `state` maps to the Sandbox `operatingMode` (Running | Suspended → 0 pods).
    let operating_mode = match runtime.spec.state.to_lowercase().as_str() {
        "suspended" | "paused" | "hibernated" => "Suspended",
        _ => "Running",
    };

    // A `Runtime` pool = a shared Sandbox many flows load into.
    let (ready, fqdn) = apply_sandbox(
        &ctx.client, &ns, &sb_name, &image, operating_mode,
        &runtime.spec.runtime_class_name, &runtime.spec.node_selector,
        &[], &labels, &owner,
    ).await?;
    let phase = if operating_mode == "Suspended" { "Hibernated" } else if ready { "Ready" } else { "Deploying" };
    let running = if operating_mode == "Suspended" { 0 } else { 1 };
    info!(runtime = %name, namespace = %ns, sandbox = %sb_name, operating_mode, ready, "reconciled runtime → Agent Sandbox");
    let status = RuntimeStatus {
        phase: phase.to_string(),
        replicas: running,
        ready_replicas: if ready { 1 } else { 0 },
        message: format!("sandbox {sb_name} ({operating_mode}) {fqdn}"),
    };
    patch_status::<Runtime>(&ctx.client, &cr_ns, &name, serde_json::json!({ "status": status })).await
}

fn runtime_labels(name: &str, extra: &BTreeMap<String, String>) -> BTreeMap<String, String> {
    let mut l = BTreeMap::new();
    l.insert("app".into(), format!("rt-{name}"));
    l.insert("a3t.io/runtime".into(), name.to_string());
    for (k, v) in extra { l.insert(k.clone(), v.clone()); }
    l
}

// ────────────────────────────── Canvas ──────────────────────────────

/// Canvas reconcile: validate, resolve the target runtime, write status.
pub async fn reconcile_canvas(
    canvas: Arc<Canvas>,
    ctx: Arc<ReconcileContext>,
) -> Result<(), ReconcileError> {
    let ns = canvas.namespace().unwrap_or_else(|| "default".into());
    let name = canvas.name_any();

    let gen = canvas.metadata.generation.unwrap_or(0);
    let owner = canvas.controller_owner_ref(&()).into_iter().collect::<Vec<_>>();
    let problems = validate_canvas(&canvas.spec);

    let status = if !problems.is_empty() {
        CanvasStatus {
            phase: "Degraded".into(),
            health: "Degraded".into(),
            message: problems.join("; "),
            observed_generation: gen,
            ..Default::default()
        }
    } else if matches!(canvas.spec.placement.mode, PlacementMode::Isolated) {
        // Isolated: the flow gets its OWN Sandbox (a new sandbox per flow), not a
        // slot in a shared pool. It inherits the flowr image + isolation settings
        // from the runtime its placement would have targeted.
        let sb_name = format!("cv-{name}");
        let mut labels = BTreeMap::new();
        labels.insert("app".into(), sb_name.clone());
        labels.insert("a3t.io/canvas".into(), name.clone());
        labels.insert("a3t.io/flow".into(), canvas.spec.flow_ref.replace('/', "."));

        let rt_api: Api<Runtime> = Api::all(ctx.client.clone());
        let runtimes = rt_api.list(&Default::default()).await?.items;
        let target = select_runtime(&canvas.spec.placement, &runtimes);
        let image = target.map(|r| r.spec.image.clone()).filter(|i| !i.is_empty()).unwrap_or_else(default_image);
        let runtime_class = target.map(|r| r.spec.runtime_class_name.clone()).unwrap_or_default();
        let node_selector = target.map(|r| r.spec.node_selector.clone()).unwrap_or_default();

        // lifecycle.state Running|Paused → Sandbox Running|Suspended (hibernated).
        let operating_mode = if matches!(canvas.spec.lifecycle.state, LifecycleState::Paused) { "Suspended" } else { "Running" };
        let session = if matches!(canvas.spec.lifecycle.session, SessionMode::Persistent) { "persistent" } else { "ephemeral" };
        // The runtime reads its flow from the shared registry, runs it once per
        // message on the canvas topic, and finds ADB through the runtime env.
        let extra_env = [
            ("A3T_CANVAS", format!("{ns}/{name}")),
            ("A3T_FLOW", canvas.spec.flow_ref.clone()),
            ("A3T_SESSION_MODE", session.to_string()),
        ];
        let (ready, fqdn) = apply_sandbox(
            &ctx.client, &ns, &sb_name, &image, operating_mode,
            &runtime_class, &node_selector, &extra_env, &labels, &owner,
        ).await?;
        let phase = if operating_mode == "Suspended" { "Paused" } else if ready { "Synced" } else { "Deploying" };
        info!(canvas = %name, namespace = %ns, mode = "isolated", sandbox = %sb_name, session, ready, "reconciled canvas → per-flow Sandbox");
        CanvasStatus {
            phase: phase.into(),
            placed_runtime: format!("(isolated:{sb_name})"),
            replicas: if operating_mode == "Suspended" { 0 } else { 1 },
            ready_replicas: if ready { 1 } else { 0 },
            health: "Healthy".into(),
            message: format!("isolated sandbox {sb_name} ({operating_mode}, {session}) {fqdn}"),
            observed_generation: gen,
            ..Default::default()
        }
    } else {
        // Shared: resolve the target runtime (routing entry = future work).
        let rt_api: Api<Runtime> = Api::all(ctx.client.clone());
        let runtimes = rt_api.list(&Default::default()).await?.items;
        let (phase, placed_runtime, message) = match select_runtime(&canvas.spec.placement, &runtimes) {
            Some(rt) => ("Synced", rt.name_any(), format!("placed on runtime '{}'", rt.name_any())),
            None => ("Pending", String::new(), "no runtime matches placement selector".to_string()),
        };
        info!(canvas = %name, namespace = %ns, phase = %phase, runtime = %placed_runtime, "reconciled canvas");
        CanvasStatus {
            phase: phase.into(),
            placed_runtime,
            health: "Healthy".into(),
            message,
            observed_generation: gen,
            ..Default::default()
        }
    };

    patch_status::<Canvas>(&ctx.client, &ns, &name, serde_json::json!({ "status": status })).await
}

// ────────────────────────────── helpers ──────────────────────────────

/// Build + server-side-apply an Agent Sandbox `Sandbox` (agents.x-k8s.io/v1beta1)
/// holding one flowr worker, then read back its readiness. This is the single place
/// both a `Runtime` pool and an isolated `Canvas` turn into a real sandbox.
///
/// `operating_mode` is `"Running"` | `"Suspended"` (hibernated → 0 pods).
/// Returns `(ready, serviceFQDN)` from the Sandbox status.
#[allow(clippy::too_many_arguments)]
async fn apply_sandbox(
    client: &Client,
    ns: &str,
    name: &str,
    image: &str,
    operating_mode: &str,
    runtime_class: &str,
    node_selector: &BTreeMap<String, String>,
    extra_env: &[(&str, String)],
    labels: &BTreeMap<String, String>,
    owner: &[OwnerReference],
) -> Result<(bool, String), ReconcileError> {
    let mut env = vec![serde_json::json!({ "name": "RUST_LOG", "value": "info" })];
    for (k, v) in runtime_env() {
        env.push(serde_json::json!({ "name": k, "value": v }));
    }
    env.extend(runtime_secret_env());
    for (k, v) in extra_env {
        env.push(serde_json::json!({ "name": k, "value": v }));
    }
    let mut pod_spec = serde_json::json!({
        "containers": [{
            "name": "flowr",
            "image": image,
            "args": ["--mode", "http", "--port", PORT.to_string()],
            "ports": [{ "containerPort": PORT, "name": "http" }],
            "env": env,
        }]
    });
    if !runtime_class.is_empty() {
        pod_spec["runtimeClassName"] = serde_json::json!(runtime_class);
    }
    // Runtime images are private on GHCR: the pull secret (in the sandbox's
    // namespace) named by `A3T_RUNTIME_PULL_SECRET`.
    if let Some(secret) = std::env::var("A3T_RUNTIME_PULL_SECRET").ok().filter(|s| !s.is_empty()) {
        pod_spec["imagePullSecrets"] = serde_json::json!([{ "name": secret }]);
    }
    if !node_selector.is_empty() {
        pod_spec["nodeSelector"] = serde_json::json!(node_selector);
    }
    let manifest = serde_json::json!({
        "apiVersion": "agents.x-k8s.io/v1beta1",
        "kind": "Sandbox",
        "metadata": { "name": name, "namespace": ns, "labels": labels, "ownerReferences": owner },
        "spec": {
            "operatingMode": operating_mode,
            "service": true,
            "podTemplate": { "spec": pod_spec }
        }
    });
    let gvk = GroupVersionKind::gvk("agents.x-k8s.io", "v1beta1", "Sandbox");
    let ar = ApiResource::from_gvk(&gvk);
    let api: Api<DynamicObject> = Api::namespaced_with(client.clone(), ns, &ar);
    api.patch(name, &PatchParams::apply("a3t-controller").force(), &Patch::Apply(&manifest)).await?;

    let (ready, fqdn) = match api.get(name).await {
        Ok(obj) => {
            let ready = obj.data.get("status").and_then(|s| s.get("conditions")).and_then(|c| c.as_array())
                .map(|arr| arr.iter().any(|c|
                    c.get("type").and_then(|t| t.as_str()) == Some("Ready")
                    && c.get("status").and_then(|s| s.as_str()) == Some("True")))
                .unwrap_or(false);
            let fqdn = obj.data.get("status").and_then(|s| s.get("serviceFQDN")).and_then(|f| f.as_str()).unwrap_or("").to_string();
            (ready, fqdn)
        }
        Err(_) => (false, String::new()),
    };
    Ok((ready, fqdn))
}

async fn patch_status<K>(client: &Client, ns: &str, name: &str, patch: serde_json::Value) -> Result<(), ReconcileError>
where
    K: Resource<Scope = kube::core::NamespaceResourceScope>
        + Clone
        + std::fmt::Debug
        + serde::de::DeserializeOwned,
    <K as Resource>::DynamicType: Default,
{
    let api: Api<K> = Api::namespaced(client.clone(), ns);
    api.patch_status(name, &PatchParams::default(), &Patch::Merge(&patch)).await?;
    Ok(())
}
