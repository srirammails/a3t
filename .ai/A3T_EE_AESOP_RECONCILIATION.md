# A3T EE / AESOP — Reconciliation & Re-evaluation

> Status: reconciliation as of 2026-07-27 · reconciles `A3T_EE_AESOP_IMPLEMENTATION.md`
> (E0–E3) and `aesop/.ai/AESOP_IMPLEMENTATION_PHASES.md` (P0–P5) against shipped code.
> Evidence = code in `flowr-system` + `aesop`, live state on vc-dev.

## 1. Executive summary

The **core thesis is proven in production**: `EE = CE + aesop crates + one setting`.
On vc-dev, the same codebase runs as CE (`allow_all`, `enforcing:false`) and EE
(`cedar`, `enforcing:true`). **E0, E1, and all of E2 (#1–#5) are done + deployed +
demoed** (2026-07-28, image `:2026-07-28.9`): policy-as-resource + SoD, the workflow
capability model (ceiling + attenuate-only composition), OIDC (live vs Keycloak) +
SHA-256 retirement, and roles-as-Cedar (Casbin retired). **E1 audit stays the in-process
ring buffer** (the OTel `AuditLayer` is still a stub, D3). **E3 done via (a)+(c) (see §7):
aesop-macros v0.2 for per-handler consumers, plus a choke-point-invariant CI lint for A3T's
centralized model — A3T handlers were not force-converted to per-handler annotations.**

The single most important correction is a **model shift** (§4): the plan's authorization
model is agent-centric, but FlowR/SWF has no agent primitive. The right model is
**workflow-centric — authorize *steps* under a *principal* inherited down the *flow tree***,
with the LLM as a step that emits steps, never a principal. Every remaining E2 item hangs off
one keystone: **uniform principal propagation** across all entry points and flow→subflow edges.

## 2. Phase-by-phase reconciliation

### A3T EE phases

| Item | Plan | Status | Evidence / caveat |
|---|---|---|---|
| **E0.1** contract crate | `flowr-policy` (deps-light: Principal/Action/Resource/ExecutionRequest/PolicyHook/AllowAll + action registry + aesop.* consts) | **DONE** | `shared/flowr-policy`; 9 tests |
| **E0.2** single choke point | one path into tool execution | **DONE** | site A `flowr-mcp-core::handle_tool_call` (mcp:call), site B `swf-runtime` executor dispatch (tool:invoke). All task handlers dispatch through site B |
| **E0.3** Deny semantics | fail clean, no retry of destructive, audited | **DONE** | `RuntimeError::Denied`; policy_deny test suites |
| **E0.4** identity | on_behalf_of; JWT→Principal::Human; agent = Principal::Agent | **DONE** | E0.4a/b/c; `Principal` 3 kinds threaded |
| **E0.5** tracing | flow_run→flow_step→tool_call + aesop.* | **DONE** | `.instrument()` spans, `aesop.decision` recorded |
| **E0.6** settings + /edition | security.* reserved; policy_hook setting; /edition | **DONE** | `SecurityConfig`; `install_policy_hook`; `GET /edition` |
| **E1** EE assembly | flowr-aesop CedarHook + policy pack + ee feature; E2E on dev cluster | **DONE + DEPLOYED + E2E PROVEN** | `flowr/crates/flowr-aesop`; live differential: `delete_flow` allowed on CE / denied on EE. **Deviations: D1, D2** |

### E2 — EE platform surface

| Item | Plan | Status | Evidence / caveat |
|---|---|---|---|
| **E2 #1 Audit** | OTel exporter subscriber; Settings→Audit page | **PARTIAL** | Built an in-process bounded ring buffer + `GET /security/audit`; demoed live. **NOT** the OTel `AuditLayer` the plan/AESOP-P1 specify (that layer is a stub — D3) |
| **E2 #2 Policy-as-resource** | cedar registry resources; policyRef + tag selectors; load order; SoD guardrail | **DONE + DEPLOYED + DEMOED** | CRUD `/registry/resources`, SoD (only SecurityAdmin/SuperAdmin write type=cedar), baseline+registry load, attachment via `principal.template.policyRef/tags`. Live: viewer→403, superadmin→201, stored policy enforces. **UI (Settings→Policies) not built** |
| **E2 #3 Template ceilings** | capability = exposed ∩ enabled ∩ org guardrails; per-field editor locking | **DONE + DEPLOYED + DEMOED (as the workflow model, §4)** | Re-expressed as a *flow* capability ceiling (any principal), gated on the real per-step resource; attenuate-only composition down subflows. Live: `exposedTools:[list_flows]`→`list_agents` denied; subflow can't widen. `lockedFields`/editor-locking still not done |
| **E2 #4 Edge authn (OIDC)** | aesop-tower AuthnLayer RS256/JWKS; retire SHA-256 | **DONE + DEPLOYED + VERIFIED LIVE** | `aesop-tower::oidc` (RS256/JWKS, 5 tests); EE behind `security.authn`; live: alice's Keycloak token→Operator, HS256 rejected under oidc, toggle both ways. SHA-256 password path retired (Argon2-only). Keycloak IdP durable on vc-dev |
| **E2 #5 Roles as Cedar** | roles as data; retire Casbin; RBAC as Cedar permits | **DONE + DEPLOYED + DEMOED** | Casbin was **inert** (never called) → deleted. Canonical roles in `flowr-policy::roles` (CE infra) + hierarchy; RBAC as Cedar (registry writes + authoring require Admin; SoD for cedar). Live: viewer denied, admin allowed. Closed the agent-tool-dispatch policy bypass. D5 bridge removed |
| **E3 Macros + coverage lint** | aesop-macros #[require_permission]/#[public]; coverage CI | **DONE via (a)+(c) — see §7 "What shipped"** | aesop-macros v0.2 built + 3 tests (for per-handler consumers like PubContext); choke-point-invariant lint `tests/choke_point_invariant.sh` (A3T's centralized guarantee, negative-tested). A3T handlers stay on explicit `policy_hook().check()` — per-handler annotation is the wrong invariant for a centralized design |

### AESOP phases (provider side)

| Phase | Status | Caveat |
|---|---|---|
| **P0 skeleton + vocab** | **DONE** | aesop-core vocab, schemas, packs embedded |
| **P1 core + tower** | **PARTIAL** | `AuthzEngine` (cedar) DONE + 8 tests (deny-by-default, N6 uid, N8 schema-validate). **`aesop-tower` AuthnLayer/AuditLayer/MetricsLayer are stubs (comments only).** Validator-CI + PubContext media_buy test are backfill |
| **P2 first consumers** | **PARTIAL** | flowr-aesop adapter DONE; A3T pack v1 DONE (but D2). **PubContext consumer not built; AuthnLayer not swapped in** |
| **P3 macros** | NOT STARTED | correctly gated on API bake |
| **P4 operability** | NOT STARTED | policy **hot-reload** is the near-term gap (today registry policies load at startup only — restart required) |
| **P5 next stage** | parked (correct) | |

## 3. Deviations from plan (things done differently — decide keep/fix)

- **D1 — flowr-aesop is not a private repo.** The plan's distribution model is a
  separate private `flowr-ee/` tree; reality is a workspace-*excluded* crate at
  `flowr-system/flowr/crates/flowr-aesop`, built only via `--features ee`. Two
  images exist (CE/EE) but from one monorepo, not public/private repos. *This matches
  how the code actually ships; the plan's "private repo" language is aspirational.*
- **D2 — policy model is `destructive` + `on_behalf_of`, not `trust_tier`.** The E1
  exit test says "denies a `trust_tier < 3` agent invoking a destructive tool." We
  shipped: forbid destructive for all; permit Agent only with `on_behalf_of`. The
  canonical `aesop/schemas/a3t.cedarschema` still has `trust_tier`, but the
  *operational* `flowr-aesop` schema (User/Agent/Service) does not — schema divergence.
  Trust-tier grading was dropped in favor of the simpler destructive guardrail.
- **D3 — audit is an in-process ring buffer, not an OTel exporter.** Works and is
  queryable, but the plan's `security.audit_exporter` OTel subscriber (aesop-tower
  AuditLayer) is unbuilt.
- **D4 — the model was agent-centric; it should be step/flow/principal-centric (§4).** The
  plan (and the E2 #3 build) assumed a first-class "agent" that mints `Principal::Agent` at
  tool calls. FlowR/SWF has no agent primitive, and prod's agent is a plain workflow. The fix
  is not to reconcile "agent runtimes" but to authorize *steps* under an inherited *principal*
  with per-step *resources* — the LLM becomes a step, not a principal.
- **D5 — role vocabulary mismatch / superadmin bridge.** SoD guardrail accepts
  `SecurityAdmin`/`SuperAdmin`/`superadmin` because the platform JWT carries a single
  lowercase role. E2 #5 (roles-as-Cedar) is what removes this bridge.

## 4. The enforcement model: steps, principals, the flow tree (not "agents")

The E2 #3 work was framed around an "agent" that the runtime doesn't actually have
(FlowR/SWF has no agent primitive — see §4.1). The correct model is **workflow-centric**:
authorize *steps*, carry a *principal* down the *flow tree*, and let the LLM be a step
that emits steps — never a principal.

### 4.1 There is no runtime "agent"

FlowR/SWF has no agent concept; the executor only knows task types. "Agent" is a
convention expressed in three inconsistent places, none of them the runtime:
- **A task type** `call:a3t:agent` — its handler *is* an agent runtime and is the **only**
  place `Principal::Agent` is ever minted (it self-identifies).
- **A string heuristic** — the registry tags a template "agent" if its YAML
  `contains("a3t:agent") || contains("call: agent")` (`registry.rs`), for UI listing only.
- **Registry metadata** — `create_agent` stores an `agent:config` record.

These don't agree: `system/agent-template` (the deployed agent) uses `call:llm` + `call:mcp`
and contains **no** `a3t:agent`, so by A3T's own heuristic it isn't even an agent — the
runtime runs it as a plain workflow. **Conclusion: agent authorization cannot live at the
task/runtime layer; it must live where the model *knows* it is driving an LLM tool-loop.**

### 4.2 The model — three per-step primitives, two propagation rules

Everything reduces to: **every step runs under a principal, and the PdP decides
`(principal, action, resource, context)` before the step runs.**

Per step:
1. **Principal** — never absent. Set once at the entry point (AESOP **IdP**), carried on
   `ExecutionContext`.
2. **Meaningful resource** — the step's *target*, not its task-type string:
   `call:llm → Llm{model}`, `call:mcp/call:tool → Tool{name}` (from `with.tool`),
   `call:flow/execute_flow → Flow{ns/name}`, memory → `Memory{scope}`.
3. **Decision point** — `policy_hook().check(...)` before `handler.execute()` (AESOP **PdP**).

Across the tree:
1. **Inherit down.** Any subflow — via a `call:flow` task, an `execute_flow` MCP call, or an
   LLM's choice — inherits the parent's principal. The *originating identity* is constant
   the whole way down.
2. **Attenuate only.** Policy attaches per flow; the effective policy at a step is the
   **composition of every flow on the call-stack** (deny-by-default, all must permit).
   Calling a subflow can *add* restrictions, never remove them.

### 4.3 Why the LLM needs no special handling

An **LLM-chosen action is an ordinary step under the same principal, checked by the same
policy** as an author-declared step. So a prompt-injected LLM *cannot make a flow do
anything the flow's principal isn't already allowed to do* — there is no privilege boundary
to cross, because the LLM is not a principal. The only thing different about an LLM flow is
its **open action set** (the author didn't enumerate steps); the answer is a *policy posture*,
not a mechanism: **LLM-driven flows get a positive, deny-by-default allow-list.** The
"template ceiling" then stops being an agent feature — it is just the allow-list attached to
that flow, enforced at every step.

Nesting falls out for free: `A(LLM)→execute_flow(B)→B(LLM)→execute_flow(C)` runs under A's
originating principal throughout, stack `[A,B,C]`, policies composed — no escape hatch.

> **NOT significant (correction, keep on record):** the `call:mcp` default endpoint
> `localhost:3099` is a swappable resource config with **no bearing on enforcement** — every
> task is checked at site B *before* it routes. The demo's 3099 failure was a config bug, not
> an enforcement gap. (It is the MCP-server default, not the LLM; the LLM is `call:llm`,
> already on OpenRouter.)

### 4.4 What the model requires in the current code

| Requirement | State | Work |
|---|---|---|
| Principal on every step, inherited down | ✅ executor check; ✅ `call:flow`/`run:workflow` child inherits | — |
| Principal set at **every** entry point | ✅ **DONE** `execute_flow`+`send_message` thread the caller; trigger→Service | — |
| Principal propagates across `execute_flow` (MCP) | ✅ **DONE + DEPLOYED** `ExecutionRequest.with_principal` carried through the queue | — |
| Resource = real target + matching action | ✅ **DONE + DEPLOYED** `policy_resource`+`policy_action`: `call:mcp`→`tool:invoke Tool{with.tool}`, `run:workflow`→`flow:start Flow{}`, `call:llm`→`llm:call Llm{}`. Schema declares `Llm`/`Flow` + `llm:call`/`flow:start` | extend to memory (`call:lancedb`) |
| Flow allow-list (capability ceiling) | ✅ **DONE + DEPLOYED + DEMOED** baseline forbids `tool`-target ∉ `exposed ∩ enabled`, any principal; llm/flow steps not tool-ceiling-gated | — |
| Composition down the call-stack (attenuate-only) | ✅ **DONE + DEPLOYED** subflow child-ctx inherits parent template; `compose_template` INTERSECTS `exposed/enabledTools` (subflow narrows, never widens); 3 unit tests | flow-stack of ancestor ids for ancestry-policies/audit (visibility only) |
| Forbid raw tool bypass | ❌ any flow can `call:mcp` directly | policy: only sanctioned flows may `call:mcp`/`call:tool` (now expressible via the `task_type` attr) |

None of these are "agent" work — all step/flow/principal work.

> **Proven live 2026-07-27** (`:2026-07-27.2`, plain workflow, no agent): a `call:mcp` step
> invoking `delete_flow`, run via `execute_flow` as the superadmin, produced the audit record
> `deny tool:invoke resource=delete_flow principal=human/usr-superadmin policies=[policy3]` —
> real target, real principal, destructive guardrail fired. Before these two fixes that step
> was checked as `Tool{"call:mcp"}` under `service/flow-runtime` and sailed through.

### 4.5 Where AESOP sits (and doesn't)

Clean division, and this is *not* AESOP's core:
- **AESOP provides** the **IdP** (entry identity → `Principal`) and the **PdP** (given
  `(principal, action, resource, context)` → allow/deny + policy ids). Both pluggable —
  CE = `AllowAll`, EE = Cedar.
- **A3T/FlowR owns** the contract that makes them plug in: establish the principal at entry,
  propagate it down the flow tree, materialize a meaningful resource per step, call the PdP
  at every step. The flow-stack and policy composition are A3T's, not AESOP's.

AESOP never learns what a "flow" or "agent" is; A3T hands it a principal + resource, it answers.

## 5. Re-evaluated plan (revised order)

Replaces the earlier agent-centric "E2.5" with the step/flow/principal model of §4. E2 #3
(the ceiling) is not a separate agent feature — it *is* the deny-by-default allow-list this
model enforces per step, so it lands as a consequence of the model, not before it.

1. ✅ **DONE + DEPLOYED — Uniform principal propagation (keystone).** `execute_flow` +
   `send_message` thread the caller onto the queued flow via `ExecutionRequest.with_principal`;
   the worker sets the root context to it; subflow edges (`run_workflow`/`execute_tool_flow`)
   already inherit. Proven live (audit shows `human/usr-superadmin` on a flow's step).
2. ✅ **DONE + DEPLOYED — Per-step resource enrichment.** `TaskHandler::policy_resource`;
   `call:mcp`→`Tool{with.tool}`, `run:workflow`→`Flow{ns/name}`, `call:llm`→`Llm{resource}`
   (all +`task_type` attr). Remaining: memory (`call:lancedb`) — same pattern.
3. ✅ **DONE + DEPLOYED + DEMOED — Flow capability ceiling (was E2 #3).** Re-expressed as a
   *workflow* rule: baseline forbids any step whose target isn't in the flow's declared
   `exposedTools ∩ enabledTools` (from `metadata.a3t`), applied to **any** principal (dropped
   the `principal is Agent` scoping). **Live:** a plain workflow declaring `exposedTools:[list_flows]`
   had `list_flows` allowed and `list_agents` denied (`policy4`, "cedar denied tool:invoke on
   Tool::list_agents") — under `human/usr-superadmin`, no agent involved. This is the "ceiling"
   with zero agent concepts.
4. **Flow-stack in `ExecContext` + policy composition** (attenuate-only down the tree). *Next —
   the nested-subflow case; single-flow allow-lists already enforce.*
4. ✅ **DONE + DEPLOYED + DEMOED — Composition down the tree (attenuate-only).** Subflow
   child-contexts inherit the parent's capability template; the subflow's executor composes via
   `compose_template` — `exposedTools`/`enabledTools` INTERSECT, so a subflow narrows but never
   widens; a silent subflow inherits the parent surface. 3 unit tests. **Live:** `parent-caps`
   (exposes `[list_flows, list_agents]`) → `run:workflow` → `child-caps` (declares
   `[list_agents, list_executions]`, trying to widen) calling `list_executions` → audit
   `allow flow:start default/child-caps` then `deny tool:invoke list_executions` (intersection is
   `[list_agents]`; the subflow could not widen). *(Remaining: a flow-stack of ancestor ids on
   `ExecContext` for ancestry-aware policies + richer audit — visibility only.)* Also fixed here:
   each step uses the **action matching its resource** (`llm:call`/`flow:start`) with `Llm`/`Flow`
   schema entities — a `.3` regression where `call:llm` fault-denied, caught + fixed in `.4`.
5. **Forbid raw `call:mcp`/tool bypass** for flows with no allow-list (now expressible via the
   `task_type` attr) — optional tightening. Same shape proves the LLM case once OpenRouter
   drives a flow whose chosen tool is out-of-list.
6. **E2 #5 roles-as-Cedar** — independent; removes the D5 superadmin bridge, unifies RBAC into
   the one engine/audit trail.
7. **E2 #4 OIDC AuthnLayer** — build `aesop-tower::AuthnLayer` (RS256/JWKS; the skipped
   AESOP-P1 tower work); one-line swap in EE; retire the SHA-256 password path.
8. **Audit upgrade (D3)** — `aesop-tower::AuditLayer` → `security.audit_exporter`; keep the
   ring buffer as the always-on cache.
9. **P4 hot-reload** — registry policies load at startup only today; add file-watch / reload.
10. **E3 macros** — gated on aesop v0.2 + API bake.

Backlog / smaller: PubContext consumer (AESOP-P2 demo), Settings→Policies UI, schema
reconciliation (canonical `trust_tier` vs operational), canvas render of deny, editor
field-locking (`lockedFields`/`bindableFields` + `/authz/batch`), sandboxed-python/js WASM tasks.

## 6. What's genuinely solid (do not re-litigate)

E0 (all 7 seams), E1 (CedarHook + one-setting upgrade, E2E proven), and E2 #2
(policy-as-resource + SoD + load/attach, demoed live) are the load-bearing wins. The
CE/EE seam is real and deployed. The gaps above are additive, not corrective — none
require reopening the contract.

## 7. E3 (annotations) — architectural fit assessment

**Status: DONE via (a)+(c) — the recommendation below. E0–E2 complete + deployed; E3 shipped as
the aesop-macros facade (for per-handler consumers) plus a choke-point-invariant CI lint (for
A3T's centralized model). A3T handlers were NOT force-converted — see "What shipped" at the end.**

Original assessment (kept for the record — E3's per-handler model may not fit A3T):

The plan's E3 = per-handler `#[require_permission(...)]`/`#[public]` annotations + a **coverage
lint** ("every path through a consumer's choke point carries a declaration or an explicit
`#[public]`; silence fails CI"). That design assumes **each handler enforces its own permission**
(the PubContext/typical-web-app shape).

A3T doesn't work that way. Enforcement is **centralized at two choke points** — site A
(`handle_tool_call`) and site B (the executor) — plus a handful of discrete gates
(`authorize_resource_write`, the authoring gate). So:
- The authorization surface is **already a small, greppable inventory**: 2 choke points + ~3
  discrete gates, all calling `policy_hook().check()`, against a **typed action registry**
  (`flowr-policy::actions`) and **canonical roles** (`flowr-policy::roles`) — the reviewability
  E3 promises largely exists.
- A per-handler coverage lint doesn't map: A3T handlers **don't** individually enforce, so "every
  handler is annotated or `#[public]`" is the wrong invariant. The right invariant — "every tool
  execution passes a choke point" — is already structurally guaranteed (there is no other path to
  execution), which was the E0.2 acceptance test.

**Options:**
- **(a) Skip/defer E3 for A3T.** Its reviewability goal is met by the centralized model + action
  registry + role vocabulary. Annotations would add a proc-macro subsystem for marginal gain.
  (aesop-macros still ships for consumers that ARE per-handler, e.g. PubContext.)
- **(b) Narrow E3 to the discrete gates.** Convert the ~3 hand-wired gates
  (`authorize_resource_write`, authoring) to `#[require_permission]` for declarative uniformity —
  small, real, but not a "coverage lint" story.
- **(c) Reframe the lint as a choke-point invariant check** (CI grep: no tool execution bypasses a
  choke point) rather than per-handler annotations — this is the invariant that actually matters
  for A3T, and it's a lint, not a macro.

Recommendation: **(a) + (c)** — A3T's enforcement is centralized by design; keep it, and add the
choke-point-invariant CI check (cheap, high-value) instead of per-handler macros. Build the
`aesop-macros` crate itself only when a per-handler consumer (PubContext) needs it. This is another
"the map assumed a shape the territory doesn't have" finding, like the agent-runtime (§4) and the
inert Casbin (E2 #5).

### What shipped (E3)

Both halves of (a)+(c), built and tested:

1. **aesop-macros v0.2** (`aesop/crates/aesop-macros`) — the annotation facade, un-excluded and in
   the workspace. `#[require_permission(action, resource_type, resource_id)]` expands to exactly one
   `engine.enforce(ctx, &action, &resource)?` guard prepended to the fn body (a `Denied`
   short-circuits via `?`); `#[public]` is an identity marker for the coverage lint. Consumer
   contract: an `async fn` with `engine: &impl Engine` + `ctx: &SecurityContext` in scope returning
   `Result<T, E: From<AesopError>>`. **3 tests pass**: permissive engine runs the body, deny-all
   short-circuits *before* the body, `#[public]` is identity. This is for **per-handler consumers**
   (PubContext `media_buy:*`), not A3T.

2. **Choke-point-invariant lint** (`flowr-system/tests/choke_point_invariant.sh`) — the reviewability
   piece that fits A3T. A dependency-free CI grep that fails if: (1) any of the 4 pinned enforcement
   sites loses its `policy_hook().check()`, (2) a new `.check()` site appears outside the manifest
   (forces review), or (3) a second `handler.execute(` dispatch appears in the executor (copy-paste
   bypass). Manifest = the 4 real sites: site A (`flowr-mcp-core/tools.rs`), site B
   (`swf-runtime/executor`), the agent-tool `GATED_WRITES` gate, and the registry write. Runs in
   <0.5s; negative-tested (removing a gate → exit 1, "GONE site B").

**A3T handlers stay on explicit `policy_hook().check()` at the centralized choke points** — their
return types (`CallToolResult`, `impl IntoResponse`) don't fit the macro's `Result<_, From<AesopError>>`
contract, and per-handler annotation is the wrong invariant for a centralized-choke-point design
(§7 above). The lint, not the macro, is A3T's guarantee.
