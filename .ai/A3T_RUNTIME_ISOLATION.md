# A3T Runtime Isolation & Hibernation (gVisor) — implementation plan

> Companion to `A3T_CANVAS_SPEC.md`. The canvas spec defines *where* a flow runs
> (placement) and *how it scales* (lifecycle); this doc defines **how the runtime
> is sandboxed (gVisor), how tenancy is isolated, and how idle flows hibernate.**
> Nothing here changes the canvas *primitives* — it adds two fields and reuses the
> existing AESOP choke point + Cedar. Status: **design (2026‑08‑29), not built.**

## 1. Why

Today every flow, agent, chat, and **Test** runs in the single `flowr` pod. That
means a test can (a) crash the shared runtime (OOM / runaway loop) and (b) load it
for everyone. We need: **blast-radius isolation**, **resource control**, and
**scale-to-zero for idle flows** — without a pod-per-user cost.

## 2. Separation of concerns (the load-bearing idea)

Isolation is four layers, and the complexity lives in different places:

| Concern | Layer | Notes |
|---|---|---|
| **Isolation boundary** | Runtime — **gVisor** sandbox | a flow can't reach another sandbox by default |
| **Enforcement point** | **AESOP hook** (in-runtime, EE) | already runs at every `call:mcp/tool/llm/resource` |
| **Access *decision*** | **Cedar** (EE) | who may reach what — the *policy* |
| **Network channel** | **NetworkPolicy** / broker (infra) | physical reachability, deny-by-default |

**The runtime stays dumb** (isolate + enforce the decision it's handed). **Cedar is
smart** (decides). This is why cross-tenant access (Mode 4 below) needs *no new
runtime mechanism* — just policy + network.

## 3. gVisor as the runtime sandbox

- Runtimes run under **gVisor (`runsc`)** via a k8s `RuntimeClass`. The controller
  sets `runtimeClassName: gvisor` on the worker Deployment it emits from a `Runtime`
  CR.
- **Prereqs:** `runsc` installed on the target nodes (containerd + the runsc shim) and
  a cluster `RuntimeClass` named `gvisor`. Nodes carry a `gvisor: "true"` label so
  placement can target them.
- **Cost:** gVisor adds syscall-interception overhead (single-digit % for I/O-light
  agent work; higher for syscall-heavy workloads). Acceptable for untrusted/test
  flows; opt-out for trusted high-throughput runtimes (plain runc).

## 4. Isolation granularity — four modes

| Mode | Meaning | Canvas mapping | Complexity |
|---|---|---|---|
| **1. Per-user gVisor** | one sandbox per user | a `Runtime` CR per user (gvisor), placement targets it | Low–Med (density cost) |
| **2. Per-namespace aggregate** | one shared gVisor runtime per namespace; many flows load in | one shared `Runtime` per namespace, `mode: shared` | **Low — the default** |
| **3. Per-flow gVisor** | a sandbox dedicated to one flow | `canvas.placement.mode: isolated` → dedicated gvisor pod | Low |
| **4. Cross-sandbox access** | a flow in sandbox A reaches a resource "in" sandbox/namespace B | **NOT a runtime feature** — see below | High (policy epic) |

**Modes 1–3 are pure canvas placement** (`mode` + `namespace` + which `Runtime` you
target) plus the gVisor runtimeClass. No new concept.

**Mode 4 is a Cedar/EE + network concern, not runtime.** The namespace hierarchy
becomes **attributes** (`principal.namespace`, `resource.namespace`/`parent`) —
same pattern as `principal.template.*` — and cross-access is a **Cedar rule** (e.g.
*permit when `principal.namespace` is an ancestor of `resource.namespace`; forbid
across siblings unless a grant exists*). It drops into **schema-from-registry** and
the existing choke point with zero runtime change. Cedar's "yes" is necessary but
not sufficient: a **NetworkPolicy** (deny-by-default, Cedar-gated opens) must carry
the allowed connection. CE = single-tenant/allow-all (no cross graph); EE turns it
on via Cedar — the usual CE/EE seam.

## 5. Hibernation — two levels (do not conflate)

The canvas spec (§4) scoped *flow* hibernation out as "flow-memory config." That is
the **cheap level, and flowr already has it.** There is a second, heavier level.

**Level A — flow-level checkpoint (have it, cheap, default).**
flowr's `checkpoints` table + `FlowDurability::Checkpoint` already persist a flow's
logical state. So: a flow waiting on a trigger → checkpoint its state → the runtime
**scales to zero** → on trigger, a fresh pod reloads the flow from the registry and
**restores state from the checkpoint** → resumes. Wake = pod boot + engine init
(~seconds), state preserved. **This satisfies ~90% of the "hibernate idle / wake on
trigger / offload ephemeral" goals with tech we already have.**

**Level B — sandbox checkpoint/restore (gVisor S/R, frontier).**
gVisor supports S/R (snapshot the whole sandbox process memory to disk, restore it).
This gives **instant wake, no re-init** — the latency optimization for RTB/trading.
But k8s has **no native "snapshot pod to disk and restore"** (Checkpoint/Restore is
an alpha KEP; gVisor S/R + a snapshot store + a restore-on-trigger controller is a
custom build). **Reserve for latency-critical runtimes; not the default.**

| Trigger | Level A (flowr checkpoint) | Level B (gVisor S/R) |
|---|---|---|
| No user / no flow | scale runtime to zero | snapshot sandbox → evict pod |
| Flow waits for trigger | checkpoint state, free the slot | snapshot, restore on trigger |
| Ephemeral flow done | checkpoint + drop from memory | (n/a — short-lived) |
| Wake latency | seconds (pod+engine boot) | ~instant (restore) |
| Build cost | **low (exists)** | **high (frontier)** |

## 6. Schema additions (clean, minimal)

**`Runtime` CR** gains one field:
```yaml
spec:
  runtimeClassName: gvisor        # sandbox; omit → plain runc (trusted)
  labels: { class: sandbox }
  nodeSelector: { gvisor: "true" }
```

**`Canvas.lifecycle`** gains a hibernation block (the home for all of §5):
```yaml
lifecycle:
  state: running                  # running | paused | hibernated
  hibernation:
    idleTimeout: 5m               # no flow/trigger for this long → hibernate
    mode: flow | sandbox          # flow = checkpoint (Level A); sandbox = gVisor S/R (Level B)
    wakeOn: [trigger, request]
```

No other primitives change. Isolation modes 1–3 remain `placement.mode` + `namespace`
+ target `Runtime`.

## 7. CE vs EE

- **CE:** ignores the canvas; single in-process runtime; no gVisor, no cross-tenant
  graph, no hibernation orchestration (LanceDB, one pod). Consistent with `EE = CE +
  aesop + one setting`.
- **EE:** controller reconciles gVisor runtimes, Level‑A hibernation via checkpoints,
  and Cedar-gated Mode‑4 access. Level‑B S/R is an EE add-on for latency-critical pools.

## 8. Complexity + phased plan

- **P-iso-1 (Low, fits now):** add `Runtime.runtimeClassName` + `Canvas.lifecycle.hibernation`
  to the CRD/spec; controller sets `runtimeClassName`/`nodeSelector` on the Deployment.
  Ship the **sandbox runtime pool** (`class=sandbox`, gVisor, resource limits, HPA,
  optional tainted node) and **route Test executions to it**. Immediately isolates
  tests from prod.
- **P-iso-2 (Med):** **Level‑A hibernation** — controller scales a runtime to zero on
  `idleTimeout`; trigger/request wakes it; flow state restored from `checkpoints`.
  Requires the deferred **queue/dispatch + trigger→wake** wiring (execution plane).
- **P-iso-3 (High, EE):** **Mode 4** as a Cedar policy pattern (namespace-hierarchy
  attributes) + NetworkPolicy deny-by-default with Cedar-gated opens.
- **P-iso-4 (Frontier):** **Level‑B gVisor S/R** snapshots for instant wake on
  latency-critical runtimes (custom controller + snapshot store).

## 9. Prereqs & open questions

- **Infra:** runsc on nodes + `RuntimeClass gvisor`; a node pool labeled `gvisor: "true"`;
  a snapshot store (object storage) for Level‑B later.
- **Execution plane:** hibernation presupposes the **queue/dispatch + trigger-wake**
  layer that is still deferred (today flows run in the one pod, off the canvas path).
- **Per-execution guardrails** (independent of pools): execution **timeout**, max
  iterations, token/tool budget, and **CPU/memory `limits`** on runtime pods — the
  real backstop so a single runaway flow is killed, not the pod/node. Add these to the
  shared pod **now** as the interim safety even before pools exist.
- **Open:** does Mode‑4's namespace hierarchy use HNC, vcluster, or a plain
  `resource.namespace` attribute convention? (Leaning: attribute convention + Cedar,
  no HNC dependency.)

## 10. Net

- **gVisor runtimeClass + modes 1–3 + Level‑A hibernation** = a **direct, low-risk fit**
  for the canvas infra (two fields + reuse of `checkpoints`).
- **Mode 4** = **Cedar (EE) + NetworkPolicy**, zero new runtime mechanism.
- **Level‑B gVisor S/R** = real but **frontier**; the schema holds it, the mechanism
  is a later custom build.

The runtime stays the dumb, sandboxed executor; the canvas places and hibernates it;
Cedar governs who crosses. Complexity is pushed to the layer that should own it.
