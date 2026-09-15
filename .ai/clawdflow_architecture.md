# ClawdFlow Architecture

> Single-process workflow engine with embedded AI agents, triggers, and memory

## Overview

ClawdFlow is a single-binary workflow engine that embeds:

- **Workflow executor** — CNCF Serverless Workflow tasks (set, do, switch, for, fork, try, etc.)
- **ZeroClaw agent runtime** — AI agents with LLM + MCP tools + memory
- **Trigger system** — Independent webhook/cron/event triggers with dynamic routing
- **ClawDB integration** — 5-type agent memory (Working, Episodic, Procedural, Semantic, Tools)
- **MCP protocol** — Model Context Protocol for Claude Desktop and tool integration
- **LanceDB storage** — Embedded columnar database for workflows, resources, state

```
┌──────────────────────────────────────────────────────────────────────┐
│                        ClawdFlow Process                            │
│                                                                      │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐              │
│  │ HTTP/SSE     │  │ MCP Protocol │  │ Webhook EP   │              │
│  │ Port 3002    │  │ (stdio/SSE)  │  │ /webhook     │              │
│  └──────┬───────┘  └──────┬───────┘  └──────┬───────┘              │
│         │                 │                  │                       │
│         ▼                 ▼                  ▼                       │
│  ┌─────────────────────────────────────────────────────┐            │
│  │                   Router Layer                       │            │
│  │  MCP tools · Agent mgmt · Trigger CRUD · Webhook    │            │
│  └──────────────────────┬──────────────────────────────┘            │
│                         │                                            │
│    ┌────────────────────┼────────────────────┐                      │
│    ▼                    ▼                    ▼                       │
│  ┌──────────┐  ┌──────────────┐  ┌────────────────┐                │
│  │Workflow  │  │ Agent        │  │ Trigger        │                │
│  │Executor  │  │ Manager      │  │ Manager        │                │
│  │          │  │              │  │                │                │
│  │ 22 task  │  │ 4 modes:     │  │ Webhook/Cron/  │                │
│  │ types    │  │ Ephemeral    │  │ Event routing  │                │
│  │          │  │ Session      │  │                │                │
│  │ call:llm │  │ Persistent   │  │ Attach/Detach  │                │
│  │ call:agent│  │ Reactive     │  │ targets        │                │
│  │ call:clawdb│ │              │  │                │                │
│  │ spawn:agent│ │ ZeroClaw     │  │ Pause/Enable   │                │
│  │ send:agent│  │ embedded     │  │                │                │
│  │ ask:agent │  │              │  │                │                │
│  │ stop:agent│  │              │  │                │                │
│  └────┬─────┘  └──────┬───────┘  └───────┬────────┘                │
│       │               │                  │                          │
│       └───────────────┼──────────────────┘                          │
│                       ▼                                              │
│  ┌─────────────────────────────────────────────────────┐            │
│  │                  LanceDB Storage                     │            │
│  │  workflows · resources · executions · state          │            │
│  └─────────────────────────────────────────────────────┘            │
│                                                                      │
│  ┌─────────────────────┐  ┌────────────────────────────┐            │
│  │  ClawDB (HTTP)       │  │  MCP Servers (external)    │            │
│  │  Agent Memory        │  │  Tool discovery + calls    │            │
│  │  AQL queries         │  │  via MCP protocol          │            │
│  └─────────────────────┘  └────────────────────────────┘            │
└──────────────────────────────────────────────────────────────────────┘
```

---

## Crate Structure

```
flowr-system/
├── shared/                         # Shared crates (backend-agnostic)
│   ├── flowr-core/                # Executor orchestration
│   ├── flowr-expr/                # jq expression engine
│   ├── flowr-types/               # Core types (WorkflowDefinition, Resource, etc.)
│   ├── flowr-storage/             # Storage trait abstractions
│   ├── flowr-tasks/               # Shared task executors (22 types)
│   ├── flowr-worker-core/         # Shared dispatcher
│   └── flowr-mcp-core/            # MCP protocol + tool handlers
│
├── clawdflow/                     # ClawdFlow (single-process)
│   └── crates/
│       ├── clawdflow-core/        # TaskContext, FlowContext, ExpressionEngine
│       ├── clawdflow-storage/     # LanceDB implementation of StorageBackend
│       ├── clawdflow-agents/      # Agent runtime + triggers (NEW)
│       │   ├── types.rs           #   AgentMode, AgentResourceConfig, messages
│       │   ├── manager.rs         #   AgentManager singleton (spawn/send/ask/stop)
│       │   ├── adapters/          #   ZeroClaw integration bridges
│       │   │   ├── mcp_tool.rs    #     MCP server → ZeroClaw Tool trait
│       │   │   └── clawdb_memory.rs#    ClawDB → ZeroClaw Memory trait
│       │   └── triggers/          #   First-class trigger system
│       │       ├── types.rs       #     Trigger, TriggerConfig, TriggerTarget
│       │       ├── cron.rs        #     Cron expression parser + scheduler
│       │       └── mod.rs         #     TriggerManager (CRUD, delivery, attach)
│       ├── clawdflow-tasks/       # Task executors (extends shared)
│       │   ├── call_agent.rs      #   5 agent executors (call/send/ask/spawn/stop)
│       │   ├── call_llm.rs        #   LLM provider calls
│       │   ├── call_clawdb.rs     #   ClawDB AQL queries
│       │   └── ...                #   All other task types
│       └── clawdflow-mcp/         # Binary: HTTP/SSE server + MCP protocol
│           ├── main.rs            #   HTTP routes, MCP handlers, webhook EP
│           ├── dispatcher.rs      #   Routes task types → executors
│           └── server.rs          #   MCP stdio server
│
├── AQL/                           # ClawDB / AQL parser (external)
│   └── crates/
│       ├── clawdb/                # ClawDB core library
│       ├── clawdb-server/         # ClawDB HTTP/gRPC server
│       └── aql-parser/            # AQL query language parser
│
└── tests/                         # Test suite
    ├── run_tests.py               # Python test runner
    └── flows/                     # Test workflow YAML files
```

---

## Agent System

### ZeroClaw Integration

ClawdFlow embeds [ZeroClaw](https://github.com/openagen/zeroclaw) as a Rust library crate. ZeroClaw provides:

- **50+ LLM providers** (OpenAI, Anthropic, Ollama, OpenRouter, etc.)
- **Native tool calling** (OpenAI function calling format)
- **Agent loop** (message → LLM → tool calls → execute → loop)
- **Tool call parsing** (native, XML, markdown formats)
- **Credential scrubbing** in logs

ClawdFlow bridges ZeroClaw to its own ecosystem via adapters:

| Adapter | From | To | Purpose |
|---------|------|----|---------|
| `McpTool` | MCP server | ZeroClaw `Tool` trait | Agent discovers and calls MCP tools |
| `ClawDbMemory` | ClawDB HTTP | ZeroClaw `Memory` trait | Agent stores/recalls from ClawDB |
| `NativeToolDispatcher` | ZeroClaw | OpenAI API | Formats tool calls for OpenAI |

### Agent Modes

| Mode | Lifecycle | Use Case |
|------|-----------|----------|
| **Ephemeral** | Create → turn → die | One-shot tasks (classify, extract, summarize) |
| **Session** | Create → turn → keep alive (TTL) → reuse | Multi-turn chat, iterative analysis |
| **Persistent** | Spawn → accept messages → live forever | Ops agents, monitoring, always-on assistants |
| **Reactive** | Spawn → triggered by events → process | Incident response, webhook handlers |

### Agent Resource Config

```yaml
name: my-agent
type_ref: agent
config:
  llm: openai-gpt              # Reference to LLM resource
  memory: agent-memory          # Reference to ClawDB resource
  mode: persistent              # ephemeral | session | persistent | reactive
  agent_id: ops-agent           # Identity (session/persistent/reactive)
  ttl: 1800                     # Session TTL in seconds
  max_iterations: 10            # Tool call loop limit
  mcp_servers:                  # Tool sources
    - url: http://localhost:3002
  system_prompt: "You are helpful."
  triggers:                     # Legacy (use TriggerManager instead)
    - type: webhook
      path: /alerts
```

### Agent Task Types

| Task | Behavior |
|------|----------|
| `call:agent` | Sync request-response (ephemeral/session) or send+wait (persistent) |
| `send:agent` | Fire-and-forget message to running agent |
| `ask:agent` | Send message, wait for reply with timeout |
| `spawn:agent` | Start a persistent/reactive agent |
| `stop:agent` | Stop a running agent |

### Agent Manager

Singleton (`AgentManager`) that manages all live agents:

```
AgentManager (static singleton)
├── agents: HashMap<String, Arc<AgentHandle>>
│   ├── "ops-agent" → AgentHandle
│   │   ├── agent: Arc<Mutex<zeroclaw::Agent>>
│   │   ├── inbox_tx: mpsc::Sender<InboxMessage>
│   │   ├── mode: Persistent
│   │   └── config: AgentResourceConfig
│   └── "session-abc" → AgentHandle (expires after TTL)
└── Background tasks
    ├── inbox_processor (per agent) — reads inbox, calls agent.turn()
    ├── cron_loop (per reactive agent) — fires at cron schedule
    └── event_loop (per reactive agent) — listens to event bus
```

---

## Trigger System

Triggers are first-class resources, independent from agents. A trigger can be created, then attached to one or more targets dynamically.

### Trigger Types

| Type | Fires When |
|------|-----------|
| **Webhook** | `POST /webhook?path=/alerts` matches trigger path |
| **Cron** | 5-field cron expression matches current time (checked every minute) |
| **Event** | Internal event bus emits matching event type |

### Trigger Targets

A single trigger can deliver to multiple targets simultaneously:

| Target | Action |
|--------|--------|
| **Agent** | Send message to agent's inbox (fire-and-forget) |
| **Workflow** | Execute workflow directly (no agent, no LLM cost) |
| **Webhook** | Forward payload to external URL |

### Trigger API

```
POST   /triggers                    Create trigger
GET    /triggers                    List all triggers
GET    /triggers/:id                Get trigger details
DELETE /triggers/:id                Delete trigger
POST   /triggers/:id/enable         Enable trigger
POST   /triggers/:id/pause          Pause trigger
POST   /triggers/:id/attach         Attach target (agent/workflow/webhook)
POST   /triggers/:id/detach         Detach target
POST   /webhook?path=/alerts        Fire webhook delivery
```

### Trigger Lifecycle Example

```bash
# 1. Create trigger (no target yet)
curl -X POST /triggers -d '{"name":"prod-alerts","config":{"type":"webhook","path":"/alerts"}}'
# → {"id": "trg-001"}

# 2. Create and spawn an agent separately
# ... (via spawn:agent workflow task)

# 3. Attach agent to trigger
curl -X POST /triggers/trg-001/attach -d '{"type":"agent","agent_id":"ops-agent"}'

# 4. Fire webhook → trigger → agent inbox → agent.turn()
curl -X POST /webhook?path=/alerts -d '{"severity":"critical"}'
# → {"delivered": 1, "targets": ["agent:ops-agent"]}

# 5. Pause trigger (stops delivery)
curl -X POST /triggers/trg-001/pause

# 6. Reassign to different agent
curl -X POST /triggers/trg-001/detach -d '{"target_id":"ops-agent"}'
curl -X POST /triggers/trg-001/attach -d '{"type":"agent","agent_id":"backup-agent"}'
```

---

## MCP Protocol

ClawdFlow exposes 16 MCP tools via the Model Context Protocol:

| Tool | Purpose |
|------|---------|
| `list_task_types` | List available task types |
| `get_task_type` | Get task type details |
| `list_flows` | List workflows |
| `get_flow` | Get workflow definition |
| `create_flow` | Create workflow from YAML/JSON |
| `update_flow` | Enable/disable workflow |
| `delete_flow` | Delete workflow |
| `execute_flow` | Execute workflow |
| `get_execution_status` | Get execution result |
| `get_state` | Read state store |
| `set_state` | Write state store |
| `list_resources` | List configured resources |
| `get_resource` | Get resource details |
| `create_resource` | Create resource |
| `delete_resource` | Delete resource |
| `list_resource_types` | List resource type templates |

### Connection Modes

| Mode | Transport | Use Case |
|------|-----------|----------|
| **Stdio** | stdin/stdout | Claude Desktop local integration |
| **HTTP/SSE** | Port 3002 | Remote access, multi-client |

---

## Storage

### LanceDB (Embedded)

All data stored in a single LanceDB directory:

| Table | Contents |
|-------|----------|
| `workflows` | Workflow definitions (YAML/JSON) |
| `resources` | Resource configs (LLM, ClawDB, MCP, agent) |
| `executions` | Workflow execution state and results |
| `state` | Key-value state store with TTL |

### ClawDB (External, HTTP)

Agent memory via AQL (Agent Query Language):

| Memory Type | Purpose | Example AQL |
|-------------|---------|-------------|
| **WORKING** | Current task context | `STORE INTO WORKING (key="x", value=42)` |
| **EPISODIC** | Past experiences | `RECALL FROM EPISODIC WHERE task="deploy"` |
| **PROCEDURAL** | How-to knowledge (workflows!) | `RECALL FROM PROCEDURAL WHERE pattern="incident"` |
| **SEMANTIC** | Facts, domain knowledge | `RECALL FROM SEMANTIC WHERE topic="pricing"` |
| **TOOLS** | Tool usage patterns | `RECALL FROM TOOLS WHERE name="list_resources"` |

---

## Deployment

### Docker Container

```bash
# Build
docker build -f clawdflow/Dockerfile -t ghcr.io/srirammails/clawdflow:latest .

# Run
docker run -d \
  --name clawdflow-service \
  --restart unless-stopped \
  -p 3002:3002 \
  -v /opt/clawdflow-data:/data/clawdflow \
  ghcr.io/srirammails/clawdflow:latest \
  --mode http --port 3002
```

### Production (37.27.195.4)

```
ClawdFlow:  http://37.27.195.4:3002  (Docker container)
ClawDB:     http://37.27.195.4:6444  (Docker container)
```

---

## Technology Stack

| Layer | Technology |
|-------|------------|
| Language | Rust 1.88+ |
| Async Runtime | Tokio |
| HTTP Server | Axum 0.7 |
| HTTP Client | reqwest 0.12 |
| Expression Engine | jaq (Rust jq) |
| Embedded Database | LanceDB 0.26 |
| Agent Runtime | ZeroClaw (git dep) |
| Agent Memory | ClawDB (AQL over HTTP) |
| Scripting | Lua (mlua), JS (rquickjs) |
| Serialization | serde/serde_json |
| Logging | tracing |

---

## Test Suite

Test flows in `tests/flows/`:

| Test | What it validates |
|------|-------------------|
| `test-call-llm` | OpenAI LLM call with tool use |
| `test-call-clawdb` | ClawDB STORE + RECALL via AQL |
| `test-call-agent` | Ephemeral agent with MCP tool calls |
| `test-agent-ephemeral` | Create → tool call → die |
| `test-agent-session` | Two turns, same session_id, remembers context |
| `test-agent-persistent` | spawn → wait → ask → stop lifecycle |
| `test-agent-reactive` | spawn with triggers → ask with tools → stop |
| `test-set/do/switch/for/fork/try/raise/wait` | Flow control tasks |
| `test-call-http` | HTTP requests |
| `test-inline-lua/js` | Embedded scripting |
| `test-call-sql` | PostgreSQL queries |
| `test-call-lancedb` | LanceDB operations |
| `test-file-io/state` | File and state operations |
