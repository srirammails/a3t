---
project: FlowR
version: "0.2"
schema_version: "1.0"
description: >
  FlowR is the Enterprise Edition (EE) of the A3T agent platform. It extends ClawdFlow CE
  with a web UI, optional Dragonfly/Iggy infrastructure, and future Kubernetes-native
  multi-agent orchestration via Agent Canvas CRDs. FlowR runs in CE mode by default
  (LanceDB, single process) and opts-in to EE features via CLI flags.

phases:
  phase_1:
    name: "CE Compatibility + UI"
    status: active
    scope: "REQ-COMPAT-*, REQ-UI-*"
  phase_2:
    name: "Infrastructure Options"
    status: planned
    scope: "REQ-INFRA-*"
  phase_3:
    name: "Agent Canvas + Traffic + Scaling"
    status: planned
    scope: "REQ-CANVAS-*, REQ-TRAFFIC-*, REQ-DRIFT-*, REQ-SCALE-*"
  phase_4:
    name: "Output Sinks"
    status: planned
    scope: "REQ-SINK-*"

glossary:
  CE: "Community Edition — ClawdFlow, open source, single process, LanceDB storage"
  EE: "Enterprise Edition — FlowR, commercial, extends CE with UI/Canvas/scaling"
  AgentCanvas: "K8s CRD for multi-agent orchestration (Phase 3)"
  ZeroClaw: "Embedded Rust agent runtime with 50+ LLM providers"
  ClawDB: "Agent memory database with 5 memory types via AQL"
  MCP: "Model Context Protocol for tool discovery and invocation"
---

# FlowR EE — Requirements Specification v0.2

## Phase 1: CE Compatibility (active)

- id: REQ-COMPAT-001
  kind: compatibility
  priority: must
  statement: "FlowR EE builds on ClawdFlow CE crates (clawdflow-core, clawdflow-storage, clawdflow-agents, clawdflow-tasks) without modification or forking."
  rationale: "CE crates are the foundation — EE adds features on top, never patches CE."
  acceptance_criteria:
    - "FlowR Cargo.toml depends on clawdflow-* crates via path or git"
    - "No patched or forked versions of CE crates exist in the EE repo"
    - "FlowR compiles with the same CE crate versions as ClawdFlow"
  status: proposed
  tags: ["compatibility", "phase-1"]

- id: REQ-COMPAT-002
  kind: compatibility
  priority: must
  statement: "FlowR in CE mode (default, no --dragonfly/--iggy flags) behaves identically to ClawdFlow — LanceDB storage, single process, MCP protocol, all agent modes."
  acceptance_criteria:
    - "flowr --mode http --port 3002 starts an identical server to clawdflow"
    - "All 22 task types produce identical results"
    - "All 4 agent modes work (ephemeral, session, persistent, reactive)"
    - "Triggers, templates, observability endpoints all function"
    - "Workflow YAML from ClawdFlow runs on FlowR without modification"
  status: proposed
  tags: ["compatibility", "phase-1"]

- id: REQ-COMPAT-003
  kind: compatibility
  priority: must
  statement: "FlowR exposes the same MCP tools and HTTP API endpoints as ClawdFlow CE, plus additional EE endpoints under /ui/* namespace."
  acceptance_criteria:
    - "All 16 MCP tools from CE are available"
    - "GET /agents, /triggers, /templates, /webhook endpoints work"
    - "EE endpoints are additive, never replace CE endpoints"
  status: proposed
  tags: ["compatibility", "phase-1"]

- id: REQ-COMPAT-004
  kind: behavior
  priority: must
  statement: "FlowR binary accepts --ui flag to enable the web dashboard. Without --ui, FlowR runs in headless CE mode."
  acceptance_criteria:
    - "flowr --mode http --port 3002 runs without UI (CE mode)"
    - "flowr --mode http --port 3002 --ui serves UI at /ui/*"
    - "UI flag does not affect API or MCP functionality"
  status: proposed
  tags: ["compatibility", "ui", "phase-1"]

## Phase 1: UI Layer (active)

- id: REQ-UI-001
  kind: behavior
  priority: must
  statement: "FlowR serves a web dashboard at /ui/ that provides an overview of live agents, triggers, templates, workflows, and resources."
  acceptance_criteria:
    - "GET /ui/ returns an HTML dashboard page"
    - "Dashboard shows counts of agents, triggers, templates, workflows"
    - "Dashboard auto-refreshes data periodically"
  status: proposed
  tags: ["ui", "dashboard", "phase-1"]

- id: REQ-UI-002
  kind: behavior
  priority: must
  statement: "The UI provides an Agents page showing all live agents with their mode, status, health metrics, and conversation history."
  acceptance_criteria:
    - "Page lists all agents from GET /agents"
    - "Each agent shows mode, status, tools count, turn count"
    - "Clicking an agent shows its conversation history from GET /agents/:id/history"
    - "Agent health metrics (turns, avg latency, success rate) are displayed"
  status: proposed
  tags: ["ui", "agents", "phase-1"]

- id: REQ-UI-003
  kind: behavior
  priority: must
  statement: "The UI provides a Templates page for creating, viewing, versioning, and activating agent templates."
  acceptance_criteria:
    - "Page lists all templates from GET /templates"
    - "Shows versions and active version for each template"
    - "User can create a new template via form or YAML editor"
    - "User can activate a specific version"
  status: proposed
  tags: ["ui", "templates", "phase-1"]

- id: REQ-UI-004
  kind: behavior
  priority: must
  statement: "The UI provides a Triggers page for creating, viewing, attaching/detaching targets, pausing/enabling, and testing triggers."
  acceptance_criteria:
    - "Page lists all triggers from GET /triggers"
    - "Shows trigger type (webhook/cron/event), targets, enabled status"
    - "User can create a new trigger via form"
    - "User can attach/detach agent or workflow targets"
    - "User can fire a test webhook from the UI"
  status: proposed
  tags: ["ui", "triggers", "phase-1"]

- id: REQ-UI-005
  kind: behavior
  priority: must
  statement: "The UI provides a Workflows page for listing, creating, executing, and viewing execution results."
  acceptance_criteria:
    - "Page lists all workflows from MCP list_flows"
    - "User can create a workflow via YAML editor"
    - "User can execute a workflow with JSON input"
    - "Execution status and output are displayed"
  status: proposed
  tags: ["ui", "workflows", "phase-1"]

- id: REQ-UI-006
  kind: behavior
  priority: must
  statement: "The UI provides a Resources page for managing LLM, ClawDB, MCP, and agent resource configurations."
  acceptance_criteria:
    - "Page lists all resources from MCP list_resources"
    - "User can create a new resource via form"
    - "User can view resource configuration details"
    - "User can delete a resource"
  status: proposed
  tags: ["ui", "resources", "phase-1"]

- id: REQ-UI-007
  kind: constraint
  priority: must
  statement: "The UI is embedded in the FlowR binary as static assets (no external build step, no npm, no CDN dependencies). Uses vanilla HTML + htmx + minimal CSS."
  rationale: "Single binary deployment — no separate frontend build or static file server needed."
  acceptance_criteria:
    - "UI assets are compiled into the binary via include_str! or rust-embed"
    - "No npm, webpack, or frontend build toolchain required"
    - "UI works offline (no CDN dependencies)"
  status: proposed
  tags: ["ui", "deployment", "phase-1"]

- id: REQ-UI-008
  kind: behavior
  priority: should
  statement: "The UI generates input forms from JSON Schema definitions attached to agent templates, providing structured user input for agent invocation."
  acceptance_criteria:
    - "An agent template with a JSON Schema input definition renders a form in the UI"
    - "Form submission sends validated input to the agent"
  status: proposed
  tags: ["ui", "input-forms", "phase-1"]

- id: REQ-UI-009
  kind: behavior
  priority: should
  statement: "The UI renders agent outputs using HTML templates, allowing rich formatted display of agent results."
  acceptance_criteria:
    - "An agent template with an output_template renders the result using that template"
    - "Template variables are populated from agent output JSON"
  status: proposed
  tags: ["ui", "output", "phase-1"]

## Phase 2: Infrastructure Options (planned)

- id: REQ-INFRA-001
  kind: behavior
  priority: must
  statement: "FlowR accepts --dragonfly <url> flag to use Dragonfly for state storage instead of LanceDB."
  acceptance_criteria:
    - "With --dragonfly, flow registry uses Dragonfly key-value store"
    - "Without --dragonfly, default LanceDB storage is used"
  status: planned
  tags: ["infrastructure", "dragonfly", "phase-2"]

- id: REQ-INFRA-002
  kind: behavior
  priority: must
  statement: "FlowR accepts --iggy <url> flag to use Apache Iggy for durable event streaming and checkpoints."
  acceptance_criteria:
    - "With --iggy, lifecycle events stream to Iggy topics"
    - "With --iggy, checkpoint durability uses Iggy sync writes"
    - "Without --iggy, events and checkpoints use LanceDB"
  status: planned
  tags: ["infrastructure", "iggy", "phase-2"]

## Phase 3: Agent Canvas (planned)

- id: REQ-CANVAS-001
  kind: behavior
  priority: must
  statement: "FlowR defines an AgentCanvas K8s CRD for declarative multi-agent orchestration."
  status: planned
  tags: ["canvas", "phase-3"]

- id: REQ-CANVAS-002
  kind: behavior
  priority: must
  statement: "A kube-rs reconciliation controller watches AgentCanvas resources and manages agent pods."
  status: planned
  tags: ["canvas", "phase-3"]

- id: REQ-TRAFFIC-001
  kind: behavior
  priority: must
  statement: "Agent Canvas supports traffic splitting via trafficShare percentage."
  status: planned
  tags: ["traffic", "phase-3"]

- id: REQ-DRIFT-001
  kind: behavior
  priority: must
  statement: "Canvas controller detects and auto-reconciles drift between spec and running state."
  status: planned
  tags: ["drift", "phase-3"]

- id: REQ-SCALE-001
  kind: behavior
  priority: must
  statement: "Agent Canvas supports metrics-based auto-scaling with HPA integration."
  status: planned
  tags: ["scaling", "phase-3"]

## Phase 4: Output Sinks (planned)

- id: REQ-SINK-001
  kind: behavior
  priority: must
  statement: "FlowR supports output sinks: Kafka/Redpanda, webhook, S3, ClickHouse."
  status: planned
  tags: ["sinks", "phase-4"]

## Performance

- id: REQ-PERF-001
  kind: sla
  priority: must
  statement: "In CE mode, FlowR performance matches ClawdFlow CE benchmarks."
  acceptance_criteria:
    - "LanceDB read/write latency identical to ClawdFlow"
    - "Agent turn latency identical to ClawdFlow"
  status: proposed
  tags: ["performance", "phase-1"]
