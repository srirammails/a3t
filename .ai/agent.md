# A3T Agent Architecture

> Agent Template → Agent → Agent Canvas

## Three-Layer Model

```
┌─────────────────────────────────────────────────────────────┐
│  Layer 1: Agent Template (CNCF Workflow)                    │
│  Reusable blueprint — versioned, validated, shareable       │
│  Example: rtb-bidder:v2.1                                   │
├─────────────────────────────────────────────────────────────┤
│  Layer 2: Agent (Running Instance)                          │
│  Single process — LLM + Tools + Memory + Triggers           │
│  4 modes: Ephemeral, Session, Persistent, Reactive          │
│  Powered by ZeroClaw runtime (embedded Rust library)        │
├─────────────────────────────────────────────────────────────┤
│  Layer 3: Agent Canvas (K8s CRD) — EE only                  │
│  Multi-agent orchestration — replicas, traffic, drift       │
│  K8s reconciliation controller                              │
└─────────────────────────────────────────────────────────────┘
```

---

## Agent Resource Model

An agent binds 6 resource types:

| Resource | Purpose | Status |
|----------|---------|--------|
| **LLMs** | Reasoning engine (50+ providers via ZeroClaw) | DONE |
| **MCP Servers** | Tools (discover + call via MCP protocol) | DONE |
| **Memory** | ClawDB — 5 memory types via AQL | DONE |
| **Variables** | Typed key-value config injected into context | TODO |
| **Flows** | Bound workflows = procedural memory | TODO |
| **Scripts** | Inline Lua/JS callable by agent | TODO |
| **UI** | Input forms / output display | EE only |

### Agent Config (current)

```yaml
name: my-agent
type_ref: agent
config:
  llm: openai-gpt
  memory: agent-memory
  mode: persistent
  agent_id: ops-agent
  mcp_servers:
    - url: http://localhost:3002
  system_prompt: "You are helpful."
  max_iterations: 10
```

### Agent Config (target)

```yaml
name: my-agent
type_ref: agent
config:
  template: rtb-bidder:v2.1       # CNCF Workflow template ref
  
  llms:                            # Multi-LLM with roles
    - ref: openai-gpt
      role: primary
    - ref: claude-sonnet
      role: fallback

  mcp_servers:                     # Tool sources
    - ref: clawdflow-tools
    - ref: monitoring-tools

  memory: agent-memory             # ClawDB ref

  variables:                       # Typed variables
    environment: production
    max_retries: 3

  flows:                           # Bound workflows
    - ref: bid-strategy
      role: strategy
    - ref: fraud-detection
      role: validation

  scripts:                         # Inline scripts
    - ref: bid-calculator
      language: lua

  mode: persistent
  agent_id: ops-agent
  system_prompt: "You are a bidding agent."
```

---

## Agent Lifecycle Modes

| Mode | Lifecycle | When to use |
|------|-----------|-------------|
| **Ephemeral** | Create → turn → die | One-shot: classify, extract, summarize |
| **Session** | Create → turn → keep alive (TTL) → reuse | Multi-turn chat, iterative analysis |
| **Persistent** | Spawn → accept messages forever | Ops agents, monitoring, always-on |
| **Reactive** | Spawn → triggered by events → process | Incident response, webhook handlers |

### Task Types

| Task | Behavior |
|------|----------|
| `call:agent` | Sync request-response |
| `send:agent` | Fire-and-forget message |
| `ask:agent` | Send + wait for reply |
| `spawn:agent` | Start persistent/reactive agent |
| `stop:agent` | Stop a running agent |

---

## ZeroClaw Integration

ZeroClaw is embedded as a Rust library (not a separate process):

```
ClawdFlow Process
  └─ AgentExecutor
       └─ ZeroClaw AgentBuilder
            ├─ Provider: create_provider_with_url("openai", key, url)
            ├─ Tools: Vec<Box<dyn Tool>> (from MCP discovery)
            ├─ Memory: Arc<dyn Memory> (ClawDB adapter)
            ├─ Dispatcher: NativeToolDispatcher
            └─ Agent.turn(message) → response
```

### Adapters

| Adapter | Bridge |
|---------|--------|
| `McpTool` | MCP server → ZeroClaw `Tool` trait |
| `ClawDbMemory` | ClawDB HTTP → ZeroClaw `Memory` trait |

---

## Memory System (ClawDB)

5 memory types accessible via AQL (Agent Query Language):

| Type | Purpose | Latency |
|------|---------|---------|
| **WORKING** | Current task context | <1ms |
| **EPISODIC** | Past experiences, outcomes | <50ms |
| **PROCEDURAL** | How-to knowledge (workflows!) | <5ms |
| **SEMANTIC** | Facts, domain knowledge | <20ms |
| **TOOLS** | Tool usage patterns | <2ms |

### Key Insight: Workflows = Procedural Memory

```
Standard agent:
  "How do I deploy?" → searches text → returns a string

ClawdFlow agent:
  "How do I deploy?"
    → RECALL FROM PROCEDURAL WHERE task = "deploy"
    → finds workflow "deploy-to-production"
    → calls execute_flow("deploy-to-production", input)
    → workflow RUNS
    → agent reports result
```

---

## CNCF DSL Integration

Agent workflows leverage the full CNCF Serverless Workflow DSL:

### Extensions for Agent Workflows

Agent workflows can use `use.extensions` for cross-cutting concerns:

```yaml
use:
  extensions:
    - name: cost-tracking
      extend: call:llm
      before:
        - name: log-llm-call
          type: set
          config:
            _llm_call_count: "${ ($context._llm_call_count // 0) + 1 }"
      after:
        - name: track-tokens
          type: set
          config:
            _total_tokens: "${ ($context._total_tokens // 0) + .usage.total_tokens }"
```

### Conditional Steps in Agent Flows

Agent workflows can skip steps conditionally:

```yaml
steps:
  - name: check-cache
    type: state:get
    config: { key: "cache:${ .query }" }
    export.as: "."

  - name: call-llm
    type: call:llm
    if: "${ .cache_hit != true }"    # Skip LLM if cache hit
    config:
      model: gpt-4o-mini
      prompt: "${ .query }"
    export.as: "."
```

### Task Timeouts for Agent Safety

```yaml
steps:
  - name: agent-turn
    type: call:agent
    timeout: "PT30S"                  # 30-second safety timeout
    config:
      mode: ephemeral
      max_iterations: 5
```

---

## Trigger System

Triggers are first-class resources, independent from agents:

```bash
# Create trigger
POST /triggers → {"name": "prod-alerts", "config": {"type": "webhook", "path": "/alerts"}}

# Attach to agent
POST /triggers/:id/attach → {"type": "agent", "agent_id": "ops-agent"}

# Fire webhook → trigger → agent inbox → agent.turn()
POST /webhook?path=/alerts → {"severity": "critical"}

# Reassign
POST /triggers/:id/detach → {"target_id": "ops-agent"}
POST /triggers/:id/attach → {"type": "agent", "agent_id": "backup-agent"}
```

Trigger targets: Agent inbox, Workflow execution, External webhook

---

## Agent Canvas (EE — Future)

K8s CRD for multi-agent orchestration:

```yaml
apiVersion: workflow.a3t.io/v1alpha1
kind: AgentCanvas
metadata:
  name: rtb-production
spec:
  triggers:
    - kind: HTTPTrigger
      path: /bid
  agents:
    - ref: bidder-primary
      template: rtb-bidder:v2.1
      replicas: 3
      trafficShare: 0.99
    - ref: bidder-canary
      template: rtb-bidder:v2.1
      replicas: 1
      trafficShare: 0.01
  outputs:
    - sink: kafka://bids-topic
```

Reconciliation: `spec parsed → diff computed → scaling → triggers bound → outputs bound`

---

## References

- **ZeroClaw**: https://github.com/openagen/zeroclaw
- **AQL Spec**: `/Users/sriram.reddy/Dev/AQL/spec/`
- **ClawDB**: `/Users/sriram.reddy/Dev/AQL/crates/clawdb-server/`
- **Agent tests**: `/Users/sriram.reddy/Dev/flowr-system/tests/flows/test-agent-*.yaml`
