# Agent CNCF Template — Drop ZeroClaw, Pure SWF

**Goal:** Replace the embedded ZeroClaw agent runtime with a reusable CNCF Serverless Workflow template. Agents become workflow instances with integrations (sub-flows as tools), extensions (lifecycle hooks), and memory (ClawDB). No custom Rust agent runtime — pure YAML + existing task types.

**Outcome:** `system/agent-template` workflow + agent instantiation via `run: workflow` + Python support restored (no more libsqlite3-sys conflict).

**Last audited:** 2026-05-22

---

## Phase 1: Core CNCF Task Types (Rust) — COMPLETE

All 6 core features required by the agent template are implemented and verified in the codebase.

### 1.1 `use.extensions` — Before/After Lifecycle Hooks — DONE
**Files:** `shared/flowr-core/src/executor.rs` + `shared/flowr-types/src/workflow.rs`

- [x] Parse `use.extensions` array from workflow definition (`ExtensionDefinition` struct, workflow.rs:162-178)
- [x] Each extension: `extend`, `when`, `before`, `after` fields
- [x] `execute_with_extensions()` runs before/after task lists (executor.rs:101-339)
- [x] `run_extensions_before()` / `run_extensions_after()` (executor.rs:343-443)
- [x] `when` condition evaluated with expression engine (executor.rs:354-364)
- [x] `then: exit` in `before` tasks skips the extended task (executor.rs:379-393)
- [x] Extensions execute in declaration order
- [x] `$context` variable available in expressions
- [~] `$task.reference` — **PARTIAL**: scope has `$task.name` but not `$task.reference` (executor.rs:479)

**Remaining fix:** Add `reference` field to task scope in `build_task_scope()` — ~1 hour.

### 1.2 `use.functions` — Reusable Task Definitions — DONE
**Files:** `shared/flowr-types/src/workflow.rs` + `shared/flowr-worker-core/src/dispatcher.rs`

- [x] `UseBlock.functions: Option<HashMap<String, Value>>` (workflow.rs:201)
- [x] Function resolver in `build_single_task()` (dispatcher.rs:397-440)
- [x] Resolves `call: functionName` from `use.functions` map
- [x] Extracts `call` or `type` field from function definition
- [x] Merges caller config over function defaults (dispatcher.rs:411-425)
- [x] Resource override supported (dispatcher.rs:431-433)
- [x] Nested function calls via recursive resolution

### 1.3 `run: workflow` — Nested Workflow Execution — DONE
**File:** `shared/flowr-tasks/src/run_workflow.rs` (278 lines)

- [x] `RunWorkflowConfig` with `namespace`, `name`, `version`, `input` (line 44)
- [x] `WorkflowResolver` trait for async resolution (lines 57-68)
- [x] Resolves workflow from registry
- [x] Executes nested workflow with provided input (lines 184-227)
- [x] Child input from config or falls back to parent input (lines 178-179)
- [x] Test: `tests/flows/test-run-workflow.yaml`

### 1.4 `for` with `while` Condition — DONE
**File:** `shared/flowr-tasks/src/for_task.rs`

- [x] `while_expr: Option<String>` field on ForExecutor (line 41)
- [x] `with_while()` setter method (lines 70-73)
- [x] While expression evaluated BEFORE each iteration (lines 109-135)
- [x] Proper break when condition is false (lines 131-134)
- [x] Scope includes iteration variables and index

### 1.5 `if` Conditional on Tasks — DONE
**Files:** `shared/flowr-core/src/executor.rs` + `shared/flowr-core/src/task.rs`

- [x] `if_condition: Option<String>` on TaskDefinition (task.rs:137)
- [x] Evaluated BEFORE task execution (executor.rs:142-158)
- [x] Task skipped entirely if condition is false
- [x] Output = input passthrough when skipped
- [x] Works in both normal and resume-with-extensions paths

### 1.6 `call: llm` with Tool Definitions + Tool Response — DONE
**Files:** `shared/flowr-tasks/src/call_llm.rs` + `clawdflow/crates/clawdflow-tasks/src/call_llm.rs`

- [x] `tools: Option<Vec<Value>>` in LlmConfig (clawdflow:71)
- [x] Anthropic: converts tools to `{name, description, input_schema}` format, sends in request (clawdflow:189-199)
- [x] OpenAI: wraps in `{type: "function", function: {name, description, parameters}}` (clawdflow:298-311)
- [x] Anthropic: `extract_anthropic_tool_calls()` parses `tool_use` content blocks (clawdflow:494-515)
- [x] OpenAI: parses `choices[0].message.tool_calls` with function arguments (clawdflow:343-358)
- [x] Returns structured `{ content, tool_calls: [{id, name, input}], model, usage, stop_reason }` (clawdflow:237-248)
- [x] 3 providers: Anthropic, OpenAI, Ollama
- [ ] Ollama: tool support not implemented (basic completion only, clawdflow:385-471)

### Data Flow (Bonus — also complete)

- [x] `input.from` / `output.as` / `export.as` transformations (executor.rs:160-292)
- [x] `$context`, `$input`, `$output`, `$task`, `$workflow`, `$runtime`, `$secrets` runtime args (scope.rs)
- [x] jq expression engine with global variable support (flowr-expr/src/engine.rs:241)
- [ ] `input.schema` / `output.schema` / `export.schema` — fields parsed but **no runtime validation** (no jsonschema dependency)

---

## Phase 2: Remove ZeroClaw Dependency (Rust) — TODO

### 2.1 ZeroClaw Fully Removed — DONE
**Files:** Workspace `Cargo.toml` + `clawdflow-agents/Cargo.toml` + `clawdflow-tasks/Cargo.toml`

- [x] ZeroClaw dependency removed from workspace `Cargo.toml` (commented out)
- [x] ZeroClaw dependency removed from `clawdflow-agents/Cargo.toml`
- [x] `zeroclaw-agent` feature removed from `clawdflow-agents`
- [x] `zeroclaw-agent` feature removed from `clawdflow-tasks`
- [x] `call_agent.rs` rewritten — pure CNCF path only (959→290 lines)
- [x] `clawdflow-agents/src/lib.rs` cleaned — no more feature gates
- [x] `manager.rs` + `adapters/` modules are dead code (not imported)
- [x] All HTTP handlers in `lib.rs` cleaned — no feature gates needed
- [x] `clawdflow-tasks` defaults: `["lua", "javascript", "python"]`

### 2.2 Keep Agent Manager for Triggers + State — DONE (no changes needed)
**File:** `clawdflow/crates/clawdflow-agents/src/manager.rs`

- [x] AgentManager already fully gated behind `#[cfg(feature = "zeroclaw-agent")]`
- [x] TriggerManager lives in `triggers/` module — independent, always available
- [x] No code changes needed — feature gate provides clean separation

### 2.3 Refactor `call:agent` Task Types — DONE
**Files:** `clawdflow/crates/clawdflow-tasks/src/call_agent.rs` + `clawdflow/crates/clawdflow-mcp/src/dispatcher.rs`

- [x] `call:agent` → delegates to `run:workflow` targeting `system/agent-template` via `execute_via_workflow()`
- [x] `send:agent` → same delegation (fire-and-forget via workflow execution)
- [x] `ask:agent` → same delegation (sync request-reply via workflow execution)
- [x] `spawn:agent` → creates webhook/cron triggers pointing to agent-template workflow
- [x] `stop:agent` → deletes triggers + clears session state
- [x] All executors accept `WorkflowResolver` via `with_resolver()` constructor
- [x] Dispatcher passes `StorageWorkflowResolver` to agent executors
- [x] Agent resource config mapped to agent-template input schema in `execute_via_workflow()`
- [x] All ZeroClaw code preserved behind `#[cfg(feature = "zeroclaw-agent")]` for backward compat

### 2.4 Enable RustPython — DONE
**File:** `clawdflow/crates/clawdflow-tasks/Cargo.toml`

- [x] `rustpython`, `rustpython-vm`, `rustpython-stdlib` dependencies enabled
- [x] `python` feature in default features: `default = ["lua", "javascript", "python"]`
- [x] No libsqlite3-sys conflict — verified via `cargo check` (zeroclaw fully removed)
- [x] Fixed `inline_script.rs` RustPython API (removed `.flatten()`, added type annotations)
- [x] Test `inline:python` task type — PASS (local server, without stdlib, direct object marshaling)

## Agent Tests — ALL PASSING

**24/24 tests pass** including 5 agent tests with live LLM calls (Groq llama-3.3-70b).

### Fixes applied:
- `run:workflow` → full `Executor` pipeline via `StorageWorkflowResolver.execute()`
- `use.functions` resolution added to clawdflow dispatcher
- `DoExecutor` + `ForExecutor`: pass `subtask.resource` to scope (was missing)
- `DoExecutor` + `ForExecutor`: evaluate `if_condition` on subtasks (was missing)
- Agent-template YAML: `steps` → `do`, `try:` → `do:`, `call: clawdb` (CNCF function syntax)
- Agent-template YAML: all data flow via `$context` (not `.config` which gets overwritten)
- Agent-template YAML: namespace-qualified resources (`default/default-llm`)
- Agent-template YAML: null-safe expressions for session restore

---

## Phase 3: Agent Template Workflow (YAML) — TODO

This is the core deliverable — the agent IS a CNCF workflow.

### 3.1 Base Agent Template — DONE
**File:** `a3t/backend/flows/system/agent-template.yaml`

The 10-point lifecycle as a CNCF workflow using `use.extensions` for hooks and `use.functions` for reusable operations:

```
do:
  1. init_turn        — set turn state, load session via state:get
  2. recall_memory    — conditional recall from working/episodic/semantic/procedural
  3. procedural_check — if hit → set response, skip LLM (flow directive)
  4. reasoning_loop   — for(1..max_iter) while(should_continue):
     a. call:llm with tools + messages + system_prompt
     b. switch on stop_reason:
        - tool_use → dispatch each tool_call → append results → continue
        - end_turn → set should_continue=false
  5. store_memory     — save working + episodic memory to ClawDB
  6. save_session     — state:set with TTL
  7. emit_response    — return final content

use.extensions (10 runtime points, all locked by default):
  - on_turn_start    → extend: init_turn, before
  - before_recall    → extend: recall_memory, before
  - after_recall     → extend: recall_memory, after
  - before_llm       → extend: call_llm (in loop), before
  - on_llm_response  → extend: call_llm (in loop), after
  - on_tool_call     → extend: dispatch_tool, before
  - on_tool_error    → extend: dispatch_tool, after, when: ${ .error != null }
  - before_store     → extend: store_memory, before
  - after_response   → extend: emit_response, after
  - on_turn_end      → extend: save_session, after

use.functions:
  - recallWorking, recallEpisodic, recallSemantic, recallProcedural
  - storeWorking, storeEpisodic
  - dispatchTool (switch on tool name → run:workflow or call:mcp)
```

### 3.2 Agent Instance Schema — DONE
**File:** `a3t/backend/flows/system/agent-instance-schema.yaml`

Define the contract for creating agent instances:
- `template` — ref to agent-template (namespace/name@version)
- `config` — identity, model, system_prompt, memory, policy
- `integrations` — array of {name, description, input_schema, flow | mcp_server}
- `extensions` — hook overrides (inline:lua or flow refs)
- `variables` — typed key-value config injected into agent context

### 3.3 Example Agents — DONE
**Files:**
- [x] `a3t/backend/flows/agents/support-agent.yaml` — support agent with KB search, ticket creation, escalation + audit extension
- [x] `a3t/backend/flows/agents/ops-agent.yaml` — reactive ops agent with health checks, log queries, restart, incident creation + budget guard + action tracking extensions

### 3.4 Rewrite existing chat_agent.yaml — DONE
**File:** `a3t/backend/flows/chat_agent.yaml`

- [x] Replace `type: call:agent` + `resource: a3t/a3t-chat-agent` with `run: workflow` targeting agent-template
- [x] Map existing agent config (LLM, memory, MCP, system_prompt) to template input schema
- [x] Version bumped to 2.0.0

---

## Phase 4: Agent Lifecycle MCP Tools — DONE

### 4.1 Template Registry Tools — DONE
**Files:** `shared/flowr-mcp-core/src/tools.rs` (definitions) + `clawdflow/crates/clawdflow-mcp/src/lib.rs` (handlers)

- [x] `create_agent_template` — register versioned template via TemplateManager
- [x] `list_agent_templates` — list templates with namespace filter
- [x] `get_agent_template` — resolve by name or name:version

### 4.2 Agent Instance Tools — DONE
- [x] `create_agent` — resolve template, merge defaults with config, create triggers, store in state
- [x] `list_agents` — list from state store registry
- [x] `get_agent` — get config + session state + message count
- [x] `delete_agent` — remove config, session, and associated triggers

### 4.3 Agent Execution Tools — DONE
- [x] `send_message` — execute system/agent-template workflow, poll for result (60s timeout), return content
- [x] `get_agent_history` — retrieve messages from `agent:session:{id}` state key
- [x] `clear_agent_session` — delete session state

---

## Phase 5: Integration Sub-Flow Library — DONE

### 5.1 Common Integration Flows — DONE
**Directory:** `a3t/backend/flows/integrations/`

- [x] `http-fetch.yaml` — generic HTTP GET/POST/PUT/DELETE with headers, body, timeout
- [x] `search-lancedb.yaml` — vector similarity search with query, table, limit, columns
- [x] `query-sql.yaml` — SQL query execution with params against any SQL resource
- [x] `send-email.yaml` — email via HTTP API (Resend/SendGrid/Mailgun compatible)
- [x] `create-ticket.yaml` — ticket creation in LanceDB with auto-ID, optional webhook notify
- [x] `web-scrape.yaml` — URL fetch + HTML-to-text extraction via inline:js

### 5.2 MCP Server Integration — DONE
Agents can reference MCP servers directly in integrations:
```yaml
integrations:
  - name: tool_name
    mcp_server: my-mcp-server    # resource ref
    mcp_tool: tool_on_server     # specific tool name
```
No sub-flow needed — `call: mcp` handles dispatch directly.

---

## Phase 6: Catalog Support (Future) — TODO

### 6.1 `use.catalogs` — External Template Registry
- Fetch agent templates from remote catalog URLs
- Standardized file structure per CNCF spec
- Version resolution (semver)

### 6.2 Template Marketplace
- Public catalog of agent templates
- Versioned, signed, tagged by vertical
- Already have `template_registry` table in A3T schema

---

## Implementation Order (Updated)

```
DONE (Phase 1):
  ✓ 1.1 use.extensions     (full before/after, when, exit)
  ✓ 1.2 use.functions       (resolver, config merge, nested)
  ✓ 1.3 run:workflow        (registry resolve, nested exec, input mapping)
  ✓ 1.4 for + while         (while_expr per iteration, break on false)
  ✓ 1.5 if condition        (pre-execution eval, skip on false)
  ✓ 1.6 call:llm + tools    (Anthropic/OpenAI tool send + parse + structured output)
  ✓ Data flow transforms    (input.from, output.as, export.as, $context/$input/$output)

NEXT:
  Phase 3 (yaml):   3.1 → 3.2 → 3.3 → 3.4        ← START HERE
                     ↑       ↑       ↑       ↑
                   template schema  examples rewrite
                   (large) (small)  (med)   (small)

  Phase 2 (remove): 2.1 → 2.4 → 2.2 → 2.3
                     ↑       ↑       ↑       ↑
                   feature  python  decouple task-types
                   (tiny)  (small)  (med)    (med)

  Phase 4 (tools):  4.1 → 4.2 → 4.3
                     ↑       ↑       ↑
                   registry instance exec
                   (small)  (med)   (small)

  Phase 5 (lib):    5.1
                     ↑
                   flows
                   (med)

FUTURE:
  Phase 6 (catalog): 6.1 → 6.2
```

**Critical path:** 3.1 (agent template YAML) → 3.4 (rewrite chat_agent) → 2.1 (flip flag) → 2.4 (python) → 4.2 (agent tools)

**All phases complete.** Remaining work is minor fixes only.

**Completed:**
- Phase 1: 6 core Rust features
- Phase 2: 4 refactors (ZeroClaw optional, Python enabled, call:agent refactored)
- Phase 3: 4 YAML files (agent template, instance schema, chat rewrite, examples)
- Phase 4: 9 MCP tools (template CRUD, agent CRUD, execution)
- Phase 5: 6 integration sub-flows

---

## Minor Fixes Backlog

| Fix | File | Status |
|-----|------|--------|
| Add `$task.reference` to task scope | `shared/flowr-core/src/executor.rs:479` | **DONE** — `reference: "/do/{task_name}"` added |
| Ollama tool support in call:llm | `clawdflow/crates/clawdflow-tasks/src/call_llm.rs:385+` | **DONE** — OpenAI-compatible tool format, tool_call parsing, structured output |
| Runtime JSON Schema validation | `jsonschema` 0.29 + executor + dispatcher + types | **DONE** |

---

## Validation Criteria

- [ ] `inline:python` works (RustPython enabled, no sqlite conflict)
- [ ] Agent template deploys as a standard CNCF workflow
- [ ] Agent instance invokes template via `run: workflow`
- [ ] Extensions fire at all 10 lifecycle points
- [ ] Procedural hit skips LLM (extension `exit` directive)
- [ ] Integration sub-flows execute as tools within reasoning loop
- [ ] Session state persists across turns via `state:get/set`
- [ ] Triggers (webhook/cron) route to agent workflows
- [ ] All 38 test workflows pass (20 current + fix remaining)
- [ ] Zero dependency on ZeroClaw crate
- [ ] Rewritten chat_agent.yaml works end-to-end
- [ ] A3T UI template editor renders lifecycle graph from YAML
