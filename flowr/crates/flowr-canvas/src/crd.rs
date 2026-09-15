//! A3T Canvas + Runtime CRD type definitions (a3t.io/v1alpha1).
//!
//! These mirror `flowr-types::canvas` (the runtime-facing schema) but are the
//! **k8s-facing** types the controller reconciles. Kept self-contained (own
//! `JsonSchema` derives) so the CRD schema can evolve independently of the
//! runtime types and `flowr-types` needs no schemars dependency.
//!
//! - `Canvas`  = a flow's deployment metadata (flowRef + placement + lifecycle + health).
//! - `Runtime` = a labeled compute pool (a Deployment of the flowr worker) flows load into.
//!
//! Triggers are deliberately NOT here — they remain runtime artifacts
//! (see A3T_CANVAS_SPEC.md §11). Ingress/cron translation is future work.

use kube::CustomResource;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

// ============================================================================
// Canvas
// ============================================================================

/// `kind: Canvas` — deployment metadata for one flow.
#[derive(CustomResource, Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[kube(
    group = "a3t.io",
    version = "v1alpha1",
    kind = "Canvas",
    plural = "canvases",
    shortname = "cv",
    status = "CanvasStatus",
    namespaced,
    printcolumn = r#"{"name":"Flow","type":"string","jsonPath":".spec.flowRef"}"#,
    printcolumn = r#"{"name":"Mode","type":"string","jsonPath":".spec.placement.mode"}"#,
    printcolumn = r#"{"name":"Runtime","type":"string","jsonPath":".status.placedRuntime"}"#,
    printcolumn = r#"{"name":"Phase","type":"string","jsonPath":".status.phase"}"#
)]
#[serde(rename_all = "camelCase")]
pub struct CanvasSpec {
    /// The flow this canvas deploys — `namespace/name`.
    pub flow_ref: String,
    /// Human description (shown in the canvas picker).
    #[serde(default)]
    pub description: String,
    /// The agent this canvas is bound to (`namespace/id`); empty on presets.
    #[serde(default)]
    pub agent_ref: String,
    #[serde(default)]
    pub placement: Placement,
    #[serde(default)]
    pub lifecycle: Lifecycle,
    #[serde(default)]
    pub health: Health,
}

/// (a) Placement — which runtime pool the flow loads into, and how.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Placement {
    #[serde(default)]
    pub mode: PlacementMode,
    /// Labels a runtime pool must carry to host this flow.
    #[serde(default)]
    pub runtime_selector: BTreeMap<String, String>,
    /// Target k8s namespace (empty = the flow's namespace).
    #[serde(default)]
    pub namespace: String,
    /// Pod tolerations (for dedicated/tainted nodes; mainly `isolated`).
    #[serde(default)]
    pub tolerations: Vec<Toleration>,
}

impl Default for Placement {
    fn default() -> Self {
        Self {
            mode: PlacementMode::Shared,
            runtime_selector: BTreeMap::new(),
            namespace: String::new(),
            tolerations: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, Default, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum PlacementMode {
    /// Load into a shared runtime pool alongside other flows (default).
    #[default]
    Shared,
    /// Dedicated container/runtime for this flow (isolation).
    Isolated,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Toleration {
    pub key: String,
    #[serde(default)]
    pub value: String,
}

/// (b) Lifecycle — how the flow runs within its runtime(s).
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Lifecycle {
    #[serde(default)]
    pub state: LifecycleState,
    /// Whether the flow carries state across triggers.
    #[serde(default)]
    pub session: SessionMode,
    #[serde(default)]
    pub scaling: Scaling,
    #[serde(default)]
    pub restart: Restart,
}
impl Default for Lifecycle {
    fn default() -> Self {
        Self {
            state: LifecycleState::Running,
            session: SessionMode::Ephemeral,
            scaling: Scaling::default(),
            restart: Restart::default(),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, Default, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum LifecycleState {
    #[default]
    Running,
    Paused,
}

/// How the flow (agent) carries state between triggers.
///
/// - `ephemeral` (default): the sandbox is brought up on a trigger, the flow runs
///   to completion, then it's torn down — nothing is carried to the next trigger.
/// - `persistent`: the flow checkpoints its state (flowr `checkpoints`), the sandbox
///   hibernates (`operatingMode=Suspended`) when idle, and on the next trigger it
///   resumes and reloads the session state + new input.
///
/// Passed to the flow runtime as `A3T_SESSION_MODE`. The suspend-on-idle /
/// resume-on-trigger automation is the (deferred) dispatch plane.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, Default, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum SessionMode {
    #[default]
    Ephemeral,
    Persistent,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Scaling {
    #[serde(default = "one")]
    pub min: u32,
    #[serde(default = "one")]
    pub max: u32,
    #[serde(default)]
    pub metric: Option<String>,
    #[serde(default)]
    pub target: Option<u32>,
}
impl Default for Scaling {
    fn default() -> Self { Self { min: 1, max: 1, metric: None, target: None } }
}
fn one() -> u32 { 1 }

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Restart {
    #[serde(default = "default_restart_policy")]
    pub policy: String,
    #[serde(default = "default_backoff")]
    pub backoff: String,
    #[serde(default = "default_max_retries")]
    pub max_retries: u32,
}
impl Default for Restart {
    fn default() -> Self { Self { policy: default_restart_policy(), backoff: default_backoff(), max_retries: default_max_retries() } }
}
fn default_restart_policy() -> String { "on-failure".to_string() }
fn default_backoff() -> String { "exponential".to_string() }
fn default_max_retries() -> u32 { 5 }

/// (c) Health — dependent resources to watch + the action on failure.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, Default)]
#[serde(rename_all = "camelCase")]
pub struct Health {
    /// Resources the flow depends on (auto-derived from the flow's tasks when empty).
    #[serde(default)]
    pub depends_on: Vec<DependsOn>,
    #[serde(default)]
    pub on_dependency_down: OnDependencyDown,
    #[serde(default)]
    pub circuit: Option<Circuit>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct DependsOn {
    /// `namespace/name` of a resource (llm, db, mcp, …).
    pub resource: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, Default, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum OnDependencyDown {
    #[default]
    Queue,
    Cordon,
    CircuitBreak,
    Fail,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Circuit {
    pub error_threshold: f64,
    pub window: String,
    #[serde(default = "default_circuit_action")]
    pub action: String,
}
fn default_circuit_action() -> String { "open".to_string() }

/// Controller-written status for a Canvas.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, Default)]
#[serde(rename_all = "camelCase")]
pub struct CanvasStatus {
    /// Pending|Deploying|Synced|Drifted|Paused|Degraded
    #[serde(default)]
    pub phase: String,
    #[serde(default)]
    pub placed_runtime: String,
    #[serde(default)]
    pub observed_generation: i64,
    #[serde(default)]
    pub replicas: u32,
    #[serde(default)]
    pub ready_replicas: u32,
    /// Healthy|Degraded|DependencyDown
    #[serde(default)]
    pub health: String,
    #[serde(default)]
    pub drift: bool,
    #[serde(default)]
    pub message: String,
}

// ============================================================================
// Runtime
// ============================================================================

/// `kind: Runtime` — a labeled compute pool (a Deployment of the flowr worker)
/// that hosts many flows. Placement matches a flow's `runtimeSelector` to these
/// labels.
#[derive(CustomResource, Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[kube(
    group = "a3t.io",
    version = "v1alpha1",
    kind = "Runtime",
    plural = "runtimes",
    shortname = "rt",
    status = "RuntimeStatus",
    namespaced,
    printcolumn = r#"{"name":"Default","type":"boolean","jsonPath":".spec.default"}"#,
    printcolumn = r#"{"name":"Ready","type":"string","jsonPath":".status.readyReplicas"}"#,
    printcolumn = r#"{"name":"Phase","type":"string","jsonPath":".status.phase"}"#
)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeSpec {
    /// Placement labels (`class`, `tier`, `zone`, …) a canvas selector matches.
    #[serde(default)]
    pub labels: BTreeMap<String, String>,
    /// k8s namespace the pool's worker pods run in.
    #[serde(default)]
    pub namespace: String,
    /// Replica bounds for the worker Deployment / HPA.
    #[serde(default)]
    pub scaling: RuntimeScaling,
    /// Node placement for the worker pods.
    #[serde(default)]
    pub node_selector: BTreeMap<String, String>,
    /// The worker container image (defaults to the running flowr image if empty).
    #[serde(default)]
    pub image: String,
    /// Sandbox runtime class (e.g. "gvisor"/runsc). Empty → the node default (runc).
    #[serde(default)]
    pub runtime_class_name: String,
    /// Desired state: "running" (default) or "suspended" (hibernated → 0 pods).
    /// Maps to the Agent Sandbox `spec.operatingMode`.
    #[serde(default = "default_state")]
    pub state: String,
    /// Explicit base URL (for CE mapping / addressing). Optional.
    #[serde(default)]
    pub url: Option<String>,
    /// The pool used when a flow declares no selector.
    #[serde(default)]
    pub default: bool,
}

fn default_state() -> String { "running".to_string() }

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct RuntimeScaling {
    #[serde(default = "one")]
    pub min: u32,
    #[serde(default = "one")]
    pub max: u32,
}
impl Default for RuntimeScaling {
    fn default() -> Self { Self { min: 1, max: 1 } }
}

/// Controller-written status for a Runtime.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, Default)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeStatus {
    /// Pending|Deploying|Ready|Degraded
    #[serde(default)]
    pub phase: String,
    #[serde(default)]
    pub replicas: u32,
    #[serde(default)]
    pub ready_replicas: u32,
    #[serde(default)]
    pub message: String,
}

/// Select the runtime a flow's placement resolves to (controller-side mirror of
/// `flowr-types::canvas::select_runtime`): a runtime matches when it carries every
/// label in the selector (and namespace, if set). Empty selector → the `default`
/// pool, else the first available.
pub fn select_runtime<'a>(
    placement: &Placement,
    runtimes: &'a [Runtime],
) -> Option<&'a Runtime> {
    let ns_ok = |r: &Runtime| placement.namespace.is_empty() || r.spec.namespace == placement.namespace;
    if placement.runtime_selector.is_empty() {
        return runtimes
            .iter()
            .filter(|r| ns_ok(r))
            .find(|r| r.spec.default)
            .or_else(|| runtimes.iter().find(|r| ns_ok(r)));
    }
    runtimes.iter().filter(|r| ns_ok(r)).find(|r| {
        placement
            .runtime_selector
            .iter()
            .all(|(k, v)| r.spec.labels.get(k).map(|rv| rv == v).unwrap_or(false))
    })
}

/// Validate a CanvasSpec. Returns the list of problems (empty = valid).
pub fn validate_canvas(spec: &CanvasSpec) -> Vec<String> {
    let mut errs = Vec::new();
    if spec.flow_ref.is_empty() {
        errs.push("spec.flowRef is required".into());
    } else if spec.flow_ref.split_once('/').is_none() {
        errs.push(format!("spec.flowRef must be 'namespace/name', got '{}'", spec.flow_ref));
    }
    if spec.lifecycle.scaling.max < spec.lifecycle.scaling.min {
        errs.push("lifecycle.scaling.max must be >= min".into());
    }
    for d in &spec.health.depends_on {
        if d.resource.split_once('/').is_none() {
            errs.push(format!("health.dependsOn.resource must be 'namespace/name', got '{}'", d.resource));
        }
    }
    errs
}
