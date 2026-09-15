# A3T EE Implementation Strategy — AESOP-based

> Status: Plan of record · July 2026
> Parent design: `.ai/aesop-design-and-a3t-ce-contract.md` (the contract; frozen)
> AESOP repo: `Dev/aesop` — `.ai/AESOP_SPEC.md`, `.ai/AESOP_IMPLEMENTATION_PHASES.md`
> Related: `.ai/A3T_IAM_AND_AUTHORIZATION.md` (platform IAM; defers to AESOP for runtime enforcement)

**Goal, stated as the acceptance test:** `EE = CE + aesop crates + policies +
one setting`. If any EE step requires refactoring CE, the CE contract was
violated and the refactor happens *in CE first*.

**Distribution constraint (decided):** EE crates are private and are never
published to the CE GitHub. Therefore **CE contains zero AESOP code and zero
AESOP dependencies** — CE defines the contract vocabulary itself (contract
§5); the private EE glue crate maps CE types onto `aesop-core`. There are two
runtimes/images: CE (public) and EE (private). CE's Settings UI shows EE
features locked; activation = run the EE image (license → download → restart,
or "contact support" as the v1 fallback).

---

## 1. Where flowr stands against the seven seams (code-level)

| Seam | Requirement | Current state in this repo | Verdict |
|---|---|---|---|
| 1 | One tool-execution choke point (`ExecutionRequest`, `ToolExecutor`) | Tool calls scattered: MCP `tools/call` dispatch (`clawdflow-mcp`), task handlers (`shared/flowr-tasks/call_http.rs`, `call_qdrant.rs`, lancedb, `run_script`), agent executor (ZeroClaw + `McpTool` adapter) each invoke directly | **Missing — the big one** |
| 2 | `PolicyHook` + `AllowAll` default + tested Deny semantics | Nothing at the execution layer. (CE-vs-EE is currently faked at the HTTP layer via `FLOWR_RBAC_PROVIDER=permissive` — wrong seam, and `RbacEnforcer::new` doesn't even recognize `permissive`, it lands there via the fallback warning) | **Missing** |
| 3 | Stable `AgentId` + `on_behalf_of` + `flow_id`; typed `Action`/`Resource` | Agent ids exist (`agt-*` in agent_configs); `on_behalf_of` absent; actions are free-form strings (MCP tool names, task type strings) | **Partial** |
| 4 | `flow_run → flow_step → tool_call` spans with `aesop.*` fields | `tracing` used ad hoc; no fixed hierarchy or field contract | **Missing** |
| 5 | AuthN in a swappable layer; handlers see `SecurityContext` only | `jwt_auth_middleware` (flowr-server/src/auth.rs) is already a tower-style layer injecting into extensions — but injects `Claims`, not a `SecurityContext{Principal}`; HS256 shared secret, not RS256/JWKS | **Closest** |
| 6 | Namespaced settings registry tolerating `security.*` | `a3t-settings` CRUD flow + `settings` table accept arbitrary dotted keys; env overrides exist (`FLOWR_*`); no defined defaults→file→env order, no `security.*` reservation | **Partial** |
| 7 (anti-goals) | No authz logic scattered in CE; no free-form P/A/R strings | Mostly clean today *because there is almost no authz* — the risk is accretion from here | **Guard now** |

---

## 2. Phase E0 — CE hardening (the contract work; in CE, public)

Everything in E0 ships in **CE** and is invisible to CE users except as better
observability. This is the prerequisite for every EE step, and it maps to
AESOP Phase 1's gate.

### E0.1 New CE crate: `shared/flowr-policy` (the contract crate)

Defines, with **no external deps beyond serde/async-trait/thiserror**:

```rust
pub enum Principal { Human{..}, Agent{ agent_id, on_behalf_of, flow_id }, Service{..} }
pub struct Action(String);            // "{domain}:{verb}", validated
pub struct Resource { type_name, id, attributes }
pub struct ExecContext { run_id, flow_id, step, attempt }
pub struct ExecutionRequest { principal, action, resource, context, params }

#[async_trait] pub trait PolicyHook: Send + Sync {
    async fn check(&self, req: &ExecutionRequest) -> PolicyDecision;
}
pub enum PolicyDecision { Allow, Deny { reason, policy_ids } }
pub struct AllowAll;                  // CE default
```

Plus the **action vocabulary registry** (one module listing every legal
action: `tool:invoke`, `flow:start`, `flow:cancel`, `mcp:call`,
`memory:read`, `memory:write`, `llm:call`, `agent:spawn`, `agent:stop`) and
the **`aesop.*` field-name constants** (mirrored from the AESOP spec §5 —
mirrored, not imported, per the distribution constraint; a CI check in the
EE repo asserts the two lists stay identical).

### E0.2 The choke point

Introduce `ToolExecutor` in the runtime and route **all** invocation sites
through it:

- MCP `tools/call` dispatch (`clawdflow-mcp`) — wraps each tool call as
  `ExecutionRequest{ action: "mcp:call", resource: Tool{name} }`.
- Flow task execution (`swf-runtime`/`flowr-tasks` handler dispatch) — the
  single place task handlers are invoked becomes the choke point for
  `tool:invoke` (call:http, call:lancedb, run:script, …).
- Agent executor (`clawdflow-agents`) — every ZeroClaw tool dispatch and
  memory/LLM call goes through the same path with `Principal::Agent`.

`hook.check(&req)` runs before every `execute()`. CE cost: one virtual call,
always Allow.

**Acceptance (contract §5.1):** grep for invocation sites — exactly one call
path into tool execution.

### E0.3 Deny semantics, defined and tested in CE

`ExecError::Denied { reason, policy_ids }` exists in CE. Semantics: the
denied step fails cleanly, surfaces to canvas/execution status, is audited,
and **destructive actions are never retried after a Deny**. A mock denying
hook exercises this in CE's test suite (this is exactly the
`DenyAllEngine`-shaped test AESOP provides on its side).

### E0.4 Identity

- `on_behalf_of` added to agent config/spawn path (three struct fields, per
  the contract; cheap now, a migration later).
- `jwt_auth_middleware` constructs `SecurityContext{ Principal::Human }` from
  Claims and stores **that** in extensions; handlers/MCP dispatch stop reading
  raw `Claims`. (Claims stays for token plumbing; authorization-facing code
  sees only `SecurityContext`.)
- Agent-to-runtime calls carry `Principal::Agent` explicitly — never ambient
  trust.

### E0.5 Tracing

Emit the fixed hierarchy `flow_run → flow_step → tool_call` with the
`aesop.*` fields at the choke point and executor. `aesop.decision` is always
`"allow"` in CE — that is a feature (CE gets first-class observability; EE's
audit trail becomes a new subscriber, not new instrumentation).

### E0.6 Settings

- Reserve namespaces in the settings registry: `runtime.*`, `canvas.*`,
  `security.*` — unknown namespaces tolerated (already true of the KV store;
  document it and add the resolution order **defaults → file/db → env**).
- Define `security.policy_hook = "allow_all"` (CE default; the only value CE
  recognizes) and `security.policies_dir`, `security.audit_exporter` as
  *reserved-but-unused* keys.
- Fold the current `FLOWR_RBAC_PROVIDER` hack into this: the HTTP-layer RBAC
  provider remains a *separate*, platform-IAM concern
  (`security.rbac_provider`), distinct from the execution-layer
  `security.policy_hook`. Fix `RbacEnforcer::new` to accept `permissive`
  without the fallback warning.
- `GET /edition` implemented in CE: `{ edition:"ce", enforcing:false,
  features:[] }` (the Settings UI already consumes `_edition` from the
  settings store; move it to this endpoint as the authoritative source, keep
  the store as cache).

**E0 exit = the contract's 7-point checklist passes.** From this moment CE can
evolve freely without blocking EE.

---

## 3. Phase E1 — EE assembly (private repo)

The EE tree is `flowr-ee` (private), containing only glue + packaging — no
forked CE logic:

```
flowr-ee/
├── crates/flowr-aesop/     # CedarHook: impl flowr_policy::PolicyHook via aesop_core
├── policies/a3t/*.cedar    # from aesop repo's starter pack, tuned
└── deploy/                 # EE image build (CE binary + ee glue + policies)
```

- `CedarHook` maps `ExecutionRequest` → Cedar request (the shapes were
  designed to coincide; the mapping is mechanical). Dependency direction:
  flowr-ee → aesop → (nothing of ours).
- EE image = CE codebase + `flowr-aesop` wired through the settings/DI seam +
  `aesop-tower` layers on the axum router (AuthnLayer replaces CE's JWT layer
  — one-line router change per seam #5) + policy pack baked in at
  `security.policies_dir`.
- **The entire enforcement upgrade is:** `security.policy_hook = "cedar"`.
  Flipping it back yields CE behavior on the same image (permissive engine),
  which is also the break-glass path.

**E1 exit test:** on the dev cluster, the EE image with the A3T starter pack
denies a `trust_tier < 3` agent invoking a destructive tool; the deny renders
in the canvas, is not retried, and appears in the audit stream with
`aesop.decision="deny"` and the policy id. Same image,
`policy_hook=allow_all` → identical CE behavior.

---

## 4. Phase E2 — EE platform surface

- **Audit**: OTel exporter subscriber (retention + PII redaction) —
  `security.audit_exporter`. Settings → Security → Audit Log page goes live
  against it.
- **Policy management**: Cedar policies as `type_ref: cedar` registry
  resources (per `A3T_IAM_AND_AUTHORIZATION.md` §6.3): explicit
  `metadata.a3t.policyRef` + tag selectors; edit/version/test in Settings →
  Policies. Load order: baked pack → policy resources. Governance guardrail:
  only SecurityAdmin/SuperAdmin may edit `cedar` resources — itself a Cedar
  policy.
- **Template ceilings**: `metadata.a3t.{exposedTools, lockedFields,
  bindableFields, runtimePolicy}` read at agent spawn into principal/resource
  attributes; the runtime capability rule becomes
  `template.exposed ∩ instance.enabled ∩ org guardrails`. This also unblocks
  the unified template/agent editor's per-field locking (`editField` action +
  `/authz/batch` for the UI).
- **Edge authn hardening**: `aesop-tower::AuthnLayer` (RS256/JWKS, OIDC)
  replaces the HS256 layer in EE; also retires the fragile SHA-256 legacy
  password path (root cause of the recurring prod superadmin breakage — EE
  moves fully to Argon2 + OIDC).
- **Roles**: the six default roles (SuperAdmin, SecurityAdmin, Admin-Infra,
  Developer, KnowledgeAdmin, Viewer) seeded as data; Casbin retired; RBAC
  expressed as Cedar `permit` policies so there is one engine and one audit
  trail.

---

## 5. Phase E3 — Annotations + InfoSec CI (tracks AESOP v0.2)

When `aesop-macros` ships: convert EE glue and any CE-side explicit
`hook.check` boilerplate to `#[require_permission]`/`#[public]`. Turn on the
**coverage lint** in both repos: every choke-point path carries a declaration
or an explicit opt-out; silence fails CI. CODEOWNERS routes changes to
annotations, the action registry, `*.cedarschema`, and `*.cedar` through
security review. This is the reviewability deliverable: the authorization
surface becomes a greppable inventory and "forgot to authorize" becomes a
build break, not an incident.

---

## 6. Sequencing & effort (honest sizing)

| Phase | Contents | Size | Depends on |
|---|---|---|---|
| **E0.1–E0.3** | contract crate + choke point + Deny semantics | ~1–2 wk (the choke-point refactor is the real work) | — |
| **E0.4–E0.6** | identity, tracing, settings namespaces, /edition | ~1 wk | E0.1 |
| **E1** | flowr-ee + CedarHook + starter pack on dev cluster | ~1 wk | E0 + AESOP Phase 1 |
| **E2** | audit exporter, policy-as-resource, ceilings, OIDC, roles | 2–4 wk, shippable in slices | E1 |
| **E3** | annotations + coverage lint | small | AESOP v0.2 |

Rules while this runs:
1. **No new authz-ish code in CE outside the hook.** Any CE feature needing a
   permission check defines an `Action` and calls the hook (AllowAll today).
2. **Vocabulary changes are reviewed** — the action registry and schemas move
   together (CE registry ↔ `aesop/schemas/a3t.cedarschema` parity check in EE CI).
3. **Nothing in E0 waits for AESOP** — E0 is pure CE work and pays for itself
   as observability even if EE never ships.

---

## 7. Summary

E0 makes CE *AESOP-ready* (choke point, hook, identity, tracing, settings —
the seven seams); E1 makes EE an *installation* (private glue crate + Cedar
policies + one setting); E2 makes it a *product* (audit, policy management,
template ceilings, OIDC, roles); E3 makes it *reviewable* (annotations +
coverage CI). At every step the CE image stays public and AESOP-free, the EE
image stays private, and the difference between them remains: crates,
policies, one setting.
