# A3T Implementation Roadmap

Staged, usable increments. Each stage delivers end-to-end value.

## Design Principles

### UX Reuse: Two Editors, Four Modes

Two core components serve all editing use cases. See A3T_ARCHITECTURE_VISION.md for full spec.

1. **AgentTemplateEditor.svelte** — `mode="template"` (all editable) or `mode="agent"` (bindings editable, structure locked)
2. **FlowEditorPane.svelte** — `mode="flow"` (CNCF SWF nodes) or `mode="canvas"` (agent/CRD nodes)

This means Stage 1 extends the existing editor, and Stage 3 extends the existing flow designer.

---

## Current State (Done)

- [x] FlowR runtime executes SWF workflows
- [x] LanceDB CRUD flows (templates, agents, resources, users, canvas, audit)
- [x] LanceDB `update` operation (no more delete+insert)
- [x] Agent templates defined (agent-template YAML + agent-engine Rust-native)
- [x] `call:a3t:agent` handler (Rust-native 10-phase lifecycle)
- [x] UI: Templates list + AgentTemplateEditor
- [x] UI: Agents list (basic create/delete)
- [x] UI: Resources list, Chat page
- [x] Triggers system exists (webhook, cron, event)
- [x] MCP tools for flow/resource/agent/trigger CRUD
- [x] Registry endpoint (`/registry/resources`) with namespace scan
- [x] Deployed on k3d at 37.27.195.4

---

## Stage 1: Agent Config from Template

**Goal:** User selects a template, configures bindings using the same editor in agent mode, gets a configured agent.

### 1.1 Template Spec: Declare Required/Optional Bindings

Template YAML needs a `bindings` section that declares what an agent must provide.

- [ ] Define `bindings` schema in agent-template and agent-engine YAML specs
  ```yaml
  # In template spec_yaml, add:
  bindings:
    required:
      - name: llm
        type: llm
        description: "LLM resource for reasoning"
      - name: system_prompt
        type: text
        description: "Agent system prompt"
    optional:
      - name: memory
        type: resource
        default: null
      - name: max_iterations
        type: number
        default: 10
      - name: temperature
        type: number
        default: 0.7
      - name: tools
        type: list
        default: []
      - name: mcp_servers
        type: list
        default: []
  ```
- [ ] Update agent-template spec_yaml with bindings section
- [ ] Update agent-engine spec_yaml with bindings section
- [ ] Redeploy templates via a3t-install

### 1.2 AgentTemplateEditor: Add `mode="agent"` Support

Reuse AgentTemplateEditor.svelte — don't build a new form.

- [ ] Add `mode` prop: `'template' | 'agent'`
- [ ] Field editability config:
  ```
  template mode: all fields editable
  agent mode:    structure fields locked, binding fields editable
  ```
- [ ] Locked fields: engine, hooks, extensions, policies — show with lock icon
- [ ] Editable fields in agent mode:
  - `name` — agent instance name (free text)
  - `llm` — resource picker dropdown (from `/registry/resources`)
  - `system_prompt` — textarea, template default as placeholder
  - `memory` — resource picker dropdown
  - `tools` — multi-select list
  - `mcp_servers` — multi-select from registered MCP resources
  - `max_iterations` — number input, template default as placeholder
  - `temperature` — number input, template default as placeholder
- [ ] Save button: in template mode calls templates CRUD, in agent mode calls agents CRUD
- [ ] Remove hardcoded defaults (`qwen-3-235b-a22b-instruct`, etc.) from AgentsList.svelte

### 1.3 Agent Create Flow (UI)

- [ ] AgentsList: "Create Agent" → template picker dropdown
- [ ] On template select → open AgentTemplateEditor with `mode="agent"`, pre-loaded with template spec
- [ ] AgentDetail: opens AgentTemplateEditor with `mode="agent"` + existing agent data

### 1.4 Backend: Validate Agent Config Against Template

- [ ] agents_crud.yaml `create` step: before insert, fetch template, validate required bindings present
- [ ] Return error if required binding missing: `"Missing required binding: llm"`

**Usable at end of Stage 1:** User browses templates, picks one, fills in bindings using the same editor (locked structure, editable bindings), saves agent config. Agent is in "configured" state.

---

## Stage 2: Deploy Agent with Default Canvas

**Goal:** User clicks "Deploy" on an agent, it gets an HTTP endpoint and starts accepting messages.

### 2.1 Agent Trigger: Auto-Create HTTP Endpoint

When agent is deployed, create a webhook trigger at `/agent/{agent-id}/chat`.

- [ ] Create `a3t-agent-deploy` flow:
  1. Read agent config from agent_configs table
  2. Read template spec from template_registry table
  3. Create trigger via FlowR trigger API:
     ```yaml
     trigger:
       name: agent-{id}-trigger
       namespace: a3t
       config:
         type: webhook
         path: /agent/{agent-id}/chat
       targets:
         - type: workflow
           namespace: a3t
           name: a3t-agent-dispatch
           input_transform:
             agent_id: "{agent-id}"
             message: "${ .body.message }"
             session_id: "${ .body.session_id }"
     ```
  4. Update agent status to `deployed`
- [ ] Create `a3t-agent-undeploy` flow (delete trigger, set status back to `configured`)

### 2.2 Agent Dispatch Flow

A dispatcher that loads agent config and runs the right template engine.

- [ ] Create `a3t-agent-dispatch` flow:
  1. Receive `{ agent_id, message, session_id }`
  2. Query agent_configs for agent_id
  3. Parse resource_bindings JSON
  4. If template is `agent-engine` → `call: a3t:agent` with config from bindings
  5. If template is `agent-template` → `run: workflow` with agent-template flow + config
  6. Return response

### 2.3 Default Canvas: Auto-Generate on Deploy

- [ ] When agent is deployed (in a3t-agent-deploy), auto-create a canvas record:
  ```yaml
  id: cvs-default-{agent-id}
  name: "{agent-name} (default)"
  spec_yaml: |
    agents:
      - ref: {agent-id}
        trigger:
          http: /agent/{agent-id}/chat
    health:
      check_interval: 30s
    restart:
      policy: on_failure
      max_retries: 3
  ```
- [ ] Link canvas to agent in agent_configs (canvas_id field)

### 2.4 UI: Deploy Button + Chat Test

- [ ] AgentDetail: "Deploy" button → calls `a3t-agent-deploy` flow
- [ ] AgentDetail: Show endpoint URL after deploy: `http://37.27.195.4:30080/agent/{id}/chat`
- [ ] AgentDetail: Status badge (draft -> configured -> deployed)
- [ ] AgentDetail: "Chat" tab → inline chat UI that POSTs to `/agent/{id}/chat`
- [ ] AgentsList: Show status column (draft/configured/deployed), deployed agents have green dot

### 2.5 MCP Server Resource Creation (parallel with 2.1)

Agents need to call tools via MCP servers. Need to register MCP server resources with target paths.

- [ ] UI: In Resources page, "Create Resource" form for type=mcp:
  - Name: e.g. `agent-tools`
  - Type: `mcp`
  - Config: `{ url: "http://flowr:3002/message", namespace: "a3t" }`
- [ ] UI: In agent config editor, MCP server picker shows registered MCP resources
- [ ] Backend: Agent dispatch resolves MCP server resources and passes URLs to agent engine
- [ ] Verify: `call:a3t:agent` handler correctly uses MCP server resource to discover and call tools

**Usable at end of Stage 2:** User creates agent, deploys, gets HTTP endpoint, chats with it. Agent runs autonomously with LLM, tools, and memory. Default canvas exists in background.

---

## Stage 3: Multi-Agent Canvas Design

**Goal:** User draws a canvas wiring multiple agents using the flow designer in canvas mode. Deploys the canvas as a unit.

### 3.1 Canvas Data Model Update

Current canvas_specs table is basic. Needs structured spec.

- [ ] Update canvas schema:
  ```
  id, org_id, name, namespace, goal,
  spec_yaml,          # full canvas YAML (agents, connections, health, deps)
  status,             # draft / designed / deployed / suspended
  generation,         # version counter
  created_by, created_at, updated_at
  ```
- [ ] Define canvas YAML schema (agents, connections, dependencies, health, restart, circuits)

### 3.2 FlowEditorPane: Add `mode="canvas"` Support

Reuse FlowEditorPane.svelte — don't build a new designer.

- [ ] Add `mode` prop: `'flow' | 'canvas'`
- [ ] Canvas node registry:
  - `AgentNode` — reference existing agent (from agent list dropdown)
  - `TriggerNode` — HTTP path, cron, event trigger
  - `HealthPolicyNode` — check_interval, unhealthy_threshold, probe config
  - `CircuitBreakerNode` — error_threshold, window, action, fallback
- [ ] Canvas edge types:
  - Data flow edges (solid) — routing expression on edge
  - Dependency edges (dashed) — lifecycle ordering with on_down action
- [ ] Canvas palette (sidebar):
  - Agent (from configured/deployed agent list)
  - Trigger (http/cron/event)
  - Health policy
  - Circuit breaker
- [ ] Canvas validation: at least 1 agent, at least 1 trigger, no circular dependencies
- [ ] Canvas output: generates canvas YAML (not SWF YAML)
- [ ] Flow mode unchanged: full CNCF SWF node types and validation

### 3.3 Canvas Deploy

- [ ] Create `a3t-canvas-deploy` flow:
  1. Parse canvas spec_yaml
  2. For each agent in canvas: deploy if not already deployed (call a3t-agent-deploy)
  3. For each connection: create iggy queue subscription
  4. For each trigger: create webhook trigger
  5. Update canvas status to `deployed`
- [ ] Create `a3t-canvas-undeploy` flow (reverse)
- [ ] UI: "Deploy Canvas" button in CanvasDetail

### 3.4 Canvas Runtime: Health + Dependencies

- [ ] Create `a3t-canvas-health` flow (runs on cron):
  1. For each agent in canvas: check health (execute test message or /health)
  2. If unhealthy_threshold hit: update agent status to `unhealthy`
  3. If dependency is unhealthy: cordon dependent agents
  4. If circuit breaker threshold: open circuit, reroute to fallback
  5. Log events to audit
- [ ] Create cron trigger for health check flow
- [ ] UI: CanvasDetail shows real-time agent health status (green/amber/red dots)

**Usable at end of Stage 3:** User creates multiple agents, draws canvas in the flow designer (canvas mode), deploys the canvas, sees health status. Multi-agent pipeline runs end-to-end.

---

## Stage 4: Hub & MCP Tool Flows

**Goal:** Templates are publishable to hub, agents can call FlowR flows as tools via MCP.

### 4.1 Tool Flows as MCP

Agents call tools via MCP protocol. FlowR flows can be exposed as MCP tools.

- [ ] Create `a3t-tool-registry` flow: registers flows as MCP-callable tools
- [ ] Each tool flow gets a tool definition:
  ```yaml
  name: create-ticket
  description: "Create a support ticket"
  input_schema:
    type: object
    properties:
      title: { type: string }
      description: { type: string }
      priority: { type: string, enum: [low, medium, high] }
  ```
- [ ] Agent dispatch passes tool definitions to agent engine
- [ ] Agent engine calls tool via MCP -> FlowR executes the flow -> returns result

### 4.2 Hub: Template Publishing

- [ ] Template status lifecycle: draft -> submitted -> approved -> published -> deprecated
- [ ] UI: TemplatesList shows status badges
- [ ] UI: "Submit for Review" button on template
- [ ] Backend: template approval flow (admin/appsec approves)
- [ ] UI: Hub browse page — search templates by category, engine, vendor
- [ ] AgentTemplateEditor `mode="hub-preview"` — read-only mode for browsing hub

### 4.3 Template Versioning

- [ ] Templates are immutable once published
- [ ] New version = new template record with same name, higher version
- [ ] Agents pin to template@version
- [ ] UI: Version selector in agent config editor

**Usable at end of Stage 4:** Templates are publishable and versioned. Agents call flows as tools via MCP. Hub is browseable.

---

## Implementation Order

```
NOW
 |
 |-- Stage 1.1: Add bindings to template specs
 |-- Stage 1.2: AgentTemplateEditor mode="agent" support
 |-- Stage 1.3: Agent create flow (template picker -> editor)
 |-- Stage 1.4: Backend validation
 |
 |-- Stage 2.5: MCP server resource creation  <-- parallel with 2.1
 |-- Stage 2.1: Agent trigger auto-create
 |-- Stage 2.2: Agent dispatch flow
 |-- Stage 2.3: Default canvas auto-generate
 |-- Stage 2.4: UI deploy + chat
 |
 |-- Stage 3.1: Canvas data model
 |-- Stage 3.2: FlowEditorPane mode="canvas" support
 |-- Stage 3.3: Canvas deploy flow
 |-- Stage 3.4: Canvas health monitoring
 |
 |-- Stage 4.1: Tool flows as MCP
 |-- Stage 4.2: Hub template publishing
 |-- Stage 4.3: Template versioning
 |
LATER
```

Each stage is independently usable. No stage depends on a later stage being complete.

## Key Files

| File | Stage | Change |
|---|---|---|
| agent-template spec_yaml | 1.1 | Add bindings section |
| agent-engine spec_yaml | 1.1 | Add bindings section |
| AgentTemplateEditor.svelte | 1.2 | Add `mode` prop, field lock/unlock |
| AgentsList.svelte | 1.3 | Template picker, remove hardcoded defaults |
| AgentDetail.svelte | 1.3, 2.4 | Editor mode="agent", deploy button, chat tab |
| agents_crud.yaml | 1.4 | Validation against template bindings |
| a3t-agent-deploy (new flow) | 2.1 | Trigger creation + status update |
| a3t-agent-dispatch (new flow) | 2.2 | Load config, run engine |
| a3t-agent-deploy flow | 2.3 | Auto-create default canvas |
| ResourcesList.svelte | 2.5 | MCP server creation form |
| canvas schema | 3.1 | Structured spec with goal, status |
| FlowEditorPane.svelte | 3.2 | Add `mode` prop, canvas node registry |
| a3t-canvas-deploy (new flow) | 3.3 | Multi-agent deploy |
| a3t-canvas-health (new flow) | 3.4 | Health check + cordon |
