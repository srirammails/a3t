# Deploying from Chat — Environment Configs and the Canvas Surface

**Status:** Planned. Canvas itself is largely built; this is the layer above it.
**Version:** 0.1 — September 2026
**Primary purpose:** Make "test this, then deploy it" a thing a user asks for in
the main chat, with separate configuration per environment and prod placement
gated by the approval mechanism built in `policy-implementation.md`.

**Related:** `A3T_CANVAS_SPEC.md` (the Canvas model — read §3–§6 first),
`policy-implementation.md` (the Ask/approve machinery this reuses),
`A3T_RUNTIME_ISOLATION.md` (sandboxing and hibernation).

---

## 1. What already exists (verified in the tree)

Canvas is not a greenfield. Working today:

- **Type model** — `shared/flowr-types/src/canvas.rs`: `CanvasSpec`, `Placement`
  (`shared` | `isolated`, `runtimeSelector`, `namespace`, `tolerations`),
  `Lifecycle` (state, `Scaling`, `Restart`), `Health` (`DependsOn`,
  `OnDependencyDown`, `Circuit`).
- **CE HTTP surface** — `flowr-ce/src/canvas.rs`: `GET /canvas/runtimes`,
  `/schema`, `/deployments`, `GET /canvas/flows/:ns/:name/placement` (preview),
  `POST …/deploy`, `POST /canvas/canvases/:ns/:name/deploy`.
- **Placement resolution** — `compute_placement` matches a canvas's selector
  against registered runtimes and reports `matched`.
- **EE bridge** — deploy pushes the spec to the controller over HTTP when
  `A3T_CONTROLLER_URL` is set; flowr itself stays k8s-free. CE falls through to
  logical placement.
- **Controller** — `flowr/crates/flowr-canvas`: `crd.rs`, `controller.rs`,
  `reconciler.rs`, `api.rs`. In the workspace. **Not E2E-tested.**
- **UI** — `CanvasDesigner.svelte`, `CanvasList.svelte`; `canvas_crud.yaml`.

So placement, isolation, selectors, scaling and health are **already specified
and largely implemented**. What is missing is everything that makes it usable
from a conversation, and one security gap.

## 2. The three gaps

### 2.1 Deploy is not authorized at all

`deploy_flow` takes no principal and never calls `policy_hook()`. There is no
`flow:deploy` action anywhere in the tree, despite `A3T_CANVAS_SPEC.md` §6
specifying exactly that. **Anyone who can reach the endpoint can place any flow
into any runtime, including a prod-tier one.**

This is the same shape as the remote-MCP hole closed in `122828f`: a real
capability with no choke point. It must be fixed before chat can reach it, because
chat multiplies the number of ways the endpoint gets called.

### 2.2 The agent cannot deploy

There are no canvas MCP tools. The agent's 39 tools cover flows, agents,
templates, resources, triggers, executions and permissions — nothing about
runtimes, placement or deployment. "Ask the chat to deploy" is impossible today,
not merely unwired.

### 2.3 There is no environment dimension

Canvas is one-per-flow. There is no notion of *this is the test placement, that
is the prod placement* — so "config for test and config for prod with different
options" has nowhere to live.

---

## 3. Environment configs: base ⊕ overlay

A flow gets **one base canvas** plus **one overlay per environment**:

```yaml
# base — what the flow NEEDS, environment-independent
spec:
  placement: { mode: shared }
  health:
    dependsOn: [ … ]          # still auto-derived from the flow's tasks
  lifecycle:
    restart: { policy: on-failure }

---
# overlay: test
environment: test
spec:
  placement:
    namespace: team-a-test
    runtimeSelector: { tier: test }
  lifecycle:
    scaling: { min: 1, max: 1 }

---
# overlay: prod
environment: prod
spec:
  placement:
    mode: isolated            # noisy-neighbour / compliance isolation
    namespace: team-a-prod
    runtimeSelector: { tier: prod, class: gpu }
    tolerations: [ { key: dedicated, value: team-a } ]
  lifecycle:
    scaling: { min: 2, max: 10, metric: queue_depth, target: 20 }
```

**The effective canvas is `base` deep-merged with the named overlay**, overlay
winning per key. The merge is ordinary configuration composition.

### 3.1 Why this is NOT the policy layering rule

`policy-implementation.md` §2.2 insists layers attenuate and never override. That
rule does **not** transfer here, and it is worth being explicit about why, because
applying it by analogy would be wrong.

Policy layers answer *how much authority exists* — there, a lower layer widening
an upper one destroys the guarantee. Deployment overlays answer *where and how big*
— and prod legitimately needs MORE than test: more replicas, stricter isolation,
dedicated nodes. An overlay that could only narrow could never express prod.

**Authority still comes from Cedar, not from the config.** The overlay says what
is wanted; `flow:deploy` decides whether this principal may have it. Config
composes freely; placement is authorized. Keeping those two separate is what lets
the merge stay a plain deep-merge without becoming a security surface.

---

## 4. Chat as the deploy surface

### 4.1 Tools, not an intent router

The instinct is to detect "this is a canvas question" and route. **Do not build a
classifier.** Give the agent canvas tools with honest descriptions and the model
routes by itself — which is how every other capability in this console already
works, and it degrades gracefully when a request is ambiguous (it asks) rather
than mis-routing silently.

### 4.2 The tool set (option C — authoring included)

| Tool | Kind | Gate |
|---|---|---|
| `list_runtimes` | read | open |
| `list_canvases` | read | open |
| `get_canvas` | read | open |
| `preview_placement` | read | open |
| `get_deployment_status` | read | open |
| `create_canvas` | write (config) | `resource:create` |
| `update_canvas` | write (config) | `resource:update` |
| `deploy_flow` | **act** | `flow:deploy` |
| `pause_flow` / `resume_flow` | **act** | `flow:deploy` |

**Authoring is inert until deploy, and that is what makes authoring tools safe to
give an agent.** `create_canvas`/`update_canvas` only write configuration; nothing
is placed anywhere until `deploy_flow`, which is separately gated, previews first
and raises an approval for prod. The dangerous verb is `deploy`, not `write`.

Two rules keep it that way:

1. **Every authoring write runs `CanvasSpec::validate()` and refuses on error**,
   returning the problems rather than storing a broken spec. The validator already
   exists and is tested; a malformed `flowRef` or `max < min` must never reach the
   store.
2. **An agent may author a PRESET freely** (`flowRef` empty — a reusable profile
   that deploys nothing) and may bind one to a flow, but binding is where the
   `flowRef` is checked. This is the existing preset concept in
   `flowr-types::canvas`, not a new mechanism.

`preview_placement` is the tool that makes the conversation work: it answers
"where would this go?" with no side effects, so a plan can be shown before it is
applied, and it is the natural thing for the agent to call before `deploy_flow`.

### 4.3 Reuse, not new plumbing

Canvas tools follow the existing **agent-runtime tool** seam exactly — the one
`create_agent` uses:

1. declare in `get_tools()` and add to `AGENT_RUNTIME_TOOLS` (flowr-mcp-core);
2. dispatch in `handle_agent_tool_call` (flowr-ce, where `canvas.rs` already is);
3. the gate in `call_mcp_tool` already runs BEFORE dispatch, so the whole
   Ask/approve/ledger path applies with no new machinery.

One refactor is needed: `compute_placement` returns `Result<Value, Response>` and
is axum-coupled. Lift it to `Result<Value, String>` so both the HTTP handler and
the tool can call it.

### 4.4 The conversation shape

```
user:  test this flow
agent: preview_placement(env=test) → "lands in team-a-test on runtime-test-2, 1 replica"
       deploy_flow(env=test)       → deployed, status Synced
user:  looks good, ship it
agent: preview_placement(env=prod) → "isolated, team-a-prod, 2-10 replicas, gpu tier"
       deploy_flow(env=prod)       → Cedar: Ask → APPROVAL CARD IN CHAT
user:  [Approve]                   → deployed
```

### 4.5 Prod deploy reuses the approval mechanism exactly

This is the part that already works. `flow:deploy` onto a prod-tier runtime
returns `PolicyDecision::Ask`; the agent suspends; the existing card renders with
the tool, the target and the arguments; answering it resumes the deploy. No new
approval machinery.

It also inherits the properties that took several rounds to get right: the
approver must already hold the authority (so nobody is prompted into granting a
placement they could not perform), a refusal is sticky for the conversation and
visible in the permissions panel, and a timeout records nothing.

**Scope caveat:** `GrantScope::Session` is the wrong lifetime for a prod deploy.
A deploy approval should be `Once` — approving a deploy must not silently
authorize the next one. Filter the offered scopes for `flow:deploy` to `Once`
only, the same way `Agent` is filtered out today for lacking persistence.

---

## 5. Phases

### Phase D1 — Authorize deployment (prerequisite, security)

1. Add `flow:deploy` to the action vocabulary; declare `Runtime` and `Namespace`
   entities in the Cedar schema.
2. Thread `principal` into `deploy_flow` / `deploy_canvas_resource` and gate at
   the existing choke point, with the resource carrying the runtime's labels
   (`tier`, `class`, `zone`) and target namespace as attributes.
3. Baseline rules: deploying to `tier == "prod"` requires Admin; a flow may only
   be placed into a namespace its team owns.
4. `Ask` is derived as everywhere else — an owner deploying to prod is a question,
   a non-owner is a denial.

**Verify:** a Viewer deploying to test succeeds and to prod is refused; an Admin's
agent deploying to prod raises the approval card; the endpoint refuses an
unauthenticated call. Cedar decisions visible in `/security/audit`.

### Phase D2 — Environment overlays

1. `environment` on the canvas record; `base` + `<env>` overlays stored together.
2. `effective_canvas(flow, env)` = deep-merge, with the merge covered by tests
   (overlay wins per key; absent overlay = base unchanged).
3. `preview_placement` and `deploy` take an `environment` argument, defaulting to
   `test` — **never** `prod`, so an unqualified "deploy" can never mean production.

**Verify:** the same flow lands in different namespaces/tiers per environment;
prod overlay expresses more replicas and `isolated` mode; a flow with no overlay
deploys on base alone.

### Phase D3 — Canvas tools in chat

1. The five tools above, dispatched through the gate that already covers MCP calls.
2. `preview_placement` is read-only and safe to call freely; `deploy_flow` is the
   gated one.
3. Tool descriptions state that deploying to prod will ask the user — the same
   "permission NOW" convention added in `e66c3bd`, so the model does not pre-check
   or refuse on stale memory.

**Verify:** "test this flow" previews then deploys to test; "ship it" previews
prod and raises the card; declining sticks for the conversation.

### Phase D4 — Status and promotion in the UI

1. Deployment status on the flow/canvas page: phase, placed runtime, replicas,
   health, drift.
2. A promotion view: what is running in test vs prod, and the diff between the
   two effective canvases.

---

## 6. Out of scope (named, not designed)

- **The controller is not E2E-tested.** It compiles and reconciles in principle;
  nothing here has been exercised against a live cluster. Phase D1–D3 are
  meaningful in CE's logical-placement mode, but "it deployed" in EE means the
  controller actually worked, and that is unproven.
- **Rollback / promotion history.** Knowing what was deployed when, and going
  back, is a real requirement and is not covered by any of the above.
- **Hibernation and sandboxing** (`A3T_RUNTIME_ISOLATION.md`) — canvas concerns,
  but independent of deploying from chat.
- **Multi-cluster / multi-region placement.** `runtimeSelector` has `zone` but
  nothing resolves it across clusters.

## 7. Open questions

1. **Where do overlays live?** A single canvas record with an `environments` map
   is simpler to read and diff; separate records per environment are easier to
   authorize independently (Cedar can gate the prod record itself). The second is
   probably right if prod config should be editable only by Admins — which is the
   same separation-of-duties argument that put Cedar policies in their own
   resource type.
2. **Should `test` deploys be gated at all?** Ungated is friendlier; gated is
   consistent. Suggest gating but permitting broadly, so the audit trail exists
   from day one.
3. **What does "deploy" mean when the flow is already running?** Redeploy,
   in-place update, or refuse without an explicit `--force`. Drift reconciliation
   in the controller assumes one of these; it should be stated.
4. **Does an agent get to deploy at all, or only propose?** An agent with
   `flow:deploy` can place workloads. Restricting agents to `preview_placement`
   and making `deploy_flow` human-only is defensible and worth deciding before
   the tools ship.
