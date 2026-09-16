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

### 2.3 Presets exist but are unreachable

`CanvasSpec` already supports presets — an empty `flowRef` marks a reusable
deployment profile, and `description` exists to label one in a picker. But there
is no tool to list them, no way to clone or bind one from a conversation, and no
rendering of one for review. The mechanism is built and has no route to the user.

---

## 3. Presets are the deployment configs

A user wanting to deploy does not want to author placement YAML. They want to
pick a known-good deployment shape, look at it, and either use it or make their
own from it. That is exactly the **preset** already in `flowr-types::canvas`: a
`CanvasSpec` with an empty `flowRef` — a reusable profile that deploys nothing
until it is bound to a flow. `description` is already documented as the text
"shown in the canvas picker to choose a preset".

So the deployment configs are presets, named for what they are for:

```
test-single        shared pool, tier=test, 1 replica, restart on-failure
prod-ha            isolated, tier=prod, 2-10 replicas on queue_depth
prod-gpu           isolated, tier=prod class=gpu, dedicated-node toleration
batch-ephemeral    shared, ephemeral session, scale to zero when idle
```

**No base ⊕ overlay merge, and no `environment` field.** An earlier draft of this
plan proposed layering a per-environment overlay onto a base canvas. Presets make
that unnecessary: "the test config" and "the prod config" are two presets, each
readable on its own, each independently authorizable. Merging would add a
composition step whose result nobody can see without evaluating it — the opposite
of "user reviews it and decides".

### 3.1 The lifecycle of a preset

```
preset  ──clone──►  a canvas of your own  ──bind──►  bound to a flow  ──deploy──►  running
(flowRef empty)     (flowRef still empty,            (flowRef set)                 (placed)
                     yours to edit)
```

- **Reuse** — bind the preset directly to the flow. Nothing is cloned; the preset
  stays shared and unedited.
- **Clone** — copy the preset into a new canvas, change what needs changing, then
  bind. The preset is untouched, so one team's edit cannot alter another's
  deployment shape.

Cloning and binding are **authoring**: they write configuration and place nothing.
Only `deploy_flow` acts.

### 3.2 Review before deciding

The user must be able to see what a preset actually does before choosing it —
placement mode, target namespace, selector, replica range, restart policy,
session mode. Prose is not enough: "prod-ha" tells you nothing about whether it
is isolated or how far it scales.

So a preset is rendered in chat as a **card**, the same way the approval question
is: a data part the agent emits, rendered by the client, with the decision on it
("Use this" / "Clone and edit"). The pattern, the stream path and the widget
plumbing already exist.

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
| `list_canvas_presets` | read | open |
| `get_canvas` | read | open |
| `list_runtimes` | read | open |
| `preview_placement` | read | open |
| `get_deployment_status` | read | open |
| `clone_canvas` | write (config) | `resource:create` |
| `create_canvas` | write (config) | `resource:create` |
| `update_canvas` | write (config) | `resource:update` |
| `bind_canvas` | write (config) | `resource:update` |
| `deploy_flow` | **act** | `flow:deploy` |
| `pause_flow` / `resume_flow` | **act** | `flow:deploy` |

`list_canvas_presets` is what makes the flow in §4.4 work — it returns presets
with their `description`, placement summary and scaling range, for rendering as
review cards. `clone_canvas` takes a preset plus a patch, so "prod-ha but with 4
replicas minimum" is one call and the shared preset is untouched.

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
user:  deploy this flow
agent: list_canvas_presets()            → renders preset cards for review
       "test-single and prod-ha fit; test-single is the usual starting point"
user:  [Use this] on test-single
agent: bind_canvas(test-single, flow)   → authoring, nothing placed
       preview_placement(flow)          → "runtime-test-2, team-a-test, 1 replica"
       deploy_flow(flow)                → deployed, Synced
user:  ship it to prod but give it 4 replicas minimum
agent: clone_canvas(prod-ha, {scaling.min: 4})   → a new canvas, prod-ha untouched
       preview_placement(flow)                    → isolated, tier=prod, 4-10
       deploy_flow(flow)                          → Cedar: Ask → APPROVAL CARD
user:  [Approve]                                  → deployed
```

Note what the agent never does: invent placement from scratch. It picks from
presets, shows them, and clones on request. The blast radius of a model getting
this wrong is a badly-scaled clone that still has to pass preview and approval.

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

### Phase D2 — Presets: list, review, reuse, clone

1. Seed a small set of presets (§3) as `type_ref: canvas` registry records with
   empty `flowRef`.
2. `list_canvas_presets` returning description + a placement/scaling summary.
3. `clone_canvas(preset, patch)` — deep-merge the patch over the preset, run
   `CanvasSpec::validate()`, refuse on error, store as a new canvas. The source
   preset is never mutated.
4. `bind_canvas(canvas, flow)` — set `flowRef`; this is the step that makes a
   canvas deployable, so it validates the ref.
5. Preset review card in chat: a data part rendering mode, namespace, selector,
   replica range, restart and session mode, with "Use this" / "Clone and edit".

**Verify:** a preset can be reused by two flows without either affecting the
other; a clone with a patch differs only in the patched keys; binding a preset
with a malformed flowRef is refused; `is_preset()` stays true for the source
after a clone.

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
