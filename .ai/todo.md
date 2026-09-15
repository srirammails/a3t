# A3T Implementation TODO

> Current focus: ClawdFlow CE — complete agent resource model

---

## Status Legend

- `[ ]` Not started
- `[~]` In progress
- `[x]` Complete

---

## CE — Completed

### Core Engine
- [x] ClawdFlow single-process engine (LanceDB storage)
- [x] MCP protocol (stdio + HTTP/SSE)
- [x] 16 MCP tools (CRUD flows, resources, state, execution)
- [x] 22+ task types (flow control, I/O, scripting, integration)
- [x] Expression engine (jq-based, compiled filter cache)
- [x] Docker build + deployment (37.27.195.4:3002)

### Agent System
- [x] ZeroClaw embedded as Rust library
- [x] 50+ LLM providers (OpenAI, Anthropic, Ollama, etc.)
- [x] MCP tool discovery + execution (McpTool adapter)
- [x] ClawDB memory adapter (AQL over HTTP)
- [x] 4 agent modes (ephemeral, session, persistent, reactive)
- [x] 5 agent task types (call, send, ask, spawn, stop)
- [x] AgentManager singleton (spawn, inbox, TTL cleanup)
- [x] Inbox processor with agent.turn() wired up
- [x] NativeToolDispatcher for OpenAI function calling

### Triggers
- [x] TriggerManager (CRUD, attach/detach, pause/enable)
- [x] Webhook triggers (POST /webhook?path=...)
- [x] Cron triggers (5-field expression parser + scheduler)
- [x] Event triggers (internal broadcast bus)
- [x] HTTP API (/triggers, /triggers/:id/attach, etc.)
- [x] Trigger → Agent delivery via AgentMessageSender trait
- [x] clawdflow-agents crate (extracted from clawdflow-tasks)

### Tests (verified locally + remote)
- [x] test-call-llm (OpenAI, tool use)
- [x] test-call-clawdb (STORE + RECALL)
- [x] test-call-agent (ephemeral with MCP tools)
- [x] test-agent-ephemeral (create → tool call → die)
- [x] test-agent-session (2 turns, remembers secret code)
- [x] test-agent-persistent (spawn → ask → stop)
- [x] test-agent-reactive (spawn → ask with tools → stop)
- [x] Webhook trigger lifecycle (create → attach → fire → pause → enable → detach → delete)

---

## CE — Next Up

### EPIC 20: Agent Resource Model

- [ ] Multi-LLM support
  - [ ] `llms` array with `role` field (primary/fallback/specialist)
  - [ ] LLM selection logic (route by query classification or round-robin)
  - [ ] Fallback on error (primary fails → try fallback)

- [ ] Variables
  - [ ] `variables: HashMap<String, Value>` in agent config
  - [ ] Inject into agent context before each turn
  - [ ] Variable scoping (agent-level vs workflow-level)
  - [ ] Runtime variable update API

- [ ] Flow Bindings
  - [ ] `flows` array in agent config with `ref` and `role`
  - [ ] Agent can list bound flows without MCP discovery
  - [ ] Agent can execute bound flows directly
  - [ ] Flow results fed back to agent context

- [ ] Script Bindings
  - [ ] `scripts` array in agent config
  - [ ] Named Lua/JS scripts callable by agent
  - [ ] Script results returned as tool output

### EPIC 21: Template Registry

- [ ] `POST /templates` — create versioned template
- [ ] `GET /templates` — list templates
- [ ] `GET /templates/:name` — get template (active version)
- [ ] `GET /templates/:name/:version` — get specific version
- [ ] `POST /templates/:name/activate/:version` — set active version
- [ ] Template validation (required fields, config schema)
- [ ] Create agent from template (`template` field in agent config)

### EPIC 22: Observability

- [ ] `GET /agents/:id/history` — conversation log
- [ ] `GET /agents/:id/health` — health status
- [ ] Per-agent metrics (turns, tool calls, errors, latency)
- [ ] Store conversation in ClawDB EPISODIC
- [ ] Agent lifecycle events (spawned, turn_started, turn_completed, stopped)

### EPIC 23: Trigger Persistence

- [ ] Store triggers in LanceDB `triggers` table
- [ ] Load triggers on server startup
- [ ] Auto-spawn agents referenced by active triggers
- [ ] Trigger execution audit log

### EPIC 24: Workflow Triggers

- [ ] Wire `TriggerWorkflowAdapter` to actual workflow executor
- [ ] Trigger can execute workflow directly (no agent, no LLM cost)
- [ ] Input mapping from webhook body to workflow input
- [ ] Track trigger → workflow execution linkage

### EPIC 25: Memory Enhancement

- [ ] `on_complete` AQL config — auto-store outcomes after each turn
- [ ] Context hydration — load from ClawDB before turn
- [ ] LINK ontology — build relationships between memories
- [ ] Procedural variable auto-tuning (learn from outcomes)

---

## EE — Future (FlowR)

### EPIC 30: Agent Canvas
- [ ] AgentCanvas CRD definition
- [ ] K8s controller (kube-rs, reconciliation loop)
- [ ] Agent pod lifecycle management
- [ ] Trigger → Ingress/Traefik route binding
- [ ] Output sink binding

### EPIC 31: Traffic Management
- [ ] trafficShare splitting
- [ ] Canary deployments
- [ ] A/B testing
- [ ] Blue/green

### EPIC 32: Drift Detection
- [ ] Spec vs status comparison
- [ ] Auto-reconciliation
- [ ] Drift alerting
- [ ] Rollback

### EPIC 33: Auto-Scaling
- [ ] Metrics-based HPA
- [ ] Scale-to-zero
- [ ] Custom scaling metrics (queue depth, latency)

### EPIC 34: UI Layer
- [ ] Input form from JSON Schema
- [ ] Output HTML template
- [ ] Agent topology graph
- [ ] Reconciliation dashboard

### EPIC 35: Output Sinks
- [ ] Kafka/Redpanda
- [ ] Webhook forwarder
- [ ] S3/object store
- [ ] ClickHouse

---

## Crate Structure

```
flowr-system/
├── shared/                  # Backend-agnostic (CE)
│   ├── flowr-core/
│   ├── flowr-expr/
│   ├── flowr-types/
│   ├── flowr-storage/
│   ├── flowr-tasks/
│   ├── flowr-worker-core/
│   └── flowr-mcp-core/
│
├── clawdflow/               # ClawdFlow CE
│   └── crates/
│       ├── clawdflow-core/
│       ├── clawdflow-storage/   (LanceDB)
│       ├── clawdflow-agents/    (agents + triggers + adapters)
│       ├── clawdflow-tasks/     (task executors)
│       └── clawdflow-mcp/       (HTTP server + MCP + binary)
│
├── flowr/                   # FlowR EE (future)
│   └── crates/
│       ├── flowr-canvas/        (K8s CRD controller)
│       ├── flowr-ui/            (UI layer)
│       ├── flowr-scaling/       (auto-scaling)
│       ├── flowr-outputs/       (output sinks)
│       └── flowr-server/        (EE binary)
│
├── AQL/                     # ClawDB (shared)
└── tests/                   # Test suite
```

---

## Deployment

| Service | URL | Container |
|---------|-----|-----------|
| ClawdFlow | http://37.27.195.4:3002 | Docker |
| ClawDB | http://37.27.195.4:6444 | Docker |

### Deploy workflow

```bash
# 1. Rsync (exclude target/ .git/ AQL/)
rsync -avz --delete --exclude='target/' --exclude='.git/' --exclude='AQL/' \
  -e "ssh -i ~/.ssh/sri_github" flowr-system/ root@37.27.195.4:/root/flowr-system/

# 2. Ensure AQL is in build context
ssh root@37.27.195.4 "cp -r /root/AQL /root/flowr-system/AQL"

# 3. Build
ssh root@37.27.195.4 "cd /root/flowr-system && docker build --no-cache -f clawdflow/Dockerfile -t ghcr.io/srirammails/clawdflow:latest ."

# 4. Deploy
ssh root@37.27.195.4 "docker stop clawdflow-service && docker rm clawdflow-service && \
  docker run -d --name clawdflow-service --restart unless-stopped \
  -p 3002:3002 -v /opt/clawdflow-data:/data/clawdflow \
  ghcr.io/srirammails/clawdflow:latest --mode http --port 3002"
```
