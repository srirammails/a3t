# AESOP — Idea, Rust Implementation Strategy, and the A3T CE Contract

**Status:** Parked (design frozen, implementation deferred)
**Version:** 0.1 — July 2026
**Primary purpose of this document:** Define the seams A3T CE must leave open so that AESOP can be added later as the EE layer *without rework*. Sections 5–7 are the actionable part; everything else is context.

---

## 1. The Idea

AESOP (Agentic Enterprise Security Orchestration Platform) is a security and governance layer for agent runtimes. It provides five capabilities behind a single declarative surface:

1. **AuthN** — establishing who (or what) a principal is: human users via JWT/OIDC, agents via runtime-issued identity.
2. **AuthZ** — deciding whether a principal may perform an action on a resource, evaluated by the Cedar policy engine.
3. **Policy** — the rules themselves, authored in Cedar, validated against a schema, versioned and deployable independently of application code.
4. **Audit** — an immutable, structured record of every security-relevant decision and action, emitted over OpenTelemetry.
5. **Metrics** — allow/deny rates, decision latency, and per-action counters, also via OpenTelemetry.

The defining design principle is **annotation as facade, middleware as enforcement, Cedar as decision**. Application code declares intent (`#[require_permission(action = "media_buy:create")]`); the declaration expands into a one-line call into a runtime crate; the runtime consults Cedar and emits telemetry. Policy logic never lives in application code, so policy can change without recompiling or redeploying a single consumer.

The second defining principle, and the reason "Agentic" is in the name: **agents are first-class principals**. An agent has a stable identity, acts on behalf of a human or a tenant, and every tool call it makes is an authorizable event — `(principal: Agent::"pc-crawler-7", action: "sql:write", resource: Table::"campaigns")` — evaluated deterministically by Cedar. This is the security expression of A3T's core thesis: deterministic-first, LLM as exception handler. Prompt injection cannot escalate what policy forbids.

### Positioning

AESOP is not a standalone product today. Its commercial destiny is **the reason A3T EE exists**: A3T CE is the deterministic agent runtime; A3T EE adds Cedar-policy enforcement on every tool call, compliance-grade audit trails, and centralized policy management. It is also internal infrastructure: PubContext's agents (which touch real money via PubMatic media buys) are the first serious consumer.

---

## 2. Scope Now vs. Later

**Rust-first, Rust-only for now.** A3T and PubContext are both Rust. Every AESOP capability ships as Rust crates consumed in-process. This is not a compromise — in-process Cedar evaluation is the highest-performance deployment mode, and having two real Rust consumers is the honest test of the abstraction.

**Deferred to a later stage (explicitly out of scope):**

- Multi-language annotation libraries (Java annotations + AOP, Python decorators, Go codegen, TypeScript decorators, .NET attributes). The architecture keeps this possible — the decision core is one service boundary away from becoming a gRPC sidecar that non-Rust facades call — but no code, templates, or codegen for other languages will be written until there is a product decision.
- The codegen pipeline (OpenAPI/manifest → SDK scaffolding).
- Sidecar deployment mode and the hosted control plane ("AESOP Cloud").
- Policy hot-reload from a remote policy store. File-based policies with process restart are sufficient for two internal consumers.
- Any UI, any open-sourcing decision, any pricing.

The rule for un-parking any of these: a concrete external consumer must exist first. Speculative platform work is how three ventures become zero.

---

## 3. Architecture (Rust)

Cargo workspace, private repository, owned distinctly from A3T and PubContext (clean IP, clean dependency direction — both consume it, neither contains it):

```
aesop/
├── crates/
│   ├── aesop-core/      # Cedar engine, principal & decision types, enforce()
│   ├── aesop-tower/     # AuthnLayer, AuditLayer, MetricsLayer (tower middleware)
│   ├── aesop-macros/    # proc-macros (#[require_permission], #[audit], #[metered]) — v0.2, not v0.1
│   └── aesop-schemas/   # Cedar schemas + policies per consumer domain, validated in CI
├── schemas/
│   ├── a3t.cedarschema          # A3T tool-call actions, agent/flow entities
│   └── pubcontext.cedarschema   # PubMatic/MCP actions (media_buy:create, ...)
└── policies/
    ├── a3t/*.cedar
    └── pubcontext/*.cedar
```

### 3.1 aesop-core

The heart is small: a `SecurityContext`, an `AuthzEngine` wrapping `cedar_policy`, and one entry point.

```rust
pub struct SecurityContext {
    pub principal: Principal,          // see §5.2 — shared type with A3T CE
    pub attributes: serde_json::Map<String, serde_json::Value>,
}

pub enum Principal {
    Human { user_id: String, roles: Vec<String> },
    Agent { agent_id: AgentId, on_behalf_of: Option<String>, flow_id: Option<String> },
    Service { service_id: String },
}

impl AuthzEngine {
    pub fn enforce(
        &self,
        ctx: &SecurityContext,
        action: &Action,        // typed, not a raw string — see §5.3
        resource: &Resource,    // typed
    ) -> Result<Allowed, AesopError>;
}
```

Non-negotiable implementation details, learned from reviewing the research draft:

- **Never build Cedar `EntityUid`s by string formatting.** `format!(r#"User::"{id}""#)` is an injection vector — an id containing `"` escapes the literal. Use `EntityUid::from_type_name_and_id(EntityTypeName::from_str(..)?, EntityId::new(id))` exclusively.
- **One enforcement point per request path.** Enforcement happens in the explicit `enforce()` call (or the macro that expands to it). Tower middleware handles AuthN, audit, and metrics only — it does *not* also run authorization, or the doc's own "don't duplicate enforcement" rule is violated and Deny semantics become ambiguous.
- **Deny-by-default.** No matching policy means `Decision::Deny`. The error type distinguishes `Denied { action, resource, policy_ids }` from engine faults, and denials are never silent — every one is an audit event.
- **Schema validation in CI.** `cedar_policy::Validator` runs against every policy file in the AESOP repo's own pipeline. A policy that doesn't validate against its schema cannot merge.

### 3.2 aesop-tower

Three `tower::Layer`s, composable onto any axum router (A3T's API surface) without touching handlers:

- `AuthnLayer` — verifies JWT/OIDC (RS256, JWKS-based key resolution, not shared secrets), constructs `SecurityContext`, inserts it into request extensions. For agent-internal calls, accepts runtime-minted agent tokens.
- `AuditLayer` — emits a structured entry/exit event per request via `tracing`, carrying the standard field set (§5.4).
- `MetricsLayer` — request counters and latency histograms via `metrics`/OTel, labeled by service, action, and decision.

### 3.3 aesop-macros (v0.2, deliberately second)

`#[require_permission(action = "...", resource = "...")]`, `#[audit(...)]`, `#[metered]` — thin attribute macros whose entire expansion is a call into `aesop-core`. Built with `syn` 2 + `quote` + `darling` (note: `syn::AttributeArgs` no longer exists in syn 2; argument parsing goes through `darling`'s meta-list support). Macros are written *after* the runtime API stabilizes through v0.1 usage, because macros calcify whatever API exists when they are written. Until then, the explicit `enforce()` call at the top of a handler is the supported pattern and is equally safe.

### 3.4 Cedar schema sketch (A3T domain)

```cedarschema
entity Agent { flow: String, on_behalf_of?: String, trust_tier: Long };
entity Human { roles: Set<String> };
entity Tool { category: String, destructive: Bool };
entity Flow { owner: String };

action "tool:invoke" appliesTo {
    principal: [Agent, Human],
    resource: [Tool],
    context: { "run_id": String, "step": Long }
};
action "flow:start", "flow:cancel" appliesTo {
    principal: [Agent, Human],
    resource: [Flow]
};
```

Example policy — the "agents never do destructive things unattended" guardrail:

```cedar
forbid(principal, action == Action::"tool:invoke", resource)
when { principal is Agent && resource.destructive && principal.trust_tier < 3 };
```

---

## 4. Rust Implementation Phases (when un-parked)

**v0.1 — Core + middleware (~2–3 weeks of focused work).** `aesop-core` with Cedar in-process, typed `Action`/`Resource`/`Principal`, `enforce()`, deny-by-default; `aesop-tower` with the three layers; A3T and PubContext schemas + starter policies; CI policy validation; integration tests proving an A3T tool call and a PubContext `media_buy:create` are allowed/denied per policy.

**v0.2 — Macros.** The three attribute macros, once v0.1's API has survived real usage in both consumers.

**v0.3 — Operability.** Policy hot-reload (file-watch first, remote store only if a real need appears), break-glass deny policies, decision-log sampling controls.

**Next stage (product decision required):** gRPC decision service extraction, non-Rust annotation facades generated from the schema + manifest, control plane. The design above keeps every one of these reachable; none of them is started early.

---

## 5. The A3T CE Contract — how CE must be built to plug AESOP in

This is the reason this document exists. AESOP is parked; these five seams are **not** — they are CE requirements, cheap now and expensive to retrofit. The test for each: *EE must be installable as an added dependency plus configuration, never as a refactor.*

### 5.1 Seam 1 — One choke point for tool execution

Every tool invocation in FlowR, no exceptions — agent-initiated, flow-step-initiated, canvas-triggered, retries, sub-agent delegation — flows through a single function:

```rust
pub struct ExecutionRequest {
    pub principal: Principal,       // who is acting (§5.2)
    pub action: Action,             // what they're doing (§5.3)
    pub resource: Resource,         // on what
    pub context: ExecContext,       // run_id, flow_id, step, attempt
    pub params: serde_json::Value,  // tool arguments
}

#[async_trait]
pub trait ToolExecutor {
    async fn execute(&self, req: ExecutionRequest) -> Result<ToolOutput, ExecError>;
}
```

If tool calls are scattered across the codebase, EE enforcement means touching everything. If they converge here, EE is a wrapper. **CE acceptance test:** grep for tool invocation sites; there is exactly one call path into tool execution.

Note that `ExecutionRequest` *is* a Cedar request in disguise — principal, action, resource, context. That is deliberate. CE defines this shape; AESOP later consumes it unchanged.

### 5.2 Seam 2 — The PolicyHook trait, defined in CE, no-op in CE

CE ships the enforcement interface and a default implementation that allows everything:

```rust
#[async_trait]
pub trait PolicyHook: Send + Sync {
    async fn check(&self, req: &ExecutionRequest) -> PolicyDecision;
}

pub enum PolicyDecision {
    Allow,
    Deny { reason: String, policy_ids: Vec<String> },
}

/// CE default. EE swaps in aesop_core::CedarHook via configuration.
pub struct AllowAll;

#[async_trait]
impl PolicyHook for AllowAll {
    async fn check(&self, _req: &ExecutionRequest) -> PolicyDecision {
        PolicyDecision::Allow
    }
}
```

The FlowR runtime calls `hook.check(&req)` before every `execute()` — in CE this costs one virtual call and always allows. Crucially, **the `Deny` path exists and is handled in CE**: `ExecError::Denied { .. }` is a defined variant, flows have defined semantics for a denied step (fail the step, surface to canvas, don't retry destructive actions), and tests exercise it with a mock denying hook. EE then changes *which decisions occur*, never *what happens when they occur*.

The hook is wired through the global settings/DI mechanism (§5.6): `security.policy_hook = "allow_all" | "cedar"` — the entire CE→EE enforcement upgrade is that one setting plus a policies directory.

### 5.3 Seam 3 — Identity and naming from day one

**Agents get stable identities.** Every agent instance carries an `AgentId` (stable across a run, attributable across runs of the same configured agent), plus `on_behalf_of` (the human or tenant it acts for) and its `flow_id`. These thread through `Principal::Agent { .. }` into every `ExecutionRequest` and every trace span. Cedar cannot authorize an anonymous principal, and an audit trail without actors is decoration. Retrofitting identity into a runtime is a data-model migration; adding it now is three struct fields.

**Actions and resources are typed and namespaced.** A newtype'd `Action` following `"{domain}:{verb}"` (`tool:invoke`, `flow:start`, `mcp:call`, `media_buy:create`) and a `Resource` carrying a type name and id — never free-form strings assembled at call sites. This naming convention is the shared vocabulary between CE's runtime, the trace fields, and AESOP's Cedar schema; if it drifts per call site, no policy can ever be written against it. Define the enum/registry in CE, document it, and treat additions as reviewed changes.

### 5.4 Seam 4 — Structured tracing everywhere, standard field names

CE emits `tracing` spans in a fixed hierarchy — `flow_run` → `flow_step` → `tool_call` — with a standard field set on every security-relevant span:

```
aesop.principal.kind    = "agent" | "human" | "service"
aesop.principal.id      = "pc-crawler-7"
aesop.on_behalf_of      = "user::sriram"        (when present)
aesop.action            = "tool:invoke"
aesop.resource.type     = "Tool"
aesop.resource.id       = "http_fetch"
aesop.decision          = "allow" | "deny"       (CE always "allow")
aesop.flow_id / aesop.run_id / aesop.step
```

CE users get first-class observability for free — this is a CE feature, not overhead. For EE, the audit trail becomes *a different subscriber on events CE already emits* (an OTel exporter with retention and PII redaction), not new instrumentation. Namespacing the fields `aesop.*` now means zero renaming later.

### 5.5 Seam 5 — AuthN at the edge, pass-through in the core

CE's API surface (axum) authenticates humans however CE chooses (even a simple token in early builds), but it does so **in a tower layer that constructs a `SecurityContext` and stores it in request extensions** — never inline in handlers. Handlers and the runtime read `SecurityContext` from extensions and propagate it; they never parse tokens. Swapping CE's simple authn for `aesop_tower::AuthnLayer` (JWT/OIDC/JWKS) is then a one-line router change. Similarly, agent-to-runtime calls carry the agent's `Principal` explicitly rather than being trusted ambiently, so agent authn can harden later without touching call sites.

### 5.6 Seam 6 — Global settings as a namespaced registry

Since global settings are already in CE scope: make the settings system a namespaced key registry (`runtime.*`, `canvas.*`, `security.*`) where unknown namespaces are tolerated and EE can register `security.*` keys (`security.policy_hook`, `security.policies_dir`, `security.audit_exporter`) without CE schema changes. Settings resolution order (defaults → file → env) defined once in CE.

### What CE must NOT do (anti-goals)

- No authorization logic in CE — no `if agent.name == "trusted"` scattered in the runtime. All such judgment is either absent (CE) or Cedar's (EE). If a CE feature needs a permission check, it defines an `Action` and calls the hook.
- No security semantics baked into individual tools — a tool declares metadata (`destructive: bool`, category); it never decides who may call it.
- No free-form principal/action/resource strings at call sites.
- No swallowing of `Deny` — the denied path is always surfaced and audited, even in CE where it can only come from tests.

### CE "AESOP-ready" acceptance checklist

1. Exactly one tool-execution call path; `ExecutionRequest` carries principal, action, resource, context.
2. `PolicyHook` trait exists; `AllowAll` is default; a denying mock hook passes the flow-semantics test suite (denied step fails cleanly, renders in canvas, no retry of destructive actions).
3. Every agent instance has `AgentId` + `on_behalf_of` + `flow_id`, present in every request and span.
4. Action/resource vocabulary is typed, namespaced, and centrally defined.
5. Span hierarchy and `aesop.*` fields emitted on every flow run, step, and tool call.
6. AuthN lives in a swappable tower layer; handlers only ever see `SecurityContext`.
7. Settings registry accepts a `security.*` namespace.

If all seven hold, "EE = CE + aesop crates + policies + one setting" is literally true, and AESOP can stay parked with zero anxiety.

---

## 6. First Consumers (when un-parked)

**A3T EE:** wrap the CE choke point with `CedarHook`; ship the A3T schema and a starter policy pack (destructive-tool gating by trust tier, flow-owner controls, deny-by-default for unrecognized actions); enable the audit exporter. This *is* the EE tier's headline: deterministic policy enforcement on every agent action, with compliance-grade audit.

**PubContext:** the highest-value single policy in the whole system — gating `media_buy:*` and other PubMatic/MCP money-touching actions behind explicit Cedar rules with full audit. An agent that can spend money is the canonical argument for this layer, and the demo ("watch a prompt-injected agent get denied by policy, with the audit event on screen") is the AESOP socialization artifact.

---

## 7. Summary

AESOP is a Cedar-decided, annotation-fronted, OTel-observed security layer whose near-term job is to be A3T EE, whose first real workload is PubContext's money-touching agents, and whose multi-language ambition is explicitly next-stage. The work that is *not* parked is small and belongs to A3T CE: one choke point, one hook trait with an allow-all default, real identities and typed action names, standard trace fields, edge-only authn, and a namespaced settings registry. Build CE to that checklist and the enterprise security story becomes an installation, not a rewrite.
