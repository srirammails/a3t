# A3T cluster architecture — one edge, pools of flows, ADB as the cluster

3 Nov 2026 · design, not yet built · decisions marked **(default)** stand until changed

## 1. Summary

A3T runs as a cluster in which **ADB is the only component that holds state**. ADB is
replicated with Raft. Everything else is a FlowR process that can die at any moment and
be replaced:

- **Edge** — the one role that holds connections: HTTP API, MCP, chat and run streams.
  It authenticates the caller and hands the work to a pool. It keeps nothing but open
  sockets.
- **Pools** — groups of FlowR processes that take tasks from their queue in ADB by lease:
  API handlers, platform flows, the gateway, agents, workers. All API work is flows, so
  the API is a pool like any other.
- **Gateway pool** — the pool through which every agent call to a tool, an LLM or memory
  goes out. Identity, delegation and policy are checked there, and only that pool can read
  downstream credentials. What the gateway does is specified in
  [A3T Agent Identity & Policy Gateway — Spec](A3T%20Agent%20Identity%20%26%20Policy%20Gateway%20—%20Spec.md);
  this document covers where it runs and how it is reached.

Each org is assigned a gateway: the A3T gateway (default) or an external one. The default
IdP is Keycloak.

```
            clients (dash, MCP clients, API callers, SecOps)
                               │
                        load balancer
                 ┌─────────────┴─────────────┐
               edge                         edge          ≥ 2 instances, one role:
                 │  sockets, authn, hand-off │            holds connections only
                 └─────────────┬─────────────┘
                               │ Arrow Flight: enqueue, claim, journal, results,
                               │ leases, change feed
                  ┌────────────┼────────────┐
                 ADB          ADB          ADB             Raft group: the only state
                  └────────────┬────────────┘
     ┌──────────────┬──────────┴──────┬────────────────┬──────────────┐
  api pool     platform pool     gateway pool      agent pools     worker pools
  (API flows)  (a3t/system ns)   (identity,        (cv-*, rt-*)    (rt-worker …)
                                  policy, vault,
                                  egress)
                                      │
                         tools · MCP servers · LLMs · external gateway
```

**Rule for every change:** if this pod dies now, can another one carry on from ADB alone?

## 2. Where we are (3 Nov 2026, vc-dev)

| Part | Today | Target |
|---|---|---|
| Console (`deploy/flowr`) | 1 replica; API + MCP + router + platform pool + stream hub + gateway port 3003 for runtimes; hostPath volumes `/data/a3t-dev/clawdflow` and `/data/a3t-dev/a3t` | Split into **edge** (sockets) + **api** and **platform** pools; no volumes |
| Runs, journals, queue, leases | ADB engine store (stage H, ee28+), claims fenced by epoch | Same, on a Raft group |
| Registry, agent state, platform tables | ADB (shared registry, `FLOWR_REGISTRY_ADB`) | Same |
| CE-only local state | Settings, triggers, old-style checkpoints in LanceDB on the console's disk | ADB |
| Runtimes (`rt-worker`, `cv-*`) | Claim from ADB directly; chat/agent output forwarded to the console over port 3003 | Output published through ADB; any edge serves the viewer |
| Waking a worker | Store notification, else polling 250 ms → 2 s | Push from the ADB change feed |
| Credentials | One `FLOWR_ENGINE_ADB_TOKEN` shared by every pod; resource secrets (`api_key_refs`) readable by any FlowR process | A credential per pool; downstream secrets readable only by the gateway pool |
| Policy | Cedar hook (`flowr-aesop`) inside every FlowR process; approval answers in the state store | Decided in the gateway pool; runtimes may keep a local pre-check |
| ADB | One node (`deploy/adb`) | Raft group of 3 (M3) |

## 3. Roles

### 3.1 Edge

- Terminates TLS (or sits behind the ingress that does), verifies the caller's token
  (user, agent workload, SecOps), and attaches the principal to the task.
- For a request: picks the target pool from the router rules (namespace, org/project
  runtime policy, flow placement), enqueues a task, and waits for its result on the ADB
  change feed. If the edge itself belongs to the target pool, it claims the task itself
  (§4.3).
- Holds long-lived connections: SSE/WebSocket streams for chat and run views, MCP
  Streamable HTTP sessions. A stream is fed from the ADB change feed, so it does not
  matter which pod produces the output.
- Holds no state beyond sockets. If an edge dies, clients reconnect to another and resume
  from a cursor (run id + last event offset).
- Same FlowR binary, role chosen by configuration **(default)**:
  `FLOWR_ROLES=edge,api,platform` on one pod is today's console;
  `FLOWR_ROLES=edge` alone is a pure edge.

### 3.2 Pools

A pool is a name plus the set of processes that claim its tasks. A process may serve
several pools. Pools in the target layout:

| Pool | Runs | Notes |
|---|---|---|
| `api` | Flows behind API and MCP endpoints | Mostly transient flows (§4.2) |
| `platform` | `a3t-*` / `system-*` namespaces (projects, releases, share, install) | Today's "console pool" |
| `gateway` | Gateway flows: verify, resolve, decide, credential, act, record | Only pool allowed to read the vault (§5.3) |
| agent pools | Chat versions (`cv-*`), agent deployments | One per runtime, as today |
| worker pools | General runs (`rt-worker`, isolated runtimes per org/project) | Scale on queue depth |

### 3.3 ADB

The Raft group holds: task queues, run state and journals, leases, registry and platform
tables, agent memory, the vault, the decision log, and the change feed every role
subscribes to. FlowR reaches it over Arrow Flight, follows the leader on a leader change,
and retries with backoff while its lease is still valid.

## 4. How work flows

### 4.1 An API call

1. The client calls the edge with a user token.
2. The edge verifies the token and builds the principal (user, org, roles).
3. The edge enqueues a task for the `api` pool (or runs it inline, §4.3) and subscribes to
   its result.
4. An `api` process claims it, runs the API flow (Cedar checks as today), writes the result.
5. The edge returns the result.

A read (list flows, get a run) is a transient flow (§4.2): no journal, so no extra Raft
commits beyond the queue entry and the result.

### 4.2 Durable and transient flows

| Mode | Journal | On a crash | Used for |
|---|---|---|---|
| **durable** (today) | Every step | Resumes from the journal on another pod | Runs with side effects, agent turns, deployments |
| **transient** | None | The task's lease lapses; the caller retries or gets an error | Reads, policy decisions, tool listing, health |

A flow is transient when declared so, or when the engine can tell every step is Pure or
Read. **(default)**: transient tasks still go through the ADB queue; if measured latency is
too high, add a leader-local, non-replicated queue for them (losing it on a leader change
is acceptable because callers retry).

### 4.3 Placement shortcut

If the process that receives a request (or reaches a step) is a member of the target pool
and has capacity, it claims the task itself. The task is still written to ADB and
claimed under a lease, so a crash is handled the same way; only the network hop to another
pod is skipped.

### 4.4 An agent tool call

1. An agent step on a runtime pod needs a tool. It holds a short-lived workload token and
   the run's delegation token (gateway spec, *Identity model*); it holds no downstream
   credential.
2. The runtime enqueues a `gateway` task carrying the call, both tokens, and the run id.
3. A gateway process runs the gateway flow: Verify → Resolve → Decide → Act → Record
   (gateway spec, *Request flow*). Approval pauses use the existing durable approvals.
4. The result goes back to the agent step; the decision event goes to the decision log in
   ADB and to OpenTelemetry.

The runtime's own journal still records the tool call as an Effect step, so a crashed
agent turn does not repeat it (L39 pattern). The gateway flow itself is transient for
reads and durable for writes, so a gateway crash after the downstream call is detected,
not repeated.

**LLM calls** follow the same path (model allow-lists, cost metering; closes L5's metering
gap). **Memory (AQL)** calls: the gateway decides and returns a short-lived ADB token
scoped to the run's namespace and scope; the data goes straight to ADB **(default)**, so
heavy memory traffic does not pass through the gateway.

### 4.5 Singletons

Some jobs must run once in the cluster: trigger and cron firing, timer and wait wake-ups,
expired-lease sweeps, retention and journal compaction, the policy and schema publisher.
Each takes a named lease in ADB (`singleton/<job>`) with an epoch; a holder that loses the
lease is fenced by the epoch on its next write. Any FlowR process with the job enabled may
hold it.

### 4.6 Streams and caches

- **Streams.** Producers write chat tokens, agent events and run events into ADB (the
  episodic stream for the run); the edge holding a viewer's socket reads them from the
  change feed. Replaces the port-3003 stream forwarding.
- **Caches.** Each process caches policies, schema, registry entries, approval answers and
  IdP signing keys, and drops an entry when the change feed reports a write to it. No
  cache is a source of truth.

## 5. Gateway per org

### 5.1 Assignment

Each org has a gateway setting in `orgs.settings`:

```json
{ "gateway": { "kind": "a3t" } }
{ "gateway": { "kind": "external", "url": "https://gw.acme.example", "topology": "a3t_in_front" } }
```

`kind: a3t` routes the org's agent calls to the `gateway` pool. `kind: external` routes
them to the external gateway in one of the topologies of the gateway spec
(*External gateways*): A3T behind a platform gateway (A3T is the PDP over AuthZEN), or A3T
in front of one (A3T decides and attaches identity; the external gateway routes and holds
tokens).

### 5.2 What always stays with A3T

Whatever the gateway, A3T keeps enforcing its own platform rules: org and project
isolation (NamespaceGuard), runtime policy, exposure (shared, training, community), and
agent envelopes. An external gateway adds rules; it cannot widen what A3T allows
**(default)**.

### 5.3 Credentials by pool

- Each pool has its own ADB credential. ADB checks it against the pool's grants:
  which queues it may claim, which tables it may read.
- The vault (gateway spec, *Credential custody*) is a set of ADB tables encrypted per
  tenant. Only the `gateway` pool's credential may read them.
- Runtimes receive no downstream secrets. A compromised runtime exposes its short-lived
  tokens and nothing else.

## 6. Decisions

Decisions 1–4 answer the questions raised on 3 Nov. The gateway spec already settles most
of them; this document follows it.

| # | Question | Decision |
|---|---|---|
| D1 | Agent identity: per definition or per deployment? | **(default)** Per definition version, as in the gateway spec: `agent://<tenant>/<agent-name>@<version>`, with owner and class. The deployment (org, project, runtime) is added to the Cedar context, so policy and audit can still tell two deployments of one agent apart. |
| D2 | Do A3T's policies apply behind an external gateway? | **(default)** Yes: platform isolation and agent envelopes always apply (§5.2); the external gateway adds rules on top. |
| D3 | Runs that outlive the user's token | **(default)** As in the gateway spec: the delegation has its own expiry; `per_step` runs pause at the next checkpoint when it expires and ask the user to re-delegate; `at_initiation` runs keep a time-bounded grant with a named approver. User tokens for downstream apps are vaulted and refreshed by the gateway (token mode 2). |
| D4 | Which IdP | **(default)** Keycloak in CE, behind the `IdentityProvider` trait; Entra, Okta, Auth0, Google and generic OIDC adapters in EE (gateway spec, *CE vs EE*). |
| D5 | Edge: separate binary or a FlowR role? | **(default)** A FlowR role chosen by `FLOWR_ROLES`. |
| D6 | Transient tasks in ADB or a leader-local queue? | **(default)** In ADB first; measure; add the local queue only if needed (§4.2). |
| D7 | Memory calls through the gateway? | **(default)** The decision goes through the gateway; the data path goes straight to ADB with a scoped token (§4.4). |

## 7. Gaps

Numbered **AC-n** (A3T cluster). Related limitation numbers in brackets.

| # | Gap | Fix | Stage |
|---|---|---|---|
| AC-1 | The console has local disk: settings, triggers, old-style checkpoints in LanceDB | Move them to ADB; drop the volumes from `deploy/flowr` | 1 |
| AC-2 | Singleton jobs assume one console | Named leases with epochs (§4.5) | 1 |
| AC-3 | Live output reaches viewers only through the console (port 3003 forwarding) | Producers write to ADB; the edge reads the change feed (§4.6) | 1 |
| AC-4 | In-process caches have no invalidation across pods | Drop on change feed (§4.6) | 1 |
| AC-5 | Workers poll (250 ms → 2 s) when remote | Push wake-ups from the ADB change feed over Flight | 1 |
| AC-6 | No edge role; console does everything | `FLOWR_ROLES`; edge enqueues and waits on results (§3.1) | 1 |
| AC-7 | No transient flow mode | Mark transient flows; skip the journal (§4.2) | 1 |
| AC-8 | One ADB token for every pod [L30] | A credential per pool, checked by ADB (§5.3) | 2 |
| AC-9 | Downstream secrets readable by any FlowR process | Vault in ADB, readable by the gateway pool only (§5.3) | 2 |
| AC-10 | Policy decided inside each runtime | Gateway pool flows; runtimes pre-check only (§4.4) | 2 |
| AC-11 | No agent workload or delegation tokens | Keycloak client per agent, token exchange (gateway spec, phase 1) | 2 |
| AC-12 | No per-org gateway setting | `orgs.settings.gateway`, router rule (§5.1) | 2 / 4 |
| AC-13 | ADB is a single node [ADB CL-1, CL-4, L11] | Raft group of 3; leader-following Flight clients; rebuild in-memory state on leader change | 3 |
| AC-14 | Pools do not scale | Controller scales each pool on its queue depth in ADB | 4 |
| AC-15 | Runs on runtimes stop if the console is down [L35] | Gone once AC-3 and AC-6 land; rewrite L35 for the ADB dependency | 1 |

## 8. Stages

1. **Stateless FlowR on one ADB** — AC-1 to AC-7, AC-15. No Raft needed.
   *Test on vc-dev:* the console as 2 replicas. Kill one mid-run: the run finishes, an open
   chat stream continues after reconnecting, each trigger fires exactly once, and a policy
   change made through one replica applies on the other.
2. **Gateway pool, gateway spec phase 1** — AC-8 to AC-12 (A3T gateway only).
   *Test:* an agent on `rt-worker` calls a tool with no credential in its pod; the call is
   allowed or denied by Cedar in the gateway pool, and the decision log shows agent and
   on-behalf-of user.
3. **ADB Raft (M3)** — AC-13. Starts after the user's end-to-end A3T tests.
   *Test:* kill the ADB leader during runs; runs pause for the election and continue.
4. **Scaling and federation** — AC-14, external gateways (gateway spec phases 2–4).

## 9. Open questions

- One Raft group puts every write through one leader. When do we split by space (M5)?
  Measure under stage 3 first.
- Should the edge also be the MCP gateway endpoint for agents outside A3T, or is that a
  separate edge deployment with its own ingress?
- Decision-log retention in ADB versus only in the customer's SIEM (also open in the
  gateway spec).
- The controller is a Kubernetes operator with its own leader election. Keep it, or move
  its reconcile loop to an ADB singleton?

## 10. Build: basic gateway + router, policy placeholder, Raft (3 Nov 2026)

User, 3 Nov: "basic routing gateway for now; policy layer later; focus on Gateway+Router
with policy placeholder and Raft." Identity (AC-11), the vault (AC-9) and Cedar in the
gateway (AC-10 decide) come later; this step gives every outbound call one path through
a `gateway` pool, with a decision point that allows everything and records it.

**G1 Router.** One `router` module replaces `engine_worker::route`. Rules, first match:
platform namespaces (`a3t*`, `system*`) → `default` (the platform pool, CA-32); the
gateway flow → `gateway`; canvas sandbox → deployment runtime → `FLOWR_DEFAULT_POOL` →
`default` (as today). Every decision is a `Route {pool, rule}` so it can be logged and
tested.

**G2 Gateway client (runtimes).** With `FLOWR_GATEWAY=pool`, the outbound handlers
(`call:http`, `call:openapi`, `call:grpc`, `call:a2a`, `call:mcp`, `call:llm`) are wrapped:
the step starts a run of `system-gateway/call` in pool `gateway` with the resolved task,
the step input and the caller (run, step, namespace, flow, principal), dedupe key =
the step's idempotency key (a retried step reuses the same gateway run, nothing is
called twice), and waits on `run:<id>` for its outcome. Agent tool flows are made of these
steps, so agent tool calls go through too. Not yet: the agent's own LLM call inside
`call:a3t:agent` (it streams tokens) — G6.

**G3 Gateway (pool `gateway`).** `system-gateway/call` is one step, `call:a3t:gateway`:
Verify (caller present; placeholder) → Resolve (the org's `settings.gateway`: `a3t`, or
`external` → not yet, refused with a clear error) → Decide (`GatewayPolicy` trait; the
placeholder allows and says so) → Act (the real handler, in this process — the only
place with downstream credentials) → Record (a `gateway_decisions` row: time, caller,
target, decision, reason, outcome, duration). The flow is seeded into the registry by
gateway processes.

**G4 vc-dev.** Deployment `gateway` (FlowR EE, `FLOWR_WORKER_POOLS=gateway`, 2 replicas,
its own pool token); runtimes get `FLOWR_GATEWAY=pool`.

**G5 Tests.** Local: a runtime's `call:http` reaches the target from the gateway process,
once per idempotency key, through a gateway kill and an ADB leader kill; a deny rule in
the placeholder (`FLOWR_GATEWAY_DENY`, for tests) fails the step with the reason; the
decision rows exist. Then the same on vc-dev.

**Status (3 Nov 2026):** G1–G5 done. vc-dev (21:30 UTC): gateway Deployment 2 replicas
on ee33 with pool token `adb-pool-gateway`; console and `rt-worker` on ee33, runtimes with
`FLOWR_GATEWAY=pool`; ADB 3 nodes on adb `l`. A runtime run's `call:http` was made by a
gateway pod as the run's user (22 ms) and its response returned; decision rows written;
a `rollout restart` of the gateway during a 10 s call drained it (the old pod finished
the call; run 10.7 s). Crash takeover mid-call is covered locally (SIGKILL).
Resolve does not read `orgs.settings.gateway` yet (CA-45).

**G6** the agent's LLM call through the gateway (streamed tokens keep going to
`stream:<session>`).

**Raft, alongside:** R1 transaction-id dedupe across a failover (CA-27); R2 faster
failover — keep the engine warm on followers instead of reopening it (CA-34); R3 group
commit (CA-24): every gateway call adds Raft writes, so throughput starts to matter.
