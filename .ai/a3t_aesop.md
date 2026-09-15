# A3T EE (AESOP) Implementation TODO

> Tracks: `.ai/A3T_EE_AESOP_IMPLEMENTATION.md` + `.ai/aesop-design-and-a3t-ce-contract.md` (7 seams)
> AESOP side: `Dev/aesop/.ai/todo.md`
> Legend: `[ ]` not started · `[~]` in progress · `[x]` done
> Invariant: `EE = CE + aesop crates + policies + one setting`. If an EE task needs a CE refactor, do it in CE first.

---

## Seam status (baseline audit)

- [~] Seam 1 — one tool-execution choke point → **missing** (calls scattered: MCP dispatch, flowr-tasks handlers, agent executor)
- [ ] Seam 2 — `PolicyHook` + `AllowAll` + tested Deny → **missing** (CE/EE currently faked at HTTP RBAC layer)
- [~] Seam 3 — `AgentId`+`on_behalf_of`+`flow_id`, typed actions → **partial** (ids yes; `on_behalf_of` + typed actions no)
- [ ] Seam 4 — `flow_run→flow_step→tool_call` + `aesop.*` fields → **missing**
- [~] Seam 5 — swappable authn layer → `SecurityContext` → **closest** (jwt middleware injects `Claims`, HS256)
- [~] Seam 6 — namespaced settings registry `security.*` → **partial** (KV store tolerant; no order/reservation)
- [ ] Seam 7 — anti-goals held (no scattered authz, no free-form P/A/R) → **guard from now**

---

## Phase E0 — CE hardening (ships in CE, public; unblocks all EE)

### E0.1 — `shared/flowr-policy` contract crate  `[ ]`
- [ ] New crate, deps: serde/async-trait/thiserror only (no aesop, no cedar)
- [ ] `Principal` / `Action` (validated `domain:verb`) / `Resource` / `ExecContext` / `ExecutionRequest`
- [ ] `PolicyHook` trait + `PolicyDecision` + `AllowAll` default impl
- [ ] Action vocabulary registry module (`tool:invoke`, `flow:start|cancel`, `mcp:call`, `memory:read|write`, `llm:call`, `agent:spawn|stop`)
- [ ] `aesop.*` field-name constants (mirrored from AESOP spec §5)
- **Gate for:** AESOP Phase 1

### E0.2 — the choke point  `[ ]`
- [ ] Introduce `ToolExecutor` + route MCP `tools/call` dispatch through it (`mcp:call`)
- [ ] Route flow task handler dispatch through it (`tool:invoke`: call:http/lancedb, run:script…)
- [ ] Route agent executor tool/memory/LLM calls through it (`Principal::Agent`)
- [ ] `hook.check(&req)` before every `execute()` (AllowAll in CE)
- [ ] **Acceptance:** grep tool-invocation sites → exactly one call path

### E0.3 — Deny semantics (defined + tested in CE)  `[ ]`
- [ ] `ExecError::Denied { reason, policy_ids }`
- [ ] Denied step fails cleanly, surfaces to canvas/execution status, audited
- [ ] Destructive actions never retried after a Deny
- [ ] Mock denying-hook test suite (mirrors AESOP `DenyAllEngine` tests)

### E0.4 — identity  `[ ]`
- [ ] Add `on_behalf_of` to agent config + spawn path
- [ ] `jwt_auth_middleware` builds `SecurityContext{Human}` into extensions; handlers/MCP stop reading raw `Claims`
- [ ] Agent→runtime calls carry `Principal::Agent` explicitly (no ambient trust)

### E0.5 — tracing  `[ ]`
- [ ] Emit `flow_run → flow_step → tool_call` spans with `aesop.*` fields at choke point + executor
- [ ] `aesop.decision="allow"` in CE (observability is a CE feature)

### E0.6 — settings + edition  `[ ]`
- [ ] Reserve `runtime.*` / `canvas.*` / `security.*` namespaces; document defaults→file/db→env order
- [ ] Reserved keys: `security.policy_hook="allow_all"`, `security.policies_dir`, `security.audit_exporter`
- [ ] Split HTTP-layer RBAC (`security.rbac_provider`) from execution-layer `security.policy_hook`
- [ ] Fix `RbacEnforcer::new` to accept `permissive` cleanly (no fallback warning)
- [ ] `GET /edition` → `{edition:"ce", enforcing:false, features:[]}`; Settings UI reads it as authoritative (store = cache)
- **E0 exit = the 7-point CE "AESOP-ready" checklist passes**

---

## Phase E1 — EE assembly (private `flowr-ee` repo)  `[ ]`   (gate: E0 + AESOP Phase 1)
- [ ] `flowr-ee/crates/flowr-aesop`: `CedarHook` impl of `flowr_policy::PolicyHook` via `aesop_core::AuthzEngine`
- [ ] Map `ExecutionRequest` → Cedar request (mechanical; shapes coincide)
- [ ] EE image = CE binary + flowr-aesop wired via settings/DI seam + policy pack at `security.policies_dir`
- [ ] `aesop-tower::AuthnLayer` replaces CE JWT layer (one-line router change)
- [ ] Enforcement upgrade = `security.policy_hook = "cedar"`; flip back = CE behavior (break-glass)
- [ ] Action-registry ↔ `aesop/schemas/a3t.cedarschema` parity check in EE CI
- **Exit:** dev-cluster EE denies `trust_tier<3` agent on destructive tool; deny renders in canvas, not retried, in audit stream; flip to `allow_all` → identical CE.

---

## Phase E2 — EE product surface  `[ ]`
- [ ] Audit: OTel exporter subscriber (retention + PII redaction) → `security.audit_exporter`; Settings→Security→Audit page live
- [ ] Policy-as-resource: Cedar policies as `type_ref: cedar` registry resources; `policyRef` + tag selectors; edit/version/test in Settings→Policies
- [ ] Governance: only SecurityAdmin/SuperAdmin edit `cedar` resources (a Cedar policy itself)
- [ ] Template ceilings: `metadata.a3t.{exposedTools,lockedFields,bindableFields,runtimePolicy}` → principal/resource attrs at spawn
- [ ] Runtime capability = `template.exposed ∩ instance.enabled ∩ org guardrails`
- [ ] Unblock unified template/agent editor: `editField` action + `/authz/batch` for per-field UI locks
- [ ] OIDC edge authn (RS256/JWKS) in EE; retire SHA-256 legacy password path (fixes recurring prod superadmin breakage)
- [ ] Six default roles seeded as data (SuperAdmin, SecurityAdmin, Admin-Infra, Developer, KnowledgeAdmin, Viewer)
- [ ] Retire Casbin; RBAC expressed as Cedar `permit` policies (one engine, one audit trail)

---

## Phase E3 — Annotations + InfoSec CI  `[ ]`   (gate: AESOP v0.2)
- [ ] Convert EE glue + CE explicit `hook.check` boilerplate to `#[require_permission]` / `#[public]`
- [ ] Coverage lint on: every choke-point path declared or explicitly public; silence fails CI
- [ ] CODEOWNERS → security review on annotations, action registry, `*.cedarschema`, `*.cedar`

---

## Standing rules while building
- [ ] No new authz-ish code in CE outside the hook (define an `Action`, call the hook)
- [ ] Vocabulary changes reviewed; CE registry ↔ AESOP schema move together
- [ ] Nothing in E0 waits on AESOP — E0 pays for itself as observability even if EE never ships

---

## Immediate next action
**Start E0.1** — the `shared/flowr-policy` contract crate (no external deps). It is the keystone: every other E0 item hangs off `ExecutionRequest` + `PolicyHook`.
