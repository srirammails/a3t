//! Canvas — a flow's **deployment metadata** (ops, not logic).
//!
//! See `flowr-system/.ai/A3T_CANVAS_SPEC.md`. A Canvas is 1:1 with a flow and
//! describes *where* the flow loads (which runtime pool, namespace, isolation),
//! *how* it scales/restarts, and its *health* (dependent resources to watch).
//!
//! - **CE** ignores the canvas beyond a simple runtime label mapping.
//! - **EE** turns it into the `a3t.io/v1alpha1` `Canvas` CR reconciled by the
//!   controller.
//!
//! Flow persistence/memory is explicitly NOT here — that is flow-level config.
//! This module is pure data + selection logic (no I/O), so it lives in CE and is
//! unit-testable on its own.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// The published JSON Schema for a `CanvasSpec` — the single source of truth for
/// authoring, verification, and the editor skeleton. Served at `GET /canvas/schema`.
pub fn canvas_json_schema() -> serde_json::Value {
    serde_json::to_value(schemars::schema_for!(CanvasSpec)).unwrap_or_default()
}

/// How a flow attaches its canvas (`flow.metadata.a3t.canvas`). Flow is central:
/// it either references a stored canvas or embeds a small one inline.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum CanvasAttachment {
    /// A stored, reusable Canvas: `{ ref: "namespace/name" }`.
    Ref { r#ref: String },
    /// A small self-contained canvas embedded in the flow: `{ inline: {…} }`.
    Inline { inline: Box<CanvasSpec> },
}

impl CanvasAttachment {
    /// The `namespace/name` for a reference, if this is a `Ref`.
    pub fn ref_target(&self) -> Option<(&str, &str)> {
        match self {
            CanvasAttachment::Ref { r#ref } => r#ref.split_once('/'),
            CanvasAttachment::Inline { .. } => None,
        }
    }
}

/// A full Canvas object (as stored / as the CR body).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, JsonSchema)]
pub struct Canvas {
    #[serde(default = "default_api_version")]
    pub api_version: String,
    #[serde(default = "default_kind")]
    pub kind: String,
    pub metadata: CanvasMeta,
    pub spec: CanvasSpec,
    #[serde(default)]
    pub status: Option<CanvasStatus>,
}

fn default_api_version() -> String { "a3t.io/v1alpha1".to_string() }
fn default_kind() -> String { "Canvas".to_string() }

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, JsonSchema)]
pub struct CanvasMeta {
    pub name: String,
    #[serde(default = "default_ns")]
    pub namespace: String,
}
fn default_ns() -> String { "default".to_string() }

/// The deployment metadata itself.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CanvasSpec {
    /// The flow this canvas deploys — `namespace/name`. Empty on a **preset**
    /// (a reusable deployment profile); set when the preset is cloned + bound.
    #[serde(default)]
    pub flow_ref: String,
    /// Human description — shown in the canvas picker to choose a preset.
    #[serde(default)]
    pub description: String,
    /// The agent this canvas is bound to (`namespace/id`), set when a preset is
    /// cloned for an agent. Empty on presets. Two-way with `agent.variables.canvasRef`.
    #[serde(default)]
    pub agent_ref: String,
    /// (a) where the flow loads.
    #[serde(default)]
    pub placement: Placement,
    /// (b) scale/pause/restart within the runtime.
    #[serde(default)]
    pub lifecycle: Lifecycle,
    /// (c) dependent-resource health.
    #[serde(default)]
    pub health: Health,
}

/// (a) Placement — which runtime pool the flow loads into, and how.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Placement {
    #[serde(default)]
    pub mode: PlacementMode,
    /// Labels a runtime pool must have to host this flow.
    #[serde(default)]
    pub runtime_selector: BTreeMap<String, String>,
    /// Target k8s namespace (isolation → policy). Empty = the flow's namespace.
    #[serde(default)]
    pub namespace: String,
    /// k8s tolerations (for dedicated/tainted nodes; used mainly by `isolated`).
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

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum PlacementMode {
    /// Load into a shared runtime pool alongside other flows (default).
    #[default]
    Shared,
    /// Dedicated container/runtime for this flow (isolation).
    Isolated,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, JsonSchema)]
pub struct Toleration {
    pub key: String,
    #[serde(default)]
    pub value: String,
}

/// (b) Lifecycle — how the flow runs within its runtime(s).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, JsonSchema)]
pub struct Lifecycle {
    #[serde(default)]
    pub state: LifecycleState,
    /// Whether the flow (agent) carries state across triggers.
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

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum LifecycleState {
    #[default]
    Running,
    Paused,
}

/// How the flow (agent) carries state between triggers.
///
/// - `ephemeral` (default): spun up on a trigger, runs to completion, torn down —
///   nothing carried to the next trigger.
/// - `persistent`: state is checkpointed, the sandbox hibernates when idle, and on
///   the next trigger it resumes with the session state + new input.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum SessionMode {
    #[default]
    Ephemeral,
    Persistent,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, JsonSchema)]
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, JsonSchema)]
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
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Health {
    /// Resources the flow depends on. Auto-derived from the flow's tasks when
    /// empty; overridable here.
    #[serde(default)]
    pub depends_on: Vec<DependsOn>,
    #[serde(default)]
    pub on_dependency_down: OnDependencyDown,
    #[serde(default)]
    pub circuit: Option<Circuit>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, JsonSchema)]
pub struct DependsOn {
    /// `namespace/name` of a resource (llm, db, mcp, …).
    pub resource: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum OnDependencyDown {
    /// Hold requests until the dependency recovers (default).
    #[default]
    Queue,
    /// Stop accepting new work (cordon).
    Cordon,
    /// Open a circuit breaker.
    CircuitBreak,
    /// Fail fast.
    Fail,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Circuit {
    pub error_threshold: f64,
    pub window: String,
    #[serde(default = "default_circuit_action")]
    pub action: String,
}
fn default_circuit_action() -> String { "open".to_string() }

/// Controller-written status (EE). Absent in CE.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CanvasStatus {
    #[serde(default)]
    pub phase: String, // Pending|Deploying|Synced|Drifted|Paused|Degraded
    #[serde(default)]
    pub placed_runtime: String,
    #[serde(default)]
    pub observed_generation: u64,
    #[serde(default)]
    pub replicas: u32,
    #[serde(default)]
    pub ready_replicas: u32,
    #[serde(default)]
    pub health: String, // Healthy|Degraded|DependencyDown
    #[serde(default)]
    pub drift: bool,
}

impl CanvasSpec {
    /// Structural validation. A **preset** (empty `flowRef`) is valid — it's a
    /// reusable deployment profile that gets a `flowRef` when cloned + bound.
    pub fn validate(&self) -> Vec<String> {
        let mut errs = Vec::new();
        if !self.flow_ref.is_empty() && self.flow_ref.split_once('/').is_none() {
            errs.push(format!("spec.flowRef must be 'namespace/name', got '{}'", self.flow_ref));
        }
        if !self.agent_ref.is_empty() && self.agent_ref.split_once('/').is_none() {
            errs.push(format!("spec.agentRef must be 'namespace/name', got '{}'", self.agent_ref));
        }
        if self.lifecycle.scaling.max < self.lifecycle.scaling.min {
            errs.push("lifecycle.scaling.max must be >= min".into());
        }
        for d in &self.health.depends_on {
            if d.resource.split_once('/').is_none() {
                errs.push(format!("health.dependsOn.resource must be 'namespace/name', got '{}'", d.resource));
            }
        }
        errs
    }

    /// Whether this canvas can be deployed (a preset cannot — it has no flow).
    pub fn is_preset(&self) -> bool {
        self.flow_ref.is_empty()
    }
}

/// A runtime pool — a labeled, namespaced flow-execution engine that hosts many
/// flows. Stored as a registry resource (`type_ref = "runtime"`); this is the
/// typed view of its `config`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeSpec {
    /// Registry resource name (pool id).
    #[serde(default)]
    pub name: String,
    /// k8s namespace the pool runs in.
    #[serde(default = "default_ns")]
    pub namespace: String,
    /// Labels used for placement matching (`class`, `tier`, `zone`, …).
    #[serde(default)]
    pub labels: BTreeMap<String, String>,
    /// The pool's base URL (for CE mapping / EE addressing). Optional.
    #[serde(default)]
    pub url: Option<String>,
    /// Marks the pool used when a flow declares no selector.
    #[serde(default)]
    pub default: bool,
}

/// Select the runtime a flow should load into (CE mapping + EE first pass).
///
/// A runtime matches when it carries every label in `selector` (namespace, if
/// set, must also match). With an empty selector the `default` runtime wins,
/// else the first available. Returns `None` when nothing matches (caller decides:
/// error, or fall back to the in-process runtime).
pub fn select_runtime<'a>(
    placement: &Placement,
    runtimes: &'a [RuntimeSpec],
) -> Option<&'a RuntimeSpec> {
    let ns_ok = |r: &RuntimeSpec| placement.namespace.is_empty() || r.namespace == placement.namespace;

    if placement.runtime_selector.is_empty() {
        return runtimes
            .iter()
            .filter(|r| ns_ok(r))
            .find(|r| r.default)
            .or_else(|| runtimes.iter().find(|r| ns_ok(r)));
    }
    runtimes.iter().filter(|r| ns_ok(r)).find(|r| {
        placement
            .runtime_selector
            .iter()
            .all(|(k, v)| r.labels.get(k).map(|rv| rv == v).unwrap_or(false))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rt(name: &str, ns: &str, labels: &[(&str, &str)], default: bool) -> RuntimeSpec {
        RuntimeSpec {
            name: name.into(),
            namespace: ns.into(),
            labels: labels.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect(),
            url: None,
            default,
        }
    }

    #[test]
    fn attachment_ref_parses_target() {
        let a = CanvasAttachment::Ref { r#ref: "default/orders".into() };
        assert_eq!(a.ref_target(), Some(("default", "orders")));
    }

    #[test]
    fn empty_selector_prefers_default_then_first() {
        let rts = vec![
            rt("a", "default", &[], false),
            rt("b", "default", &[], true),
        ];
        let p = Placement::default();
        assert_eq!(select_runtime(&p, &rts).unwrap().name, "b");

        let rts2 = vec![rt("a", "default", &[], false)];
        assert_eq!(select_runtime(&p, &rts2).unwrap().name, "a");

        assert!(select_runtime(&p, &[]).is_none());
    }

    #[test]
    fn selector_matches_all_labels() {
        let rts = vec![
            rt("cpu", "default", &[("class", "cpu"), ("tier", "prod")], false),
            rt("gpu", "default", &[("class", "gpu"), ("tier", "prod")], false),
        ];
        let mut p = Placement::default();
        p.runtime_selector.insert("class".into(), "gpu".into());
        p.runtime_selector.insert("tier".into(), "prod".into());
        assert_eq!(select_runtime(&p, &rts).unwrap().name, "gpu");

        // A selector label the pool lacks → no match.
        p.runtime_selector.insert("zone".into(), "eu".into());
        assert!(select_runtime(&p, &rts).is_none());
    }

    #[test]
    fn namespace_constrains_selection() {
        let rts = vec![
            rt("shared", "team-b", &[("class", "gpu")], false),
            rt("mine", "team-a", &[("class", "gpu")], false),
        ];
        let mut p = Placement { namespace: "team-a".into(), ..Default::default() };
        p.runtime_selector.insert("class".into(), "gpu".into());
        assert_eq!(select_runtime(&p, &rts).unwrap().name, "mine");
    }

    #[test]
    fn validate_flags_missing_and_malformed() {
        // A preset (empty flowRef) is valid.
        let mut spec = CanvasSpec::default();
        assert!(spec.validate().is_empty());
        assert!(spec.is_preset());
        spec.flow_ref = "orders".into(); // no namespace
        assert!(spec.validate().iter().any(|e| e.contains("must be 'namespace/name'")));
        spec.flow_ref = "default/orders".into();
        assert!(spec.validate().is_empty());
        assert!(!spec.is_preset());
        spec.lifecycle.scaling = Scaling { min: 5, max: 2, metric: None, target: None };
        assert!(spec.validate().iter().any(|e| e.contains("max must be >= min")));
    }

    #[test]
    fn canvas_yaml_roundtrips() {
        let yaml = r#"
apiVersion: a3t.io/v1alpha1
kind: Canvas
metadata:
  name: orders-pipeline
  namespace: default
spec:
  flowRef: default/orders-flow
  placement:
    mode: isolated
    runtimeSelector: { class: gpu, tier: prod }
    namespace: team-a
  lifecycle:
    state: running
    scaling: { min: 1, max: 5, metric: queue_depth, target: 20 }
  health:
    dependsOn:
      - resource: default/openrouter-llm
    onDependencyDown: queue
"#;
        let c: Canvas = serde_yaml::from_str(yaml).expect("parse");
        assert_eq!(c.spec.placement.mode, PlacementMode::Isolated);
        assert_eq!(c.spec.placement.runtime_selector.get("class").unwrap(), "gpu");
        assert_eq!(c.spec.lifecycle.scaling.max, 5);
        assert_eq!(c.spec.health.depends_on[0].resource, "default/openrouter-llm");
        assert!(c.spec.validate().is_empty());
    }
}
