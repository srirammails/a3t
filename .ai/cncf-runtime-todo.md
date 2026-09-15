# CNCF SWF Runtime Refactor — Full Spec Compliance

**Goal:** Refactor ClawdFlow into a proper CNCF Serverless Workflow 1.0.3 runtime with:
1. `swf-parser` — standalone DSL parser crate (no execution logic)
2. `swf-runtime` — pluggable execution engine trait (version-selectable)
3. `flowr-core` — FlowR's runtime implementation of the traits
4. All YAML files in native CNCF DSL syntax (do/call/with, not steps/type/config)

**Non-goal:** FlowR EE (K8s CRDs, scaling) — that layer sits above and is unaffected.

**Estimated effort:** ~3 weeks

---

## Architecture

```
YAML/JSON
    │
    ▼
┌──────────────┐     ┌───────────────┐     ┌──────────────┐
│  swf-parser  │────▶│  swf-runtime  │────▶│  flowr-core  │
│              │     │               │     │              │
│  Pure parse  │     │  Traits +     │     │  Implements  │
│  CNCF types  │     │  dispatch     │     │  handlers    │
│  Validation  │     │  Version reg  │     │  Storage     │
│  No execution│     │  Data flow    │     │  Expression  │
└──────────────┘     └───────────────┘     └──────────────┘
```

---

## Phase 1: `swf-parser` Crate — DONE

Pure CNCF SWF DSL 1.0.3 parser. Zero execution logic. Zero runtime dependencies.
Only depends on `serde`, `serde_json`, `serde_yaml`.

### 1.1 Document Model

```
swf-parser/src/
├── lib.rs              — pub mod + parse() entry point
├── document.rs         — Document { dsl, namespace, name, version }
├── workflow.rs         — WorkflowDefinition { document, use, do, input, output, schedule, timeout }
├── task.rs             — NamedTask, Task enum
├── call.rs             — CallTask, CallTarget { Http, Grpc, OpenApi, AsyncApi, A2A, Mcp, Function }
├── call_http.rs        — HttpCall { method, endpoint, headers, body }
├── call_grpc.rs        — GrpcCall { proto, service, method }
├── call_openapi.rs     — OpenApiCall { document, operation }
├── call_asyncapi.rs    — AsyncApiCall { document, channel, message }
├── call_a2a.rs         — A2ACall { agent, skill }
├── call_mcp.rs         — McpCall { server, tool, arguments }
├── run.rs              — RunTask { Container, Shell, Script, Workflow }
├── flow_control.rs     — DoTask, ForTask, ForkTask, SwitchTask, TryTask
├── emit.rs             — EmitTask { event }
├── listen.rs           — ListenTask { to }
├── raise.rs            — RaiseTask { error }
├── set.rs              — SetTask (Map<String, Value>)
├── wait.rs             — WaitTask { duration }
├── event.rs            — Event, EventFilter, EventProperties, EventConsumptionStrategy
├── extension.rs        — Extension { extend, when, before, after }
├── resource.rs         — ExternalResource, Endpoint, URITemplate
├── auth.rs             — Authentication { Basic, Bearer, Certificate, Digest, OAuth2, OIDC }
├── catalog.rs          — Catalog { endpoint }, function versioning
├── error.rs            — Error (RFC 7807), StandardErrorTypes
├── retry.rs            — Retry { delay, backoff, limit }
├── timeout.rs          — Timeout, Duration (ISO 8601)
├── schedule.rs         — Schedule { every, cron, after, on }
├── schema.rs           — Input { from, schema }, Output { as, schema }, Export { as, schema }
├── directive.rs        — FlowDirective { continue, exit, end, goto }
├── lifecycle.rs        — WorkflowLifecycleEvent, TaskLifecycleEvent (CloudEvents)
├── use_block.rs        — UseBlock { extensions, functions, secrets, retries, errors, timeouts, catalogs, authentications }
├── parser.rs           — parse_yaml(), parse_json(), detect version, validate
└── validation.rs       — Required field checks, ref validation, schema validation
```

### 1.2 Stories

- [x] 1.2.1 — `Document` struct + `WorkflowDefinition` top-level parsing
- [x] 1.2.2 — `Task` enum with all 12 task types (Call, Do, Emit, For, Fork, Listen, Raise, Run, Set, Switch, Try, Wait)
- [x] 1.2.3 — `CallTask` with all 7 targets (HTTP, gRPC, OpenAPI, AsyncAPI, A2A, MCP, Function)
- [x] 1.2.4 — `RunTask` with all 4 processes (Container, Shell, Script, Workflow)
- [x] 1.2.5 — Flow control types (Do, For, Fork, Switch, Try) with nested task parsing
- [x] 1.2.6 — `UseBlock` (extensions, functions, secrets, retries, errors, timeouts, catalogs, authentications)
- [x] 1.2.7 — `Extension` with before/after/when/extend
- [x] 1.2.8 — `Authentication` (all 6 types: Basic, Bearer, Certificate, Digest, OAuth2, OIDC)
- [x] 1.2.9 — `ExternalResource`, `Endpoint`, `URITemplate`
- [x] 1.2.10 — `Catalog` with function versioning and resolution
- [x] 1.2.11 — `Event`, `EventFilter`, `EventConsumptionStrategy`, `Emit`, `Listen`
- [x] 1.2.12 — `Error` (RFC 7807), `StandardErrorTypes`, `Retry` with backoff
- [x] 1.2.13 — `Schedule` (every, cron, after, on)
- [x] 1.2.14 — `Input`/`Output`/`Export` with `from`/`as`/`schema`
- [x] 1.2.15 — `Timeout`, `Duration` (ISO 8601 parsing)
- [x] 1.2.16 — `FlowDirective` (continue, exit, end, goto)
- [x] 1.2.17 — `LifecycleEvent` types (Workflow + Task, CloudEvents format)
- [x] 1.2.18 — `parse_yaml()` / `parse_json()` entry points with version detection
- [x] 1.2.19 — Validation: required fields, circular refs, schema checks
- [x] 1.2.20 — Unit tests: parse all CNCF spec examples from the DSL reference

---

## Phase 2: `swf-runtime` Crate — DONE

Execution engine traits and generic dispatch. No task implementations — those go in `flowr-core`.

### 2.1 File Structure

```
swf-runtime/src/
├── lib.rs
├── traits.rs           — RuntimeProvider, TaskHandler traits
├── registry.rs         — RuntimeRegistry (version → provider)
├── executor.rs         — Generic executor: workflow → tasks → handlers
├── context.rs          — ExecutionContext, TaskContext
│                         RuntimeExpressionArgs: $context, $input, $output,
│                         $task, $workflow, $runtime, $secrets, $authorization
├── data_flow.rs        — Input validation/transform, Output transform/validate,
│                         Export transform/validate, Context merge
├── events.rs           — Lifecycle event emission (CloudEvents wire format)
├── error.rs            — RuntimeError (maps to CNCF error types)
└── scope.rs            — Expression scope builder (jq variables)
```

### 2.2 Stories

- [x] 2.2.1 — `RuntimeProvider` trait: `supported_versions()`, `execute()`
- [x] 2.2.2 — `TaskHandler` trait: `execute(task, ctx) → TaskResult`
- [x] 2.2.3 — `RuntimeRegistry`: version-based provider selection
- [x] 2.2.4 — `ExecutionContext`: workflow metadata, storage, HTTP client, expression engine
- [x] 2.2.5 — `TaskContext`: input, scope, position, instance_id, runtime args
- [x] 2.2.6 — Data flow pipeline: input.from → validate → execute → output.as → validate → export.as → validate
- [x] 2.2.7 — Extension dispatch: resolve extensions, run before/after at each task
- [x] 2.2.8 — Lifecycle event emission: workflow started/completed/faulted, task started/completed
- [x] 2.2.9 — `FlowDirective` handling: continue, exit, end, goto
- [x] 2.2.10 — Timeout enforcement per-task and per-workflow

---

## Phase 3: Full Integration — IN PROGRESS

### Done so far:
- [x] `flowr-types/workflow.rs` rewritten — old types DELETED, re-exports swf-parser
- [x] New `WorkflowRecord` wraps raw CNCF YAML + FlowR metadata
- [x] `flowr-storage` trait uses `WorkflowRecord`
- [x] `flowr-mcp-core` create_flow uses `WorkflowRecord::from_yaml()`
- [x] `flowr-worker-core` removed from shared workspace
- [x] Shared workspace compiles clean

### Remaining to complete Phase 3:
- [ ] Delete `clawdflow-mcp/src/dispatcher.rs` (old dispatcher)
- [ ] Delete old `execute_workflow()`, rename v2 as only path
- [ ] Remove `flowr-worker-core` dep from clawdflow Cargo.toml
- [ ] Update `clawdflow-storage` LanceDB to store/retrieve `WorkflowRecord`
- [ ] Implement ClawdFlow TaskHandlers (call:llm, call:clawdb, call:lancedb, state:*, inline:*)
- [ ] Wire ClawdFlow handlers into WorkflowExecutor

Original plan:

### 3.1 File Structure

```
flowr-core/src/                    (was shared/flowr-core + shared/flowr-tasks)
├── lib.rs
├── runtime.rs          — FlowRRuntime implements RuntimeProvider
├── expr.rs             — jq expression engine (existing flowr-expr)
├── handlers/           — One handler per task type
│   ├── mod.rs          — HandlerRegistry
│   ├── call_http.rs
│   ├── call_grpc.rs    — stub (returns unimplemented)
│   ├── call_openapi.rs — stub
│   ├── call_asyncapi.rs — stub
│   ├── call_a2a.rs     — stub
│   ├── call_mcp.rs
│   ├── call_function.rs — use.functions + catalog resolution
│   ├── do_task.rs
│   ├── emit.rs
│   ├── for_task.rs
│   ├── fork.rs
│   ├── listen.rs       — stub
│   ├── raise.rs
│   ├── run_container.rs — stub
│   ├── run_shell.rs
│   ├── run_script.rs
│   ├── run_workflow.rs
│   ├── set.rs
│   ├── switch.rs
│   ├── try_task.rs
│   └── wait.rs
└── scripts/
    ├── lua.rs
    ├── javascript.rs
    └── python.rs
```

### 3.2 ClawdFlow-Specific Handlers (clawdflow-tasks)

```
clawdflow/crates/clawdflow-tasks/src/handlers/
├── call_llm.rs         — Anthropic, OpenAI, Ollama
├── call_clawdb.rs      — AQL memory
├── call_lancedb.rs     — Vector DB
├── call_sql.rs         — SQL databases
├── call_qdrant.rs      — Vector search
├── call_human.rs       — Human-in-the-loop
└── state.rs            — state:get, state:set, state:delete
```

### 3.3 Stories

- [ ] 3.3.1 — `FlowRRuntime` implements `RuntimeProvider` for "1.0.3"
- [ ] 3.3.2 — `HandlerRegistry`: maps `Task` enum variants → handlers
- [ ] 3.3.3 — Migrate existing task executors to `TaskHandler` trait
- [ ] 3.3.4 — Wire `flowr-expr` as the expression engine
- [ ] 3.3.5 — Wire `flowr-storage` for resources, state, workflows
- [ ] 3.3.6 — ClawdFlow handlers: call:llm, call:clawdb, call:lancedb, call:sql, call:qdrant, state:*
- [ ] 3.3.7 — JSON Schema validation in data flow (existing jsonschema integration)
- [ ] 3.3.8 — `run:workflow` handler uses `RuntimeProvider.execute()` for nested workflows

---

## Phase 4: Convert All YAML Files (~2-3 days)

Convert 65 workflow files from legacy format to CNCF DSL 1.0.3.

### 4.1 Syntax Transformation

```yaml
# BEFORE (legacy)
namespace: default
name: my-flow
version: "1.0.0"
steps:
  - name: fetch
    type: call:http
    config:
      method: GET
      url: "https://api.example.com"
    export.as: "{data: .}"

  - name: process
    type: set
    config:
      result: "${ .data.value * 2 }"
    export.as: "."

# AFTER (CNCF DSL 1.0.3)
document:
  dsl: '1.0.3'
  namespace: default
  name: my-flow
  version: '1.0.0'
do:
  - fetch:
      call: http
      with:
        method: GET
        endpoint:
          uri: "https://api.example.com"
      output:
        as: "{data: .}"
  - process:
      set:
        result: "${ .data.value * 2 }"
      output:
        as: "."
```

### 4.2 Stories

- [ ] 4.2.1 — Convert 44 test YAML files in `tests/flows/`
- [ ] 4.2.2 — Convert `a3t/backend/flows/system/agent-template.yaml`
- [ ] 4.2.3 — Convert `a3t/backend/flows/system/agent-instance-schema.yaml`
- [ ] 4.2.4 — Convert `a3t/backend/flows/chat_agent.yaml`
- [ ] 4.2.5 — Convert `a3t/backend/flows/agents/*.yaml` (2 files)
- [ ] 4.2.6 — Convert `a3t/backend/flows/integrations/*.yaml` (6 files)
- [ ] 4.2.7 — Convert `a3t/backend/flows/*_crud.yaml` (6+ files)

---

## Phase 5: Wire ClawdFlow Binary (~2 days)

Update `clawdflow-mcp` to use new crate structure.

### 5.1 Stories

- [ ] 5.1.1 — `clawdflow-mcp/src/lib.rs`: use `swf-parser::parse()` + `RuntimeRegistry`
- [ ] 5.1.2 — `clawdflow-mcp/src/dispatcher.rs`: remove (replaced by `swf-runtime` executor)
- [ ] 5.1.3 — `execute_workflow()`: parse → registry.execute()
- [ ] 5.1.4 — MCP `create_flow` tool: accept CNCF DSL, validate via `swf-parser`
- [ ] 5.1.5 — Remove `flowr-worker-core` crate (merged into `swf-runtime`)
- [ ] 5.1.6 — Remove legacy `flowr-types/workflow.rs` StepDefinition (replaced by `swf-parser`)

---

## Phase 6: Tests (~2-3 days)

### 6.1 Stories

- [ ] 6.1.1 — `swf-parser` unit tests: parse every CNCF spec example
- [ ] 6.1.2 — `swf-runtime` unit tests: data flow, extension dispatch, lifecycle events
- [ ] 6.1.3 — All 29 integration tests passing with CNCF YAML syntax
- [ ] 6.1.4 — Agent template tests: tool dispatch, multi-turn, session, memory skip
- [ ] 6.1.5 — CNCF CTK (Conformance Test Kit) scenarios — as many as applicable
- [ ] 6.1.6 — Build + deploy to remote, verify `37.27.195.4:30080`

---

## Files Affected

### New Crates

| Crate | Location | Purpose |
|-------|----------|---------|
| `swf-parser` | `shared/swf-parser/` | Pure CNCF DSL parser |
| `swf-runtime` | `shared/swf-runtime/` | Pluggable execution traits |

### Refactored Crates

| Crate | Change |
|-------|--------|
| `flowr-core` | Implements `RuntimeProvider`, handlers replace task executors |
| `flowr-types` | Slim re-export from `swf-parser` + FlowR extras (durability, storage types) |
| `clawdflow-tasks` | Handlers for ClawdFlow-specific task types (LLM, ClawDB, etc.) |
| `clawdflow-mcp` | Uses `swf-parser` + `RuntimeRegistry`, removes old dispatcher |

### Removed

| Crate/File | Reason |
|------------|--------|
| `flowr-worker-core` | Merged into `swf-runtime` |
| `flowr-types/workflow.rs` StepDefinition | Replaced by `swf-parser::NamedTask` |
| `clawdflow-mcp/src/dispatcher.rs` | Replaced by `swf-runtime` executor |

### YAML Files (65 total)

| Directory | Count | Status |
|-----------|-------|--------|
| `tests/flows/` | 44 | Convert to CNCF DSL |
| `a3t/backend/flows/system/` | 2 | Convert |
| `a3t/backend/flows/agents/` | 2 | Convert |
| `a3t/backend/flows/integrations/` | 6 | Convert |
| `a3t/backend/flows/*.yaml` | 11 | Convert |

---

## CNCF Spec Feature Coverage

| Feature | swf-parser | flowr-core handler | Status |
|---------|-----------|-------------------|--------|
| **Document** | ✓ parse | — | New |
| **Call: HTTP** | ✓ parse | ✓ existing | Refactor |
| **Call: gRPC** | ✓ parse | stub | New |
| **Call: OpenAPI** | ✓ parse | stub | New |
| **Call: AsyncAPI** | ✓ parse | stub | New |
| **Call: A2A** | ✓ parse | stub | New |
| **Call: MCP** | ✓ parse | ✓ existing | Refactor |
| **Call: Function** | ✓ parse | ✓ existing (use.functions) | Refactor |
| **Do** | ✓ parse | ✓ existing | Refactor |
| **Emit** | ✓ parse | stub | New |
| **For** | ✓ parse | ✓ existing | Refactor |
| **Fork** | ✓ parse | ✓ existing | Refactor |
| **Listen** | ✓ parse | stub | New |
| **Raise** | ✓ parse | ✓ existing | Refactor |
| **Run: Container** | ✓ parse | stub | New |
| **Run: Shell** | ✓ parse | ✓ existing | Refactor |
| **Run: Script** | ✓ parse | ✓ existing | Refactor |
| **Run: Workflow** | ✓ parse | ✓ existing | Refactor |
| **Set** | ✓ parse | ✓ existing | Refactor |
| **Switch** | ✓ parse | ✓ existing | Refactor |
| **Try** | ✓ parse | ✓ existing | Refactor |
| **Wait** | ✓ parse | ✓ existing | Refactor |
| **Extensions** | ✓ parse | ✓ existing | Refactor |
| **Schedule** | ✓ parse | ✓ triggers exist | Wire |
| **Authentication** | ✓ parse (all 6) | partial (basic, bearer) | Expand |
| **Catalogs** | ✓ parse | not implemented | New |
| **Lifecycle Events** | ✓ parse | not implemented | New |
| **Secrets** | ✓ parse | ✓ $secrets in scope | Refactor |
| **External Resources** | ✓ parse | ✓ resource registry | Refactor |
| **Error (RFC 7807)** | ✓ parse | ✓ partial | Expand |
| **Retry** | ✓ parse | ✓ existing in try | Refactor |
| **Timeout** | ✓ parse | ✓ existing | Refactor |
| **JSON Schema validation** | ✓ parse | ✓ existing (jsonschema) | Refactor |

**Parser: 100% coverage. Handlers: ~70% implemented, ~30% stubs.**
Stubs (gRPC, OpenAPI, AsyncAPI, A2A, Container, Emit, Listen, Catalogs, Lifecycle Events) can be implemented incrementally — the parser and runtime architecture supports them from day 1.

---

## Validation Criteria

- [ ] `swf-parser` parses ALL examples from the CNCF SWF DSL reference doc
- [ ] `swf-parser` rejects invalid workflows with clear error messages
- [ ] `RuntimeRegistry` selects correct runtime by DSL version
- [ ] All 29 existing tests pass with CNCF YAML syntax
- [ ] Agent template works end-to-end with tool dispatch + multi-turn
- [ ] `cargo test --workspace` passes
- [ ] Build + deploy to `37.27.195.4:30080`
- [ ] No legacy `steps/type/config` YAML anywhere in the codebase
