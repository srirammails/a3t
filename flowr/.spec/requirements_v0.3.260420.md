---
project: FlowR
version: "0.3"
schema_version: "1.0"
description: >
  FlowR is the Enterprise Edition (EE) of the A3T agent platform. It extends ClawdFlow CE
  with a web UI, static YAML agent loading, agent templates with parameter binding,
  optional Dragonfly/Iggy infrastructure, and future Kubernetes-native multi-agent
  orchestration via Agent Canvas CRDs. FlowR runs in CE mode by default (LanceDB,
  single process) and opts-in to EE features via CLI flags.

  The design principle is: prove the data model and runtime locally first, then wrap in
  CRDs. The agent YAML schema is the primary design artifact — it becomes the CRD spec
  later. The Admin API is the control plane — the K8s controller becomes another client.

phases:
  phase_1a:
    name: "CE Compatibility + UI"
    status: verified
    scope: "REQ-COMPAT-*"
    notes: "10/10 tests pass on both clawdflow and flowr binaries"
  phase_1b:
    name: "UI Layer"
    status: active
    scope: "REQ-UI-*"
  phase_2:
    name: "Agent YAML Schema + Static Loading"
    status: active
    scope: "REQ-SCHEMA-*, REQ-LOADER-*"
    notes: "Must be stable before Phase 5 (CRD). Schema changes are cheap now, expensive after CRD ships."
  phase_3:
    name: "Templates + Parameter Binding"
    status: planned
    scope: "REQ-TMPL-*"
  phase_4:
    name: "Admin API Completeness"
    status: planned
    scope: "REQ-API-*"
  phase_5:
    name: "Infrastructure Options (Dragonfly/Iggy)"
    status: planned
    scope: "REQ-INFRA-*"
  phase_6:
    name: "Agent Canvas CRD"
    status: planned
    scope: "REQ-CANVAS-*, REQ-TRAFFIC-*, REQ-DRIFT-*, REQ-SCALE-*"
    notes: "Only begin after Phase 2-4 are stable. CRD spec = proven YAML schema."
  phase_7:
    name: "Output Sinks"
    status: planned
    scope: "REQ-SINK-*"

glossary:
  CE: "Community Edition — ClawdFlow, open source, single process, LanceDB storage"
  EE: "Enterprise Edition — FlowR, commercial, extends CE with UI/Canvas/scaling"
  AgentCanvas: "K8s CRD for multi-agent orchestration (Phase 6)"
  ZeroClaw: "Embedded Rust agent runtime with 50+ LLM providers"
  ClawDB: "Agent memory database with 5 memory types via AQL"
  MCP: "Model Context Protocol for tool discovery and invocation"
  AQL: "Agent Query Language for ClawDB memory operations"
  baseFlow: "The default CNCF Serverless Workflow entry point for an agent"
  proceduralAction: "A named workflow bound to an agent, callable by the agent's LLM"
  aql_namespace: "Agent's identity in the ADB/ClawDB memory system — unique per agent instance"
  deploymentEnvelope: "A set of K8s manifests that deploy an agent (precursor to CRD)"
---

# FlowR EE — Requirements Specification v0.3

## Phase 1a: CE Compatibility (verified)

- id: REQ-COMPAT-001
  kind: compatibility
  priority: must
  statement: "FlowR EE builds on ClawdFlow CE crates without modification or forking."
  status: verified
  tags: ["compatibility", "phase-1a"]

- id: REQ-COMPAT-002
  kind: compatibility
  priority: must
  statement: "FlowR in CE mode behaves identically to ClawdFlow — all task types, agent modes, triggers, templates, observability."
  status: verified
  tags: ["compatibility", "phase-1a"]

- id: REQ-COMPAT-003
  kind: compatibility
  priority: must
  statement: "FlowR exposes the same MCP tools and HTTP endpoints as CE, plus additive EE endpoints."
  status: verified
  tags: ["compatibility", "phase-1a"]

- id: REQ-COMPAT-004
  kind: behavior
  priority: must
  statement: "FlowR binary accepts --ui flag for web dashboard. Without --ui, headless CE mode."
  status: verified
  tags: ["compatibility", "phase-1a"]

## Phase 1b: UI Layer (active)

- id: REQ-UI-001
  kind: behavior
  priority: must
  statement: "FlowR serves a web dashboard at /ui/ with overview of agents, triggers, templates, workflows, resources."
  status: proposed
  tags: ["ui", "phase-1b"]

- id: REQ-UI-002
  kind: behavior
  priority: must
  statement: "Agents page with mode, status, health metrics, conversation history."
  status: proposed
  tags: ["ui", "phase-1b"]

- id: REQ-UI-003
  kind: behavior
  priority: must
  statement: "Templates page with CRUD, versioning, activation."
  status: proposed
  tags: ["ui", "phase-1b"]

- id: REQ-UI-004
  kind: behavior
  priority: must
  statement: "Triggers page with CRUD, attach/detach, pause/enable, test fire."
  status: proposed
  tags: ["ui", "phase-1b"]

- id: REQ-UI-005
  kind: behavior
  priority: must
  statement: "Workflows page with CRUD, execute with JSON input, view results."
  status: proposed
  tags: ["ui", "phase-1b"]

- id: REQ-UI-006
  kind: behavior
  priority: must
  statement: "Resources page with CRUD for LLM, ClawDB, MCP, agent resources."
  status: proposed
  tags: ["ui", "phase-1b"]

- id: REQ-UI-007
  kind: constraint
  priority: must
  statement: "UI embedded in binary via include_str!. No npm, no CDN. Vanilla HTML + JS + CSS."
  status: proposed
  tags: ["ui", "phase-1b"]

- id: REQ-UI-008
  kind: behavior
  priority: should
  statement: "Input form generation from JSON Schema on agent templates."
  status: proposed
  tags: ["ui", "phase-1b"]

- id: REQ-UI-009
  kind: behavior
  priority: should
  statement: "Agent output rendering via HTML templates."
  status: proposed
  tags: ["ui", "phase-1b"]

## Phase 2: Agent YAML Schema + Static Loading (active)

- id: REQ-SCHEMA-001
  kind: schema
  priority: must
  statement: "An agent is defined by a YAML file with a stable schema covering: name, mode, description, triggers, baseFlow, proceduralActions, parameters, adb, and mode-specific configuration."
  rationale: "The YAML schema is the primary design artifact. It becomes the CRD spec later. Changes are cheap now, expensive after CRD ships."
  acceptance_criteria:
    - "A JSON Schema file defines the agent YAML format"
    - "The schema validates all four mode examples (ephemeral, session, persistent, reactive)"
    - "The schema rejects unknown fields and missing required fields"
  status: proposed
  tags: ["schema", "phase-2"]

- id: REQ-SCHEMA-002
  kind: schema
  priority: must
  statement: "Triggers are declared at the top level of the agent YAML, not nested under mode-specific blocks. Mode-specific config (inbox, session TTL) lives in its own block."
  rationale: "Consistent structure across modes. Avoids trigger duplication."
  acceptance_criteria:
    - "All four mode examples use the same top-level triggers: array"
    - "Reactive agents have a reactive: block for inbox config only"
    - "Session agents have a session: block for TTL and ID extraction only"
  status: proposed
  tags: ["schema", "phase-2"]

- id: REQ-SCHEMA-003
  kind: schema
  priority: must
  statement: "Procedural actions are named workflow references in the agent YAML. Each action specifies exactly one of: path (local file), ref (registered name), or inline (embedded CNCF SWF definition)."
  rationale: "Agents bind workflows as callable actions. Single-source constraint prevents ambiguity."
  acceptance_criteria:
    - "proceduralActions accepts path, ref, and inline forms"
    - "Validation rejects entries with zero or multiple source types"
    - "path references are resolved relative to the agent YAML file location"
  status: proposed
  tags: ["schema", "phase-2"]

- id: REQ-SCHEMA-004
  kind: schema
  priority: must
  statement: "Each agent has an aql_namespace that serves as its identity in ClawDB. The namespace must be unique within a runtime instance."
  rationale: "AQL namespace is how agents isolate their memory. Two agents sharing a namespace share state — this must be explicit, not accidental."
  acceptance_criteria:
    - "aql_namespace is a required field (or derived from agent name if omitted)"
    - "Runtime rejects two agents with the same aql_namespace unless explicitly configured"
  status: proposed
  tags: ["schema", "phase-2"]

- id: REQ-SCHEMA-005
  kind: schema
  priority: must
  statement: "Triggers support types: http, cron, iggy, broadcast, event. Each type has type-specific fields. Cron and event triggers may specify a flow field to invoke a specific procedural action instead of the baseFlow."
  rationale: "Reactive agents need multiple trigger types. Direct flow routing avoids base flow branching for scheduled tasks."
  acceptance_criteria:
    - "http triggers have path, port, timeout fields"
    - "cron triggers have schedule, optional flow reference, optional timezone"
    - "iggy triggers have address, topic, consumerGroup, concurrency"
    - "If trigger has no flow field, it invokes baseFlow"
    - "If trigger has flow field, it invokes that named procedural action directly"
  status: proposed
  tags: ["schema", "phase-2"]

- id: REQ-SCHEMA-006
  kind: schema
  priority: must
  statement: "Session mode configuration includes TTL and session ID extraction. Session ID can be extracted from header, body path, or jq expression."
  acceptance_criteria:
    - "session.ttl accepts duration strings (30m, 1h, 24h)"
    - "session.idExpression uses the jq expression engine for flexible extraction"
    - "If no idExpression, falls back to body.session_id"
  status: proposed
  tags: ["schema", "phase-2"]

- id: REQ-SCHEMA-007
  kind: schema
  priority: must
  statement: "Reactive mode configuration includes inbox capacity and overflow policy (drop-oldest, block, reject)."
  acceptance_criteria:
    - "reactive.inbox.capacity sets the mpsc channel buffer size"
    - "reactive.inbox.overflow controls behavior when capacity is exceeded"
  status: proposed
  tags: ["schema", "phase-2"]

- id: REQ-SCHEMA-008
  kind: schema
  priority: should
  statement: "Agent-level durability field sets the default FlowDurability for all flows executed by the agent. Individual flows may override."
  acceptance_criteria:
    - "durability: completion or durability: checkpoint at agent level"
    - "If not specified, flows use their own durability setting"
  status: proposed
  tags: ["schema", "phase-2"]

- id: REQ-SCHEMA-009
  kind: schema
  priority: must
  statement: "The agent YAML does NOT contain deployment concerns (replicas, resource limits, hosting). These belong in the deployment envelope (Phase 6 CRD). The YAML describes what the agent does, not how it's deployed."
  rationale: "Separation of agent logic from deployment. Same YAML runs locally and on K8s."
  acceptance_criteria:
    - "No fields for replicas, cpu, memory, nodeSelector in agent YAML"
    - "Agent YAML is valid and runnable with clawdflow run locally"
  status: proposed
  tags: ["schema", "phase-2"]

- id: REQ-SCHEMA-010
  kind: schema
  priority: should
  statement: "The adb block defaults to CLI flag or environment variable (CLAWDFLOW_ADB). Only specified in YAML when non-default."
  acceptance_criteria:
    - "Agent YAML without adb block uses the runtime default"
    - "clawdflow run --adb=localhost:6379 sets the default for all agents"
  status: proposed
  tags: ["schema", "phase-2"]

- id: REQ-LOADER-001
  kind: behavior
  priority: must
  statement: "clawdflow accepts a run subcommand that loads an agent from a YAML file, validates it, resolves all references, and instantiates the agent in the runtime."
  acceptance_criteria:
    - "clawdflow run ./my-agent.yaml starts the agent and blocks"
    - "Invalid YAML produces clear error with file path and line number"
    - "Missing flow file references fail at load time, not at invocation time"
  status: proposed
  tags: ["loader", "phase-2"]

- id: REQ-LOADER-002
  kind: behavior
  priority: must
  statement: "clawdflow accepts a validate subcommand that checks an agent YAML against the schema, resolves references, and reports errors without running the agent."
  acceptance_criteria:
    - "clawdflow validate ./my-agent.yaml exits 0 if valid, non-zero with errors if invalid"
    - "Reports missing referenced flow files"
    - "Reports schema violations (unknown fields, missing required fields, type mismatches)"
  status: proposed
  tags: ["loader", "phase-2"]

- id: REQ-LOADER-003
  kind: behavior
  priority: must
  statement: "Flow files referenced by path in proceduralActions are resolved relative to the agent YAML file's directory."
  acceptance_criteria:
    - "path: ./flows/score-page.yaml resolves relative to the agent YAML location"
    - "Absolute paths are supported"
    - "Missing files produce a clear error at validation/load time"
  status: proposed
  tags: ["loader", "phase-2"]

- id: REQ-LOADER-004
  kind: behavior
  priority: should
  statement: "Parameters declared in the agent YAML are injected into the flow execution context and accessible via jq expressions (${ .agent.parameters.<name> })."
  rationale: "Runtime parameter injection uses the existing jq expression engine. No new templating language."
  acceptance_criteria:
    - "A flow can reference ${ .agent.parameters.floor_multiplier } in expressions"
    - "Missing parameters produce clear errors"
  status: proposed
  tags: ["loader", "phase-2"]

## Phase 3: Templates + Parameter Binding (planned)

- id: REQ-TMPL-001
  kind: behavior
  priority: must
  statement: "An AgentTemplate is a reusable agent definition with typed parameter declarations. An agent YAML can reference a template and provide parameter values instead of defining everything inline."
  acceptance_criteria:
    - "Template YAML declares parameters with name, type, required, default, description"
    - "Agent YAML references template via template.path or template.ref"
    - "Parameters are validated against the template's parameter declarations"
  status: proposed
  tags: ["templates", "phase-3"]

- id: REQ-TMPL-002
  kind: behavior
  priority: must
  statement: "Template parameters that affect structural values (trigger topic names, AQL namespaces) are resolved at load time using jq expression substitution."
  rationale: "jq is already the expression engine. No second templating language (no Go templates, no Jinja). Structural values must be known at registration, not deferred to runtime."
  acceptance_criteria:
    - "topic: ${ .parameters.aql_namespace }-requests resolves at load time"
    - "Unresolvable expressions fail at load time with clear error"
  status: proposed
  tags: ["templates", "phase-3"]

- id: REQ-TMPL-003
  kind: behavior
  priority: must
  statement: "clawdflow run supports both inline agent YAML (all fields defined) and template-based agent YAML (template reference + parameters). The effective spec is the same in both cases."
  acceptance_criteria:
    - "Inline and template-based agents produce identical runtime behavior"
    - "clawdflow validate works on both forms"
  status: proposed
  tags: ["templates", "phase-3"]

## Phase 4: Admin API Completeness (planned)

- id: REQ-API-001
  kind: interface
  priority: must
  statement: "The Admin API is the single control plane for all agent, template, flow, and trigger operations. CLI and future K8s controller are both clients of this API."
  rationale: "One source of truth. Controller never pokes at internal state directly."
  acceptance_criteria:
    - "CLI commands map 1:1 to API calls"
    - "No runtime state mutation outside the API"
  status: proposed
  tags: ["api", "phase-4"]

- id: REQ-API-002
  kind: interface
  priority: must
  statement: "Agent lifecycle API: POST /agents (register from spec), GET /agents (list), GET /agents/{name} (detail + status), PATCH /agents/{name} (update), DELETE /agents/{name} (remove), POST /agents/{name}/pause, POST /agents/{name}/resume."
  acceptance_criteria:
    - "All endpoints return JSON"
    - "PATCH validates the updated spec before applying"
    - "DELETE cleans up agent state (stops inbox, removes from manager)"
    - "pause/resume stop/start trigger processing without destroying the agent"
  status: proposed
  tags: ["api", "phase-4"]

- id: REQ-API-003
  kind: interface
  priority: must
  statement: "Template rendering API: POST /templates/{name}/render accepts parameters and returns the effective agent spec without instantiating."
  rationale: "Allows preview and validation before creating an agent from a template."
  acceptance_criteria:
    - "Returns fully resolved agent spec JSON"
    - "Reports parameter validation errors"
  status: proposed
  tags: ["api", "phase-4"]

- id: REQ-API-004
  kind: interface
  priority: must
  statement: "Agent status API: GET /agents/{name}/status returns detailed runtime status including mode, uptime, turn count, last turn timestamp, memory health, tool count, trigger bindings."
  acceptance_criteria:
    - "Status includes all fields from AgentHealth struct"
    - "Status reflects current runtime state, not persisted state"
  status: proposed
  tags: ["api", "phase-4"]

- id: REQ-API-005
  kind: interface
  priority: must
  statement: "Agent events API: GET /agents/{name}/events returns lifecycle events (spawned, turn_started, turn_completed, turn_failed, stopped, trigger_fired)."
  acceptance_criteria:
    - "Events are ordered by timestamp"
    - "Events include agent_id, event_type, timestamp, optional details"
  status: proposed
  tags: ["api", "phase-4"]

## Phase 5: Infrastructure Options (planned)

- id: REQ-INFRA-001
  kind: behavior
  priority: must
  statement: "FlowR accepts --dragonfly <url> for state storage instead of LanceDB."
  status: planned
  tags: ["infrastructure", "phase-5"]

- id: REQ-INFRA-002
  kind: behavior
  priority: must
  statement: "FlowR accepts --iggy <url> for durable event streaming and checkpoints."
  status: planned
  tags: ["infrastructure", "phase-5"]

## Phase 6: Agent Canvas CRD (planned)

- id: REQ-CANVAS-001
  kind: behavior
  priority: must
  statement: "AgentCanvas K8s CRD spec is the proven agent YAML schema wrapped in apiVersion/kind/metadata/spec/status. No schema invention at CRD time."
  rationale: "CRD ossifies fast. Schema must be proven locally before committing to CRD."
  acceptance_criteria:
    - "CRD spec fields match the agent YAML schema from Phase 2"
    - "CRD status is populated by controller from GET /agents/{name}/status API"
  status: planned
  tags: ["canvas", "phase-6"]

- id: REQ-CANVAS-002
  kind: behavior
  priority: must
  statement: "K8s controller is a thin translation layer: watch Agent CRs → POST/PATCH/DELETE to Admin API. Controller does not mutate runtime state directly."
  rationale: "Admin API is the contract. Controller is ~500 lines of code."
  acceptance_criteria:
    - "Agent CR create → POST /agents with spec"
    - "Agent CR update → PATCH /agents/{name}"
    - "Agent CR delete → DELETE /agents/{name}"
    - "Controller periodically GET /agents/{name}/status and patches CR status subresource"
  status: planned
  tags: ["canvas", "phase-6"]

- id: REQ-CANVAS-003
  kind: behavior
  priority: must
  statement: "AgentCanvas CRD adds deployment concerns (replicas, trafficShare, resource limits) that are absent from the agent YAML schema."
  rationale: "Separation: agent YAML = what it does. Canvas CRD = how it's deployed."
  acceptance_criteria:
    - "Canvas spec includes replicas, trafficShare, resources, nodeSelector"
    - "These fields are consumed by the controller for pod management, not by the agent runtime"
  status: planned
  tags: ["canvas", "phase-6"]

- id: REQ-TRAFFIC-001
  kind: behavior
  priority: must
  statement: "Agent Canvas supports traffic splitting via trafficShare percentage."
  status: planned
  tags: ["traffic", "phase-6"]

- id: REQ-DRIFT-001
  kind: behavior
  priority: must
  statement: "Canvas controller detects and auto-reconciles drift."
  status: planned
  tags: ["drift", "phase-6"]

- id: REQ-SCALE-001
  kind: behavior
  priority: must
  statement: "Agent Canvas supports metrics-based auto-scaling with HPA."
  status: planned
  tags: ["scaling", "phase-6"]

## Phase 7: Output Sinks (planned)

- id: REQ-SINK-001
  kind: behavior
  priority: must
  statement: "FlowR supports output sinks: Kafka/Redpanda, webhook, S3, ClickHouse."
  status: planned
  tags: ["sinks", "phase-7"]

## Performance

- id: REQ-PERF-001
  kind: sla
  priority: must
  statement: "In CE mode, FlowR performance matches ClawdFlow CE benchmarks."
  status: proposed
  tags: ["performance"]

## Design Constraints (cross-cutting)

- id: REQ-DESIGN-001
  kind: constraint
  priority: must
  statement: "The jq expression engine (already implemented) is the only expression/templating language. No Go templates, no Jinja, no CUE. Parameters use jq for both runtime injection and load-time structural resolution."
  rationale: "One evaluation context. Users learn one language. The engine already exists and is proven."
  status: proposed
  tags: ["design"]

- id: REQ-DESIGN-002
  kind: constraint
  priority: must
  statement: "All references (flow files, templates, resources) are validated at load time. Missing references fail loudly with file path and error. No lazy resolution at invocation time."
  rationale: "Strict validation matches what a CRD admission controller will enforce later. Fail fast during development."
  status: proposed
  tags: ["design"]

- id: REQ-DESIGN-003
  kind: constraint
  priority: must
  statement: "Flow ownership: procedural actions are owned by the agent (deleted with it) unless marked as shared library refs."
  rationale: "Clear lifecycle. Agent removal doesn't orphan flows. Shared flows persist across agents."
  acceptance_criteria:
    - "path and inline flows are agent-owned"
    - "ref flows are shared libraries, not deleted with agent"
  status: proposed
  tags: ["design"]

- id: REQ-DESIGN-004
  kind: constraint
  priority: must
  statement: "The CLI and the future K8s controller only interact with the Admin API. Neither directly mutates storage, runtime state, or in-process agent objects."
  rationale: "Single source of truth. Prevents state divergence between CR and runtime."
  status: proposed
  tags: ["design"]
