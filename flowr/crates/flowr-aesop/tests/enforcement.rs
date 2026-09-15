//! E1 enforcement proof: the `CedarHook`, installed as CE's `PolicyHook`,
//! allows/denies real `ExecutionRequest`s through the Cedar baseline pack.
//! This is the shape of the goal demo — flip the hook, and the runtime enforces.

use flowr_aesop::CedarHook;
use flowr_policy::{
    actions, AgentId, ExecutionRequest, PolicyDecision, PolicyHook, Principal, Resource,
};

fn req(principal: Principal, destructive: bool) -> ExecutionRequest {
    let resource = Resource::new("Tool", "some_tool").with_attr("destructive", destructive);
    ExecutionRequest {
        principal,
        action: actions::tool_invoke(),
        resource,
        context: Default::default(),
        attributes: Default::default(),
        params: serde_json::json!({}),
    }
}

fn human() -> Principal {
    Principal::Human { user_id: "sriram".into(), roles: vec!["admin".into()] }
}

fn agent_for_user() -> Principal {
    Principal::Agent {
        agent_id: AgentId("pc-crawler-7".into()),
        on_behalf_of: Some("user::sriram".into()),
        flow_id: Some("acme/campaign".into()),
        // No inherited roles: this agent acts for a plain user with no
        // Admin/SuperAdmin, so the destructive forbid binds the approver too and
        // the denial stays a denial rather than becoming a question.
        roles: vec![],
        owner: Some("user::sriram".into()),
    }
}

fn agent_ambient() -> Principal {
    // No on_behalf_of — ambient trust, which the baseline refuses.
    Principal::Agent { agent_id: AgentId("rogue".into()), on_behalf_of: None, flow_id: None, roles: vec![], owner: None }
}

#[tokio::test]
async fn human_allowed_on_safe_tool() {
    let hook = CedarHook::baseline().unwrap();
    assert!(matches!(
        hook.check(&req(human(), false)).await,
        PolicyDecision::Allow
    ));
}

#[tokio::test]
async fn agent_on_behalf_of_allowed_on_safe_tool() {
    let hook = CedarHook::baseline().unwrap();
    assert!(matches!(
        hook.check(&req(agent_for_user(), false)).await,
        PolicyDecision::Allow
    ));
}

#[tokio::test]
async fn agent_denied_on_destructive_tool() {
    // The guardrail: forbid overrides the agent permit for destructive tools.
    let hook = CedarHook::baseline().unwrap();
    match hook.check(&req(agent_for_user(), true)).await {
        PolicyDecision::Deny { policy_ids, reason } => {
            assert!(!policy_ids.is_empty(), "deny cites the forbid policy: {reason}");
        }
        other => panic!("destructive tool must be denied — got {other:?}"),
    }
}

#[tokio::test]
async fn ambient_agent_denied() {
    // No on_behalf_of → no permit matches → deny-by-default.
    let hook = CedarHook::baseline().unwrap();
    assert!(matches!(
        hook.check(&req(agent_ambient(), false)).await,
        PolicyDecision::Deny { .. }
    ));
}

#[tokio::test]
async fn real_destructive_task_type_auto_denied() {
    // End-to-end: a real task type the choke point classifies destructive via
    // `Resource::tool()` is denied even for a human — the guardrail (forbid)
    // wins. This is what a real flow calling `call:rotate` will hit.
    let hook = CedarHook::baseline().unwrap();
    let request = ExecutionRequest {
        principal: human(),
        action: actions::tool_invoke(),
        resource: Resource::tool("call:rotate"),
        context: Default::default(),
        attributes: Default::default(),
        params: serde_json::json!({}),
    };
    assert!(matches!(hook.check(&request).await, PolicyDecision::Deny { .. }));
}

#[tokio::test]
async fn template_exposed_tools_gates_through_hook() {
    // E2 #2a end-to-end: the acting agent's template data rides in the request's
    // `attributes`, and an InfoSec-style policy resource gates on
    // `principal.template.exposedTools`. Tool in the set → allow; not in → deny.
    const SCHEMA: &str = r#"
entity User { roles?: Set<String> };
entity Agent { on_behalf_of?: String, template?: { "exposedTools": Set<String> } };
entity Service;
entity Tool { destructive?: Bool, name?: String };
action "tool:invoke" appliesTo { principal: [User, Agent, Service], resource: [Tool] };
"#;
    const POLICY: &str = r#"
permit(principal is Agent, action == Action::"tool:invoke", resource)
when {
  principal has template && resource has name &&
  principal.template.exposedTools.contains(resource.name)
};
"#;
    let hook = CedarHook::new(POLICY, SCHEMA).unwrap();
    let mk = |tool: &str| ExecutionRequest {
        principal: agent_for_user(),
        action: actions::tool_invoke(),
        resource: Resource::tool(tool),
        context: Default::default(),
        attributes: serde_json::json!({ "template": { "exposedTools": ["call:llm", "call:mcp"] } })
            .as_object()
            .unwrap()
            .clone(),
        params: serde_json::json!({}),
    };
    assert!(matches!(hook.check(&mk("call:llm")).await, PolicyDecision::Allow), "exposed tool allowed");
    assert!(matches!(hook.check(&mk("call:sql")).await, PolicyDecision::Deny { .. }), "non-exposed tool denied");
}

#[tokio::test]
async fn service_allowed_on_safe_tool() {
    // The runtime service (internal flow execution, e.g. template list via
    // call:lancedb) is permitted for non-destructive tools — else the platform's
    // own flows break.
    let hook = CedarHook::baseline().unwrap();
    let p = Principal::Service { service_id: "flow-runtime".into() };
    assert!(matches!(hook.check(&req(p, false)).await, PolicyDecision::Allow));
}

#[tokio::test]
async fn service_denied_on_destructive_tool() {
    // ...but even the runtime service is denied a destructive tool (forbid wins).
    let hook = CedarHook::baseline().unwrap();
    let p = Principal::Service { service_id: "flow-runtime".into() };
    match hook.check(&req(p, true)).await {
        PolicyDecision::Deny { .. } => {}
        other => panic!("destructive tool must be denied even for service — got {other:?}"),
    }
}

// ── E2 #2b: separation of duties on Cedar policy resources (§6.4) ──

fn write_resource(principal: Principal, type_ref: &str) -> ExecutionRequest {
    ExecutionRequest {
        principal,
        action: actions::resource_update(),
        resource: Resource::registry(type_ref, "some-policy"),
        context: Default::default(),
        attributes: Default::default(),
        params: serde_json::json!({}),
    }
}

fn user_with_roles(roles: &[&str]) -> Principal {
    Principal::Human {
        user_id: "u1".into(),
        roles: roles.iter().map(|s| s.to_string()).collect(),
    }
}

#[tokio::test]
async fn security_admin_may_write_cedar_policy() {
    // InfoSec owns policy: a SecurityAdmin may update a `type_ref: cedar` resource.
    let hook = CedarHook::baseline().unwrap();
    let req = write_resource(user_with_roles(&["SecurityAdmin"]), "cedar");
    assert!(matches!(hook.check(&req).await, PolicyDecision::Allow));
}

#[tokio::test]
async fn plain_user_may_not_write_cedar_policy() {
    // An app builder without the InfoSec role cannot rewrite the policy that
    // governs them — the SoD forbid overrides the general resource-write permit.
    let hook = CedarHook::baseline().unwrap();
    let req = write_resource(user_with_roles(&["Editor"]), "cedar");
    match hook.check(&req).await {
        PolicyDecision::Deny { policy_ids, reason } => {
            assert!(!policy_ids.is_empty(), "deny cites the SoD forbid: {reason}");
        }
        other => panic!("non-InfoSec user must not write a cedar policy — got {other:?}"),
    }
}

#[tokio::test]
async fn platform_superadmin_may_write_cedar_policy() {
    // E2 #5: the AuthN layer canonicalizes the platform `superadmin` claim to the
    // canonical roles (`SuperAdmin` + `SecurityAdmin`); the guardrail honors those.
    let hook = CedarHook::baseline().unwrap();
    let roles = flowr_policy::roles::canonicalize("superadmin");
    let principal = Principal::Human { user_id: "u1".into(), roles };
    let req = write_resource(principal, "cedar");
    assert!(matches!(hook.check(&req).await, PolicyDecision::Allow));
}

#[tokio::test]
async fn authoring_tools_require_admin_but_reads_are_open() {
    // E2 #5 Phase C: a human user needs Admin to author (create_flow/agent/…);
    // reads (list_/get_) stay open to any authenticated user.
    let hook = CedarHook::baseline().unwrap();
    let mk = |roles: &[&str], tool: &str| ExecutionRequest {
        principal: user_with_roles(roles),
        action: actions::mcp_call(),
        resource: Resource::tool(tool),
        context: Default::default(),
        attributes: Default::default(),
        params: serde_json::json!({}),
    };
    assert!(matches!(hook.check(&mk(&["Admin"], "create_flow")).await, PolicyDecision::Allow), "admin may create_flow");
    assert!(matches!(hook.check(&mk(&["Viewer"], "create_flow")).await, PolicyDecision::Deny { .. }), "viewer may not create_flow");
    assert!(matches!(hook.check(&mk(&["Viewer"], "list_flows")).await, PolicyDecision::Allow), "viewer may list_flows (reads open)");
}

#[tokio::test]
async fn resource_management_requires_admin_role() {
    // E2 #5 Phase C: managing a (non-cedar) resource requires the `Admin` role —
    // the first real role-based rule. A viewer/operator cannot; an admin can.
    let hook = CedarHook::baseline().unwrap();
    let admin = write_resource(user_with_roles(&["Admin"]), "llm");
    assert!(matches!(hook.check(&admin).await, PolicyDecision::Allow), "admin may write an llm resource");
    let viewer = write_resource(user_with_roles(&["Viewer"]), "llm");
    assert!(matches!(hook.check(&viewer).await, PolicyDecision::Deny { .. }), "viewer may not write resources");
}

// ── E2 #2c: registry policies layer on top of the baked baseline (§6.3) ──

#[tokio::test]
async fn registry_policy_layers_on_baseline() {
    // An InfoSec-authored `type_ref: cedar` resource that forbids a specific
    // tool by name. Loaded alongside the baseline, its forbid applies — while
    // the baseline's own guardrails (e.g. destructive-tool forbid) still hold.
    let registry = vec![r#"
forbid(principal, action == Action::"tool:invoke", resource)
when { resource has name && resource.name == "call:wire-transfer" };
"#
    .to_string()];
    let hook = CedarHook::with_registry_policies(&registry, None, &[]).unwrap();

    // Baseline still lets a human call an ordinary safe tool…
    let ok = ExecutionRequest {
        principal: human(),
        action: actions::tool_invoke(),
        resource: Resource::tool("call:llm"),
        context: Default::default(),
        attributes: Default::default(),
        params: serde_json::json!({}),
    };
    assert!(matches!(hook.check(&ok).await, PolicyDecision::Allow));

    // …but the attached registry policy forbids the named tool for everyone.
    let blocked = ExecutionRequest {
        principal: human(),
        action: actions::tool_invoke(),
        resource: Resource::tool("call:wire-transfer"),
        context: Default::default(),
        attributes: Default::default(),
        params: serde_json::json!({}),
    };
    match hook.check(&blocked).await {
        PolicyDecision::Deny { policy_ids, .. } => assert!(!policy_ids.is_empty()),
        other => panic!("attached registry forbid must deny the named tool — got {other:?}"),
    }
}

#[tokio::test]
async fn baseline_holds_with_no_registry_policies() {
    // The zero-attachment case degrades to exactly the baseline pack.
    let hook = CedarHook::with_registry_policies(&[], None, &[]).unwrap();
    assert!(matches!(hook.check(&req(human(), false)).await, PolicyDecision::Allow));
    assert!(matches!(hook.check(&req(human(), true)).await, PolicyDecision::Deny { .. }));
}

// ── E2 #2c-ii: fine-grained ATTACHMENT (§6.3) ──
// Rules stay central; a flow opts in via `metadata.a3t.policyRef` (explicit) or
// carries a `metadata.a3t.tags` entry a policy targets (selector). Both ride as
// `principal.template.*` (the whole `metadata.a3t` blob, threaded by #2a), so the
// central rule self-scopes and only fires for the flows it's attached to.

/// A tool-invoke request whose acting flow carries the given `metadata.a3t`
/// template blob (the shape the executor lifts onto `principal.template.*`).
fn tool_req_with_template(tool: &str, template: serde_json::Value) -> ExecutionRequest {
    ExecutionRequest {
        principal: agent_for_user(),
        action: actions::tool_invoke(),
        resource: Resource::tool(tool),
        context: Default::default(),
        attributes: serde_json::json!({ "template": template })
            .as_object()
            .unwrap()
            .clone(),
        params: serde_json::json!({}),
    }
}

#[tokio::test]
async fn explicit_policy_ref_scopes_the_forbid() {
    // A registry policy that only fires for flows referencing it by name.
    let registry = vec![r#"
forbid(principal, action == Action::"tool:invoke", resource)
when {
  resource has name && resource.name == "call:wire-transfer" &&
  principal has template && principal.template has policyRef &&
  principal.template.policyRef.contains("no-wire-transfer")
};
"#
    .to_string()];
    let hook = CedarHook::with_registry_policies(&registry, None, &[]).unwrap();

    // Flow that opted in → the named tool is denied.
    let attached = tool_req_with_template(
        "call:wire-transfer",
        serde_json::json!({ "policyRef": ["no-wire-transfer"] }),
    );
    assert!(matches!(hook.check(&attached).await, PolicyDecision::Deny { .. }),
        "flow that references the policy is bound by it");

    // Flow that did NOT opt in → the same tool call is allowed (baseline permit).
    let unattached = tool_req_with_template(
        "call:wire-transfer",
        serde_json::json!({ "policyRef": ["some-other-policy"] }),
    );
    assert!(matches!(hook.check(&unattached).await, PolicyDecision::Allow),
        "flow that does not reference the policy is not bound by it");
}

#[tokio::test]
async fn tag_selector_scopes_the_forbid() {
    // A registry policy that auto-attaches to any flow tagged `pii`.
    let registry = vec![r#"
forbid(principal, action == Action::"tool:invoke", resource)
when {
  resource has name && resource.name == "call:external-http" &&
  principal has template && principal.template has tags &&
  principal.template.tags.contains("pii")
};
"#
    .to_string()];
    let hook = CedarHook::with_registry_policies(&registry, None, &[]).unwrap();

    let pii_flow = tool_req_with_template(
        "call:external-http",
        serde_json::json!({ "tags": ["pii", "prod"] }),
    );
    assert!(matches!(hook.check(&pii_flow).await, PolicyDecision::Deny { .. }),
        "pii-tagged flow is bound by the tag-selected policy");

    let plain_flow = tool_req_with_template(
        "call:external-http",
        serde_json::json!({ "tags": ["prod"] }),
    );
    assert!(matches!(hook.check(&plain_flow).await, PolicyDecision::Allow),
        "untagged flow is not bound by the tag-selected policy");
}

// ── E2 #3: agent template ceiling — capability = exposed ∩ enabled (§6.5) ──

/// A tool-invoke carrying a flow template with the given exposed/enabled sets,
/// run under `principal` (the flow's principal — any kind).
fn tool_req_as(principal: Principal, tool: &str, template: serde_json::Value) -> ExecutionRequest {
    ExecutionRequest {
        principal,
        action: actions::tool_invoke(),
        resource: Resource::tool(tool),
        context: Default::default(),
        attributes: serde_json::json!({ "template": template })
            .as_object()
            .unwrap()
            .clone(),
        params: serde_json::json!({}),
    }
}

/// The common case: the acting principal is the flow's agent.
fn agent_tool_req(tool: &str, template: serde_json::Value) -> ExecutionRequest {
    tool_req_as(agent_for_user(), tool, template)
}

#[tokio::test]
async fn llm_call_and_flow_start_are_permitted_and_not_tool_ceiling_gated() {
    // Regression: `call:llm`→`llm:call` on `Llm`, `run:workflow`→`flow:start` on
    // `Flow`. These must (a) not fault the schema (their entity types + actions are
    // declared) and (b) be allowed by the baseline — and the *tool* ceiling
    // (exposedTools) must NOT bite them, even when the flow declares exposedTools,
    // because exposedTools gates tools, not models/subflows.
    let hook = CedarHook::baseline().unwrap();
    let tmpl = serde_json::json!({ "template": { "exposedTools": ["search_docs"] } });
    let attrs = tmpl.as_object().unwrap().clone();

    let llm = ExecutionRequest {
        principal: agent_for_user(),
        action: actions::llm_call(),
        resource: Resource::new("Llm", "default-llm"),
        context: Default::default(),
        attributes: attrs.clone(),
        params: serde_json::json!({}),
    };
    assert!(matches!(hook.check(&llm).await, PolicyDecision::Allow), "llm:call allowed, not tool-ceiling gated");

    let flow = ExecutionRequest {
        principal: agent_for_user(),
        action: actions::flow_start(),
        resource: Resource::new("Flow", "default/child"),
        context: Default::default(),
        attributes: attrs,
        params: serde_json::json!({}),
    };
    assert!(matches!(hook.check(&flow).await, PolicyDecision::Allow), "flow:start allowed, not tool-ceiling gated");
}

#[tokio::test]
async fn flow_ceiling_binds_any_principal_not_just_agents() {
    // Workflow model: the ceiling bounds the *flow*, so it applies to whatever
    // principal runs the flow — a Human here, not only an Agent. A step outside
    // the flow's declared surface is denied regardless of who runs it.
    let hook = CedarHook::baseline().unwrap();
    let tmpl = serde_json::json!({ "exposedTools": ["search_docs"] });
    let human_flow = Principal::Human { user_id: "alice".into(), roles: vec![] };
    assert!(matches!(
        hook.check(&tool_req_as(human_flow.clone(), "search_docs", tmpl.clone())).await,
        PolicyDecision::Allow
    ), "in-surface tool allowed for a human-run flow");
    assert!(matches!(
        hook.check(&tool_req_as(human_flow, "delete_flow", tmpl)).await,
        PolicyDecision::Deny { .. }
    ), "out-of-surface tool denied for a human-run flow");
}

#[tokio::test]
async fn template_ceiling_enforces_exposed_intersect_enabled() {
    // exposed = {call:llm, call:mcp, call:sql}; enabled = {call:llm, call:mcp}.
    // Capability = the intersection = {call:llm, call:mcp}.
    let hook = CedarHook::baseline().unwrap();
    let tmpl = serde_json::json!({
        "exposedTools": ["call:llm", "call:mcp", "call:sql"],
        "enabledTools": ["call:llm", "call:mcp"]
    });

    // In both sets → allowed.
    assert!(matches!(
        hook.check(&agent_tool_req("call:llm", tmpl.clone())).await,
        PolicyDecision::Allow
    ), "tool in exposed∩enabled is allowed");

    // Outside the capability, run by the agent's OWNER → asked, not refused.
    // The ceiling is how the owner configured their own agent, so widening it is
    // a question they are entitled to answer. `agent_for_user()` is owned by the
    // human it acts for.
    assert!(matches!(
        hook.check(&agent_tool_req("call:sql", tmpl.clone())).await,
        PolicyDecision::Ask { .. }
    ), "exposed-but-not-enabled tool asks its owner");
    assert!(matches!(
        hook.check(&agent_tool_req("call:shell", tmpl.clone())).await,
        PolicyDecision::Ask { .. }
    ), "tool outside the ceiling asks its owner");

    // Same agent, same tools, run by SOMEONE ELSE → denied. A user who did not
    // configure the agent cannot talk it out of its limits, so the ceiling is
    // still a real boundary for everyone but its owner.
    let not_the_owner = Principal::Agent {
        agent_id: AgentId("pc-crawler-7".into()),
        on_behalf_of: Some("user::someone-else".into()),
        flow_id: Some("acme/campaign".into()),
        roles: vec![],
        owner: Some("user::sriram".into()),
    };
    assert!(matches!(
        hook.check(&tool_req_as(not_the_owner, "call:shell", tmpl.clone())).await,
        PolicyDecision::Deny { .. }
    ), "a non-owner must not be able to widen the agent's ceiling");
}

#[tokio::test]
async fn agent_without_ceiling_is_unconstrained() {
    // CE parity: an agent that declares no exposedTools/enabledTools is not
    // narrowed by the ceiling — only the other baseline guardrails apply.
    let hook = CedarHook::baseline().unwrap();
    // No template at all.
    assert!(matches!(
        hook.check(&req(agent_for_user(), false)).await,
        PolicyDecision::Allow
    ), "agent with no template ceiling may call a safe tool");
    // Template present but without the ceiling sets → still unconstrained.
    let tmpl = serde_json::json!({ "runtimePolicy": "standard" });
    assert!(matches!(
        hook.check(&agent_tool_req("call:anything", tmpl)).await,
        PolicyDecision::Allow
    ), "template without exposed/enabled sets does not constrain");
}

#[tokio::test]
async fn template_ceiling_still_bows_to_destructive_guardrail() {
    // Even a tool inside the ceiling is denied if it's destructive — the
    // org-wide guardrail wins (capability ∩ org guardrails).
    let hook = CedarHook::baseline().unwrap();
    let tmpl = serde_json::json!({ "exposedTools": ["call:rotate"], "enabledTools": ["call:rotate"] });
    assert!(matches!(
        hook.check(&agent_tool_req("call:rotate", tmpl)).await,
        PolicyDecision::Deny { .. }
    ), "destructive tool denied even when the ceiling would allow it");
}

#[tokio::test]
async fn default_policy_applies_regardless_of_attachment() {
    // A default (no policyRef/tags guard) fires for every flow, attached or not.
    let registry = vec![r#"
forbid(principal, action == Action::"tool:invoke", resource)
when { resource has name && resource.name == "call:danger" };
"#
    .to_string()];
    let hook = CedarHook::with_registry_policies(&registry, None, &[]).unwrap();

    // No template attachment at all → default still applies.
    let bare = ExecutionRequest {
        principal: human(),
        action: actions::tool_invoke(),
        resource: Resource::tool("call:danger"),
        context: Default::default(),
        attributes: Default::default(),
        params: serde_json::json!({}),
    };
    assert!(matches!(hook.check(&bare).await, PolicyDecision::Deny { .. }));
}

// ── Data-operation RBAC (op_policy framework) ──
// A DB-shaped handler (call:lancedb and any future backend) declares its
// read/write/delete bucket via `flowr_policy::op_policy`, which lands as the
// `operation` attribute on the Tool resource. These prove the baseline forbid
// keys off it, so operations *within one handler* are distinguishable — the
// gap that made a Viewer's "list template" and "delete template" identical.

fn db_step(principal: Principal, operation: &str) -> ExecutionRequest {
    // Exactly what the executor choke point emits for a step carrying
    // `metadata.a3t.policy` — the handler contributes nothing but its task type.
    let annotation = serde_json::json!({
        "op": operation,
        "resource": { "type": "Template", "id": "tpl-1" }
    });
    let sp = flowr_policy::op_policy::StepPolicy::parse(&annotation)
        .expect("annotation parses");
    ExecutionRequest {
        principal,
        action: actions::tool_invoke(),
        resource: sp.apply(Resource::tool("call:lancedb")),
        context: Default::default(),
        attributes: Default::default(),
        params: serde_json::json!({}),
    }
}

#[tokio::test]
async fn viewer_may_read_but_not_write_or_delete_through_a_db_handler() {
    let hook = CedarHook::baseline().unwrap();
    let viewer = || user_with_roles(&["Viewer"]);
    assert!(
        matches!(hook.check(&db_step(viewer(), "read")).await, PolicyDecision::Allow),
        "viewer may read (call:lancedb query)"
    );
    assert!(
        matches!(hook.check(&db_step(viewer(), "read")).await, PolicyDecision::Allow),
        "viewer may read (call:lancedb list_tables)"
    );
    match hook.check(&db_step(viewer(), "write")).await {
        PolicyDecision::Deny { policy_ids, reason } => {
            assert!(!policy_ids.is_empty(), "deny cites the operation forbid: {reason}");
        }
        other => panic!("viewer must not write through call:lancedb — got {other:?}"),
    }
    assert!(
        matches!(hook.check(&db_step(viewer(), "delete")).await, PolicyDecision::Deny { .. }),
        "viewer must not delete through call:lancedb"
    );
}

#[tokio::test]
async fn admin_may_write_and_delete_through_a_db_handler() {
    // The exemption must not over-block legitimate admins.
    let hook = CedarHook::baseline().unwrap();
    for role in ["Admin", "SuperAdmin"] {
        for op in ["read", "write", "delete"] {
            assert!(
                matches!(hook.check(&db_step(user_with_roles(&[role]), op)).await, PolicyDecision::Allow),
                "{role} may perform {op}"
            );
        }
    }
}

#[tokio::test]
async fn agents_have_no_exemption_from_the_operation_forbid() {
    // Agent/Service entities carry no `roles` attribute, so the `unless` clause
    // can never satisfy for them — an LLM-driven agent cannot write or delete
    // through a DB handler even when acting on behalf of a user, while reads
    // still flow. Same shape as the destructive-tool guardrail.
    let hook = CedarHook::baseline().unwrap();
    assert!(
        matches!(hook.check(&db_step(agent_for_user(), "read")).await, PolicyDecision::Allow),
        "agent may still read"
    );
    assert!(
        matches!(hook.check(&db_step(agent_for_user(), "delete")).await, PolicyDecision::Deny { .. }),
        "agent must not delete through a DB handler"
    );
    assert!(
        matches!(hook.check(&db_step(Principal::service("flow-runtime"), "write")).await, PolicyDecision::Deny { .. }),
        "service must not write through a DB handler"
    );
}

#[tokio::test]
async fn unknown_operation_fails_closed() {
    // A typo'd bucket in the YAML (`op: delet`) must not silently drop the
    // `operation` attribute and disable the gate — it resolves to the most
    // restrictive bucket instead.
    let hook = CedarHook::baseline().unwrap();
    assert!(
        matches!(
            hook.check(&db_step(user_with_roles(&["Viewer"]), "delet")).await,
            PolicyDecision::Deny { .. }
        ),
        "a malformed op must fail closed for a viewer"
    );
    // ...and an admin is still allowed, so it degrades to Delete, not to a hard block.
    assert!(matches!(
        hook.check(&db_step(user_with_roles(&["Admin"]), "delet")).await,
        PolicyDecision::Allow
    ));
}

// ── Chat transcript access (session persistence) ──────────────────────────
// Sessions became owner-attributed so they follow a user across devices. The
// flip side is that a transcript is now readable by *someone*, so the rules that
// decide who must be pinned down: owners, plus SuperAdmin/SecurityAdmin for
// incident review, and nobody else.

fn session_req(principal: Principal, action: flowr_policy::Action, owner: &str, asker_is_owner: bool)
    -> ExecutionRequest {
    ExecutionRequest {
        principal,
        action,
        resource: Resource::new("Session", "chat-abc")
            .with_attr("owner", owner)
            .with_attr("is_owner", asker_is_owner),
        context: Default::default(),
        attributes: Default::default(),
        params: serde_json::json!({}),
    }
}

fn viewer(id: &str) -> Principal {
    Principal::Human { user_id: id.into(), roles: vec!["Viewer".into()] }
}

#[tokio::test]
async fn owner_may_read_and_delete_their_own_session() {
    let hook = CedarHook::baseline().unwrap();
    for action in [actions::session_read(), actions::session_delete()] {
        assert!(matches!(
            hook.check(&session_req(viewer("alice"), action.clone(), "alice", true)).await,
            PolicyDecision::Allow
        ), "owner must be allowed to {}", action.as_str());
    }
}

#[tokio::test]
async fn a_different_user_may_not_read_or_delete_someone_elses_session() {
    let hook = CedarHook::baseline().unwrap();
    for action in [actions::session_read(), actions::session_delete()] {
        assert!(matches!(
            hook.check(&session_req(viewer("bob"), action.clone(), "alice", false)).await,
            PolicyDecision::Deny { .. }
        ), "a non-owner must not {}", action.as_str());
    }
}

#[tokio::test]
async fn infosec_may_read_any_session_for_audit() {
    let hook = CedarHook::baseline().unwrap();
    for role in ["SuperAdmin", "SecurityAdmin"] {
        let p = Principal::Human { user_id: "auditor".into(), roles: vec![role.into()] };
        assert!(matches!(
            hook.check(&session_req(p, actions::session_read(), "alice", false)).await,
            PolicyDecision::Allow
        ), "{role} must be able to read another user's session");
    }
}

#[tokio::test]
async fn infosec_read_access_does_not_extend_to_deleting_someone_elses_session() {
    // The audit exception is read-only on purpose: an auditor reviewing a
    // transcript must not be able to destroy the evidence.
    let hook = CedarHook::baseline().unwrap();
    let p = Principal::Human { user_id: "auditor".into(), roles: vec!["SuperAdmin".into()] };
    assert!(matches!(
        hook.check(&session_req(p, actions::session_delete(), "alice", false)).await,
        PolicyDecision::Deny { .. }
    ));
}

#[tokio::test]
async fn plain_admin_is_not_an_auditor() {
    // Admin governs the registry, not other people's conversations.
    let hook = CedarHook::baseline().unwrap();
    let p = Principal::Human { user_id: "ops".into(), roles: vec!["Admin".into()] };
    assert!(matches!(
        hook.check(&session_req(p, actions::session_read(), "alice", false)).await,
        PolicyDecision::Deny { .. }
    ));
}

#[tokio::test]
async fn an_agent_may_not_read_a_users_session_even_on_their_behalf() {
    // Transcript access is a human decision; an LLM-driven principal has no
    // ownership and no role, so it gets no path in.
    let hook = CedarHook::baseline().unwrap();
    assert!(matches!(
        hook.check(&session_req(agent_for_user(), actions::session_read(), "sriram", false)).await,
        PolicyDecision::Deny { .. }
    ));
}

// ── Interactive approval: how an allow/deny engine produces a question ────────
//
// Cedar never says "ask". `Ask` is derived: when the AGENT is denied, the hook
// re-runs the identical request as the human it acts for. Allowed for them but
// not for the agent = a delegation the human may widen, so it becomes a prompt.
// Denied for both = a real denial. These tests pin that distinction, because
// getting it wrong turns the chat approval card into a privilege-escalation path.

fn agent_with_roles(roles: &[&str]) -> Principal {
    Principal::Agent {
        agent_id: AgentId("dev-agent".into()),
        on_behalf_of: Some("usr-sriram".into()),
        flow_id: Some("a3t/dev".into()),
        roles: roles.iter().map(|r| r.to_string()).collect(),
        owner: Some("usr-sriram".into()),
    }
}

#[tokio::test]
async fn agent_exceeding_an_admins_delegation_is_a_question_not_a_denial() {
    // Destructive tool: the baseline forbids it for agents outright, but exempts
    // Admin humans. The agent inherits Admin, so the human behind it COULD do
    // this by hand — exactly the case the dev-time prompt exists for.
    let hook = CedarHook::baseline().unwrap();
    match hook.check(&req(agent_with_roles(&["Admin"]), true)).await {
        PolicyDecision::Ask { scopes, reason, .. } => {
            assert!(!scopes.is_empty(), "an Ask must offer at least one grant scope");
            assert!(
                scopes.contains(&flowr_policy::GrantScope::Session),
                "dev-time default scope must be on offer: {scopes:?}"
            );
            assert!(reason.contains("authorize"), "reason should say it can be approved: {reason}");
        }
        other => panic!("expected Ask for an Admin's agent — got {other:?}"),
    }
}

#[tokio::test]
async fn agent_of_a_viewer_is_denied_outright_never_asked() {
    // The escalation guard. A Viewer cannot run a destructive tool themselves,
    // so their agent must not be able to prompt them into granting one — that
    // would let the chat box manufacture authority nobody held.
    let hook = CedarHook::baseline().unwrap();
    match hook.check(&req(agent_with_roles(&["Viewer"]), true)).await {
        PolicyDecision::Deny { .. } => {}
        other => panic!("a Viewer's agent must be denied, never asked — got {other:?}"),
    }
}

#[tokio::test]
async fn a_triggered_run_is_never_asked() {
    // Requirement: an event-triggered agent stands on its own policy. With no
    // human behind it there is nobody to prompt, so it must deny rather than
    // block forever waiting for an approval that cannot arrive.
    let hook = CedarHook::baseline().unwrap();
    match hook.check(&req(agent_ambient(), true)).await {
        PolicyDecision::Deny { .. } => {}
        other => panic!("an ambient/triggered agent must be denied — got {other:?}"),
    }
}

// ── Removing a tool from an agent must ASK, not just fail ────────────────────
//
// The reported case: take `list_flows` out of an agent's exposed tools, run it,
// and expect a prompt. It produced a flat denial instead, because the approver
// re-evaluation inherited the agent's own `template.exposedTools` and so hit the
// very same capability-ceiling forbid. The agent's configured ceiling is not a
// fact about the human, and the owner is the authority over it.

fn agent_with_ceiling(roles: &[&str], exposed: &[&str]) -> (Principal, serde_json::Map<String, serde_json::Value>) {
    let p = Principal::Agent {
        agent_id: AgentId("mytestagent".into()),
        on_behalf_of: Some("usr-sriram".into()),
        flow_id: Some("a3t/mytestagent".into()),
        roles: roles.iter().map(|r| r.to_string()).collect(),
        owner: Some("usr-sriram".into()),
    };
    let attrs = serde_json::json!({
        "template": { "exposedTools": exposed }
    })
    .as_object()
    .unwrap()
    .clone();
    (p, attrs)
}

fn tool_req(
    principal: Principal,
    attributes: serde_json::Map<String, serde_json::Value>,
    tool: &str,
) -> ExecutionRequest {
    ExecutionRequest {
        principal,
        action: flowr_policy::actions::mcp_call(),
        resource: flowr_policy::Resource::tool(tool),
        attributes,
        context: Default::default(),
        params: serde_json::json!({}),
    }
}

#[tokio::test]
async fn a_tool_removed_from_the_agent_asks_its_owner() {
    let hook = CedarHook::baseline().unwrap();
    // `list_flows` has been taken out of the agent's exposed tools.
    let (p, attrs) = agent_with_ceiling(&["Admin"], &["get_flow"]);
    match hook.check(&tool_req(p, attrs, "list_flows")).await {
        PolicyDecision::Ask { .. } => {}
        other => panic!("removing a tool from an agent should ask its owner — got {other:?}"),
    }
}

#[tokio::test]
async fn a_ceiling_prompt_does_not_need_admin() {
    // `list_flows` is not destructive and is not an authoring tool, so any user
    // may run it. A plain user's agent should therefore be asked, not refused —
    // the ceiling is the agent's configuration, not a privilege boundary.
    let hook = CedarHook::baseline().unwrap();
    let (p, attrs) = agent_with_ceiling(&[], &["get_flow"]);
    match hook.check(&tool_req(p, attrs, "list_flows")).await {
        PolicyDecision::Ask { .. } => {}
        other => panic!("a non-destructive tool outside the ceiling should ask — got {other:?}"),
    }
}

#[tokio::test]
async fn dropping_the_ceiling_does_not_loosen_org_policy() {
    // The containment check for the fix. Org-level rules key off roles and
    // resource attributes, never `template`, so they must still bind the approver
    // even though the ceiling no longer does.
    let hook = CedarHook::baseline().unwrap();

    // Destructive tool, agent acting for a plain user: the destructive forbid
    // binds the human too, so this stays a denial rather than becoming askable.
    let (p, attrs) = agent_with_ceiling(&["Viewer"], &["get_flow"]);
    match hook.check(&tool_req(p, attrs, "delete_flow")).await {
        PolicyDecision::Deny { .. } => {}
        other => panic!("a Viewer's agent must not be able to ask for a destructive tool — got {other:?}"),
    }

    // Authoring tool, agent acting for a non-Admin: the authoring RBAC forbid is
    // scoped to `principal is User`, so the approver is caught by it.
    let (p2, attrs2) = agent_with_ceiling(&[], &["get_flow"]);
    match hook.check(&tool_req(p2, attrs2, "create_agent")).await {
        PolicyDecision::Deny { .. } => {}
        other => panic!("a non-Admin must not be asked to authorize authoring — got {other:?}"),
    }
}
