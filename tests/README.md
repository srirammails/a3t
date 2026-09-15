# ClawdFlow Test Suite

Test workflows for verifying all ClawdFlow task types.

## Test Location

All test workflow YAML files are in `/tests/flows/`

## Test Results Summary

**Last Run:** 2026-04-07 | **Passed:** 16/27 (59%)

| Category       | Task Type       | Test Name             | Status | Notes |
|----------------|-----------------|----------------------|--------|-------|
| **Flow Control** | set           | test-set             | ✅ Pass | |
|                | do              | test-do              | ✅ Pass | |
|                | switch          | test-switch          | ✅ Pass | |
|                | for             | test-for             | ✅ Pass | |
|                | fork            | test-fork            | ✅ Pass | Parallel execution |
|                | try             | test-try             | ✅ Pass | |
|                | raise           | test-raise           | ✅ Pass | |
|                | wait            | test-wait            | ✅ Pass | |
| **I/O**        | call:http       | test-call-http       | ✅ Pass | |
|                | file:read/write | test-file-io         | ✅ Pass | |
|                | state:get/set   | test-state           | ✅ Pass | State persistence |
| **Script**     | run:shell       | test-run-shell       | ✅ Pass | |
|                | run:script      | test-run-script      | ❌ N/A | Task type not implemented |
|                | run:workflow    | test-run-workflow    | ❌ N/A | Task type not implemented |
|                | inline:lua      | test-inline-lua      | ✅ Pass | ~20μs latency |
|                | inline:js       | test-inline-js       | ✅ Pass | QuickJS runtime |
|                | inline:python   | test-inline-python   | ❌ N/A | Disabled (compile w/ --features python) |
| **AI**         | call:llm        | test-call-llm        | 🔧 N/C | Set ANTHROPIC_API_KEY env var |
|                | call:agent      | test-call-agent      | 🔧 N/C | Requires agent resource |
|                | call:human      | test-call-human      | 🔧 N/C | Requires human-in-loop resource |
| **Integration**| call:lancedb    | test-call-lancedb    | ✅ Pass | Vector DB operations |
|                | call:sql        | test-call-sql        | ✅ Pass | PostgreSQL queries |
|                | call:qdrant     | test-call-qdrant     | 🔧 N/C | No Qdrant server available |
|                | call:clawdb     | test-call-clawdb     | 🔧 N/C | ClawDB resource needed |
|                | call:mcp        | test-call-mcp        | 🔧 N/C | MCP connection failed |
|                | call:redis      | test-call-redis      | ❌ N/A | Task type not implemented |
|                | mcp-clawdb      | test-mcp-clawdb      | 🔧 N/C | MCP resource needed |

### Legend
- ✅ Pass: Test executed successfully
- ⚠️ Fail: Test failed, needs investigation (expression/input errors)
- 🔧 N/C: Not Configured - requires external resource configuration
- ❌ N/A: Not Available - task type not implemented in clawdflow

## Running Tests

### Automated Test Runner

```bash
cd tests
python3 run_tests.py
```

This runs all tests against the remote server (http://37.27.195.4:3002) and saves results to `test_results.json`.

### Manual Testing via MCP

Tests can also be executed manually using the ClawdFlow MCP server:

```bash
# Execute a test flow
mcp__clawdflow__execute_flow name="test-set" input='{"name": "Test", "value": 42}'

# Check execution status
mcp__clawdflow__get_execution_status instance_id="<instance-id>"
```

## Key Findings

### Nested Step Format
Flow control tasks (`do`, `switch`, `for`, `fork`, `try`) require nested steps to be **full StepDefinition objects** with `name`, `type`, and `config` fields:

```yaml
# CORRECT
- name: my-do-task
  type: do
  config:
    - name: step1
      type: set
      config:
        value: 1
    - name: step2
      type: set
      config:
        value: 2

# INCORRECT (shorthand)
- name: my-do-task
  type: do
  config:
    - set:
        value: 1
```

### For Loop Config
```yaml
config:
  each: item       # Variable name for current item
  in: "${ .items }" # Collection expression
  at: idx          # Index variable (optional)
  do:              # Array of full step definitions
    - name: process
      type: set
      config: ...
```

### Switch Cases
```yaml
config:
  - when: "${ .status == \"approved\" }"
    then:
      - name: approved-branch
        type: set
        config: ...
  - default:      # Default case
      - name: default-branch
        type: set
        config: ...
```

### Fork Branches
```yaml
config:
  branches:
    - name: branch-1
      do:
        - name: task-1
          type: set
          config: ...
  compete: false  # Optional: true for first-wins mode
```

### Try/Catch
```yaml
config:
  do:
    - name: risky-task
      type: call:http
      config: ...
  catch:
    - errors: ["*"]  # Catch all errors
      do:
        - name: handle-error
          type: set
          config: ...
  retry:            # Optional
    limit: 3
    delay_ms: 1000
```

## Inline Script State Access

Scripts access workflow input via `state` variable:
- **Lua**: `state.field`
- **JavaScript**: `state.field` or `state["field"]`
- **Python**: `state["field"]` or `state.get("field")`

Output via `result` variable.
