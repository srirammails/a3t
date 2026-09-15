# A3T × OpenShell Integration Guide

**Audience:** Claude Code, working on the A3T codebase.
**Scope:** How to integrate NVIDIA OpenShell's Rust crates into FlowR so that every agent and every flow — whether running in its own sandbox Pod or co-located in a FlowR worker process — executes under a uniform, layered policy model.
**Status:** Design-phase implementation guide. Treat as authoritative context; deviations require human approval.

---

## 0. Orientation — what and why

A3T is a platform for autonomous agents. Agents are CNCF Serverless Workflows executed by **FlowR** (Rust runtime, ~18k lines). Agent memory is **ClawDB**, queried via AQL. Agents call tools via MCP servers, reason via LLMs, and learn by updating procedural/episodic memory. All of this has been working with an in-process **ZeroClaw** abstraction.

The problem ZeroClaw doesn't solve: **how do enterprises trust an autonomous agent**. Kubernetes Pod isolation is too coarse — it doesn't see what the agent is trying to do at the inference layer, the tool-call layer, or the flow-call layer. A compromised (prompt-injected) agent can do real damage with only the capabilities its Pod already has.

NVIDIA released **OpenShell** (`github.com/NVIDIA/OpenShell`) in March 2026 as an open-source, Apache-2.0, **Rust** runtime for sandboxing autonomous agents. It composes Landlock (kernel filesystem), seccomp-BPF (syscalls), an HTTP CONNECT proxy with OPA Rego evaluation (network L4+L7), and an inference router. It ships as a workspace of Rust crates.

**A3T's play:** link OpenShell's crates into FlowR, write a Rust controller that renders A3T's domain-specific CRs into OpenShell policy YAML + Pod specs, and offer three editing surfaces (UI for app teams, CR for platform teams, ClusterPolicy for security teams). The result is an honest "NVIDIA-backed security" pitch with A3T-native UX.

**NemoClaw is not in the picture.** NemoClaw is NVIDIA's OpenClaw-specific reference stack (Node.js plugin + Python blueprint). A3T replaces it with its own controller and operational surface. We consume OpenShell directly.

---

## 1. The OpenShell crate map

From `github.com/NVIDIA/OpenShell` as of late April 2026. All crates are Rust, Apache-2.0.

### 1.1 Crates A3T links directly

| Crate | Role in A3T |
|---|---|
| `openshell-policy` | Policy evaluation. YAML schema parsing, OPA/Rego engine, L7 request canonicalization. **Used in-process by FlowR for hot-path policy checks.** |
| `openshell-router` | Inference router. Intercepts LLM calls, applies routing rules, strips disallowed headers, rewrites endpoints. Used by FlowR's `CallLlmTaskExecutor`. |
| `openshell-sandbox` | Sandbox lifecycle primitives. Landlock + seccomp application, CA injection for TLS inspection, filesystem prep. Used by the A3T sandbox bootstrap binary. |
| `openshell-ocsf` | OCSF audit event schema (Open Cybersecurity Schema Framework). Every policy decision in A3T emits OCSF events to `flowr.audit` on Iggy. |
| `openshell-providers` | Credential/provider bundles (Anthropic, OpenAI, Nemotron endpoints, GitHub tokens). Credentials never enter the sandbox; the router substitutes them. |
| `openshell-prover` | Z3-backed formal policy prover. A3T admission webhook uses this to statically verify flow-call policy intersection soundness before accepting an Agent CR. |
| `openshell-driver-kubernetes` | Provisions sandboxes as K8s Pods. The A3T controller uses this to materialize Agent CRs into `sandbox Pod + ConfigMap + NetworkPolicy`. |

### 1.2 Crates A3T ignores

| Crate | Why we skip |
|---|---|
| `openshell-cli` | We're a controller, not a human at a terminal. |
| `openshell-tui` | k9s-like dashboard. A3T has its own UI. |
| `openshell-server` / `openshell-gateway` | The OpenShell control plane. **A3T's controller replaces this.** We reuse the primitives, not the daemon. |
| `openshell-driver-vm` | microVM backend. Nice to have later for stronger isolation; not needed for v1. |
| `openshell-bootstrap` | Their gateway-side image transfer logic. Not relevant once we're not running their gateway. |

### 1.3 License & dependency hygiene

- Everything is Apache-2.0. No copyleft concerns.
- **Preserve LICENSE notices** in any file you vendor or derive from.
- **Don't use NVIDIA trademarks** to imply endorsement. "Built on NVIDIA OpenShell" factual — fine. "NVIDIA-certified A3T" — not fine.
- Depend via git (`git = "https://github.com/NVIDIA/OpenShell", rev = "<pinned-sha>"`) until they publish to crates.io. **Always pin by commit SHA, never by branch.** Alpha software API churn is expected.

---

## 2. Two deployment modes for FlowR + agents

A3T supports two modes for how agent code runs relative to the FlowR worker. The CR declares which mode; the controller materializes accordingly. **Both modes must share the same policy evaluation, differing only in enforcement topology.**

### 2.1 Mode A — Co-located (single process)

**When:** Low-risk agents, trusted tool surface, latency-critical. Example: an RTB bid agent whose "autonomous" behavior is bounded to scoring decisions over a fixed flow graph.

**Topology:**

```
┌───────────────────────────────────────────┐
│ Pod: flowr-worker                         │
│  ┌─────────────────────────────────────┐  │
│  │ flowr-worker process (Rust)         │  │
│  │                                      │  │
│  │  FlowR executor                      │  │
│  │  + openshell-policy evaluator        │  │
│  │  + openshell-router (in-process)     │  │
│  │  + agent flow runs as steps          │  │
│  │                                      │  │
│  │  Landlock applied at proc startup    │  │
│  │  seccomp-BPF applied at proc startup │  │
│  │  egress → in-process CONNECT proxy   │  │
│  └─────────────────────────────────────┘  │
└───────────────────────────────────────────┘
```

**Trust guarantees:** Kernel-level Landlock + seccomp still hold (applied once to the worker process). Policy checks at every task executor boundary. Network egress forced through the in-process proxy. The agent cannot escape the process without a kernel bug.

**Trust limits:** The agent shares address space with FlowR. A memory-corruption vulnerability in a FlowR dependency could bypass policy. Acceptable risk for the bounded-autonomy case; unacceptable for open-ended agents.

### 2.2 Mode B — Sandboxed (two processes, two Pods, gRPC)

**When:** Open-ended agents, untrusted LLM output, broad tool access, prompt-injection exposure. Default for anything the CR doesn't explicitly mark as low-risk.

**Topology:**

```
┌─────────────────────────────┐      ┌──────────────────────────────────┐
│ Pod: flowr-worker           │      │ Pod: agent-sandbox-<flowInstance>│
│                             │      │                                  │
│  flowr-worker (Rust)        │      │  a3t-sandbox-bootstrap (Rust)    │
│  - orchestration            │      │  - loads policy ConfigMap         │
│  - talks to Iggy, Dragonfly │ gRPC │  - applies Landlock              │
│  - talks to ClawDB          │◄────►│  - applies seccomp-BPF            │
│  - regorus/openshell-policy │      │  - execs agent-runner             │
│  - egress proxy (in-proc)   │      │                                  │
│                             │      │  agent-runner (Rust, same binary │
│                             │      │  as flowr-worker, subset of tasks)│
│                             │      │  - NO direct net access          │
│                             │      │  - NO ClawDB access              │
│                             │      │  - all outbound → flowr-worker   │
│                             │      │    via Unix socket mounted in    │
└─────────────────────────────┘      └──────────────────────────────────┘
```

**Trust guarantees:** Agent code runs in a separate process with its own Landlock/seccomp policy, typically much tighter than FlowR's. Every outbound tool call and inference call routes through the flowr-worker's policy + proxy, evaluated against the agent's policy context. Mode-A's in-process enforcement becomes the enforcement layer for the remote agent-runner.

**Trust limits:** Pod-to-Pod traffic on a Unix socket (mounted via shared `emptyDir` volume or a dedicated socket-proxy sidecar — see §6.4). No network namespace escape vector because the agent Pod has `networkPolicy: deny-all` except to the flowr-worker socket.

### 2.3 How the CR picks a mode

```yaml
apiVersion: a3t.io/v1
kind: Agent
spec:
  runtime:
    mode: co-located | sandboxed     # default: sandboxed
    coLocatedJustification: "..."    # required if mode=co-located
```

Admission webhook enforces: `co-located` requires a matching `ClusterPolicy.allowCoLocated: true` and a non-empty justification string logged into the audit trail. Platform teams set the default cluster-wide.

---

## 3. Policy, layered

Three layers of policy. Each layer is owned by a different team, stored as a different CRD, and has different rights over the final effective policy applied to a running agent.

```
┌─────────────────────────────────────────────────────────┐
│  ClusterPolicy (cluster-scoped, Security team owns)     │
│  ─ hard ceiling                                          │
│  ─ admission webhook rejects any Agent CR that violates │
└────────────────────┬────────────────────────────────────┘
                     │ narrows
                     ▼
┌─────────────────────────────────────────────────────────┐
│  PolicyPreset (namespaced, Platform team owns)          │
│  ─ named bundles: restricted / standard / elevated      │
│  ─ Agent CRs reference a preset by name                 │
└────────────────────┬────────────────────────────────────┘
                     │ narrows
                     ▼
┌─────────────────────────────────────────────────────────┐
│  Agent (namespaced, app team owns, UI-editable)         │
│  ─ declarative request: "I need tool X, LLM Y, ..."     │
│  ─ resolves against preset + cluster policy              │
└────────────────────┬────────────────────────────────────┘
                     │ intersected & rendered by controller
                     ▼
┌─────────────────────────────────────────────────────────┐
│  Effective OpenShell YAML (generated, immutable)        │
│  ─ applied to sandbox Pod and/or flowr-worker process    │
└─────────────────────────────────────────────────────────┘
```

**Composition rule: intersection, always.** Layers never widen. A lower layer can only equal or tighten what the upper layer permits. The admission webhook rejects any Agent CR whose effective policy would exceed its preset, and any preset that would exceed the cluster policy.

### 3.1 ClusterPolicy — the security ceiling

**Owner:** Security team.
**Storage:** cluster-scoped CRD, GitOps-synced, protected by RBAC (no app team RBAC access).
**Purpose:** Invariants the organization will not compromise on.

```yaml
apiVersion: a3t.io/v1
kind: ClusterPolicy
metadata:
  name: default
spec:
  # Hard denylists — nothing below may override
  denied:
    egressHosts:
      - "*.public-paste.*"
      - "*.anonymous-upload.*"
    callableFlowPatterns:
      - "*-admin"
      - "*-destructive"
      - "system-*"
    memoryStatements:
      - FORGET        # no agent may delete its own history ever
    inferenceProviders:
      - "unapproved-*"

  # Gates — require security approval to deviate
  requiresApproval:
    - coLocatedMode
    - publicInternetEgress
    - fullRepoWriteAccess

  # Defaults pushed down to all namespaces
  defaults:
    sandboxMode: sandboxed
    auditLevel: all_decisions
    tlsInspection: terminate
    landlockMode: hard_requirement

  # Approved provider tiers (references openshell-providers bundles)
  approvedProviderTiers:
    - tier: trusted
      providers: [nemotron-local, claude-sonnet-via-router]
    - tier: restricted
      providers: [gpt-4.1-via-router]
      conditions:
        - noCustomerData: true
```

### 3.2 PolicyPreset — the platform team's curated bundles

**Owner:** Platform team.
**Storage:** namespaced CRD, GitOps-synced.
**Purpose:** Turn OpenShell's complexity into named choices app teams can pick from a dropdown.

```yaml
apiVersion: a3t.io/v1
kind: PolicyPreset
metadata:
  name: standard
  namespace: a3t-system
spec:
  # Filesystem (static OpenShell section)
  filesystem:
    readOnly: [/usr, /lib, /etc, /opt/flowr/bin, /opt/a3t/skills]
    readWrite: [/sandbox, /tmp, /var/run/flowr]

  # Process (static OpenShell section)
  process:
    allowNetworkNs: false
    allowUserNs: false
    noNewPrivs: true
    seccompProfile: a3t-default

  # Network allowlist — what the preset permits app teams to request
  allowedEndpoints:
    - host: "*.a3t.internal"
      ports: [8010, 50051]
      tlsInspection: terminate
    - host: inference.local
      ports: [443]
      purpose: llm

  # Inference providers app teams may select
  allowedInferenceProviders: [nemotron-local, claude-sonnet-via-router]

  # Max authority the preset grants
  maxMemoryScopes: [private, shared]
  maxCallableFlowDepth: 3

  # Defaults the preset applies unless the Agent CR overrides (within bounds)
  defaults:
    auditLevel: all_decisions
```

Three canonical presets ship with A3T: `restricted` (read-only tools, local inference only), `standard` (tool write access within namespace, approved LLMs), `elevated` (cross-namespace tool access, frontier LLMs). Platform teams fork these for their org.

### 3.3 Agent — the app team's declaration

**Owner:** App team.
**Storage:** namespaced CRD, UI-editable.
**Purpose:** Say what the agent needs, not how the sandbox implements it.

```yaml
apiVersion: a3t.io/v1
kind: Agent
metadata:
  name: rtb-bid-agent
  namespace: rtb-team
spec:
  # What the agent does
  flow: rtb-decision-flow
  memory:
    db: clawdb-shared
    scopes: [private, shared]      # read; narrower writable below

  # Which preset it runs under
  preset: standard

  # Runtime mode
  runtime:
    mode: sandboxed

  # Tool surface — picked from MCP registry, must be in preset allowlist
  tools:
    - name: ctx-scorer-mcp
      access: read-only
    - name: brand-safety-mcp
      access: read-only

  # Inference — picked from preset.allowedInferenceProviders
  inference:
    preferred: nemotron-local
    fallback: [claude-sonnet-via-router]

  # Flow-call authority — agent may invoke these child flows
  callableFlows:
    - name: bid-scoring
      maxDepth: 2
    - name: outcome-recorder
      maxDepth: 1

  # Memory writes — narrower than reads typically
  memoryWrites:
    statements: [STORE, LINK, UPDATE]      # no FORGET (cluster-denied anyway)
    scopes: [private]

  # Audit
  audit:
    level: all_decisions                   # inherits from preset if omitted
```

**Nothing in the Agent CR mentions Landlock, seccomp, binaries, TLS termination, or OPA.** That's the controller's job to fill in from the preset and cluster policy.

---

## 4. CRDs, controller, and reconciliation

### 4.1 CRD catalog

| CRD | Scope | Owner | Purpose |
|---|---|---|---|
| `ClusterPolicy` | Cluster | Security | Hard ceiling |
| `PolicyPreset` | Namespace | Platform | Named bundles |
| `Agent` | Namespace | App | Agent declaration |
| `AgentPolicyRequest` | Namespace | App → Platform | Capability escalation requests |
| `MCPServer` | Namespace | Platform | Registered MCP endpoints (A3T already has this) |
| `LLM` | Namespace | Platform | Registered inference endpoints (A3T already has this) |
| `Flow` | Namespace | App | FlowR flow definitions (A3T already has this) |

### 4.2 Controller responsibilities

The A3T controller is a Rust binary using `kube-rs`. It runs as a Deployment, uses leader election, and watches `Agent`, `PolicyPreset`, `ClusterPolicy`, `AgentPolicyRequest`, `MCPServer`, `LLM`, `Flow`.

Its reconciliation for an `Agent` CR:

```
1. Resolve references
   - Load Agent.spec.preset → PolicyPreset
   - Load cluster-scoped ClusterPolicy (there's always exactly one named "default")
   - Load referenced MCPServer, LLM, Flow CRs
   - If any missing/invalid → status: Degraded, requeue

2. Compute effective policy (intersection)
   - Start with ClusterPolicy as ceiling
   - Intersect with PolicyPreset (reject if preset exceeds ceiling; that's a preset bug, alert Platform team)
   - Intersect with Agent.spec (reject Agent if it exceeds preset; status: Rejected, event emitted)
   - Produce EffectivePolicy struct

3. Static analysis
   - openshell-prover::verify(effective_policy):
     - No reachable state where a denied endpoint is accessible
     - Flow-call graph is acyclic through policy constraints
     - Memory write scopes ⊆ read scopes
   - If verification fails → status: Rejected with specific SMT counterexample

4. Render to OpenShell YAML
   - effective → openshell::PolicySpec (via openshell-policy crate)
   - Serialize to YAML
   - Compute content hash (for drift detection)

5. Materialize K8s resources
   - ConfigMap: <agent-name>-policy containing the YAML
   - Pod or Deployment (mode-dependent):
     * co-located: update the shared flowr-worker to include this agent's policy in its multi-tenant evaluator
     * sandboxed: create agent-sandbox-<agent>-<instanceId> Pod via openshell-driver-kubernetes
   - NetworkPolicy: deny-all + allowlist to flowr-worker only (sandboxed mode)
   - ServiceAccount with scoped RBAC
   - PodDisruptionBudget for sandbox stability

6. Report status
   - status.phase = Ready | Rejected | Degraded | Pending
   - status.policyHash (for GitOps drift detection)
   - status.sandboxPodName
   - status.lastAuditEvent (from Iggy)
   - Record Kubernetes Events for human operators
```

### 4.3 Admission webhook

A separate Rust binary, same repo. Validates CRs before the API server persists them:

- **Schema validation** (JSON Schema from OpenAPI spec)
- **Reference validation** (preset exists, MCPs exist)
- **Policy intersection soundness** (effective policy ≤ preset ≤ cluster)
- **Z3 verification of invariants** via `openshell-prover`
- **Co-located mode gating** (requires `ClusterPolicy.allowCoLocated: true`)

Admission webhooks must respond in <10 seconds. Z3 runs must be bounded; if proof exceeds 5 seconds, fall back to "soundly reject" with status telling operator to retry offline.

### 4.4 Controller code layout

```
a3t-controller/
├── Cargo.toml
├── crates/
│   ├── a3t-api-types/           # CRD types, generated + hand-written
│   ├── a3t-policy-resolver/     # CR → EffectivePolicy logic
│   ├── a3t-policy-render/       # EffectivePolicy → OpenShell YAML
│   ├── a3t-reconciler/          # kube-rs reconcile loops
│   ├── a3t-webhook/             # validating admission webhook
│   ├── a3t-sandbox-bootstrap/   # in-sandbox Rust binary (Mode B)
│   └── a3t-prover-adapter/      # openshell-prover wrapper
└── deploy/
    ├── crds/                    # CRD manifests
    ├── rbac/                    # ServiceAccount, ClusterRole, RoleBinding
    ├── webhook/                 # ValidatingWebhookConfiguration
    └── controller-deploy.yaml
```

---

## 5. Policy enforcement inside FlowR

FlowR today has task executors that do things: `CallHttpTaskExecutor`, `CallMcpTaskExecutor`, `CallSqlTaskExecutor`, `RunWorkflowTaskExecutor`, etc. Every one of them becomes policy-aware.

### 5.1 FlowContext extension

FlowR's `FlowContext` currently carries runtime handles (Dragonfly, Iggy, Qdrant, River). Add:

```rust
pub struct FlowContext {
    pub dragonfly: DragonflyPool,
    pub iggy: IggyClient,
    pub claw_db: ClawDbClient,
    pub qdrant: QdrantClient,
    pub river: RiverHandle,

    // NEW
    pub policy: PolicyContext,
}

pub struct PolicyContext {
    evaluator: Arc<openshell_policy::Evaluator>,
    router: Arc<openshell_router::InferenceRouter>,
    audit: Arc<AuditSink>,                 // OCSF events → Iggy

    flow_instance_id: String,
    agent_id: Option<String>,
    call_chain: Vec<FlowCallFrame>,        // for propagation through RunWorkflowTaskExecutor
    effective_policy: Arc<EffectivePolicy>,
}
```

**Construction:** FlowR's flow-start hook consults the Agent CR (for agent flows) or the Flow CR (for non-agent flows) to build the initial `PolicyContext`. Non-agent flows get a "permissive-within-namespace" default context; agent flows get the full evaluator loaded with their agent's policy.

**Cost:** `PolicyContext` is cloneable cheaply (Arc everywhere). Adding it to every step invocation is free on the hot path — no allocation, no syscall. Only actually *evaluating* policy costs anything, and that happens at task executor boundaries, not step transitions.

### 5.2 Policy check at task executors

Every `CallXxxTaskExecutor` gains a pre-execution check:

```rust
#[async_trait]
impl Step for CallHttpTaskExecutor {
    type Input = HttpCallInput;
    type Output = HttpCallOutput;

    async fn execute(
        &self,
        input: Self::Input,
        ctx: &FlowContext,
    ) -> Result<Self::Output, StepError> {
        // 1. Policy check
        let request = PolicyRequest::Http {
            method: input.method.clone(),
            url: input.url.clone(),
            headers: input.headers.redacted(),
            caller_binary: env::current_exe()?,
        };

        ctx.policy.check(&request).await
            .map_err(|decision| {
                ctx.policy.audit.emit_denial(&request, &decision);
                StepError::PolicyDenied(decision)
            })?;

        ctx.policy.audit.emit_allow(&request);

        // 2. Actual work (unchanged)
        self.do_http_call(input).await
    }
}
```

`ctx.policy.check` calls into `openshell-policy`. In Mode A (co-located), that's the full enforcement. In Mode B (sandboxed), the agent-runner Pod makes a gRPC call to flowr-worker's policy endpoint and flowr-worker calls `openshell-policy` there — same code path, one hop earlier.

### 5.3 The critical case: RunWorkflowTaskExecutor

This is the flow-call escape hatch you correctly flagged earlier. An agent flow calling a child flow must not escape its policy.

```rust
impl Step for RunWorkflowTaskExecutor {
    type Input = RunWorkflowInput;
    type Output = RunWorkflowOutput;

    async fn execute(
        &self,
        input: Self::Input,
        ctx: &FlowContext,
    ) -> Result<Self::Output, StepError> {
        // 1. Is the agent even allowed to call this flow?
        let flow_call_req = PolicyRequest::FlowCall {
            target_flow: input.flow_name.clone(),
            depth: ctx.policy.call_chain.len() + 1,
        };
        ctx.policy.check(&flow_call_req).await?;

        // 2. Load the child flow's declared policy (from the Flow CR)
        let child_flow = ctx.flow_registry.get(&input.flow_name).await?;
        let child_declared_policy = child_flow.policy_spec.clone();

        // 3. INTERSECT — never widen
        let child_effective = ctx.policy.effective_policy
            .intersect_with(&child_declared_policy)?;

        // 4. Build child context with tightened policy + updated call chain
        let mut child_ctx = ctx.clone();
        child_ctx.policy = ctx.policy.child(
            input.flow_name.clone(),
            Arc::new(child_effective),
        );

        // 5. Execute
        self.executor.run(input, &child_ctx).await
    }
}
```

The intersection operator lives in `a3t-policy-resolver` and is **the most security-critical piece of A3T code**. It must:

- Never produce a result broader than either input.
- Be associative and commutative (so deep call chains are deterministic).
- Be property-tested exhaustively.
- Have a companion proof in `openshell-prover` that checks specific agent policies have the intersection property statically.

### 5.4 Inference routing

`CallLlmTaskExecutor` doesn't call the LLM directly. It asks `openshell-router`:

```rust
let routed = ctx.policy.router.route(LlmCall {
    provider_requested: input.provider,
    messages: input.messages,
    tools: input.tools,
    agent_context: ctx.policy.agent_id.clone(),
}).await?;

// router returns: actual endpoint to hit, headers to use (credentials injected),
// any redaction applied, final provider chosen (may differ from requested per policy)
```

The router handles: credential injection (credentials live in `openshell-providers` bundles, not in the agent), endpoint rewriting (e.g., route `claude-sonnet` requests through the org's proxy for DLP), header filtering (strip agent-controlled headers that could be injection vectors), fallback routing (if preferred provider unavailable).

### 5.5 Audit

Every policy decision — allow, deny, route — emits an OCSF event:

```rust
ctx.policy.audit.emit(OcsfEvent {
    activity_id: ActivityId::Access,
    category_uid: CategoryUid::AppActivity,
    class_uid: ClassUid::PolicyDecision,
    severity_id: severity_from_decision(&decision),
    time: now_millis(),
    actor: OcsfActor {
        process: process_info(),
        user: ctx.policy.agent_id.clone().map(to_ocsf_user),
    },
    metadata: OcsfMetadata {
        product: "a3t".into(),
        version: VERSION.into(),
    },
    // ... request details, decision, matched rule
});
```

The `AuditSink` buffers events and flushes to Iggy's `flowr.audit` topic. In Mode B, the agent-runner's audit events flow through flowr-worker's audit sink (same topic, just an extra hop). Never drop audit events — if Iggy is unavailable, spill to a local durable queue and retry.

---

## 6. Implementation sequencing

Build in this order. Each step is independently shippable; each ends with a working demo.

### 6.1 Phase 1 — Policy plumbing (2-3 weeks)

**Goal:** FlowR task executors gain policy checks. No agent sandbox yet. Non-agent flows can be denied based on policy.

1. Add `openshell-policy` as a FlowR workspace dependency (pinned SHA).
2. Add `PolicyContext` to `FlowContext`. Default value permits everything (so existing flows don't break).
3. Add policy check to `CallHttpTaskExecutor`, `CallMcpTaskExecutor`, `CallSqlTaskExecutor`, `CallRedisTaskExecutor`, `CallQdrantTaskExecutor`.
4. Add policy check to `RunWorkflowTaskExecutor` with intersection logic.
5. Wire `AuditSink` to Iggy.
6. Integration test: a non-agent flow with a policy that denies HTTP to `evil.example.com` — observe the denial, verify the audit event.

Ship: FlowR has an in-process policy framework. No CRDs yet, policy passed via flow definition for now.

### 6.2 Phase 2 — CRDs and controller skeleton (2 weeks)

**Goal:** Operators can declare Agent CRs; controller renders OpenShell YAML and writes ConfigMaps. Not yet actually wiring sandboxes.

1. Define CRDs: `Agent`, `PolicyPreset`, `ClusterPolicy`, `AgentPolicyRequest`.
2. Build `a3t-controller` with `kube-rs`, watching all four CRDs.
3. Implement `a3t-policy-resolver`: CR → EffectivePolicy via intersection.
4. Implement `a3t-policy-render`: EffectivePolicy → OpenShell YAML (via `openshell-policy::PolicySpec`).
5. Implement admission webhook with basic intersection checks (no Z3 yet).
6. Integration test: create an Agent CR, observe ConfigMap appearance with correct YAML, observe rejection if Agent exceeds preset.

Ship: Declarative policy management works end-to-end on paper. FlowR still doesn't consume the ConfigMaps; humans can read them and verify correctness.

### 6.3 Phase 3 — Mode A (co-located) wiring (2 weeks)

**Goal:** FlowR workers load policy from ConfigMaps emitted by the controller. Non-sandboxed agents run with policy enforcement in-process.

1. flowr-worker gains a `policy-watcher` that watches its namespace's ConfigMaps labeled `a3t.io/policy=true`.
2. On CM update, rebuild the `PolicyContext` for affected agent flows. Hot-reload for network rules; refuse reload for static rules (require Pod restart — controller handles this via a rolling update).
3. flowr-worker applies Landlock + seccomp at startup based on the union of co-located agents' static policies. Fail-closed on Landlock errors (`mode: hard_requirement`).
4. Integration test: deploy an RTB agent in co-located mode, verify it can hit ctx-scorer-mcp but not evil.example.com, with the denial appearing in `flowr.audit`.

Ship: Co-located mode works. This covers the bulk of the latency-critical use cases.

### 6.4 Phase 4 — Mode B (sandboxed) wiring (3-4 weeks)

**Goal:** Agent flows run in their own Pods with kernel-level isolation; outbound calls proxy through flowr-worker.

1. Build `a3t-sandbox-bootstrap` binary. At startup: parse policy CM, apply Landlock + seccomp via `openshell-sandbox`, exec the `agent-runner` subcommand of flowr-worker.
2. Controller logic for sandboxed mode: on Agent CR with `mode: sandboxed`, create a sandbox Pod per flow instance (or pooled, see §7.3).
3. gRPC service between agent-runner and flowr-worker. Define in `proto/a3t-sandbox.proto`. Covers: HTTP call proxy, MCP call proxy, LLM inference proxy, ClawDB proxy, audit event submission.
4. NetworkPolicy: sandbox Pod can only talk to its flowr-worker via a Unix socket mounted from a shared emptyDir. **Prefer Unix socket over Pod IP** — no network namespace crossing means the NetworkPolicy is enforced by absence, not allowlist. If emptyDir sharing isn't sufficient (different nodes), use a socket-proxy sidecar co-located with the sandbox.
5. Integration test: deploy the same RTB agent in sandboxed mode, run fault injection (agent tries to open a raw socket — blocked by seccomp; agent tries to read /etc/shadow — blocked by Landlock; agent tries to call evil.example.com — blocked by proxy).

Ship: Both modes work. A3T's full value proposition is deliverable.

### 6.5 Phase 5 — UI and escalation (3 weeks)

**Goal:** App teams can manage agents through A3T console; escalations flow through AgentPolicyRequest.

1. UI backend: REST/gRPC service that reads CRs via the K8s API (not direct — must go through K8s for audit), exposes read/write endpoints.
2. UI frontend: extend the existing A3T resources catalog (`a3t_resources.html`) with an Agents view, Agent editor (CR-shape form, not raw YAML), policy-request creator.
3. AgentPolicyRequest workflow: UI creates the CR, platform team approves via `kubectl` or GitHub PR against a GitOps-synced copy, approval triggers Agent CR patch.
4. Drift detection: UI shows policy hash vs. current status.policyHash. Mismatch → "pending reconcile" badge.

Ship: Three-surface policy management as promised.

### 6.6 Phase 6 — Hardening (ongoing)

- Add Z3 verification to admission webhook (`openshell-prover`).
- Add `openshell-driver-vm` microVM option for ultra-high-risk agents.
- Audit-event querying UI (OCSF-aware filters).
- Formal SMT proofs of the intersection operator.
- Multi-cluster policy federation.

---

## 7. Critical rules and gotchas

### 7.1 Policy must never widen

The intersection operator is the single most security-critical piece of code in A3T. Every commit touching `a3t-policy-resolver` gets security-review-required. Property-test generators: random ClusterPolicy × PolicyPreset × Agent triples, assert `effective ⊆ preset ⊆ cluster`. Fuzz for weeks. Re-run on every upgrade of `openshell-policy`.

### 7.2 Audit events are not optional

Every policy decision emits an event. If the audit sink fails, the decision **fails closed**. Losing audit events is worse than being unavailable; never trade correctness for throughput on this path. The spill-to-local-disk fallback is mandatory, not aspirational.

### 7.3 Sandbox Pod lifecycle

Per-flow-instance Pods are correct for long-running agent flows (publisher onboarding — days). For short-lived agent invocations (every bid decision — milliseconds), a pooled sandbox model is necessary or Pod creation latency dominates. The pooling strategy:

- Pool per (Agent CR × preset × policy hash) tuple.
- Warm pool size configurable in the Agent CR (`spec.runtime.warmPool: 3`).
- Scale on demand up to `maxPool`. Idle pods torn down after TTL.
- Policy hash change → drain and replace pool. Never reuse a Pod with stale policy.

### 7.4 Static vs. dynamic policy fields

OpenShell distinguishes these:

- **Static** (filesystem_policy, process, landlock): locked at sandbox creation. Changing requires Pod recreation. Controller handles via rolling update.
- **Dynamic** (network_policies, inference routing): hot-reloadable on running sandboxes. Controller posts ConfigMap updates; `openshell-policy`'s hot-reload picks them up.

A3T admission webhook tags each policy-relevant Agent CR field with static/dynamic. UI shows a "requires restart" warning on static fields. Controller schedules rolling updates conservatively.

### 7.5 Mode-A Landlock constraints

Landlock applies to the whole process, not per-agent. In co-located mode, flowr-worker's Landlock policy is the **union** of all its hosted agents' requirements (least restrictive wins because you can't un-grant after startup). This means:

- Co-located mode is for agents with similar trust profiles.
- An elevated-preset agent and a restricted-preset agent should never share a worker.
- Controller groups agents by preset into separate Deployments. Within a Deployment, all co-located agents share a Landlock/seccomp profile.

### 7.6 OpenShell is alpha

As of April 2026, OpenShell is alpha, API-unstable. Assume breaking changes. Mitigations:

- Pin by SHA, never by branch or version tag.
- Wrap every OpenShell import behind an A3T-internal adapter trait (`a3t-prover-adapter`, `a3t-policy-adapter`). When OpenShell's API changes, only the adapter changes.
- Maintain a fork. Non-trivial integration is non-negotiably going to require upstream patches at some point.
- Track NVIDIA OpenShell releases; dedicate one engineer to upstream-watch.

### 7.7 Testing

- Unit: every executor's policy check, every intersection case, every CR validation rule.
- Property: intersection soundness, policy-layer monotonicity, audit completeness.
- Integration: full agent deploys in kind cluster with all CRDs, both modes, happy + denied paths.
- Adversarial: red-team tests where a "malicious" flow tries to exfiltrate data through every known side channel — DNS timing, HTTP headers, nested flow calls, etc. These are the tests that sell the CISO pitch.
- Chaos: kill flowr-worker during a sandboxed agent's tool call — verify agent Pod fails closed rather than proceeding unchecked.

### 7.8 Observability

Prometheus metrics exported by flowr-worker and a3t-controller:

```
a3t_policy_decisions_total{result="allow|deny|route", category="http|mcp|flow|inference|memory"}
a3t_policy_decision_duration_seconds{...}
a3t_sandbox_pods{phase="pending|running|terminating"}
a3t_intersection_latency_seconds
a3t_audit_events_dropped_total            # MUST be zero; alert on any non-zero
a3t_policy_hash_drift{agent="..."}
a3t_prover_verifications_total{result="proved|refuted|timeout"}
```

### 7.9 What NOT to do

- **Don't** embed OPA binary and shell out to it. Use `openshell-policy`'s embedded evaluator.
- **Don't** write policies in raw Rego at the Agent CR layer. Only OpenShell YAML. The controller compiles.
- **Don't** bypass the admission webhook in controller reconciliation. Webhook is the only validator; the controller trusts what the API server accepts.
- **Don't** cache policy decisions. They're cheap and caching creates stale-decision bugs. The only caching is inside `openshell-policy` itself.
- **Don't** let the UI write anywhere but the K8s API. No side-channel state stores.
- **Don't** skip the audit event because "this call is clearly fine." Every decision, always.

---

## 8. What "done" looks like

A successful A3T + OpenShell integration ships when:

1. An app team can declare an `Agent` CR referencing a `PolicyPreset`, click through a UI, and watch their agent come online with policy enforced.
2. A platform team can roll out a ClusterPolicy change via GitOps and see every non-conforming agent reported in a status dashboard within one reconcile cycle.
3. A security team can pull an OCSF audit log from Iggy for any flow instance and trace every policy decision that instance made, in order.
4. A red-team exercise against a deployed agent fails to exfiltrate data, fails to execute out-of-policy tool calls, and fails to escape the sandbox — with all attempts logged.
5. A CISO demo answers "how do you prevent X" with a specific technical mechanism for every X, grounded in concrete OpenShell crate code.

The pitch, validated:

> A3T runs autonomous agents as CNCF Serverless Workflows on the FlowR Rust runtime. Every flow executes under layered policy — cluster-wide security invariants, platform-curated presets, per-agent declarations — rendered into NVIDIA OpenShell policy YAML and enforced at four layers: Landlock for filesystem, seccomp-BPF for syscalls, an HTTP CONNECT proxy with OPA Rego evaluation for network, and an inference router for LLM routing. Policies compose by intersection only, never widening, verified statically via Z3. Every decision emits an OCSF audit event to Apache Iggy. Three surfaces — SRE YAML, Platform presets, App-team UI — over one policy source of truth in Kubernetes CRs.

That's the product.

---

## Appendix A — File locations

- This guide: `docs/A3T_OPENSHELL_INTEGRATION.md`
- CRD schemas: `deploy/crds/*.yaml`
- OpenAPI spec (for UI codegen): `api/openapi.yaml`
- Controller code: `a3t-controller/crates/*`
- FlowR integration: upstream `flowr/crates/flowr-core/src/policy.rs` (new file)
- Sandbox bootstrap: `a3t-controller/crates/a3t-sandbox-bootstrap/`
- Integration tests: `tests/integration/` (kind-based)
- Adversarial tests: `tests/adversarial/`

## Appendix B — Required reading before contributing

- `github.com/NVIDIA/OpenShell/README.md`
- `github.com/NVIDIA/OpenShell/docs/architecture/security-policy.md`
- `docs.nvidia.com/openshell/latest/reference/policy-schema.html`
- CNCF Serverless Workflow Specification
- OCSF 1.3 schema reference
- `openshell-prover` examples directory

## Appendix C — Open questions (track in issues)

- Do we fork OpenShell or depend upstream? Decision pending alpha→beta transition.
- Pool warm-start model — per-agent vs. per-preset — benchmark needed.
- Cross-cluster policy federation — required for enterprise multi-region deployments, design TBD.
- Policy dry-run mode — enable "would-deny" logging without enforcement during migration — needed for existing ZeroClaw agents transitioning to OpenShell policy.
