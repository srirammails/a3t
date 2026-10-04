// A3T EE baseline policy pack (v1). Deny-by-default: anything not permitted
// here is denied. `forbid` overrides `permit`.

// The runtime step actions a flow performs: invoke a tool / MCP tool, call an
// LLM, start a subflow. Deny-by-default otherwise. `forbid`s below (destructive
// tools, flow capability ceiling) still override these permits.

// Humans may perform any step action.
permit(
  principal,
  action in [Action::"tool:invoke", Action::"mcp:call", Action::"llm:call", Action::"flow:start"],
  resource
) when { principal is User };

// Agents may — but only when acting on behalf of a user (never on ambient
// trust). This is the `on_behalf_of` guardrail.
permit(
  principal,
  action in [Action::"tool:invoke", Action::"mcp:call", Action::"llm:call", Action::"flow:start"],
  resource
) when { principal is Agent && principal has on_behalf_of };

// The runtime service itself. Internal flow execution runs as
// Principal::Service{"flow-runtime"} — trusted machinery, not the threat model.
// The forbids below still override this.
permit(
  principal,
  action in [Action::"tool:invoke", Action::"mcp:call", Action::"llm:call", Action::"flow:start"],
  resource
) when { principal is Service };

// Destructive tools are forbidden by default — but Admin/SuperAdmin *humans* are
// exempt (they hold the delete ACL). Agents/services (LLM-driven) and non-admin
// users stay blocked, so a destructive action always needs a trusted human. The
// SoD forbid below (Cedar `type` resources) still overrides this for InfoSec-only
// resources. `forbid` overrides the permits above.
forbid(
  principal,
  action,
  resource
)
when { resource has destructive && resource.destructive }
unless {
  principal is User && principal has roles &&
  (principal.roles.contains("Admin") || principal.roles.contains("SuperAdmin"))
};

// ── Data-operation RBAC (op_policy framework) ──
// Generalizes the destructive-tool forbid above to any handler that declares
// an `operation` attribute via flowr_policy::op_policy (today: call:lancedb;
// future DB-shaped handlers opt in the same way, no new action/schema needed).
// Agent/Service principals carry no `roles` attribute in the Cedar schema, so
// they have no exemption path here — same shape as the destructive forbid,
// applied to the new attribute. This is what makes a Viewer's "list" and a
// Viewer's "delete" through the same call:lancedb handler distinguishable.
forbid(
  principal,
  action,
  resource
)
when { resource has operation && ["write", "delete"].contains(resource.operation) }
unless {
  principal is User && principal has roles &&
  (principal.roles.contains("Admin") || principal.roles.contains("SuperAdmin"))
};

// ── Flow capability ceiling (E2 #3 → workflow model, §6.5) ──
// A flow declares its capability surface in `metadata.a3t.{exposedTools,
// enabledTools}`, which the runtime threads onto `principal.template.*` (#2a).
// Capability = exposed ∩ enabled: any step (its real target now named by
// `policy_resource`) must be in BOTH declared sets. This bounds the *flow*, not
// an "agent" — it applies to every principal running the flow, so an LLM-chosen
// step is confined exactly like an author-declared one. A flow that declares no
// sets is unconstrained by this rule (CE parity). `forbid` overrides the permits.
forbid(
  principal,
  action in [Action::"tool:invoke", Action::"mcp:call"],
  resource
) when {
  principal has template && principal.template has exposedTools &&
  resource has name &&
  !(principal.template.exposedTools.contains(resource.name))
};
forbid(
  principal,
  action in [Action::"tool:invoke", Action::"mcp:call"],
  resource
) when {
  principal has template && principal.template has enabledTools &&
  resource has name &&
  !(principal.template.enabledTools.contains(resource.name))
};

// ── Chat transcript access (session persistence) ──
// A conversation belongs to the principal that created it. Deny-by-default means
// no permit here = nobody reads anybody's transcript, so both cases are explicit.
// `is_owner` is computed at the endpoint, which is the only place that knows both
// the row's `owner` and who is asking.
permit(
  principal,
  action in [Action::"session:read", Action::"session:delete"],
  resource
) when { resource has is_owner && resource.is_owner };

// InfoSec exception, for incident review. This lets a SuperAdmin/SecurityAdmin
// read ANY user's chat history — deliberate and audited, not a convenience.
// Remove this permit to make transcripts owner-only.
permit(
  principal,
  action == Action::"session:read",
  resource
) when {
  principal is User && principal has roles &&
  (principal.roles.contains("SuperAdmin") || principal.roles.contains("SecurityAdmin"))
};

// ── Registry governance (E2 #2b §6.4, tightened by E2 #5 Phase C RBAC) ──
// Managing registry resources (LLM/MCP/agent config, …) requires the `Admin`
// role — the first real role-based rule (Casbin never enforced any). The AuthN
// layer expands the hierarchy, so a superadmin holds `Admin`; operators/viewers
// do not. Writing a `type == "cedar"` resource additionally requires InfoSec
// (SoD forbid below).
permit(
  principal,
  action in [Action::"resource:create", Action::"resource:update", Action::"resource:delete"],
  resource
) when {
  principal is User && principal has roles &&
  (principal.roles.contains("Admin") || principal.roles.contains("SecurityAdmin"))
};

// Separation of duties: Cedar *policy* resources are owned by InfoSec. Nobody
// may create/update/delete a `type == "cedar"` resource unless they hold the
// SecurityAdmin or SuperAdmin role — an app builder cannot rewrite the policy
// that governs them. `forbid` overrides the `permit` above.
forbid(
  principal,
  action in [Action::"resource:create", Action::"resource:update", Action::"resource:delete"],
  resource
)
when { resource has type && resource.type == "cedar" }
unless {
  principal is User && principal has roles &&
  // Canonical InfoSec roles (E2 #5). The AuthN layer canonicalizes the provider's
  // role claim (`superadmin`→SuperAdmin+SecurityAdmin) so only these names appear.
  (principal.roles.contains("SecurityAdmin") || principal.roles.contains("SuperAdmin"))
};

// ── Authoring RBAC (E2 #5 Phase C) ──
// A human user needs the `Admin` role to author flows/agents/templates. These
// tools are gated at the agent-tool dispatch (the site-A bypass is now closed).
// Agents/services are governed by their capability ceiling, not this human gate,
// so the forbid is scoped to `principal is User`.
forbid(
  principal,
  action in [Action::"mcp:call", Action::"tool:invoke"],
  resource
)
when {
  principal is User && resource has name &&
  ["create_flow", "create_agent", "create_agent_template", "delete_agent"].contains(resource.name)
}
unless { principal has roles && principal.roles.contains("Admin") };

// ── Deployment placement (canvas) ──
//
// Deploying is placing a flow into a runtime pool. It is deliberately NOT
// covered by the blanket step-action permits above: those let a principal RUN
// work, which is not the same authority as deciding WHERE work runs and how much
// of it. So `flow:deploy` is permitted here explicitly and narrowly.

// Any authenticated principal may deploy into a non-production pool. Test
// placement is how people work; gating it would make the audit trail the only
// benefit and the friction constant.
permit(
  principal,
  action == Action::"flow:deploy",
  resource
) when { resource has label_tier && resource.label_tier != "prod" };

// A pool with no tier at all is treated as non-production. Pools are authored
// data and an unlabelled one must not become a backdoor into prod, so this is
// paired with the forbid below rather than standing on its own.
permit(
  principal,
  action == Action::"flow:deploy",
  resource
) unless { resource has label_tier };

// Admins may place work anywhere. Deny-by-default means the forbids below cannot
// grant anything on their own — without this permit, prod and isolated placement
// would be unreachable for everyone, which is a denial dressed up as a guardrail.
// Scoped to `principal is User` deliberately: see the Ask note below.
permit(
  principal,
  action == Action::"flow:deploy",
  resource
) when {
  principal is User && principal has roles &&
  (principal.roles.contains("Admin") || principal.roles.contains("SuperAdmin"))
};

// Production placement requires Admin, and only a HUMAN may hold it.
//
// An agent acting for an Admin therefore does not get prod deployment outright —
// it gets an `Ask`, because the human behind it would be allowed. That is the
// delegation prompt, and it is the behaviour we want: a model may propose a
// production deploy, a person confirms it.
forbid(
  principal,
  action == Action::"flow:deploy",
  resource
)
when { resource has label_tier && resource.label_tier == "prod" }
unless {
  principal is User && principal has roles &&
  (principal.roles.contains("Admin") || principal.roles.contains("SuperAdmin"))
};

// Dedicated (isolated) placement also requires Admin. An isolated flow takes a
// whole runtime rather than sharing one, so it is a capacity decision as much as
// an isolation one, and it should not be reachable by asking a model nicely.
forbid(
  principal,
  action == Action::"flow:deploy",
  resource
)
when { resource has mode && resource.mode == "isolated" }
unless {
  principal is User && principal has roles &&
  (principal.roles.contains("Admin") || principal.roles.contains("SuperAdmin"))
};

// ── Operating deployments ──
//
// Pausing, resuming and undeploying are gated exactly like placing: whoever may
// put work into a pool may also stop it there. Stopping production is as
// consequential as starting it — a paused prod agent is an outage — so prod is
// Admin-only in both directions, and an agent acting for an Admin gets an Ask.
permit(
  principal,
  action in [Action::"flow:pause", Action::"flow:resume", Action::"flow:undeploy"],
  resource
) when { resource has label_tier && resource.label_tier != "prod" };

permit(
  principal,
  action in [Action::"flow:pause", Action::"flow:resume", Action::"flow:undeploy"],
  resource
) unless { resource has label_tier };

permit(
  principal,
  action in [Action::"flow:pause", Action::"flow:resume", Action::"flow:undeploy"],
  resource
) when {
  principal is User && principal has roles &&
  (principal.roles.contains("Admin") || principal.roles.contains("SuperAdmin"))
};

forbid(
  principal,
  action in [Action::"flow:pause", Action::"flow:resume", Action::"flow:undeploy"],
  resource
)
when { resource has label_tier && resource.label_tier == "prod" }
unless {
  principal is User && principal has roles &&
  (principal.roles.contains("Admin") || principal.roles.contains("SuperAdmin"))
};

// ── Runtime state ──
//
// A runtime hosts many teams' deployments, so hibernating, pausing or resuming
// it is an operator decision whatever its tier. Admins only, and only humans:
// an agent acting for an Admin gets an Ask, so a model can propose it and a
// person confirms.
permit(
  principal,
  action in [Action::"runtime:hibernate", Action::"runtime:pause", Action::"runtime:resume"],
  resource
) when {
  principal is User && principal has roles &&
  (principal.roles.contains("Admin") || principal.roles.contains("SuperAdmin"))
};
