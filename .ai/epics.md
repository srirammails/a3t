# EPICs — ClawdFlow + FlowR + A3T

## Three Products

| Product | Role | Scope |
|---------|------|-------|
| **ClawdFlow** | Pure CNCF SWF runtime | Workflows, tasks, resources, state, triggers. No K8s, no agents. |
| **FlowR** | Runtime + K8s CRDs | ClawdFlow + deployment, scaling, routing, sinks. |
| **A3T** | Agent platform | Agent templates (CNCF workflows), memory (ClawDB), UI. |

---

## Completed EPICs (FlowR POC)

| EPIC | Description | Status |
|------|-------------|--------|
| 0 | Infrastructure validation (Iggy, Dragonfly) | DONE |
| 1 | Expression Engine (jq) | DONE |
| 2 | Executor + Durability + Recovery | DONE |
| 3 | All 22 Task Types | DONE |
| 4 | FlowRegistry + Admin API | DONE |
| 5 | SuspendManager (Kameo actors) | DONE |
| 6 | Lifecycle Events (CloudEvents) | DONE |

## Completed EPICs (ClawdFlow)

| EPIC | Description | Status |
|------|-------------|--------|
| 10 | ClawdFlow single-process engine (LanceDB, MCP) | DONE |
| 11 | ZeroClaw agent integration (embedded library) | DONE |
| 12 | 4 agent modes (ephemeral/session/persistent/reactive) | DONE |
| 13 | 5 agent task types (call/send/ask/spawn/stop) | DONE |
| 14 | MCP tool adapter (discover + call) | DONE |
| 15 | ClawDB memory adapter (AQL over HTTP) | DONE |
| 16 | Trigger system (first-class, webhook/cron/event) | DONE |
| 17 | Trigger HTTP API (CRUD, attach/detach, pause/enable) | DONE |
| 18 | clawdflow-agents crate extraction | DONE |
| 19 | Docker build + remote deployment (37.27.195.4) | DONE |

---

## CE EPICs — In Progress / Next

### EPIC 20: Complete Agent Resource Model

**Goal**: Agent has all 6 resource types (LLMs, MCP, Memory, Variables, Flows, Scripts)

| Story | Description | Status |
|-------|-------------|--------|
| 20.1 | Multi-LLM support (primary/fallback/specialist routing) | TODO |
| 20.2 | Variables — typed key-value config injected into agent context | TODO |
| 20.3 | Flow bindings — agent can list and execute bound workflows directly | TODO |
| 20.4 | Script bindings — agent can invoke named Lua/JS scripts | TODO |
| 20.5 | Agent resource config migration (flat → structured) | TODO |

### EPIC 21: Agent Template Registry

**Goal**: Versioned, validated, reusable agent templates

| Story | Description | Status |
|-------|-------------|--------|
| 21.1 | Template CRUD API (`/templates`) | TODO |
| 21.2 | Template versioning (create v1, v2, set active) | TODO |
| 21.3 | Template validation (JSON Schema for workflow defs) | TODO |
| 21.4 | Template → Agent instantiation (create agent from template) | TODO |
| 21.5 | Template import/export (YAML/JSON files) | TODO |

### EPIC 22: Agent Observability

**Goal**: Queryable agent history, health, and metrics

| Story | Description | Status |
|-------|-------------|--------|
| 22.1 | Agent conversation history API (`GET /agents/:id/history`) | TODO |
| 22.2 | Agent health endpoint (`GET /agents/:id/health`) | TODO |
| 22.3 | Agent metrics (turns, tool calls, latency, token usage) | TODO |
| 22.4 | Store conversation history in ClawDB EPISODIC | TODO |
| 22.5 | Agent event emission (spawn, turn, stop events) | TODO |

### EPIC 23: Trigger Persistence

**Goal**: Triggers survive server restart

| Story | Description | Status |
|-------|-------------|--------|
| 23.1 | Store triggers in LanceDB (not just in-memory) | TODO |
| 23.2 | Load triggers on startup | TODO |
| 23.3 | Auto-spawn agents referenced by triggers | TODO |
| 23.4 | Trigger execution history/audit log | TODO |

### EPIC 24: Workflow Execution from Triggers

**Goal**: Triggers can execute workflows directly (no agent needed)

| Story | Description | Status |
|-------|-------------|--------|
| 24.1 | Wire TriggerWorkflowAdapter to actual executor | TODO |
| 24.2 | Input mapping (webhook body → workflow input) | TODO |
| 24.3 | Execution tracking (trigger → instance_id) | TODO |

### EPIC 25: Agent Memory Enhancement

**Goal**: Deeper ClawDB integration for learning agents

| Story | Description | Status |
|-------|-------------|--------|
| 25.1 | `on_complete` AQL — auto-store outcomes in EPISODIC | TODO |
| 25.2 | Context assembly from memory before each turn | TODO |
| 25.3 | LINK ontology (relationship building between memories) | TODO |
| 25.4 | Procedural variable auto-tuning | TODO |

---

## EE EPICs — FlowR (Future)

### EPIC 30: Agent Canvas (K8s CRD)

| Story | Description |
|-------|-------------|
| 30.1 | Define AgentCanvas CRD schema |
| 30.2 | K8s controller (kube-rs) — reconciliation loop |
| 30.3 | Agent pod management (create/scale/delete) |
| 30.4 | Trigger binding (Traefik/Ingress routes) |
| 30.5 | Output sink binding (Kafka, webhook, S3) |

### EPIC 31: Traffic Management

| Story | Description |
|-------|-------------|
| 31.1 | Traffic splitting (trafficShare %) |
| 31.2 | Canary deployments (gradual rollout) |
| 31.3 | A/B testing (header/cookie routing) |
| 31.4 | Blue/green deployment |

### EPIC 32: Drift Detection & Reconciliation

| Story | Description |
|-------|-------------|
| 32.1 | Spec vs status comparison |
| 32.2 | Auto-reconciliation (scale to desired) |
| 32.3 | Drift alerting |
| 32.4 | Rollback support |

### EPIC 33: Auto-Scaling

| Story | Description |
|-------|-------------|
| 33.1 | Metrics-based scaling (latency, queue depth) |
| 33.2 | HPA integration |
| 33.3 | Scale-to-zero for idle agents |

### EPIC 34: UI Layer

| Story | Description |
|-------|-------------|
| 34.1 | Input form generation from JSON Schema |
| 34.2 | Output HTML template rendering |
| 34.3 | Agent topology visualization (graph view) |
| 34.4 | Reconciliation dashboard |
| 34.5 | Real-time log streaming |

### EPIC 35: Output Sinks

| Story | Description |
|-------|-------------|
| 35.1 | Kafka/Redpanda producer |
| 35.2 | Webhook forwarder |
| 35.3 | S3/object store writer |
| 35.4 | ClickHouse direct insert |

---

## Implementation Order

```
DONE:
  EPIC 0-6    FlowR POC (shared crates)
  EPIC 10-19  ClawdFlow CE (agents, triggers, deployment)

NEXT (CE):
  EPIC 20     Complete agent resource model
  EPIC 21     Agent template registry
  EPIC 22     Agent observability
  EPIC 23     Trigger persistence
  EPIC 24     Workflow triggers
  EPIC 25     Memory enhancement

FUTURE (EE):
  EPIC 30     Agent Canvas (K8s CRD)
  EPIC 31     Traffic management
  EPIC 32     Drift detection
  EPIC 33     Auto-scaling
  EPIC 34     UI layer
  EPIC 35     Output sinks
```
