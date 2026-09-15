//! `flowr-aesop` — the A3T EE glue crate.
//!
//! [`CedarHook`] implements CE's [`flowr_policy::PolicyHook`] by translating a
//! CE [`ExecutionRequest`] into an aesop [`SecurityContext`]/[`Action`]/
//! [`Resource`] and running it through [`aesop_core::AuthzEngine`] (Cedar).
//! This *is* the CE→EE enforcement upgrade: CE ships `AllowAll`; installing
//! this hook via `security.policy_hook = "cedar"` turns policy into enforcement,
//! with **no CE code change**.
//!
//! Dependency direction: `flowr-ee → { flowr-policy (CE), aesop-core }`. Neither
//! CE nor aesop depends on this crate.

use aesop_core::{
    AesopError, Action as AesopAction, AgentId as AesopAgentId, AuthzEngine, Engine,
    Principal as AesopPrincipal, Resource as AesopResource, SecurityContext,
};
use flowr_policy::{ExecutionRequest, PolicyDecision, PolicyHook, Principal};

/// The A3T baseline policy pack, embedded so the EE binary is self-contained.
pub const A3T_SCHEMA: &str = include_str!("../policies/a3t/a3t.cedarschema");
/// The same base schema in Cedar JSON — the merge base for registry schema
/// fragments (JSON deep-merges cleanly; cedarschema text can't extend an entity).
pub const A3T_SCHEMA_JSON: &str = include_str!("../policies/a3t/a3t.schema.json");
pub const A3T_BASELINE: &str = include_str!("../policies/a3t/baseline.cedar");

/// Recursively merge `over` into `base`: objects merge key-by-key (so entity
/// `shape.attributes` and action maps union), everything else is overwritten by
/// `over`. This gives "fragment extends/overrides base, per entity, all the way
/// down" — exactly the schema-composition semantics we want.
fn deep_merge(base: &mut serde_json::Value, over: serde_json::Value) {
    use serde_json::Value;
    match (base, over) {
        (Value::Object(b), Value::Object(o)) => {
            for (k, v) in o {
                match b.get_mut(&k) {
                    Some(bv) => deep_merge(bv, v),
                    None => { b.insert(k, v); }
                }
            }
        }
        (b, o) => *b = o,
    }
}

/// Build the effective schema (Cedar JSON string) = embedded base ⊕ optional
/// registry base override ⊕ each per-resource fragment. A fragment that isn't
/// valid JSON, or that yields an unparseable merged schema, is **skipped** (the
/// base and the other fragments survive) — one bad resource can't black out authz.
fn build_merged_schema(base_override: Option<&str>, fragments: &[String]) -> String {
    use serde_json::Value;
    let mut merged: Value = serde_json::from_str(A3T_SCHEMA_JSON)
        .unwrap_or_else(|_| Value::Object(Default::default()));
    if let Some(b) = base_override {
        match serde_json::from_str::<Value>(b) {
            Ok(bv) => deep_merge(&mut merged, bv),
            Err(e) => tracing::warn!("base-schema override is not valid JSON ({e}); ignoring"),
        }
    }
    for (i, f) in fragments.iter().enumerate() {
        match serde_json::from_str::<Value>(f) {
            Ok(fv) => {
                let mut trial = merged.clone();
                deep_merge(&mut trial, fv);
                let s = trial.to_string();
                if AuthzEngine::schema_parses(&s) {
                    merged = trial;
                } else {
                    tracing::warn!("schema fragment #{i} produces an invalid schema; skipping it");
                }
            }
            Err(e) => tracing::warn!("schema fragment #{i} is not valid JSON ({e}); skipping it"),
        }
    }
    merged.to_string()
}

/// A [`PolicyHook`] backed by Cedar. Fails **closed**: any engine fault or an
/// unrepresentable request denies.
pub struct CedarHook {
    engine: AuthzEngine,
}

impl CedarHook {
    /// Build from explicit Cedar policy + schema source.
    pub fn new(policies_src: &str, schema_src: &str) -> Result<Self, String> {
        let engine = AuthzEngine::new(policies_src, Some(schema_src)).map_err(|e| e.to_string())?;
        Ok(Self { engine })
    }

    /// Build from the embedded A3T baseline pack (schema + policies).
    pub fn baseline() -> Result<Self, String> {
        Self::new(A3T_BASELINE, A3T_SCHEMA)
    }

    /// Build the enforced policy set as **baked baseline + attached registry
    /// policies** (E2 #2c load order, §6.3). Each entry in `registry_policies`
    /// is the Cedar source of a `type_ref: cedar` resource owned by InfoSec.
    /// The baseline guardrails (deny-by-default, destructive-tool forbid, SoD)
    /// always apply; registry policies layer permits/forbids on top. All are
    /// validated against the single A3T schema — a malformed registry policy
    /// fails the whole load (fail closed) rather than silently dropping.
    /// `base_override` is an optional registry `system/base-schema` (Cedar JSON)
    /// replacing/extending the embedded base; `schema_fragments` are per-resource
    /// Cedar-JSON fragments merged on top. The merged schema is used to validate
    /// the policy set (N8). Falls back to the embedded base schema if the merged
    /// schema can't validate the policies.
    pub fn with_registry_policies(
        registry_policies: &[String],
        base_override: Option<&str>,
        schema_fragments: &[String],
    ) -> Result<Self, String> {
        let mut policies = String::from(A3T_BASELINE);
        for (i, src) in registry_policies.iter().enumerate() {
            policies.push_str("\n// ── attached registry policy #");
            policies.push_str(&i.to_string());
            policies.push_str(" ──\n");
            policies.push_str(src);
            policies.push('\n');
        }
        let schema_json = build_merged_schema(base_override, schema_fragments);
        match Self::new(&policies, &schema_json) {
            Ok(hook) => Ok(hook),
            Err(e) => {
                // Merged schema didn't validate the policy set — fall back to the
                // embedded base so authz keeps working (loudly logged).
                tracing::warn!("merged registry schema rejected ({e}); using embedded base schema");
                Self::new(&policies, A3T_SCHEMA)
            }
        }
    }

    /// Load a policy pack from a directory: all `*.cedar` files concatenated as
    /// the policy set, and a `*.cedarschema` if present (else the embedded A3T
    /// schema). This is `security.policies_dir` — policies are deployable and
    /// editable without recompiling the binary.
    pub fn from_dir(dir: &str) -> Result<Self, String> {
        let mut policies = String::new();
        let mut schema: Option<String> = None;
        let entries = std::fs::read_dir(dir).map_err(|e| format!("read {dir}: {e}"))?;
        let mut paths: Vec<_> = entries
            .filter_map(|e| e.ok().map(|e| e.path()))
            .collect();
        paths.sort();
        for path in paths {
            match path.extension().and_then(|s| s.to_str()) {
                Some("cedar") => {
                    let s = std::fs::read_to_string(&path)
                        .map_err(|e| format!("read {}: {e}", path.display()))?;
                    policies.push_str(&s);
                    policies.push('\n');
                }
                Some("cedarschema") => {
                    schema = Some(
                        std::fs::read_to_string(&path)
                            .map_err(|e| format!("read {}: {e}", path.display()))?,
                    );
                }
                _ => {}
            }
        }
        if policies.trim().is_empty() {
            return Err(format!("no *.cedar policies found in {dir}"));
        }
        Self::new(&policies, schema.as_deref().unwrap_or(A3T_SCHEMA))
    }
}

/// CE `Principal` → aesop `Principal` (identical shape, distinct crates: CE is
/// dependency-light and must not import aesop).
fn to_aesop_principal(p: &Principal) -> AesopPrincipal {
    match p {
        Principal::Human { user_id, roles } => AesopPrincipal::Human {
            user_id: user_id.clone(),
            roles: roles.clone(),
        },
        Principal::Agent { agent_id, on_behalf_of, flow_id } => AesopPrincipal::Agent {
            agent_id: AesopAgentId(agent_id.0.clone()),
            on_behalf_of: on_behalf_of.clone(),
            flow_id: flow_id.clone(),
        },
        Principal::Service { service_id } => AesopPrincipal::Service {
            service_id: service_id.clone(),
        },
    }
}

#[async_trait::async_trait]
impl PolicyHook for CedarHook {
    async fn check(&self, req: &ExecutionRequest) -> PolicyDecision {
        let ctx = SecurityContext {
            principal: to_aesop_principal(&req.principal),
            // E2 #2a: the flow/agent template data (`principal.template.*`) rides
            // here so InfoSec-owned policy resources can gate on it.
            attributes: req.attributes.clone(),
        };

        // The action shape was already validated at the CE edge; re-validate to
        // convert into the aesop type. A malformed action fails closed.
        let action = match AesopAction::new(req.action.as_str()) {
            Ok(a) => a,
            Err(e) => {
                return PolicyDecision::Deny {
                    reason: format!("invalid action '{}': {e}", req.action.as_str()),
                    policy_ids: vec![],
                }
            }
        };

        let resource = AesopResource {
            type_name: req.resource.type_name.clone(),
            id: req.resource.id.clone(),
            attributes: req.resource.attributes.clone(),
        };

        match self.engine.enforce(&ctx, &action, &resource) {
            Ok(_allowed) => PolicyDecision::Allow,
            Err(AesopError::Denied { policy_ids, action, resource_type, resource_id }) => {
                PolicyDecision::Deny {
                    reason: format!("cedar denied {action} on {resource_type}::{resource_id}"),
                    policy_ids,
                }
            }
            // Engine faults deny (fail closed) — an allow is never fabricated
            // from a fault path.
            Err(e) => PolicyDecision::Deny {
                reason: format!("policy engine fault: {e}"),
                policy_ids: vec![],
            },
        }
    }
}
