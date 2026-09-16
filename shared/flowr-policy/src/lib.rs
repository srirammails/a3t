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

pub mod grants;
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
        /// The authority this agent actually carries, **inherited from the
        /// invoking human and attenuated** — see [`Principal::agent_for`].
        ///
        /// Without this an agent acting for a Viewer and one acting for an Admin
        /// were indistinguishable to policy: `on_behalf_of` carried the user's
        /// *name* but none of their *authority*, so every agent got the same
        /// blanket permit. Rules can now say `principal.roles.contains("Admin")`
        /// for an Agent exactly as they already do for a User.
        #[serde(default)]
        roles: Vec<String>,
        /// Who owns this agent — the principal that created it. The owner is the
        /// default authority over what the agent may be granted; anything beyond
        /// the owner's own reach has to be requested from InfoSec.
        #[serde(default)]
        owner: Option<String>,
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

    /// The roles this principal carries, whether human or agent.
    pub fn roles(&self) -> &[String] {
        match self {
            Principal::Human { roles, .. } => roles,
            Principal::Agent { roles, .. } => roles,
            Principal::Service { .. } => &[],
        }
    }

    /// Who owns this principal, where that is meaningful (agents only today).
    pub fn owner(&self) -> Option<&str> {
        match self {
            Principal::Agent { owner, .. } => owner.as_deref(),
            _ => None,
        }
    }

    /// Derive the principal an agent runs as when invoked by `caller`.
    ///
    /// **Attenuate-only.** The agent inherits the caller's authority and may
    /// declare *less*, never more:
    ///
    /// - invoked by a human → inherits that human's roles, intersected with
    ///   `role_ceiling` when the agent declares one;
    /// - invoked by another agent → inherits that agent's already-attenuated
    ///   roles, so a chain of agents can only ever narrow;
    /// - invoked by a service → carries no roles at all. A service has no roles
    ///   to lend, and an agent must not manufacture authority from ambient trust.
    ///
    /// An agent that declares no ceiling inherits the caller's roles unchanged —
    /// the same "declaring nothing does not narrow" rule the tool ceiling uses
    /// (`exposedTools`/`enabledTools`), so the two read consistently.
    pub fn agent_for(
        agent_id: AgentId,
        caller: &Principal,
        flow_id: Option<String>,
        owner: Option<String>,
        role_ceiling: Option<&[String]>,
    ) -> Principal {
        let inherited: Vec<String> = match caller {
            Principal::Human { roles, .. } => roles.clone(),
            Principal::Agent { roles, .. } => roles.clone(),
            Principal::Service { .. } => Vec::new(),
        };
        let roles = match role_ceiling {
            Some(ceiling) => inherited
                .into_iter()
                .filter(|r| ceiling.iter().any(|c| c == r))
                .collect(),
            None => inherited,
        };
        Principal::Agent {
            agent_id,
            on_behalf_of: match caller {
                Principal::Human { user_id, .. } => Some(user_id.clone()),
                Principal::Agent { on_behalf_of, .. } => on_behalf_of.clone(),
                Principal::Service { .. } => None,
            },
            flow_id,
            roles,
            owner,
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

    /// Build a `Runtime` resource for a deployment check.
    ///
    /// Deployment is authorized on the TARGET, not the flow: the question is
    /// which pool and namespace a principal may place work into. The pool's
    /// labels (`tier`, `class`, `zone`, …) ride as attributes so a policy can say
    /// "prod tier needs Admin" or "only this team's namespace" without naming
    /// individual flows. `mode` distinguishes a shared placement from a dedicated
    /// one, which is a different cost and a different blast radius.
    pub fn runtime(
        name: impl Into<String>,
        namespace: impl Into<String>,
        mode: impl Into<String>,
        labels: impl IntoIterator<Item = (String, String)>,
    ) -> Self {
        let mut r = Self::new("Runtime", name)
            .with_attr("namespace", namespace.into())
            .with_attr("mode", mode.into());
        for (k, v) in labels {
            // Labels are authored data, so they are namespaced under their own
            // keys rather than merged into the top level where they could shadow
            // `namespace` or `mode` and quietly change what a rule matches.
            r = r.with_attr(format!("label_{k}"), v);
        }
        r
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

/// How widely an interactive approval applies once granted. Narrowest first —
/// the approver is offered these in order, and the UI defaults to the narrowest.
///
/// All three are the *same mechanism*: a grant is a Cedar `permit` layered over
/// the baseline. Only its lifetime and where it is stored differ, so a
/// session-scoped "yes" and a persisted one cannot drift apart in behaviour.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum GrantScope {
    /// This one call. Consumed on use; never stored.
    Once,
    /// Every matching call for the rest of this chat session. In memory, dies
    /// with the session — the dev-time default.
    Session,
    /// Persisted against the agent, surviving restarts. Written as an
    /// `agent-grant` registry resource, bounded by the owner's own authority.
    Agent,
}

impl GrantScope {
    pub fn as_str(self) -> &'static str {
        match self {
            GrantScope::Once => "once",
            GrantScope::Session => "session",
            GrantScope::Agent => "agent",
        }
    }
}

/// The outcome of a policy check.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PolicyDecision {
    Allow,
    Deny {
        reason: String,
        policy_ids: Vec<String>,
    },
    /// The policy cannot settle this alone — a human must decide.
    ///
    /// Distinct from `Deny` in one way that matters: `Ask` is only ever returned
    /// where the *approver* already holds the authority being asked for. It is a
    /// delegation prompt, never a privilege-escalation path, so a user cannot be
    /// asked to grant something they could not do themselves.
    ///
    /// Treated as NOT allowed everywhere (`is_allow()` is false). A caller that
    /// cannot conduct an interactive prompt must degrade to denial, which is why
    /// every consumer matches exhaustively rather than `if let ... Deny`.
    Ask {
        reason: String,
        policy_ids: Vec<String>,
        /// Which scopes this approver may choose from, narrowest first.
        scopes: Vec<GrantScope>,
    },
}

impl PolicyDecision {
    /// True only for `Allow`. `Ask` is deliberately false: an unanswered
    /// question is not permission, so any site that has not been taught to
    /// prompt fails closed instead of leaking access.
    pub fn is_allow(&self) -> bool {
        matches!(self, PolicyDecision::Allow)
    }
    /// Whether this decision needs a human before anything can proceed.
    pub fn is_ask(&self) -> bool {
        matches!(self, PolicyDecision::Ask { .. })
    }
    /// Telemetry value (`aesop.decision`).
    pub fn label(&self) -> &'static str {
        match self {
            PolicyDecision::Allow => "allow",
            PolicyDecision::Deny { .. } => "deny",
            PolicyDecision::Ask { .. } => "ask",
        }
    }
    /// Collapse an `Ask` to a `Deny` — for choke points with no way to reach a
    /// human (a triggered run, a worker). The reason is preserved so the audit
    /// trail shows an approval was wanted and nobody was there to give it.
    pub fn non_interactive(self) -> Self {
        match self {
            PolicyDecision::Ask { reason, policy_ids, .. } => PolicyDecision::Deny {
                reason: format!("{reason} (approval required, but this run has no interactive approver)"),
                policy_ids,
            },
            other => other,
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
                roles: vec![],
                owner: None,
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
            roles: vec![],
            owner: None,
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
            other => panic!("expected Deny, got {other:?}"),
        }
    }

    fn human(roles: &[&str]) -> Principal {
        Principal::human("usr-sriram", roles.iter().map(|s| s.to_string()).collect())
    }

    #[test]
    fn agent_inherits_caller_roles_when_it_declares_no_ceiling() {
        let p = Principal::agent_for(
            AgentId("agt-1".into()), &human(&["Admin", "Viewer"]), None, None, None,
        );
        assert_eq!(p.roles(), ["Admin", "Viewer"]);
        assert_eq!(p.on_behalf_of(), Some("usr-sriram"));
    }

    #[test]
    fn agent_ceiling_narrows_but_never_widens() {
        // Caller is only a Viewer; the agent's ceiling naming Admin must NOT
        // conjure it. This is the whole point of attenuate-only.
        let p = Principal::agent_for(
            AgentId("agt-1".into()), &human(&["Viewer"]), None, None,
            Some(&["Admin".to_string(), "Viewer".to_string()]),
        );
        assert_eq!(p.roles(), ["Viewer"]);

        // Caller is an Admin but the agent declares a Viewer-only ceiling.
        let p = Principal::agent_for(
            AgentId("agt-1".into()), &human(&["Admin", "Viewer"]), None, None,
            Some(&["Viewer".to_string()]),
        );
        assert_eq!(p.roles(), ["Viewer"]);
    }

    #[test]
    fn agent_chain_only_ever_narrows() {
        let first = Principal::agent_for(
            AgentId("agt-1".into()), &human(&["Admin", "Viewer"]), None, None,
            Some(&["Viewer".to_string()]),
        );
        // A sub-agent inherits the PARENT AGENT's already-attenuated roles, not
        // the original human's — so delegation cannot launder authority back in.
        let second = Principal::agent_for(AgentId("agt-2".into()), &first, None, None, None);
        assert_eq!(second.roles(), ["Viewer"]);
        assert_eq!(second.on_behalf_of(), Some("usr-sriram"));
    }

    #[test]
    fn agent_invoked_by_a_service_carries_no_roles() {
        // A service has no roles to lend. An event-triggered agent must not
        // manufacture authority out of ambient trust.
        let p = Principal::agent_for(
            AgentId("agt-1".into()), &Principal::service("trigger"), None, None, None,
        );
        assert!(p.roles().is_empty());
        assert_eq!(p.on_behalf_of(), None);
    }

    #[test]
    fn ask_is_not_allow() {
        let ask = PolicyDecision::Ask {
            reason: "needs approval".into(),
            policy_ids: vec!["p1".into()],
            scopes: vec![GrantScope::Once, GrantScope::Session],
        };
        // The fail-closed contract: anything that has not learned to prompt
        // must treat Ask as "no".
        assert!(!ask.is_allow());
        assert!(ask.is_ask());
        assert_eq!(ask.label(), "ask");
    }

    #[test]
    fn ask_collapses_to_deny_without_an_approver() {
        let ask = PolicyDecision::Ask {
            reason: "needs approval".into(),
            policy_ids: vec!["p1".into()],
            scopes: vec![GrantScope::Session],
        };
        match ask.non_interactive() {
            PolicyDecision::Deny { reason, policy_ids } => {
                assert!(reason.contains("no interactive approver"), "{reason}");
                assert_eq!(policy_ids, vec!["p1"]);
            }
            other => panic!("expected Deny, got {other:?}"),
        }
        // Allow and Deny pass through untouched.
        assert!(PolicyDecision::Allow.non_interactive().is_allow());
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
