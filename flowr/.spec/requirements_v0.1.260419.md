---
project: FlowR
version: "0.5"
schema_version: "1.0"
description: >
  FlowR is the Enterprise Edition (EE) of the A3T workflow runtime. It extends ClawdFlow CE
  with Kubernetes-native multi-agent orchestration via Agent Canvas CRDs, traffic management,
  drift detection, auto-scaling, UI layer, and output sinks. FlowR uses Iggy for durable
  event streaming, Dragonfly for hot state, and Kameo actors for suspension management.
  Durability is a per-workflow parameter (Completion vs Checkpoint), not architecture.

glossary:
  AgentCanvas: "Kubernetes Custom Resource Definition (CRD) for declarative multi-agent orchestration — defines triggers, agents, replicas, traffic shares, and output sinks"
  FlowDurability: "Per-workflow durability choice — Completion (Iggy append at end) or Checkpoint (Iggy sync write at marked steps)"
  Iggy: "Apache Iggy — high-performance message streaming used for audit logs, checkpoints, and lifecycle events"
  Dragonfly: "Redis-compatible in-memory store used for flow registry, hot state, signal bus, and suspended flows"
  Kameo: "Rust actor framework used for suspension management (Wait/Listen tasks)"
  CloudEvents: "CNCF standard event format used for workflow and task lifecycle events"
  Reconciliation: "K8s controller pattern — continuously diff desired state (spec) against actual state (status) and take corrective action"
  TrafficShare: "Percentage of incoming requests routed to a specific agent variant within an Agent Canvas"
  DriftDetection: "Comparison of desired Canvas spec against actual running state to detect and correct divergence"
  HPA: "Horizontal Pod Autoscaler — K8s native scaling mechanism based on metrics"
  kube-rs: "Rust Kubernetes client library used for CRD controllers and API interactions"
  OutputSink: "Destination for agent/workflow outputs — Kafka, webhook, S3, or ClickHouse"
---

# FlowR EE — Requirements Specification

## Core Runtime (inherited from CE shared crates)

- id: REQ-FLOWR-001
  kind: behavior
  priority: must
  statement: "FlowR implements all 22 CNCF Serverless Workflow task types via the shared flowr-tasks crate."
  rationale: "EE inherits the same task coverage as CE through shared crates."
  acceptance_criteria:
    - "All 22 task executors from flowr-tasks are available in the FlowR server"
    - "Task behavior is identical to CE for all shared task types"
  source: "architecture.md"
  status: verified
  tags: ["core", "tasks", "shared"]

- id: REQ-FLOWR-002
  kind: behavior
  priority: must
  statement: "FlowR supports per-workflow durability selection: Completion mode (Iggy append at workflow end) and Checkpoint mode (Iggy sync write at marked steps with recovery)."
  rationale: "Durability as a parameter is FlowR's core design innovation — low-latency flows use Completion, mission-critical flows use Checkpoint."
  acceptance_criteria:
    - "A workflow with durability=Completion only writes to Iggy at workflow completion"
    - "A workflow with durability=Checkpoint writes to Iggy at marked checkpoint steps"
    - "A checkpointed workflow can be resumed from its last checkpoint after crash recovery"
  source: "architecture.md"
  status: verified
  tags: ["core", "durability"]

- id: REQ-FLOWR-003
  kind: behavior
  priority: must
  statement: "The jq-based expression engine (flowr-expr) evaluates ${ ... } expressions with compiled filter caching and scope variable injection."
  acceptance_criteria:
    - "Expressions resolve correctly against scope variables ($runtime, $workflow, $task, $context)"
    - "Compiled filters are cached for reuse"
  source: "architecture.md"
  status: verified
  tags: ["core", "expressions", "shared"]

## Admin API

- id: REQ-API-001
  kind: interface
  priority: must
  statement: "FlowR exposes an Axum-based REST Admin API on port 8000 for workflow CRUD, execution, event querying, and health checks."
  acceptance_criteria:
    - "POST /flows registers a workflow definition"
    - "GET /flows lists all workflows"
    - "GET /flows/{ns}/{name} retrieves a workflow"
    - "PATCH /flows/{ns}/{name} toggles enabled/disabled or swaps active version"
    - "DELETE /flows/{ns}/{name}/{version} removes a version"
    - "POST /flows/{ns}/{name}/execute triggers workflow execution"
    - "GET /flows/{ns}/{name}/events returns workflow lifecycle events"
    - "GET /flows/{ns}/{name}/instances/{id}/events returns instance events"
    - "GET /health/ready and GET /health/live return health status"
  source: "architecture.md"
  status: verified
  tags: ["api", "admin"]

## Suspension

- id: REQ-SUSPEND-001
  kind: behavior
  priority: must
  statement: "FlowR supports actor-based flow suspension via Kameo for Wait (timer-based) and Listen (event-filter-based) tasks with automatic restart recovery from Dragonfly."
  acceptance_criteria:
    - "Wait task suspends execution and resumes after the specified duration"
    - "Listen task suspends execution and resumes when a matching event arrives"
    - "Suspended flows are persisted in Dragonfly and recovered on server restart"
    - "Each suspended flow has a Kameo actor managing its timer or event filter"
  source: "architecture.md"
  status: verified
  tags: ["suspension", "kameo"]

## Lifecycle Events

- id: REQ-EVENT-001
  kind: behavior
  priority: must
  statement: "FlowR emits CloudEvents for all workflow lifecycle transitions: started, suspended, resumed, cancelled, faulted, completed."
  acceptance_criteria:
    - "Each workflow lifecycle transition emits a CloudEvent to the Iggy flowr.events topic"
    - "Events follow the io.serverlessworkflow.workflow.*.v1 type format"
  source: "architecture.md"
  status: verified
  tags: ["events", "cloudevents"]

- id: REQ-EVENT-002
  kind: behavior
  priority: must
  statement: "FlowR emits CloudEvents for all task lifecycle transitions: started, suspended, resumed, retried, cancelled, faulted, completed."
  acceptance_criteria:
    - "Each task lifecycle transition emits a CloudEvent to the Iggy flowr.events topic"
    - "Events follow the io.serverlessworkflow.task.*.v1 type format"
  source: "architecture.md"
  status: verified
  tags: ["events", "cloudevents"]

## Infrastructure

- id: REQ-INFRA-001
  kind: constraint
  priority: must
  statement: "FlowR requires Apache Iggy (TCP port 8090, HTTP port 3000) for audit logging, checkpoints, and durability."
  acceptance_criteria:
    - "FlowR connects to Iggy on startup and creates required topics (flowr.events, flowr.checkpoints)"
    - "Checkpoint writes use Iggy sync mode for data safety"
  source: "architecture.md"
  status: verified
  tags: ["infrastructure", "iggy"]

- id: REQ-INFRA-002
  kind: constraint
  priority: must
  statement: "FlowR requires Dragonfly (Redis-compatible, port 6379) for flow registry, hot state, signal bus, and suspended flow storage."
  acceptance_criteria:
    - "Flow definitions are stored in Dragonfly with the key pattern flow:{ns}:{name}:*"
    - "Suspended flows are persisted in Dragonfly for crash recovery"
  source: "architecture.md"
  status: verified
  tags: ["infrastructure", "dragonfly"]

## Agent Canvas (K8s CRD)

- id: REQ-CANVAS-001
  kind: behavior
  priority: must
  statement: "FlowR defines an AgentCanvas Kubernetes CRD (apiVersion: workflow.a3t.io/v1alpha1) that declares triggers, agents with templates and replicas, traffic shares, and output sinks in a single resource."
  rationale: "Agent Canvas is the EE differentiator — declarative multi-agent orchestration on K8s."
  acceptance_criteria:
    - "AgentCanvas CRD is installable via kubectl apply"
    - "CRD schema validates triggers, agents (with ref, template, replicas, trafficShare), and outputs"
    - "kubectl get agentcanvas lists deployed canvases"
  source: "agent.md, epics.md EPIC 30.1"
  status: proposed
  tags: ["canvas", "crd", "epic-30"]

- id: REQ-CANVAS-002
  kind: behavior
  priority: must
  statement: "A kube-rs reconciliation controller watches AgentCanvas resources and continuously reconciles desired state (spec) against actual state (running pods, services, routes)."
  acceptance_criteria:
    - "Controller detects new AgentCanvas creation and provisions agent pods"
    - "Controller detects AgentCanvas updates and adjusts running state"
    - "Controller detects AgentCanvas deletion and cleans up all resources"
    - "Reconciliation loop runs continuously and corrects drift"
  source: "agent.md, epics.md EPIC 30.2"
  status: proposed
  tags: ["canvas", "controller", "epic-30"]

- id: REQ-CANVAS-003
  kind: behavior
  priority: must
  statement: "The Canvas controller manages agent pod lifecycle: creating pods from agent templates, scaling replicas up/down, and deleting pods on canvas removal."
  acceptance_criteria:
    - "Applying an AgentCanvas with replicas:3 creates 3 agent pods"
    - "Updating replicas from 3 to 5 creates 2 additional pods"
    - "Updating replicas from 5 to 2 terminates 3 pods"
    - "Deleting the AgentCanvas deletes all associated pods"
  source: "epics.md EPIC 30.3"
  status: proposed
  tags: ["canvas", "pods", "epic-30"]

- id: REQ-CANVAS-004
  kind: behavior
  priority: must
  statement: "The Canvas controller binds triggers defined in the Canvas spec to Traefik/Ingress routes, creating HTTP entry points for agent invocation."
  acceptance_criteria:
    - "An HTTPTrigger with path /bid creates a corresponding Ingress/IngressRoute"
    - "Incoming requests to the trigger path are routed to agent pods"
    - "Deleting the Canvas removes the associated routes"
  source: "epics.md EPIC 30.4"
  status: proposed
  tags: ["canvas", "triggers", "ingress", "epic-30"]

- id: REQ-CANVAS-005
  kind: behavior
  priority: must
  statement: "The Canvas controller binds output sinks (Kafka, webhook, S3) defined in the Canvas spec, routing agent outputs to the configured destinations."
  acceptance_criteria:
    - "A sink of type kafka://bids-topic produces agent outputs to the Kafka topic"
    - "Output binding is wired during reconciliation"
  source: "epics.md EPIC 30.5"
  depends_on: ["REQ-SINK-001"]
  status: proposed
  tags: ["canvas", "outputs", "epic-30"]

## Traffic Management

- id: REQ-TRAFFIC-001
  kind: behavior
  priority: must
  statement: "Agent Canvas supports traffic splitting via trafficShare percentage, distributing incoming requests across agent variants proportionally."
  acceptance_criteria:
    - "Two agents with trafficShare 0.99 and 0.01 receive ~99% and ~1% of traffic respectively"
    - "Traffic shares within a canvas must sum to 1.0"
  source: "agent.md, epics.md EPIC 31.1"
  status: proposed
  tags: ["traffic", "splitting", "epic-31"]

- id: REQ-TRAFFIC-002
  kind: behavior
  priority: should
  statement: "Agent Canvas supports canary deployments with gradual traffic rollout from a baseline to a canary agent variant."
  acceptance_criteria:
    - "A canary agent starts receiving a small percentage of traffic"
    - "Traffic share can be increased incrementally via Canvas spec updates"
    - "Canary can be rolled back by setting its traffic share to 0"
  source: "epics.md EPIC 31.2"
  status: proposed
  tags: ["traffic", "canary", "epic-31"]

- id: REQ-TRAFFIC-003
  kind: behavior
  priority: should
  statement: "Agent Canvas supports A/B testing via header-based or cookie-based routing to specific agent variants."
  acceptance_criteria:
    - "Requests with a specific header value are routed to a designated agent variant"
    - "Requests with a specific cookie are routed to a designated agent variant"
  source: "epics.md EPIC 31.3"
  status: proposed
  tags: ["traffic", "ab-testing", "epic-31"]

- id: REQ-TRAFFIC-004
  kind: behavior
  priority: should
  statement: "Agent Canvas supports blue/green deployment where traffic switches entirely from one version to another."
  acceptance_criteria:
    - "Two agent variants (blue, green) can be defined in the Canvas"
    - "Traffic can be switched from blue to green atomically via spec update"
  source: "epics.md EPIC 31.4"
  status: proposed
  tags: ["traffic", "blue-green", "epic-31"]

## Drift Detection & Reconciliation

- id: REQ-DRIFT-001
  kind: behavior
  priority: must
  statement: "The Canvas controller detects drift between the desired spec and actual running state (pod count, versions, traffic configuration) and logs discrepancies."
  acceptance_criteria:
    - "If a pod is manually deleted, the controller detects the drift within one reconciliation cycle"
    - "Drift is logged with details of what diverged (expected vs actual)"
  source: "epics.md EPIC 32.1"
  status: proposed
  tags: ["drift", "detection", "epic-32"]

- id: REQ-DRIFT-002
  kind: behavior
  priority: must
  statement: "The Canvas controller auto-reconciles drift by scaling pods, updating routes, and correcting configuration to match the desired spec."
  acceptance_criteria:
    - "If a pod is deleted, the controller recreates it to match desired replica count"
    - "If traffic routes diverge, the controller corrects them"
  source: "epics.md EPIC 32.2"
  status: proposed
  tags: ["drift", "reconciliation", "epic-32"]

- id: REQ-DRIFT-003
  kind: behavior
  priority: should
  statement: "Drift events trigger alerts via configurable notification channels."
  acceptance_criteria:
    - "When drift is detected, an alert event is emitted"
    - "Alert includes canvas name, drift type, and corrective action taken"
  source: "epics.md EPIC 32.3"
  status: proposed
  tags: ["drift", "alerting", "epic-32"]

- id: REQ-DRIFT-004
  kind: behavior
  priority: should
  statement: "The Canvas controller supports rollback to a previous known-good spec version."
  acceptance_criteria:
    - "Canvas maintains a history of applied specs"
    - "A rollback command restores the previous spec and reconciles"
  source: "epics.md EPIC 32.4"
  status: proposed
  tags: ["drift", "rollback", "epic-32"]

## Auto-Scaling

- id: REQ-SCALE-001
  kind: behavior
  priority: must
  statement: "Agent Canvas supports metrics-based auto-scaling using latency and queue depth as scaling signals."
  acceptance_criteria:
    - "When average request latency exceeds a threshold, replicas scale up"
    - "When queue depth exceeds a threshold, replicas scale up"
    - "When metrics return to normal, replicas scale down"
  source: "epics.md EPIC 33.1"
  status: proposed
  tags: ["scaling", "metrics", "epic-33"]

- id: REQ-SCALE-002
  kind: behavior
  priority: must
  statement: "Agent Canvas integrates with Kubernetes HPA for pod auto-scaling based on standard and custom metrics."
  acceptance_criteria:
    - "Canvas spec can define scaling min/max replicas"
    - "HPA is created with appropriate metrics targets"
  source: "epics.md EPIC 33.2"
  status: proposed
  tags: ["scaling", "hpa", "epic-33"]

- id: REQ-SCALE-003
  kind: behavior
  priority: should
  statement: "Agent Canvas supports scale-to-zero for idle agents, spinning up pods only when triggered."
  acceptance_criteria:
    - "An agent with no traffic for a configurable idle period scales to zero replicas"
    - "Incoming traffic triggers scale-up from zero"
    - "First request latency after scale-up is within acceptable cold-start bounds"
  source: "epics.md EPIC 33.3"
  status: proposed
  tags: ["scaling", "scale-to-zero", "epic-33"]

## UI Layer

- id: REQ-UI-001
  kind: behavior
  priority: must
  statement: "FlowR generates input forms from JSON Schema definitions attached to agent templates, providing structured user input for agent invocation."
  acceptance_criteria:
    - "An agent template with a JSON Schema input definition renders a form in the UI"
    - "Form submission sends validated input to the agent"
  source: "epics.md EPIC 34.1"
  status: proposed
  tags: ["ui", "input-forms", "epic-34"]

- id: REQ-UI-002
  kind: behavior
  priority: should
  statement: "FlowR renders agent outputs using HTML templates, allowing rich formatted display of agent results."
  acceptance_criteria:
    - "An agent template with an output HTML template renders the result using that template"
    - "Template variables are populated from agent output JSON"
  source: "epics.md EPIC 34.2"
  status: proposed
  tags: ["ui", "output", "epic-34"]

- id: REQ-UI-003
  kind: behavior
  priority: should
  statement: "FlowR provides an agent topology graph view showing the relationships between agents, triggers, workflows, and output sinks within a Canvas."
  acceptance_criteria:
    - "The UI renders a directed graph of Canvas components"
    - "Nodes represent agents, triggers, and sinks; edges represent data flow"
  source: "epics.md EPIC 34.3"
  status: proposed
  tags: ["ui", "topology", "epic-34"]

- id: REQ-UI-004
  kind: behavior
  priority: should
  statement: "FlowR provides a reconciliation dashboard showing Canvas desired state vs actual state with drift indicators."
  acceptance_criteria:
    - "Dashboard shows current replica count vs desired for each agent"
    - "Drift events are displayed with timestamps and resolution status"
  source: "epics.md EPIC 34.4"
  status: proposed
  tags: ["ui", "dashboard", "epic-34"]

- id: REQ-UI-005
  kind: behavior
  priority: may
  statement: "FlowR supports real-time log streaming from agent pods in the UI."
  acceptance_criteria:
    - "User can select an agent pod and view its logs in real-time"
    - "Log stream updates automatically without page refresh"
  source: "epics.md EPIC 34.5"
  status: proposed
  tags: ["ui", "logs", "epic-34"]

## Output Sinks

- id: REQ-SINK-001
  kind: behavior
  priority: must
  statement: "FlowR supports output sinks for routing agent and workflow outputs to external systems: Kafka/Redpanda, webhook, S3/object store, and ClickHouse."
  acceptance_criteria:
    - "A Kafka sink produces agent output to the configured topic"
    - "A webhook sink POSTs agent output to the configured URL"
    - "An S3 sink writes agent output to the configured bucket/path"
    - "A ClickHouse sink inserts agent output into the configured table"
  source: "epics.md EPIC 35"
  status: proposed
  tags: ["sinks", "output", "epic-35"]

- id: REQ-SINK-002
  kind: behavior
  priority: must
  statement: "Kafka/Redpanda output sink produces messages to a configured topic with agent output as the message payload."
  acceptance_criteria:
    - "Agent output is serialized as JSON and produced to the Kafka topic"
    - "Message includes metadata (agent_id, canvas, timestamp) in headers"
  source: "epics.md EPIC 35.1"
  status: proposed
  tags: ["sinks", "kafka", "epic-35"]

- id: REQ-SINK-003
  kind: behavior
  priority: must
  statement: "Webhook output sink forwards agent output as an HTTP POST to a configured external URL."
  acceptance_criteria:
    - "Agent output is POSTed as JSON to the webhook URL"
    - "HTTP response status is logged for observability"
  source: "epics.md EPIC 35.2"
  status: proposed
  tags: ["sinks", "webhook", "epic-35"]

- id: REQ-SINK-004
  kind: behavior
  priority: should
  statement: "S3/object store output sink writes agent output as JSON objects to a configured bucket and path prefix."
  acceptance_criteria:
    - "Agent output is written as a JSON file to S3 with a unique key"
    - "Key includes timestamp and agent_id for partitioning"
  source: "epics.md EPIC 35.3"
  status: proposed
  tags: ["sinks", "s3", "epic-35"]

- id: REQ-SINK-005
  kind: behavior
  priority: should
  statement: "ClickHouse output sink inserts agent output directly into a configured ClickHouse table."
  acceptance_criteria:
    - "Agent output fields are mapped to ClickHouse table columns"
    - "Inserts use the ClickHouse HTTP interface"
  source: "epics.md EPIC 35.4"
  status: proposed
  tags: ["sinks", "clickhouse", "epic-35"]

## Performance

- id: REQ-PERF-001
  kind: sla
  priority: must
  statement: "Iggy async write latency is under 10 microseconds for Completion-mode durability."
  acceptance_criteria:
    - "Benchmark shows p99 Iggy async append latency < 10μs"
  source: "architecture.md"
  status: verified
  tags: ["performance", "iggy"]

- id: REQ-PERF-002
  kind: sla
  priority: must
  statement: "Iggy sync write latency is under 5 milliseconds for Checkpoint-mode durability."
  acceptance_criteria:
    - "Benchmark shows p99 Iggy sync write latency < 5ms"
  source: "architecture.md"
  status: verified
  tags: ["performance", "iggy"]

- id: REQ-PERF-003
  kind: sla
  priority: must
  statement: "Dragonfly read latency is under 1 millisecond for flow registry lookups."
  acceptance_criteria:
    - "Benchmark shows p99 Dragonfly GET latency < 1ms"
  source: "architecture.md"
  status: verified
  tags: ["performance", "dragonfly"]

## Compatibility

- id: REQ-COMPAT-001
  kind: compatibility
  priority: must
  statement: "FlowR EE builds on the same shared crates as ClawdFlow CE (flowr-core, flowr-expr, flowr-types, flowr-storage, flowr-tasks, flowr-worker-core, flowr-mcp-core) without modification."
  rationale: "Shared crate compatibility ensures feature parity on the core engine and avoids code divergence."
  acceptance_criteria:
    - "FlowR EE compiles with the same shared crate versions as ClawdFlow CE"
    - "No patches or forks of shared crates exist in the EE codebase"
  source: "clawdflow_architecture.md, todo.md"
  status: verified
  tags: ["compatibility", "shared-crates"]

- id: REQ-COMPAT-002
  kind: compatibility
  priority: must
  statement: "Workflow definitions created for ClawdFlow CE execute identically on FlowR EE without modification."
  rationale: "Users should be able to migrate from CE to EE without rewriting workflows."
  acceptance_criteria:
    - "A workflow YAML that runs on ClawdFlow CE runs on FlowR EE with identical output"
    - "All 22 task types produce the same results on both editions"
  source: "architecture.md"
  status: proposed
  tags: ["compatibility", "migration"]
