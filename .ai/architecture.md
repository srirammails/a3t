# FlowR System Architecture

> CNCF Serverless Workflow Runtime in Rust

---

## Three Products

| Product | Role | What it knows about |
|---------|------|-------------------|
| **ClawdFlow** | Pure CNCF SWF runtime | Workflows, tasks, resources, state, triggers. No K8s, no agents. |
| **FlowR** | Runtime + K8s deployment | ClawdFlow + CRDs, controllers, scaling, routing, output sinks. |
| **A3T** | Agent template platform | Agent templates (CNCF workflows with lifecycle conventions), agent memory (ClawDB), template registry, UI. |

**Key rule:** Agent concepts NEVER leak into ClawdFlow. An "agent" is a CNCF workflow pattern defined in A3T. ClawdFlow executes it like any other workflow.

---

## Crate Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                         swf-parser                              │
│  Pure CNCF SWF DSL 1.0.3 parser. No execution. No runtime.     │
│  YAML/JSON → typed AST (Document, Task, Call, Extension, etc.) │
│  Validates structure. Detects DSL version.                      │
└───────────────────────────┬─────────────────────────────────────┘
                            │ WorkflowDefinition
                            ▼
┌─────────────────────────────────────────────────────────────────┐
│                        swf-runtime                              │
│  Pluggable execution engine. Traits + generic dispatch.         │
│  RuntimeProvider trait (per DSL version)                        │
│  TaskHandler trait (per task type)                              │
│  Data flow pipeline (input → validate → execute → output)      │
│  Extension dispatch, lifecycle events, flow directives          │
└───────────────────────────┬─────────────────────────────────────┘
                            │ RuntimeProvider trait
                            ▼
┌─────────────────────────────────────────────────────────────────┐
│                        flowr-core                               │
│  FlowR's implementation of RuntimeProvider for SWF 1.0.3       │
│  Task handlers: HTTP, MCP, Shell, Script, Workflow, Set, etc.  │
│  jq expression engine (flowr-expr)                             │
│  Inline scripts: Lua (~20μs), JavaScript (~100μs), Python      │
└───────────────────────────┬─────────────────────────────────────┘
                            │
              ┌─────────────┴──────────────┐
              ▼                            ▼
┌──────────────────────┐    ┌──────────────────────────┐
│     ClawdFlow CE     │    │       FlowR EE           │
│                      │    │                          │
│  LanceDB storage     │    │  K8s CRDs (AgentCanvas)  │
│  MCP server (HTTP)   │    │  Distributed workers     │
│  Docker deployment   │    │  Iggy + Dragonfly        │
│  call:llm, call:sql  │    │  Traffic management      │
│  call:clawdb, etc.   │    │  Auto-scaling            │
└──────────────────────┘    └──────────────────────────┘
              │
              ▼
┌──────────────────────────────────────────────────────┐
│                     A3T                              │
│  Agent templates (CNCF workflows with conventions)   │
│  10-point lifecycle via use.extensions               │
│  Agent memory (ClawDB: working/episodic/semantic/    │
│    procedural tiers)                                 │
│  Template registry (versioned, defaults, visibility) │
│  SvelteKit UI                                        │
│  Integration sub-flows                               │
└──────────────────────────────────────────────────────┘
```

---

## CNCF SWF DSL 1.0.3 — Full Spec

The runtime implements the complete CNCF Serverless Workflow DSL:

### Task Types

| Task | Purpose | Handler Status |
|------|---------|---------------|
| **Call** | Call services/functions | HTTP ✓, MCP ✓, gRPC stub, OpenAPI stub, AsyncAPI stub, A2A stub |
| **Do** | Sequential subtasks | ✓ |
| **Emit** | Emit CloudEvents | stub |
| **For** | Iterate collection (with `while`) | ✓ |
| **Fork** | Parallel execution | ✓ |
| **Listen** | Wait for events | stub |
| **Raise** | Raise error (RFC 7807) | ✓ |
| **Run** | Run processes | Shell ✓, Script ✓, Workflow ✓, Container stub |
| **Set** | Set data | ✓ |
| **Switch** | Conditional branching | ✓ |
| **Try** | Error handling with retry | ✓ |
| **Wait** | Timer pause | ✓ |

### DSL Features

| Feature | Status |
|---------|--------|
| `document` block (dsl version, namespace, name, version) | ✓ |
| `use.extensions` (before/after lifecycle hooks) | ✓ |
| `use.functions` (reusable task definitions) | ✓ |
| `use.secrets` ($secrets in expressions) | ✓ |
| `use.catalogs` (external function registry) | planned |
| `use.authentications` (Basic, Bearer, OAuth2, etc.) | partial |
| `input` / `output` / `export` (transform + schema validation) | ✓ |
| `if` condition on tasks | ✓ |
| `timeout` per-task and per-workflow | ✓ |
| `then` flow directive (continue, exit, end, goto) | ✓ |
| `schedule` (cron, every, after, on) | via trigger system |
| Runtime expressions (jq): $context, $input, $output, $task, $workflow, $runtime | ✓ |
| Lifecycle events (CloudEvents) | planned |
| JSON Schema validation at all data flow checkpoints | ✓ |

---

## YAML Format — CNCF DSL 1.0.3

All workflows use native CNCF syntax:

```yaml
document:
  dsl: '1.0.3'
  namespace: default
  name: example
  version: '1.0.0'

use:
  extensions:
    - logging:
        extend: all
        before:
          - log:
              call: http
              with:
                method: post
                endpoint: https://logs.example.com
                body:
                  message: "${ \"Executing \" + $task.reference }"

do:
  - fetchData:
      call: http
      with:
        method: get
        endpoint:
          uri: https://api.example.com/data
      output:
        as: "{data: .body}"

  - processData:
      set:
        result: "${ .data.value * 2 }"

  - saveResult:
      call: http
      with:
        method: post
        endpoint:
          uri: https://api.example.com/results
        body: "${ {value: .result} }"
```

---

## Expression Engine

Runtime: **jq** (default per CNCF spec)

Available arguments in expressions:

| Argument | Available In | Description |
|----------|-------------|-------------|
| `$context` | All except workflow input.from | Workflow context data |
| `$input` | Task definition, output.as, export.as | Task's transformed input |
| `$output` | export.as only | Task's transformed output |
| `$secrets` | input.from only | Workflow secrets |
| `$task` | Task scope | { name, reference, definition, input, output, startedAt } |
| `$workflow` | All | { id, definition, input, startedAt } |
| `$runtime` | All | { name, version, metadata } |
| `$authorization` | Task definition, output.as, export.as | Resolved auth |

---

## Data Flow (per CNCF spec)

```
Workflow Input
  → validate(input.schema)
  → transform(input.from)
  → $context = result

For each task:
  → evaluate(if) — skip if false
  → validate(input.schema)
  → transform(input.from) → $input
  → execute task
  → transform(output.as) → $output
  → validate(output.schema)
  → transform(export.as) → merge into $context
  → validate(export.schema)
  → apply flow directive (continue/exit/end/goto)

Workflow Output
  → transform(output.as)
  → validate(output.schema)
```

---

## Deployment

| Service | URL | Runtime |
|---------|-----|---------|
| ClawdFlow | `37.27.195.4:30080` | Docker on k3d |
| ClawDB | `37.27.195.4:6444` | Docker |

Build: `docker build -f clawdflow/Dockerfile -t ghcr.io/srirammails/clawdflow:latest .`

---

## Reference

- CNCF SWF Spec: `.ai/cncf-serverless-workflow-dsl.md`
- Runtime refactor todo: `.ai/cncf-runtime-todo.md`
- Agent template spec: `.ai/AGENT_TEMPLATE_SPEC.md`
- Agent migration (completed): `.ai/agent-cncf-todo.md`
