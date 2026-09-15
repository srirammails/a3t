# A3T Canvas — reconciled specification (2026-08-01)

> Supersedes the "Canvas = Layer-3 multi-agent wiring graph" model in
> `A3T_ARCHITECTURE_VISION.md` §Canvas and the `AgentCanvas` CRD sketches in
> `agent.md` / EPIC 30. Those conflated *flow composition* (logic) with
> *deployment* (ops). This spec separates them.

## 1. The four concepts (final)

| # | Concept | What it is |
|---|---------|------------|
| 1 | **Flow / Workflow** | The **fundamental type**. A CNCF-serverless-workflow document: tasks, control flow, resource references. All logic lives here. |
| 2 | **Flow runtime** | The **application engine / container** that executes flows (the `flowr` binary + `swf-runtime`). A labeled, namespaced, deployable unit. |
| 3 | **Agent** | **Not a separate primitive — a *kind of flow*.** Any flow whose tasks include `call:llm`, `call:mcp`, and/or `call:a3t:agent`. Deployed exactly like any other flow. |
| 4 | **Canvas** | **A flow's deployment metadata, expressed as YAML.** It is *ops*, not logic. It (a) deploys/pauses/restores the flow in the right runtime, (b) manages/scales/hibernates/restarts that runtime, (c) manages flow health by watching the flow's dependent-resource events. In EE it becomes a **k3s Custom Resource**; in CE it is **ignored**. |

**Key reframing:** Canvas does **not** own a monolithic multi-flow graph. It is
the deploy/scale/health envelope around **one flow** (Option 1), plus that flow's
**connections** to other flows/resources (below). Multi-flow topology is a
*runtime* view computed from live connections — never a static IaC state file.

## 1a. Connections & the runtime graph (Option 1 — locked 2026-08-02)

**Canvas = per-flow deployment config.** One canvas : one flow. A developer can
reference **other flows** two ways; anything that is not a referenced flow is a
**resource** (LLM/DB/MCP/… — and a running flow exposed as a resource):

1. **Sub-flow — static load (reusable function).** The referenced flow is loaded
   **into this canvas's runtime** and called in-process (`run:workflow`). It is
   *bundled/co-deployed* with this canvas; no separate deployment. Semantics: a
   reusable function. Capability composition is attenuate-only (existing ceiling).
2. **Flow resource — connect to a running instance (external process).** Binds to
   a **separately-running** flow via its I/O (endpoint/queue). Semantics: an
   external process / service call. The CR **dependency-deploys** it: if the
   target flow isn't already running, deploy it (else bind to the running one).

**The graph is a RUNTIME feature, not a static one.** There is no Terraform-style
desired-state file. The dependency/topology graph and "what depends on X / impact
of removing X" are **computed on demand from the live set of Canvas CRs + their
connections** (Kubernetes/etcd is the state store; `status` + ownerReferences
carry the edges). This gives Option-2's useful dependency-awareness without a
parallel state engine.

| Connection kind | What it is | Deploy behavior | FlowR mechanism |
|---|---|---|---|
| **sub-flow** | reusable function, in-process | loaded with this canvas (bundled) | `run:workflow` |
| **flow resource** | external running process | CR ensures target deployed (dependency-deploy) | I/O bind (endpoint/queue) |
| **resource** | LLM / DB / MCP / secret | referenced, not deployed by this canvas | `call:llm`/`call:mcp`/`call:clawdb` |

```
Flow (logic, incl. multi-agent composition)
  └─ referenced by ─► Canvas (deployment metadata: where/scale/health)
                        └─ parsed by ─► k3s Canvas CR + controller (EE)
                                          places flow into ─► Flow runtime (labeled, namespaced)
```

## 2. Flow is central — it *refers to* OR *embeds* its canvas

Flow is the source of truth for A3T; the canvas hangs off the flow two ways:

- **Reference** (shared / reusable canvas):
  `metadata.a3t.canvas.ref: <namespace>/<name>` → a stored Canvas.
- **Embed** (small inline config — the common case):
  `metadata.a3t.canvas.inline: { … }` so a flow is self-contained.

Either way the flow document stays pure logic; the canvas block is metadata. At
deploy, the **effective canvas** (the inline block, or the resolved ref) produces
a Canvas CR whose `spec.flowRef` = this flow. So **flow→canvas is the authoring
link**; `canvas.spec.flowRef` is the runtime link the controller reconciles.

Storage: reuse the `canvas_specs` table (restructure `spec_yaml` to §3).

## 3. Canvas YAML schema

```yaml
apiVersion: a3t.io/v1alpha1          # ONE canonical group (retire workflow.a3t.io/*)
kind: Canvas
metadata:
  name: orders-pipeline
  namespace: default
spec:
  flowRef: default/orders-flow       # the flow this canvas deploys (derived when embedded)

  # (a) placement — which runtime the flow LOADS INTO, and how
  placement:
    mode: shared                     # shared | isolated  (see §4)
    runtimeSelector:                 # pick a runtime POOL by label; governed by policy (§6)
      class: gpu
      tier: prod
    namespace: team-a                # k8s namespace → isolation → policy
    tolerations: [ { key: dedicated, value: team-a } ]   # k8s taints (isolated/dedicated nodes)
    # CE ignores this block except a simple runtime label mapping (see §5).

  # (b) lifecycle — scale/pause/restart the flow within its runtime(s)
  lifecycle:
    state: running                   # running | paused
    scaling: { min: 1, max: 5, metric: queue_depth, target: 20 }   # concurrency/replicas for THIS flow
    restart: { policy: on-failure, backoff: exponential, maxRetries: 5 }

  # (c) health — monitor the flow's dependent resources
  health:
    dependsOn:                       # AUTO-DERIVED from the flow's tasks; overridable
      - resource: default/openrouter-llm   # from call:llm
      - resource: default/orders-db         # from call:clawdb
      - resource: default/tools-mcp         # from call:mcp
    onDependencyDown: queue          # queue | cordon | circuit-break | fail
    circuit: { errorThreshold: 0.5, window: 1m, action: open }

status:                              # written by the controller (EE)
  phase: Synced                      # Pending|Deploying|Synced|Drifted|Paused|Degraded
  placedRuntime: runtime-gpu-prod-2
  observedGeneration: 4
  replicas: 2
  readyReplicas: 2
  health: Healthy                    # Healthy|Degraded|DependencyDown
  drift: false
  conditions: [ ... ]
```

Health `dependsOn` is **derived from the flow**: the executor already resolves
`call:llm`→Llm, `call:mcp`→Tool/MCP, `call:clawdb`→DB (same enrichment AESOP uses
at the choke point). The controller reuses that to know what to watch — the
author only overrides when needed.

## 4. Flow runtime — a shared, labeled, policy-governed pool

- A **Runtime is a pool that runs MANY flows** — a deployment of the `flowr`
  engine, registered as a resource (`type_ref: runtime`) with **labels**
  (`class`, `tier`, `zone`, …), bound to a **namespace**, optionally on
  **tainted** nodes. Flows **load into** runtimes; a runtime hosts many.
- The **CR deploys flows to runtimes based on policy** (§6). k8s primitives —
  **namespaces** (isolation), **labels** (selection), **taints/tolerations**
  (dedicated nodes) — are leveraged at **both the flow and the container level**.
- Two placement **modes** (`spec.placement.mode`):
  - **shared** — the flow is mapped (by `runtimeSelector` labels) into a shared
    runtime pool alongside other flows. The default.
  - **isolated** — the flow gets its **own** container/runtime (dedicated
    labels/taints), for noisy-neighbor or compliance isolation.
- Labels + namespaces + taints are the **policy surface** (§6): Cedar decides
  *which flows may load into which runtimes/namespaces*.

> **Out of scope here (see `A3T_RUNTIME_ISOLATION.md`):** flow *persistence/memory*
> — flow-level checkpoint state (Level A) — is a **flow config**, not canvas metadata.
> But **runtime sandboxing (gVisor), tenancy isolation modes, and hibernation policy**
> ARE canvas/runtime concerns: they add `Runtime.runtimeClassName` + a
> `Canvas.lifecycle.hibernation` block. Level‑A hibernation *uses* the flow's
> checkpoint state but is *orchestrated* by the canvas (scale-to-zero on idle, wake on
> trigger). Sandbox-level checkpoint/restore (gVisor S/R, Level B) and cross-tenant
> access (Mode 4 = Cedar + NetworkPolicy) are covered in that doc.

## 5. CE vs EE — the seam (consistent with `EE = CE + aesop + one setting`)

| Aspect | CE | EE |
|--------|----|----|
| Canvas file | **Ignored** (except a plain runtime label mapping) | Parsed into the A3T **Canvas CR** |
| Placement | Flow loads into a runtime by **simple label mapping** (a default runtime, or one pinned by label) | **CR schedules flows into runtime pools by policy** (`runtimeSelector` ∩ Cedar) |
| Modes | shared mapping only | **shared / isolated** (§4) |
| Scaling | None (fixed) | `lifecycle.scaling` (per-flow concurrency/replicas) |
| Health / drift | None (basic liveness) | `health.dependsOn` monitoring + drift reconciliation |
| Placement policy | Namespace/label mapping only | Cedar-gated placement + namespace/taint isolation (§6) |

CE stays dependency-light and canvas-free — a flow just loads into a runtime. EE
adds the CR + controller + canvas semantics. This mirrors the AESOP policy-hook
seam exactly: **the canvas is inert in CE and authoritative in EE.**

## 6. Policy integration (AESOP / Cedar)

Placement is an authorization decision, so it goes through the existing choke
point + Cedar model:

- New action **`flow:deploy`** (or `canvas:apply`) on resource **`Runtime{labels}`**
  / **`Namespace{name}`**.
- Cedar rules gate *which principal may deploy which flow into which
  runtime/namespace* — e.g. `forbid deploy to runtime.tier=="prod" unless
  principal has Admin`, or `forbid placement into namespace X unless flow.team==X`.
- Reuses `flowr-policy` actions registry + roles + the audit trail. Runtime labels
  ride as resource attributes (same pattern as `principal.template.*`).

This makes "runtimes labeled + namespace isolation to satisfy policy" a first-class
Cedar concern, not ad-hoc config.

## 7. The k3s Custom Resource + controller

- **CR:** a **dedicated A3T CRD** — `apiVersion: a3t.io/v1alpha1`, `kind: Canvas`
  (schema = §3). Purpose-built for A3T (not a generic k8s object); the controller
  understands flows, runtimes, and A3T resources.
- **Controller:** reconcile loop — watch Canvas CRs → resolve `flowRef` + select a
  runtime pool (`runtimeSelector` ∩ Cedar policy) → load/patch the flow into that
  runtime (or a dedicated one for `isolated`) → apply scaling → watch `dependsOn`
  resource events → write `status` (phase, placedRuntime, readyReplicas, health,
  drift, observedGeneration).
- **Build on what exists:** the `flowr-canvas` crate is *already a kube-rs CRD
  controller* — but today it watches **`Agent`** CRs (`crd.rs` `kind: Agent`,
  reconciled from `agent-crd.yaml`). Since **agent = flow**, that Agent CRD is now
  redundant. **Re-point this controller at the `Canvas` CR** (rename intent:
  `flowr-canvas` → the canvas/flow-deployment controller) and **deprecate the
  Agent CRD** — agent-flows deploy through Canvas like any flow. The existing
  watch→diff→reconcile skeleton + RBAC carry over.

## 8. Reconciliation with current code

| Existing | Fate |
|----------|------|
| `canvas_specs` / `canvas_events` tables + `a3t-canvas` CRUD flow | **Superseded in P1** by modeling Canvas + Runtime as **registry resources** (`type_ref = "canvas"` / `"runtime"`) — Rust-native, gets registry CRUD + Cedar RBAC for free (same pattern as cedar-policy resources). `canvas_events` can still serve as the deploy/drift audit later. |
| `flowr-canvas` crate (Agent CRD controller) | **Repurpose** into the Canvas controller (§7). |
| `agent-crd.yaml` (`kind: Agent`) | **Deprecate** — fold into Flow + Canvas. |
| UI `CanvasList` (real CRUD) / `CanvasDetail` (mock drift UI) | Keep list; back `CanvasDetail`'s drift/reconciliation view with real CR `status`. Fix the `a3t-canvas-list/-create/-delete` vs single-`a3t-canvas`-with-`action` naming mismatch. |
| Divergent field shapes (storage row / vision doc / UI YAML) | **Replaced** by §3 (single schema). |
| `apiVersion` split (`workflow.a3t.io` vs `a3t.io`) | **Settle on `a3t.io/v1alpha1`.** |

## 9. Phased implementation plan

- **P0 — Spec + schema (this doc).** Lock §3 schema, `a3t.io/v1alpha1`, CE/EE
  behavior. Add a JSON Schema for the Canvas body + validate on CRUD.
- **P1 — CE deployment path.** Flow → standard Deployment; flow→runtime pinning
  by label; Runtime as a registered, labeled resource. No CR. (Unblocks E2E.)
- **P2 — Cedar placement policy.** `flow:deploy` action + Runtime/Namespace
  resources; gate placement at the deploy choke point.
- **P3 — EE Canvas CR + controller.** Repurpose `flowr-canvas`; CRD install;
  reconcile place/scale/hibernate; deprecate Agent CRD.
- **P4 — Health + drift.** Derive `dependsOn` from the flow; watch resource
  events; write `status.health`/`drift`; back the `CanvasDetail` UI with it.

## 10. Decisions (resolved 2026-08-01)

1. **Reference direction — RESOLVED: flow-central.** The flow **refers to** its
   canvas (`metadata.a3t.canvas.ref`) or **embeds** a small one
   (`metadata.a3t.canvas.inline`). Flow is the source of truth; the CR's
   `spec.flowRef` is derived (§2).
2. **Runtime granularity — RESOLVED: shared pool.** A runtime runs **many flows**;
   the CR schedules flows into runtimes by policy. Isolation is opt-in per flow via
   `mode: isolated` + namespaces/taints. Modes are **shared | isolated** only —
   flow persistence/memory is a flow-level config, out of Canvas scope (§4).
3. **Canvas CRD — RESOLVED: a dedicated A3T CRD** (`kind: Canvas`,
   `a3t.io/v1alpha1`), purpose-built for A3T (§7).
4. **Agent CRD — deprecate** (agent = flow + canvas); the `flowr-canvas` reconciler
   is repurposed to watch the Canvas CR (§7, §8).

Model locked. **P1 (CE deploy path: flow→runtime label mapping + Runtime as a
labeled multi-flow resource) is the piece that unblocks E2E** — safe to start.

## 11. Triggers & ingress — runtime artifacts (ingress translation is future work)

**Decision (2026-08-25): triggers stay runtime artifacts; the canvas carries NO
`ingress` block.** Triggers (webhook / cron / event) are already implemented in the
runtime (`flowr-ce-agents` TriggerManager) and are the invocation surface for flows.
They are **not** modeled as canvas fields and are **not** translated 1:1 into K8s
objects (no per-trigger `Ingress`/`CronJob`). Keeping them in the runtime avoids
duplicating trigger config into the deploy object and keeps the canvas small.

The canvas therefore remains exactly §3: `flowRef` + `placement` + `lifecycle` +
`health`. **No `ingress` block.**

**Future work — ingress/cron → K8s translation (deferred until the P3 controller):**

- **One base ingress per runtime, not per trigger.** The controller gives each
  runtime a single base URL — a Traefik `IngressRoute` `Host(<runtime-base>) →
  runtime Service`. Every webhook trigger path mounts under that base:
  `trigger URL = <runtime base> + trigger.ep_path`
  (e.g. `https://rt-gpu.dev.a3t.io/webhook/agent/{id}/chat`). The runtime's
  TriggerManager owns path routing; the controller creates **no object per trigger**.
- **Cron stays in the runtime** (leader-elected across runtime replicas), not a K8s
  `CronJob`.
- **Where translation lives:** internal to A3T + the controller. A trigger declared
  at *flow* level (flow metadata) is translated by the controller when the flow is
  placed; if a future canvas ever declares triggers inline, that block would be the
  thing translated. Revisit whether an optional `spec.ingress` block earns its place
  once the controller exists — for now it does not.

## 12. Onboarding shape (context for the deploy model)

Two intended entry points, both consistent with §1–§5:

- **(A) Single-node install:** `install.sh` → k3d + one **runtime** container + the UI
  + `kubectl` + a **sample canvas & flow**. The user can then `kubectl apply` a
  Canvas and a flow directly (the Canvas is the deploy object, §2/§10.1).
- **(B) UI-first:** after install, open the UI, log in with default creds, author or
  upload a flow, and connect to the a3t MCP server / a local LLM.

Both funnel to the same model: a **flow** is the unit, a **runtime** is the compute it
loads into, and a **canvas** binds the two (`kubectl apply canvas`, canvas has
`flowRef`). Triggers remain runtime artifacts per §11.
