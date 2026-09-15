# Agent Template Specification

**Status:** Design-phase spec. Authoritative for UI and CRD implementation.

---

## 0. What an agent template is

An agent template is a **versioned, reusable blueprint** that defines what an agent is — its capabilities, constraints, and defaults. It is NOT a flow. It is NOT steps. It is a contract.

An agent instance is a template + concrete bindings (which LLM endpoint, which ClawDB namespace, which triggers). The template author decides which capabilities are exposed to the executor and which are locked.

---

## 1. The 15 capabilities

Every agent has up to 15 capabilities. A template may use some or all. Each capability has a visibility per field: `required`, `optional`, `locked`, `hidden`.

### 1.1 System Prompt

The agent's personality, instructions, and behavioral constraints.

```yaml
systemPrompt:
  content: |
    You are a customer-support chat agent.
    Before reasoning, the runtime will check procedural memory.
    Prefer FlowR actions over describing actions in prose.
  visibility: locked     # executor cannot change
```

Multiple prompts can be layered (base + role-specific + instance-specific) but the template sets the base.

### 1.2 Model(s)

LLM bindings with roles. At least one required.

```yaml
models:
  - role: primary
    ref: ${.bindings.llm}           # placeholder, instance fills
    temperature: 0.3
    maxTokens: 1024
    topP: 0.9
    visibility: required            # executor must bind
  - role: fallback
    ref: ${.bindings.llm_fallback}
    temperature: 0.5
    maxTokens: 512
    visibility: optional            # executor can bind or skip
  - role: advisor
    ref: ${.bindings.advisor_llm}
    purpose: "meta-reasoning about tool selection"
    visibility: hidden              # template handles internally
```

Roles: `primary`, `fallback`, `specialist`, `advisor`, `goal_evaluator`.

### 1.3 Tools (MCP Servers)

External tool access via MCP protocol.

```yaml
tools:
  mcpServers:
    - ref: mcp/flowr
      access: invoke
      capabilities: [run_workflow, get_workflow_status]
      visibility: locked
    - ref: mcp/clawdb
      access: read-write
      capabilities: [aql_query, aql_recall, aql_store]
      visibility: locked
    - ref: ${.bindings.domain_mcp}
      access: invoke
      visibility: optional           # executor adds domain-specific MCP
```

### 1.4 Memory

AQL tiers with per-tier permissions. Backed by ClawDB.

```yaml
memory:
  db: ${.bindings.memory_namespace}   # instance fills
  tiers:
    working:
      read: true
      write: true
      scope: turn
      visibility: hidden
    episodic:
      read: true
      write: true
      scope: session
      visibility: locked
    semantic:
      read: true
      write: false
      visibility: locked
    procedural:
      read: true
      write: false
      lookup_before_llm: true         # THE key optimization
      visibility: locked
```

`lookup_before_llm: true` means: before invoking the LLM, query procedural memory for a matching pattern. If confidence > threshold, execute the pattern directly and skip the LLM call entirely. This is how "agents write rules, rules do the work."

### 1.5 Flows (Callable Sub-Workflows)

Durable workflows the agent can invoke as tool calls during a turn. These are NOT steps in the agent — they're actions the agent can trigger.

```yaml
flows:
  callable:
    - name: issue-refund
      namespace: payments
      maxDepth: 2
      visibility: locked
    - name: escalate-to-human
      namespace: support
      maxDepth: 1
      visibility: locked
    - name: ${.bindings.custom_flow}
      visibility: optional
```

### 1.6 Skills (Scripts)

Inline Lua/JS functions the agent can call. Faster than MCP for simple transformations.

```yaml
skills:
  - name: compute-bid-price
    language: lua
    source: |
      local base = state.input.base_price
      local multiplier = state.input.floor_multiplier
      return { bid = base * multiplier }
    visibility: locked
  - name: format-response
    language: js
    source: "return { text: input.raw.trim().slice(0, 280) };"
    visibility: hidden
```

### 1.7 Agent-to-Agent Protocol

How this agent communicates with other agents. Can call agents, can be called by agents.

```yaml
agentProtocol:
  exposeAs: chat                     # other agents call this one as MCP tool "chat"
  canCall:
    - ref: specialist/legal-agent
      protocol: ask                  # request-reply
    - ref: specialist/billing-agent
      protocol: send                 # fire-and-forget
  visibility: optional
```

### 1.8 Triggers

How the agent gets activated. Template declares supported types; instance configures paths/topics.

```yaml
triggers:
  supported: [http, iggy, cron, mcp_tool, event]
  defaults:
    - kind: http
      schemaMap:
        body.text: .message
        cookie.sid: .session_id
  visibility: required               # executor must configure at least one
```

### 1.9 Sinks (Outputs)

Where the agent sends its response after a turn.

```yaml
sinks:
  - kind: http
    from: .reply
    status: 200
    visibility: locked
  - kind: iggy
    topic: ${.bindings.reply_topic}
    partitionKey: .session_id
    visibility: optional
```

### 1.10 Context Schema

The input contract — typed fields available to the agent. Kills tribal knowledge about jq expressions.

```yaml
contextSchema:
  fields:
    - name: message
      type: string
      required: true
      description: "User's message text"
    - name: session_id
      type: string
      required: true
      description: "Session identifier for memory scoping"
    - name: user_id
      type: string
      required: false
    - name: context
      type: object
      required: false
      description: "Additional context from the caller"
    - name: trace_id
      type: string
      required: false
      injected: true                 # runtime injects, not user-provided
```

### 1.11 Execution Model

How the agent lives and dies.

```yaml
execution:
  mode: session                      # ephemeral | session | persistent | reactive
  ttl: 1800s                        # session mode: expire after 30min idle
  warmPool: 0                       # persistent/reactive: pre-warm instances
  maxConcurrentSessions: 100
  visibility: optional               # executor can override mode
```

### 1.12 Security Policy

Infrastructure-level security from OpenShell integration.

```yaml
policy:
  presetRequirement: ">=standard"    # must use at least "standard" preset
  runtime:
    mode: sandboxed                  # sandboxed | co-located
    coLocatedJustification: ""       # required if co-located
  declares:
    egressHosts: []                  # all egress through MCP
    callableFlowPatterns: ["payments-*", "support-*"]
  visibility: locked                 # executor cannot weaken policy
```

### 1.13 Guardrails

Content and cost-level constraints (distinct from security policy).

```yaml
guardrails:
  inputValidation: true
  outputValidation: true
  piiRedaction: true
  contentFilter: true
  costCap:
    perTurn: "$0.05"
    perSession: "$1.00"
  rateLimit: "100/min"
  maxResponseLength: 2000
  visibility: locked
```

### 1.14 Lifecycle Hooks

AQL queries that run at specific points in the agent's turn.

```yaml
hooks:
  on_before_turn: |
    RECALL FROM EPISODIC WHERE session_id = "${.session_id}" LIMIT 10
  on_complete: |
    STORE INTO EPISODIC (
      session_id = "${.session_id}",
      role = "assistant",
      content = "${.output}",
      outcome = "${.outcome}"
    ) SCOPE session
  on_error: |
    STORE INTO EPISODIC (
      session_id = "${.session_id}",
      role = "system",
      content = "error: ${.error}"
    ) SCOPE session
  visibility: hidden
```

### 1.14b Workflow Extensions (CNCF `use.extensions`)

Before/after interceptors applied to workflow steps. These are CNCF Serverless Workflow extensions that wrap task execution with cross-cutting concerns.

```yaml
extensions:
  - name: cost-guard
    extend: call:llm
    when: "${ $context._llm_call_count > 10 }"
    before:
      - name: check-budget
        type: raise
        config:
          error:
            type: BudgetExceeded
            detail: "LLM call limit reached"
    visibility: locked
  - name: audit-trail
    extend: all
    after:
      - name: audit-log
        type: set
        config:
          _audit: "${ $task.name }"
    visibility: hidden
```

Extensions are resolved at workflow build time into `ResolvedExtension` objects and executed around each matching task. Extension failures are logged but don't block the main task execution.

### 1.15 Observability

Audit, tracing, and metrics configuration.

```yaml
observability:
  auditLevel: all_decisions          # all_decisions | failures_only | none
  tracing: true                      # emit OpenTelemetry spans
  metricsExport: true
  ocsf:
    enabled: true
    topic: flowr.audit               # Iggy topic for OCSF events
  costTracking: true
  visibility: hidden
```

---

## 2. Visibility model

Each field in the template has a visibility that controls what the agent executor sees when creating an instance:

| Visibility | Template author | Agent executor |
|-----------|----------------|----------------|
| `required` | Sets default or placeholder | Must provide value |
| `optional` | Sets default | Can override or leave default |
| `locked` | Sets value | Cannot change (shown as read-only) |
| `hidden` | Sets value | Not shown at all |

The template form has a visibility toggle (small dropdown or icon) next to each field. The instance form auto-generates from the template, showing only `required` and `optional` fields, with `locked` shown as read-only labels.

---

## 3. Complete CRD example

```yaml
apiVersion: a3t.io/v1
kind: AgentTemplate
metadata:
  name: a3t-chat-agent
  namespace: a3t
  labels:
    a3t.io/version: "1.0.0"
    a3t.io/vertical: "customer-support"

spec:
  systemPrompt:
    content: |
      You are a customer-support chat agent.
      Before reasoning, check procedural memory for matching patterns.
      Prefer FlowR actions over describing actions in prose.
    visibility: locked

  models:
    - role: primary
      ref: ${.bindings.llm}
      temperature: 0.3
      maxTokens: 1024
      maxIterations: 10
      visibility: required

  tools:
    mcpServers:
      - ref: mcp/flowr
        access: invoke
        capabilities: [run_workflow, get_workflow_status]
        visibility: locked
      - ref: mcp/clawdb
        access: read-write
        capabilities: [aql_query, aql_recall, aql_store]
        visibility: locked

  memory:
    db: ${.bindings.memory_namespace}
    tiers:
      working:    { read: true, write: true, scope: turn }
      episodic:   { read: true, write: true, scope: session }
      semantic:   { read: true, write: false }
      procedural: { read: true, write: false, lookup_before_llm: true }
    visibility: locked

  flows:
    callable:
      - name: issue-refund
        namespace: payments
        maxDepth: 2
      - name: escalate-to-human
        namespace: support
        maxDepth: 1
    visibility: locked

  skills: []

  agentProtocol:
    exposeAs: chat
    canCall: []
    visibility: optional

  triggers:
    supported: [http, iggy, mcp_tool]
    visibility: required

  sinks:
    - kind: http
      from: .reply
      status: 200
    visibility: locked

  contextSchema:
    fields:
      - { name: message, type: string, required: true }
      - { name: session_id, type: string, required: true }
      - { name: user_id, type: string, required: false }
      - { name: context, type: object, required: false }

  execution:
    mode: session
    ttl: 1800s
    visibility: optional

  policy:
    presetRequirement: ">=standard"
    runtime:
      mode: sandboxed
    declares:
      egressHosts: []
      callableFlowPatterns: ["payments-*", "support-*"]
    visibility: locked

  guardrails:
    inputValidation: true
    piiRedaction: true
    costCap: { perTurn: "$0.05" }
    rateLimit: "100/min"
    visibility: locked

  hooks:
    on_before_turn: "RECALL FROM EPISODIC WHERE session_id = \"${.session_id}\" LIMIT 10"
    on_complete: "STORE INTO EPISODIC (...) SCOPE session"
    visibility: hidden

  observability:
    auditLevel: all_decisions
    tracing: true
    ocsf: { enabled: true, topic: flowr.audit }
    visibility: hidden
```

---

## 4. Instance from template

The executor creates an instance that only fills what the template exposes:

```yaml
apiVersion: a3t.io/v1
kind: Agent
metadata:
  name: a3t-chat-agent-prod
  namespace: customer-support

spec:
  templateRef:
    name: a3t-chat-agent
    namespace: a3t
    version: "1.0.0"

  preset: standard

  bindings:
    llm: llm/qwen-3-235b-a22b-instruct
    memory_namespace: chat-agent-prod

  triggers:
    - kind: http
      path: /v1/chat
    - kind: mcp_tool
      exposeAs: chat

  execution:
    mode: session
    ttl: 3600s                       # override template default
```

The executor didn't need to configure: tools, memory tiers, flows, skills, policy, guardrails, hooks, observability, system prompt, sinks, context schema. All locked/hidden by the template author.

---

## 5. UI implications

### Template editor (author view)
- All 15 sections visible
- Each field has a visibility dropdown: required / optional / locked / hidden
- Graph shows agent node with connection ports for each configured capability
- Form is scrollable with sections
- Code shows the full CRD YAML

### Instance editor (executor view)
- Auto-generated from template
- Only shows `required` (must fill) and `optional` (can override) fields
- `locked` shown as read-only labels
- `hidden` not rendered
- Much simpler form — potentially just: pick LLM, set triggers, go

### Test bench
- Mock MCP, mock memory, mock policy
- Test runner with fixtures (cold path + hot path)
- Assertions on: procedural_hit, llm_calls, flow_calls, memory_writes, output

---

## 6. Primary Workflow (16th capability)

The primary workflow is the agent's execution pipeline — auto-generated from the 15 capabilities, generally locked by the template designer. It leverages CNCF Serverless Workflow DSL features including `if` conditions, `timeout`, and `use.extensions`.

```yaml
primaryWorkflow:
  visibility: locked
  use:
    extensions:
      - name: cost-tracking
        extend: call:llm
        after:
          - name: track-tokens
            type: set
            config:
              _total_tokens: "${ ($context._total_tokens // 0) + .usage.total_tokens }"
  steps:
    - name: input-guardrails
      type: guardrail:input
      enabled: true
      locked: true
    - name: context-assembly
      type: memory:recall
      enabled: true
      locked: true
    - name: procedural-lookup
      type: memory:procedural
      enabled: true
      locked: true
      on_hit: skip_to:output-guardrails
    - name: llm-call
      type: model:invoke
      if: "${ $context._procedural_hit != true }"
      timeout: "PT30S"
      enabled: true
      locked: true
    - name: tool-loop
      type: tools:dispatch
      if: "${ .tool_calls != null }"
      timeout: "PT60S"
      enabled: true
      locked: true
      connectedFlows:
        - ref: issue-refund
        - ref: escalate-to-human
    - name: output-guardrails
      type: guardrail:output
      enabled: true
      locked: true
    - name: memory-write
      type: memory:store
      enabled: true
      locked: true
    - name: deliver
      type: sink:emit
      enabled: true
      locked: true
```

### Graph visualization

Three node types:
- **Agent node** — 15 capability ports with visibility badges
- **Primary workflow** — connected to agent, expandable/collapsible sub-graph showing pipeline steps
- **Sub-flow nodes** — connected from tool-loop step, each expandable to show its own steps

### Extension points

The template designer can mark insertion points in the pipeline where the executor can add custom steps. These appear as dashed "+" markers between locked steps.

### Sub-flows

Sub-flows declared in `flows.callable` connect from the tool-loop step. Each can be:
- **Collapsed**: shows as a single node with name + step count
- **Expanded**: shows full pipeline of the sub-flow's steps

---

## 7. Runtime turn trace (10 steps)

1. **Trigger fires** (HTTP/Iggy/cron) → schemaMap normalizes to context schema
2. **Working memory populated** with normalized input
3. **Procedural lookup** (AQL RECALL FROM PROCEDURAL) → if hit > threshold, skip LLM
4. **If cache miss**: recall episodic + semantic context via on_before_turn hook
5. **LLM invoked** with assembled context + tool specs (`if` condition skips if procedural hit)
6. **LLM may call tools** (FlowR workflows via MCP, skills, other agents) — with `timeout` safety
7. **Tool results fed back** to LLM for next iteration
8. **Final response assembled** after max_iterations or stop
9. **on_complete hook** stores outcome into episodic memory
10. **Sink fires** (HTTP response / Iggy publish / webhook callback)

**Extensions fire transparently** at each step — cost-tracking after LLM calls, audit logging around all steps, etc. They are resolved once at workflow build time and executed as fire-and-forget interceptors.

The procedural lookup at step 3 is the architectural payoff: if hit rate is 60%, average turn cost drops 60% in both latency and LLM spend.
