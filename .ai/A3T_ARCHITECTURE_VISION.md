# A3T Platform Architecture Vision

## Overview

A3T is a universal agent orchestration platform built on FlowR + ADB (distributed transactional in-memory store with LanceDB backing and iggy queues). It provides a marketplace (Hub) for agent templates, tenant-level agent configuration, and visual canvas-based multi-agent orchestration with full lifecycle management.

## Core Principle

**Template defines behavior. Agent binds resources. Canvas wires runtime.**

Three layers, three owners, three lifecycles — cleanly separated.

---

## Platform Layers

```
Hub (Marketplace)          Users (Config)           Business (Canvas)

┌──────────────┐     ┌──────────────────┐     ┌─────────────────────┐
│ ServiceNow   │     │ My Support Agent │     │ Customer Success    │
│  Template    │────>│  + my LLM        │────>│  Canvas             │
│              │     │  + my SNOW creds │     │                     │
├──────────────┤     ├──────────────────┤     │  [Triage]           │
│ Salesforce   │     │ My Sales Agent   │     │     ├──>[Support]   │
│  Template    │────>│  + my LLM        │────>│     └──>[Sales]     │
│              │     │  + my SF creds   │     │            └──>[CRM]│
├──────────────┤     ├──────────────────┤     └─────────────────────┘
│ CrewAI       │     │ My Research Team │          = one business goal
│  Template    │────>│  + my LLM        │──┐       "Automate CS"
└──────────────┘     └──────────────────┘  │
                                           │  ┌─────────────────────┐
                                           └─>│ DevOps Pipeline     │
                                              │  Canvas             │
                                              │  [Ops]──>[Jira]     │
                                              └─────────────────────┘
                                                = another goal
                                                "Automate DevOps"
```

### Layer 1: Agent Template (Hub/Marketplace)

A reusable, versioned, vendor-published blueprint. Defines the agent's lifecycle, hooks, extensions, tools, table schemas, and required bindings. Does NOT contain tenant-specific config.

```yaml
AgentTemplate:
  id: tpl-servicenow-incident
  vendor: servicenow
  version: 2.1.0
  category: itsm
  engine:
    type: flowr                    # or crewai, autogen, langgraph
    runtime: agent-engine
  description: "Handle ServiceNow incidents"
  required_bindings:               # what user MUST provide
    - name: llm
      type: llm
    - name: snow_instance
      type: credential
      schema: { instance_url: string, api_key: secret }
  optional_bindings:               # what user CAN override
    - name: system_prompt
      default: "You are an ITSM agent..."
    - name: max_iterations
      default: 10
  provided:                        # what template brings
    tools: [create_incident, update_incident, resolve_incident]
    flows: [incident-triage, incident-escalation]
    tables: [incident_cache, resolution_kb]
  status: [draft, submitted, approved, rejected, deprecated]
```

### Layer 2: Agent (User Configuration)

A template bound to tenant-specific resources. Self-sufficient — contains everything needed to run autonomously on any FlowR instance.

```yaml
Agent:
  id: agt-my-snow-agent
  template: tpl-servicenow-incident@2.1.0
  org_id: org-acme
  bindings:
    llm: a3t/claude-cli-max
    snow_instance:
      instance_url: acme.service-now.com
      api_key: $secret:acme-snow-key
  overrides:
    system_prompt: "You are Acme Corp's IT support..."
    max_iterations: 15
  status: [draft, configured, ready]
```

### Layer 3: Canvas (Business Goal)

Runtime orchestration of multiple agents. Defines wiring, routing, triggers, health checks, dependencies, restart policies, and circuit breakers. A canvas = one business outcome.

```yaml
Canvas:
  id: cvs-acme-customer-success
  org_id: org-acme
  name: "Acme Customer Success"
  goal: "Automate L1 support, escalate L2 to humans"

  agents:
    - ref: agt-acme-triage
      trigger: { http: /api/v1/support/intake }
    - ref: agt-my-snow-agent
    - ref: agt-my-sf-agent
    - ref: agt-acme-escalation
      trigger: { http: /api/v1/support/escalations }

  connections:
    - from: agt-acme-triage
      to: agt-my-snow-agent
      when: "${ .category == 'incident' }"
    - from: agt-acme-triage
      to: agt-my-sf-agent
      when: "${ .category == 'sales' }"
    - from: agt-my-snow-agent
      to: agt-acme-escalation
      when: "${ .confidence < 0.3 }"

  health:
    check_interval: 10s
    rules:
      - agent: agt-my-snow-agent
        probe:
          type: execution
          input: { message: "health check" }
          timeout: 5s
        unhealthy_threshold: 3

  dependencies:
    - agent: agt-my-snow-agent
      requires: [agt-acme-triage]
      on_dependency_down: cordon
    - agent: agt-acme-escalation
      requires: [agt-my-snow-agent]
      on_dependency_down: queue

  restart:
    - agent: agt-acme-triage
      policy: always
      backoff: exponential
      max_retries: 5
    - agent: agt-my-snow-agent
      policy: on_failure
      max_retries: 3
      on_max_retries: cordon

  circuits:
    - edge: agt-acme-triage -> agt-my-snow-agent
      error_threshold: 0.5
      window: 60s
      action: open
      fallback: agt-acme-escalation

  status: [draft, designed, deploy_requested, deployed, suspended]
```

### Default Canvas

Deploying a standalone agent without a canvas auto-creates an implicit single-agent canvas:

```
[trigger: /agent/{id}/chat] -> [Agent] -> [response]
```

Default: HTTP trigger, 256Mi/0.5 CPU, 1 replica, no dependencies.

---

## Multi-Engine Support

Templates abstract the runtime engine. A3T supports non-FlowR agents (CrewAI, AutoGen, LangGraph) through wrapper templates. Canvas doesn't care what engine runs the agent.

```
Hub Categories:
├── By Engine
│   ├── FlowR Native — best performance, full ADB integration
│   ├── CrewAI — bring existing CrewAI agents
│   ├── AutoGen — bring existing AutoGen teams
│   └── LangGraph — bring existing LangGraph graphs
```

### Engine Interface

Every template, regardless of engine, exposes:

```yaml
interface:
  input:    { message: string, session_id: string }
  output:   { content: string, tool_calls_made: int }
  bindings: [llm, memory, tools]
  health:   { endpoint: /health, method: execution }
```

### Engine Types

```yaml
# Native FlowR
engine:
  type: flowr
  runtime: agent-engine

# CrewAI wrapper
engine:
  type: crewai
  image: a3t/crewai-wrapper:1.2
  entrypoint: crew.kickoff

# AutoGen wrapper
engine:
  type: autogen
  image: a3t/autogen-wrapper:0.4
  entrypoint: groupchat.run

# LangGraph wrapper
engine:
  type: langgraph
  image: a3t/langgraph-wrapper:0.1
  entrypoint: graph.invoke
```

### Sidecar Model

Non-FlowR engines run as sidecars in the FlowR pod. The wrapper translates between FlowR's execution model and the external runtime. ADB is the universal data plane — all engines use it for memory, state, and provenance.

```
┌─── FlowR Pod ────────────────────────────────┐
│                                               │
│  FlowR Runtime (main container)               │
│  ┌─────────┐ ┌─────────┐ ┌─────────────────┐ │
│  │ Agent A  │ │ Agent B │ │ Agent C         │ │
│  │ (flowr)  │ │ (flowr) │ │ (crewai)        │ │
│  └─────────┘ └─────────┘ └────────┬────────┘ │
│                                    │ HTTP     │
│                              ┌─────v────────┐ │
│                              │ crewai-wrapper│ │
│                              │ (sidecar)     │ │
│                              └──────────────┘ │
│                                               │
│  ADB Layer (shared by all engines)            │
│  ┌──────────────────────────────────────────┐ │
│  │ iggy queues + LanceDB + memory           │ │
│  └──────────────────────────────────────────┘ │
└───────────────────────────────────────────────┘
```

---

## Infrastructure Model

All agents run inside FlowR+ADB containers. K8s manages FlowR instances, not individual agents.

```
Outer layer (k8s):   FlowR container replicas, resources, health, networking
Inner layer (FlowR): Agent lifecycle, routing, execution, provenance, data flow
```

### CRD Hierarchy

The CRD operator bridges k8s and FlowR — it translates between k8s primitives and FlowR API calls.

```
FlowRCluster          <- infra (pods, storage, networking) [k8s objects]
  ├── AgentTemplate   <- reusable blueprint                [FlowR API]
  ├── Agent           <- template + config                 [FlowR API]
  ├── Canvas          <- wiring + triggers + health        [FlowR API]
  └── Resource        <- LLM, DB, MCP bindings             [FlowR API]
```

Only FlowRCluster creates k8s objects. All other CRDs create FlowR objects via the FlowR API.

### Canvas State Machine

```
                    ┌──────────┐
          deploy    │          │  health check pass
       ┌───────────>│  READY   │<──────────┐
       │            │          │            │
       │            └────┬─────┘            │
       │                 │                  │
       │          health check fail         │
       │          (threshold hit)           │
       │                 │                  │
       │            ┌────v─────┐      restart succeeds
       │            │          │────────────┘
       │            │UNHEALTHY │
       │            │          │────────┐
       │            └────┬─────┘        │
       │                 │         max retries
       │          dependency down       │
       │                 │              │
       │            ┌────v─────┐   ┌────v─────┐
       │            │          │   │           │
       │            │ CORDONED │   │  FAILED   │
       │            │          │   │           │
       │            └──────────┘   └───────────┘
       │            stops accepting  manual intervention
       │            new requests     needed
       │                 │
       │          dependency recovers
       │                 │
       └─────────────────┘
```

---

## Persona Model (RBAC)

Three teams, three views into the same platform:

```
                Template  Agent  Canvas  Deploy  Monitor

Dev Team          RW       RW     RW      R       R
SRE Team          R        R      RW      RW      RW
AppSec Team       R+gate   R      R       R       R+audit
```

### Roles

```yaml
developer:
  templates:  [read, use]
  agents:     [create, read, update, delete]
  canvas:     [create, read, update, delete]
  deploy:     [read]
  monitor:    [read]

sre:
  templates:  [read]
  agents:     [read]
  canvas:     [read, update]        # health/restart policies only
  deploy:     [create, read, update, delete]
  monitor:    [read, acknowledge, silence]

appsec:
  templates:  [read, approve, reject]
  agents:     [read, audit]
  canvas:     [read]
  deploy:     [read]
  monitor:    [read, audit]
```

### Approval Pipeline

```
Template published to Hub
        │
   AppSec gate ──── reject ──> back to vendor
        │
   approved
        │
   Dev configures Agent
        │
   Dev draws Canvas
        │
   SRE reviews Canvas runtime config
        │
   SRE deploys
        │
   Monitor (all three teams see their view)
```

---

## Provenance

Every agent execution is traced across the multi-agent chain via a propagated `trace_id`:

```yaml
provenance:
  trace_id: "tr-abc123"
  entries:
    - agent: support-agent
      instance: "019f-..."
      pod: pod-1
      input_hash: "sha256:..."
      output_hash: "sha256:..."
      triggered_by: "http:/intake"

    - agent: ops-agent
      instance: "019f-..."
      pod: pod-2
      input_hash: "sha256:..."    # matches support-agent output_hash
      triggered_by: "agent:support-agent/019f-..."
```

---

## UI Channels: Console and Studio (2 Nov 2026)

A3T has two UI channels over the same platform and API. Full design:
**`A3T_STUDIO_ARCHITECTURE.md`**.

| Channel | For | What it is |
|---|---|---|
| **Console** | Everyone — business users, SRE, AppSec, developers without write access | Section pages: inventory, details, status, run history, chat. Read and light actions. No editors. |
| **Studio** | Developers with write access (read-only for those without) | An IDE: Explorer of flows and resources, tabbed documents, panels. Author with AI, compose flows and resources into one deployable workflow, attach to a canvas and deploy, see runtimes, deployments, runs, timelines and checkpoints, connect a dev runtime and attach to running flows (logs, checkpoints), query databases, chat with LLMs, call MCP tools, fire triggers, AQL over agent/flow memory by tier. |

The Studio runs in two hosts from one codebase — the web dash and a **VS Code
extension** — so its model (`studio-core`: items, kinds, workspace, adapters) is
plain TypeScript with no UI or host dependencies, its components (`studio-ui`)
make no network calls, and host differences sit behind one `StudioHost`
interface.

How this maps onto the persona model above: Dev Team works in the Studio; SRE uses
the Console and the Studio's canvas, runtime and run documents; AppSec uses the
Console (policies, audit, approvals). Every Console detail offers *Open in Studio*
where the user may edit.

The screens sketch below is the original (June 2026) section list; the Console is
its current form, and the editors it describes are Studio documents.

## UX Screens

```
┌─────────────────────────────────────────────────┐
│  A3T Platform                                   │
├──────┬──────────┬────────┬─────────┬────────────┤
│ Hub  │ Agents   │ Canvas │ Deploy  │ Monitor    │
├──────┴──────────┴────────┴─────────┴────────────┤
│                                                  │
│  Hub:     Browse/search templates, install       │
│  Agents:  Configure installed templates          │
│  Canvas:  Wire agents, draw connections          │
│  Deploy:  Canvas -> running on FlowR             │
│  Monitor: Health, provenance, logs, metrics      │
│                                                  │
└──────────────────────────────────────────────────┘
```

### UX Reuse: Two Editors, Four Modes

Two core editor components serve all editing use cases via a `mode` prop.

#### Editor 1: AgentTemplateEditor (Template + Agent Config)

Same component, different field editability per mode:

```
AgentTemplateEditor.svelte
  mode="template"                mode="agent"
  ─────────────────              ─────────────────
  Name:    [editable]            Name:   [editable]
  Engine:  [editable]            Engine: [locked]
  Hooks:   [editable]            Hooks:  [locked]
  Policies:[editable]            Policies:[locked]

  ── Bindings ──                 ── Bindings ──
  LLM:     [define schema]       LLM:   [pick resource]
  Memory:  [define schema]       Memory:[pick resource]
  Tools:   [define list]         Tools: [select subset]
  Prompt:  [set default]         Prompt:[override]
  MaxIter: [set default]         MaxIter:[override]

  [Save Template]                [Save Agent]
```

Field editability rules:

```
Field            template    agent
─────────────    ────────    ─────────
name             editable    editable
engine           editable    locked
hooks            editable    locked
extensions       editable    locked
policies         editable    locked
llm              editable    editable (resource picker)
system_prompt    editable    editable (override)
memory           editable    editable (resource picker)
tools            editable    editable (select subset)
mcp_servers      editable    editable (resource picker)
max_iterations   editable    editable (override)
temperature      editable    editable (override)
```

Locked fields show template value with lock icon and tooltip "Defined by template".
Editable fields in agent mode show template default as placeholder.

#### Editor 2: FlowEditorPane (Flow Designer + Canvas Designer)

Same xyflow canvas, different node registry and validation per mode:

```
FlowEditorPane.svelte
  mode="flow"                    mode="canvas"
  ─────────────────              ─────────────────
  Node types:                    Node types:
   StepNode (call:*)              AgentNode
   ControlNode (switch/for)       TriggerNode
   FlowMarkerNode (start/end)     HealthPolicyNode
   SubFlowNode                    CircuitBreakerNode
   JoinNode

  Edges:                         Edges:
   Control flow (then: target)    Data flow (output -> input)
                                  Dependency (dashed, lifecycle)

  Palette:                       Palette:
   call:http, call:lancedb        Agent (from agent list)
   switch, for, try               Trigger (http/cron/event)
   set, emit                      Health policy
                                  Circuit breaker

  Validates: CNCF SWF spec      Validates: Canvas CRD spec
  Outputs:  workflow YAML        Outputs:  canvas YAML
```

Shared between both modes:
- Pan/zoom, grid, minimap
- Node drag/drop from palette
- Edge drawing (click source -> click target)
- Property panel (click node -> edit props in sidebar)
- YAML preview tab
- Undo/redo

#### Component Summary

| Component | Reused As | Mode Prop |
|---|---|---|
| AgentTemplateEditor.svelte | Template editor, Agent config editor | `mode: 'template' \| 'agent'` |
| FlowEditorPane.svelte | Flow designer, Canvas designer | `mode: 'flow' \| 'canvas'` |
| EditorShell.svelte | Shared chrome for both editors | Shared toolbar, panels, save |

This cuts Stage 1 (Agent Config) and Stage 3 (Canvas Design) effort roughly in half.

---

## Analogies

| A3T | Kubernetes | Salesforce | Helm |
|---|---|---|---|
| Hub Template | Container Image | AppExchange Package | Helm Chart |
| Agent | Pod Spec | Installed App + Config | Helm Release |
| Canvas | Deployment + Service + HPA | Flow/Process Builder | Application (ArgoCD) |
| Engine Interface | Container Runtime Interface | - | - |
| ADB | etcd + shared volumes | Platform data layer | - |
