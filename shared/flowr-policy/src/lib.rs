//! flowr-policy — the A3T CE policy-enforcement contract.
//!
//! This crate is the load-bearing seam between A3T CE and A3T EE (AESOP). It
//! ships in **CE** and contains **no** aesop/cedar dependency: CE defines the
//! request shape, the hook interface with an allow-all default, the typed
//! action vocabulary, and the telemetry field names. EE later provides a
//! `PolicyHook` implementation (`CedarHook`) that maps [`ExecutionRequest`]
//! onto Cedar — the request shape is a Cedar request in disguise, by design.
//!
//! Contract references: `.ai/A3T_EE_AESOP_IMPLEMENTATION.md` (E0.1) and
//! `.ai/aesop-design-and-a3t-ce-contract.md` (seams §5.1–5.3).
//!
//! Invariants this crate encodes:
//! - Deny-by-default lives in the *implementation*; CE's default is AllowAll.
//! - The Deny path exists and is a first-class outcome even in CE (§5.2).
//! - Actions are typed `domain:verb` from a central registry, never free-form
//!   strings assembled at call sites (§5.3).

pub mod actions;
pub mod audit;
pub mod op_policy;
pub mod roles;
pub mod telemetry;

use serde::{Deserialize, Serialize};
use std::sync::{Arc, RwLock};

/// Process-wide policy hook. This is the `security.policy_hook` dependency-
/// injection seam: CE never sets it (defaults to [`AllowAll`]); EE calls
/// [`set_policy_hook`] at startup with a `CedarHook`. The entire CE→EE
/// enforcement upgrade is that one call, driven by the setting.
///
/// It is an `RwLock`, not a `OnceLock`, so the hook can be **re-installed at
/// runtime** — this is the mechanism behind EE's policy hot-reload
/// (`POST /security/policies/reload`): rebuild the `CedarHook` from the current
/// registry policies and swap it in, no process restart. Reads are on the tool-
/// call hot path but uncontended (writes happen only on install/reload), so an
/// `RwLock` read is effectively free here.
static HOOK: RwLock<Option<Arc<dyn PolicyHook>>> = RwLock::new(None);

/// Install (or **replace**) the process policy hook. Called once at startup and
/// again on every hot-reload. Returns `Err` only if the lock is poisoned.
pub fn set_policy_hook(hook: Arc<dyn PolicyHook>) -> Result<(), &'static str> {
    let mut w = HOOK.write().map_err(|_| "policy hook lock poisoned")?;
    *w = Some(hook);
    Ok(())
}

/// The active policy hook. Defaults to [`AllowAll`] (CE) if none was installed.
pub fn policy_hook() -> Arc<dyn PolicyHook> {
    // Fast path: a hook is installed — clone the Arc under a read lock.
    if let Ok(r) = HOOK.read() {
        if let Some(h) = r.as_ref() {
            return h.clone();
        }
    }
    // Cold path: nothing installed yet (CE, or before startup install). Seed the
    // default AllowAll so subsequent reads take the fast path.
    match HOOK.write() {
        Ok(mut w) => w.get_or_insert_with(|| Arc::new(AllowAll)).clone(),
        // Lock poisoned: fail safe to a fresh AllowAll (CE semantics) rather than panic.
        Err(_) => Arc::new(AllowAll),
    }
}

/// Stable identity of an agent instance: stable within a run, attributable
/// across runs of the same configured agent.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AgentId(pub String);

impl std::fmt::Display for AgentId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Who (or what) is acting. Constructed at the edge (authn) and by the agent
/// spawn path; threaded, never inferred ambiently.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Principal {
    Human {
        user_id: String,
        #[serde(default)]
        roles: Vec<String>,
    },
    Agent {
        agent_id: AgentId,
        /// The human or tenant this agent acts for.
        #[serde(default)]
        on_behalf_of: Option<String>,
        #[serde(default)]
        flow_id: Option<String>,
    },
    Service {
        service_id: String,
    },
}

impl Principal {
    /// Construct a human principal (e.g. mapped from validated JWT claims at
    /// the AuthN layer). `roles` come from the token; the policy — not this
    /// crate — decides what they grant.
    pub fn human(user_id: impl Into<String>, roles: Vec<String>) -> Self {
        Principal::Human { user_id: user_id.into(), roles }
    }

    /// Construct a service principal (an internal, non-user caller).
    pub fn service(service_id: impl Into<String>) -> Self {
        Principal::Service { service_id: service_id.into() }
    }

    /// Telemetry discriminant (`aesop.principal.kind`).
    pub fn kind(&self) -> &'static str {
        match self {
            Principal::Human { .. } => "human",
            Principal::Agent { .. } => "agent",
            Principal::Service { .. } => "service",
        }
    }
    /// Telemetry id (`aesop.principal.id`).
    pub fn id(&self) -> &str {
        match self {
            Principal::Human { user_id, .. } => user_id,
            Principal::Agent { agent_id, .. } => &agent_id.0,
            Principal::Service { service_id } => service_id,
        }
    }
    pub fn on_behalf_of(&self) -> Option<&str> {
        match self {
            Principal::Agent { on_behalf_of, .. } => on_behalf_of.as_deref(),
            _ => None,
        }
    }
}

/// A typed, namespaced action: `"{domain}:{verb}"`. Never a free-form string
/// at a call site — construct via [`Action::new`] (validated) or the
/// constants in [`actions`].
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Action(String);

impl Action {
    /// Validates `domain:verb` shape. Vocabulary additions are reviewed
    /// changes (see [`actions`]).
    pub fn new(s: impl Into<String>) -> Result<Self, PolicyError> {
        let s = s.into();
        let mut parts = s.splitn(2, ':');
        match (parts.next(), parts.next()) {
            (Some(d), Some(v)) if !d.is_empty() && !v.is_empty() => Ok(Self(s)),
            _ => Err(PolicyError::InvalidAction(s)),
        }
    }
    /// Wrap a known-good vocabulary literal from [`actions`] without
    /// re-validating. Debug builds still assert the shape.
    pub fn known(s: &'static str) -> Self {
        debug_assert!(s.splitn(2, ':').filter(|p| !p.is_empty()).count() == 2, "bad action literal: {s}");
        Self(s.to_string())
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for Action {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// A typed resource: Cedar entity type name + id, plus attributes the policy
/// may read (e.g. `destructive: true`, `owner`, `category`). Security-relevant
/// metadata lives here; a tool never decides who may call it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Resource {
    pub type_name: String,
    pub id: String,
    #[serde(default)]
    pub attributes: serde_json::Map<String, serde_json::Value>,
}

impl Resource {
    pub fn new(type_name: impl Into<String>, id: impl Into<String>) -> Self {
        Self {
            type_name: type_name.into(),
            id: id.into(),
            attributes: Default::default(),
        }
    }
    pub fn with_attr(mut self, k: impl Into<String>, v: impl Into<serde_json::Value>) -> Self {
        self.attributes.insert(k.into(), v.into());
        self
    }

    /// Build a `Tool` resource with security-relevant metadata the policy reads.
    /// Today that is `destructive` (see [`is_destructive`]); more attributes
    /// (category, owner, …) attach here as tool metadata grows. In CE (AllowAll)
    /// these are ignored; in EE the Cedar pack keys off them (e.g. the
    /// destructive-tool guardrail).
    pub fn tool(name: impl AsRef<str> + Into<String>) -> Self {
        let destructive = is_destructive(name.as_ref());
        let name_str = name.as_ref().to_string();
        Self::new("Tool", name)
            .with_attr("destructive", destructive)
            .with_attr("name", name_str)
    }

    /// Build a registry `Resource` for a governance check (E2 #2b). Carries the
    /// resource's `type` (e.g. `cedar`) so the SoD guardrail can single out
    /// policy resources — only InfoSec roles may write `type == "cedar"`. In CE
    /// (AllowAll) this is ignored; in EE the baseline pack forbids the write
    /// unless the principal holds SecurityAdmin/SuperAdmin.
    pub fn registry(type_ref: impl Into<String>, name: impl Into<String>) -> Self {
        let type_str = type_ref.into();
        Self::new("Resource", name)
            .with_attr("type", type_str)
    }
}

/// Classify whether a tool/task id denotes a destructive operation — the
/// primary security-relevant attribute policies gate on. Heuristic + explicit
/// list; a real deployment sources this from tool metadata (E2 template
/// ceilings), but the classification lives in the CE contract so the *shape*
/// of the signal is stable regardless of source.
pub fn is_destructive(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    const DESTRUCTIVE_SUBSTR: &[&str] =
        &["delete", "drop", "purge", "remove", "destroy", "rotate", "truncate", "wipe"];
    if DESTRUCTIVE_SUBSTR.iter().any(|s| n.contains(s)) {
        return true;
    }
    // Arbitrary command execution is destructive by default.
    matches!(n.as_str(), "run:shell")
}

/// Execution context threaded through every request and span.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ExecContext {
    pub run_id: Option<String>,
    pub flow_id: Option<String>,
    pub step: Option<u64>,
    pub attempt: Option<u32>,
}

/// The single shape that every tool invocation is expressed as at the choke
/// point. This *is* a Cedar request in disguise: principal, action, resource,
/// context. CE defines it; AESOP consumes it unchanged.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionRequest {
    pub principal: Principal,
    pub action: Action,
    pub resource: Resource,
    #[serde(default)]
    pub context: ExecContext,
    /// Ambient principal attributes the policy may read — notably the acting
    /// flow/agent's **template** data (`{"template": {"exposedTools": [...],
    /// "runtimePolicy": "sandboxed", ...}}`), so InfoSec-owned policy resources
    /// can gate on `principal.template.*`. Empty in the base case; populated at
    /// the choke point from the flow's `metadata.a3t` / the agent's config.
    #[serde(default)]
    pub attributes: serde_json::Map<String, serde_json::Value>,
    /// Tool arguments (not used for the authorization decision itself).
    #[serde(default)]
    pub params: serde_json::Value,
}

/// The outcome of a policy check.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PolicyDecision {
    Allow,
    Deny {
        reason: String,
        policy_ids: Vec<String>,
    },
}

impl PolicyDecision {
    pub fn is_allow(&self) -> bool {
        matches!(self, PolicyDecision::Allow)
    }
    /// Telemetry value (`aesop.decision`).
    pub fn label(&self) -> &'static str {
        match self {
            PolicyDecision::Allow => "allow",
            PolicyDecision::Deny { .. } => "deny",
        }
    }
}

/// The enforcement interface. CE calls `hook.check(&req)` before every tool
/// `execute()`. EE swaps [`AllowAll`] for a `CedarHook` via the
/// `security.policy_hook` setting — that one swap is the whole CE→EE
/// enforcement upgrade.
#[async_trait::async_trait]
pub trait PolicyHook: Send + Sync {
    async fn check(&self, req: &ExecutionRequest) -> PolicyDecision;
}

/// CE default: allows everything. One virtual call, always `Allow`.
#[derive(Debug, Default, Clone)]
pub struct AllowAll;

#[async_trait::async_trait]
impl PolicyHook for AllowAll {
    async fn check(&self, _req: &ExecutionRequest) -> PolicyDecision {
        PolicyDecision::Allow
    }
}

/// Denies everything. Used only in tests to exercise CE's Deny path
/// (E0.3: denied step fails cleanly, is audited, destructive not retried).
#[derive(Debug, Default, Clone)]
pub struct DenyAll;

#[async_trait::async_trait]
impl PolicyHook for DenyAll {
    async fn check(&self, _req: &ExecutionRequest) -> PolicyDecision {
        PolicyDecision::Deny {
            reason: "deny-all (test hook)".into(),
            policy_ids: vec!["deny-all".into()],
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum PolicyError {
    #[error("invalid action name (want domain:verb): {0}")]
    InvalidAction(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    fn agent_req(action: &str, destructive: bool) -> ExecutionRequest {
        ExecutionRequest {
            principal: Principal::Agent {
                agent_id: AgentId("agt-1".into()),
                on_behalf_of: Some("user::sriram".into()),
                flow_id: Some("flow-1".into()),
            },
            action: Action::new(action).unwrap(),
            resource: Resource::new("Tool", "http_fetch").with_attr("destructive", destructive),
            context: ExecContext {
                run_id: Some("run-1".into()),
                step: Some(3),
                ..Default::default()
            },
            attributes: Default::default(),
            params: serde_json::json!({}),
        }
    }

    #[test]
    fn destructive_classifier_and_tool_resource() {
        assert!(is_destructive("delete_flow"));
        assert!(is_destructive("call:rotate"));
        assert!(is_destructive("run:shell"));
        assert!(!is_destructive("call:llm"));
        assert!(!is_destructive("list_flows"));
        let r = Resource::tool("call:rotate");
        assert_eq!(r.type_name, "Tool");
        assert_eq!(r.attributes.get("destructive"), Some(&serde_json::json!(true)));
        let safe = Resource::tool("call:llm");
        assert_eq!(safe.attributes.get("destructive"), Some(&serde_json::json!(false)));
    }

    #[test]
    fn action_shape_validated() {
        assert!(Action::new("tool:invoke").is_ok());
        assert!(Action::new("media_buy:create").is_ok());
        assert!(Action::new("nope").is_err());
        assert!(Action::new(":v").is_err());
        assert!(Action::new("d:").is_err());
    }

    #[test]
    fn principal_telemetry_accessors() {
        let p = Principal::Agent {
            agent_id: AgentId("agt-1".into()),
            on_behalf_of: Some("user::x".into()),
            flow_id: None,
        };
        assert_eq!(p.kind(), "agent");
        assert_eq!(p.id(), "agt-1");
        assert_eq!(p.on_behalf_of(), Some("user::x"));
    }

    #[tokio::test]
    async fn allow_all_allows() {
        let d = AllowAll.check(&agent_req("tool:invoke", true)).await;
        assert!(d.is_allow());
        assert_eq!(d.label(), "allow");
    }

    #[tokio::test]
    async fn deny_all_denies_with_policy_ids() {
        let d = DenyAll.check(&agent_req("tool:invoke", true)).await;
        match d {
            PolicyDecision::Deny { policy_ids, .. } => assert_eq!(policy_ids, vec!["deny-all"]),
            PolicyDecision::Allow => panic!("expected Deny"),
        }
    }

    #[test]
    fn request_roundtrips_json() {
        let req = agent_req("mcp:call", false);
        let s = serde_json::to_string(&req).unwrap();
        let back: ExecutionRequest = serde_json::from_str(&s).unwrap();
        assert_eq!(back.action.as_str(), "mcp:call");
        assert_eq!(back.principal.id(), "agt-1");
    }
}
