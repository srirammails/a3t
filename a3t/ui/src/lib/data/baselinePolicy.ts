// AUTO-GENERATED from flowr-aesop/policies/a3t/baseline.cedar — the exact file
// compiled into the EE binary via include_str!. Shown read-only on /security so
// operators can see the deny-by-default baseline that governs the runtime.
// Regenerate if baseline.cedar changes.

export const BASELINE_POLICY = `// A3T EE baseline policy pack (v1). Deny-by-default: anything not permitted
// here is denied. \`forbid\` overrides \`permit\`.

// The runtime step actions a flow performs: invoke a tool / MCP tool, call an
// LLM, start a subflow. Deny-by-default otherwise. \`forbid\`s below (destructive
// tools, flow capability ceiling) still override these permits.

// Humans may perform any step action.
permit(
  principal,
  action in [Action::"tool:invoke", Action::"mcp:call", Action::"llm:call", Action::"flow:start"],
  resource
) when { principal is User };

// Agents may — but only when acting on behalf of a user (never on ambient
// trust). This is the \`on_behalf_of\` guardrail.
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

// Destructive tools are forbidden for everyone in the baseline pack. A tighter
// pack can scope this to trust tier; here it is a blanket guardrail that
// overrides the permits above (forbid wins).
forbid(
  principal,
  action,
  resource
) when { resource has destructive && resource.destructive };

// ── Flow capability ceiling (E2 #3 → workflow model, §6.5) ──
// A flow declares its capability surface in \`metadata.a3t.{exposedTools,
// enabledTools}\`, which the runtime threads onto \`principal.template.*\` (#2a).
// Capability = exposed ∩ enabled: any step (its real target now named by
// \`policy_resource\`) must be in BOTH declared sets. This bounds the *flow*, not
// an "agent" — it applies to every principal running the flow, so an LLM-chosen
// step is confined exactly like an author-declared one. A flow that declares no
// sets is unconstrained by this rule (CE parity). \`forbid\` overrides the permits.
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

// ── Registry governance (E2 #2b §6.4, tightened by E2 #5 Phase C RBAC) ──
// Managing registry resources (LLM/MCP/agent config, …) requires the \`Admin\`
// role — the first real role-based rule (Casbin never enforced any). The AuthN
// layer expands the hierarchy, so a superadmin holds \`Admin\`; operators/viewers
// do not. Writing a \`type == "cedar"\` resource additionally requires InfoSec
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
// may create/update/delete a \`type == "cedar"\` resource unless they hold the
// SecurityAdmin or SuperAdmin role — an app builder cannot rewrite the policy
// that governs them. \`forbid\` overrides the \`permit\` above.
forbid(
  principal,
  action in [Action::"resource:create", Action::"resource:update", Action::"resource:delete"],
  resource
)
when { resource has type && resource.type == "cedar" }
unless {
  principal is User && principal has roles &&
  // Canonical InfoSec roles (E2 #5). The AuthN layer canonicalizes the provider's
  // role claim (\`superadmin\`→SuperAdmin+SecurityAdmin) so only these names appear.
  (principal.roles.contains("SecurityAdmin") || principal.roles.contains("SuperAdmin"))
};

// ── Authoring RBAC (E2 #5 Phase C) ──
// A human user needs the \`Admin\` role to author flows/agents/templates. These
// tools are gated at the agent-tool dispatch (the site-A bypass is now closed).
// Agents/services are governed by their capability ceiling, not this human gate,
// so the forbid is scoped to \`principal is User\`.
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
`;
