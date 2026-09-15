# Allowed BDD Steps — ClawdFlow v2.0

Canonical step vocabulary for the ClawdFlow CNCF SWF runtime.

**For developers:** use ONLY phrases from this file when writing `.feature` files.
**For LLMs:** use the Generation Rules section when producing scenarios from `requirements_*.yaml`.
**For reviewers:** reject PRs that introduce steps not in this vocabulary without going through Contribution Rules.

---

## Conventions

- Third person, declarative. "The workflow completes" not "I see it complete."
- Present tense throughout.
- Placeholders in angle brackets: `<name>`, `<value>`, `<port>`.
- One concept per step.
- Every scenario carries at least one `@req-<id>` tag.
- Every feature file declares its requirements file: `# requirements: requirements_<name>.yaml`

---

## Vocabulary

### Given — Preconditions

#### Parser & Runtime

| Step | Purpose | Parameters | Used by |
|------|---------|------------|---------|
| Given the workflow YAML: | Load workflow from inline docstring | none | requirements_runtime |
| Given a workflow YAML `<file>` | Load workflow from file | file: path | requirements_runtime |
| Given the runtime is initialized | Create runtime with default config | none | requirements_runtime |
| Given the runtime is initialized with storage | Create runtime with storage backend | none | requirements_runtime |
| Given the workflow `<ns>/<name>` is deployed | Deploy workflow to storage | ns, name: strings | requirements_runtime |

#### ClawdFlow Infrastructure

| Step | Purpose | Parameters | Used by |
|------|---------|------------|---------|
| Given ClawdFlow is started on port `<port>` | Start ClawdFlow in HTTP mode | port: integer | requirements_clawdflow |
| Given ClawdFlow is started with `<args>` | Start with CLI arguments | args: string | requirements_clawdflow |
| Given ClawdFlow is restarted | Restart process | none | requirements_clawdflow |
| Given ClawdFlow is started on port `<port>` with log capture enabled | Start with log interception | port: integer | requirements_clawdflow |

#### Resources

| Step | Purpose | Parameters | Used by |
|------|---------|------------|---------|
| Given the resource `<name>` of type `<type>` exists | Register a resource | name, type: strings | requirements_runtime |
| Given an LLM resource `<name>` is configured | Register LLM resource with defaults | name: string | requirements_clawdflow |
| Given an LLM resource `<name>` is configured with API key `<key>` | Register LLM with known key | name, key: strings | requirements_clawdflow |

#### Workflows & Fixtures

| Step | Purpose | Parameters | Used by |
|------|---------|------------|---------|
| Given the workflow definition at `<path>` | Load workflow YAML from fixture | path: string | requirements_clawdflow |

#### Triggers

| Step | Purpose | Parameters | Used by |
|------|---------|------------|---------|
| Given a trigger `<name>` exists with type `<type>` and path `<path>` | Create webhook trigger | name, type, path: strings | requirements_clawdflow |
| Given a trigger `<name>` exists with type `<type>` and expression `<expr>` | Create cron trigger | name, type, expr: strings | requirements_clawdflow |
| Given a trigger target workflow `<flow>` is attached to `<trigger>` | Attach workflow target | flow, trigger: strings | requirements_clawdflow |

#### Docker & Build

| Step | Purpose | Parameters | Used by |
|------|---------|------------|---------|
| Given the Dockerfile at `<path>` | Reference Dockerfile | path: string | requirements_clawdflow |
| Given a running Docker container `<tag>` with port `<port>` mapped | Start container | tag: string, port: integer | requirements_clawdflow |

### When — Actions

#### Parsing

| Step | Purpose | Parameters | Used by |
|------|---------|------------|---------|
| When I parse the workflow | Parse YAML into WorkflowDefinition | none | requirements_runtime |

#### Execution

| Step | Purpose | Parameters | Used by |
|------|---------|------------|---------|
| When I execute the workflow | Execute with empty input | none | requirements_runtime |
| When I execute the workflow with input `<json>` | Execute with inline JSON | json: string | requirements_runtime |
| When I execute the workflow with input: | Execute with docstring JSON | none | requirements_runtime |
| When I execute workflow `<name>` with input `<json>` | Execute named workflow | name: string, json: JSON | requirements_clawdflow |

#### MCP

| Step | Purpose | Parameters | Used by |
|------|---------|------------|---------|
| When I list MCP tools | List tools via MCP | none | requirements_clawdflow |
| When I call MCP tool `<tool>` with name `<name>` | Call MCP tool | tool, name: strings | requirements_clawdflow |
| When I send an MCP request via stdin | Send MCP via stdio | none | requirements_clawdflow |

#### State

| Step | Purpose | Parameters | Used by |
|------|---------|------------|---------|
| When I set state key `<key>` to value `<value>` | Write state | key, value: strings | requirements_clawdflow |
| When I set state key `<key>` to value `<value>` with TTL `<seconds>` second | Write with TTL | key, value: strings, seconds: integer | requirements_clawdflow |

#### Flow & Resource CRUD

| Step | Purpose | Parameters | Used by |
|------|---------|------------|---------|
| When I create a flow `<name>` via the API | Create flow | name: string | requirements_clawdflow |
| When I delete the flow `<name>` via the API | Delete flow | name: string | requirements_clawdflow |

#### Triggers

| Step | Purpose | Parameters | Used by |
|------|---------|------------|---------|
| When I POST /triggers with name `<name>` and type `<type>` and path `<path>` | Create trigger | name, type, path: strings | requirements_clawdflow |
| When I DELETE /triggers/`<id>` | Delete trigger | id: string | requirements_clawdflow |
| When I POST /triggers/`<id>`/pause | Pause trigger | id: string | requirements_clawdflow |
| When I POST /triggers/`<id>`/enable | Enable trigger | id: string | requirements_clawdflow |
| When I POST /triggers/`<id>`/attach with target type `<type>` and agent_id `<id>` | Attach target | type, id: strings | requirements_clawdflow |
| When I POST /triggers/`<id>`/detach with target_id `<target_id>` | Detach target | id, target_id: strings | requirements_clawdflow |
| When I POST /webhook?path=`<path>` with body `<json>` | Fire webhook | path: string, json: JSON | requirements_clawdflow |

#### Templates

| Step | Purpose | Parameters | Used by |
|------|---------|------------|---------|
| When I create a template `<name>` at version `<version>` via POST /templates | Create template | name, version: strings | requirements_clawdflow |
| When I GET /templates/`<name>` | Get template | name: string | requirements_clawdflow |
| When I POST /templates/`<name>`/activate/`<version>` | Activate version | name, version: strings | requirements_clawdflow |
| When I delete template `<name>` | Delete template | name: string | requirements_clawdflow |

#### Time

| Step | Purpose | Parameters | Used by |
|------|---------|------------|---------|
| When `<duration>` elapses | Advance time | duration: e.g. "2s" | requirements_clawdflow |

#### Docker

| Step | Purpose | Parameters | Used by |
|------|---------|------------|---------|
| When I run docker build with tag `<tag>` | Build Docker image | tag: string | requirements_clawdflow |

### Then — Assertions

#### Parse Results

| Step | Purpose | Parameters | Used by |
|------|---------|------------|---------|
| Then parsing succeeds | Assert no parse error | none | requirements_runtime |
| Then parsing fails | Assert parse error | none | requirements_runtime |
| Then parsing fails with error containing `<text>` | Assert specific error | text: string | requirements_runtime |
| Then the document dsl is `<version>` | Assert DSL version | version: string | requirements_runtime |
| Then the document name is `<name>` | Assert name | name: string | requirements_runtime |
| Then the document namespace is `<ns>` | Assert namespace | ns: string | requirements_runtime |
| Then the workflow has `<n>` tasks | Assert task count | n: integer | requirements_runtime |
| Then task `<name>` is a `<type>` task | Assert task type | name, type: strings | requirements_runtime |
| Then task `<name>` has `<n>` subtasks | Assert subtask count | name: string, n: integer | requirements_runtime |
| Then the workflow has `<n>` extensions | Assert extension count | n: integer | requirements_runtime |
| Then extension `<name>` extends `<target>` | Assert extension target | name, target: strings | requirements_runtime |
| Then the workflow has `<n>` functions | Assert function count | n: integer | requirements_runtime |
| Then function `<name>` calls `<target>` | Assert function call | name, target: strings | requirements_runtime |

#### Execution Results

| Step | Purpose | Parameters | Used by |
|------|---------|------------|---------|
| Then execution succeeds | Assert no runtime error | none | requirements_runtime |
| Then execution fails | Assert runtime error | none | requirements_runtime |
| Then execution fails with error containing `<text>` | Assert specific error | text: string | requirements_runtime |
| Then the output field `<field>` is `<value>` | Assert output field | field, value: strings | requirements_runtime |
| Then the output field `<field>` contains `<text>` | Assert field contains | field, text: strings | requirements_runtime |
| Then the output field `<field>` is null | Assert null | field: string | requirements_runtime |
| Then the output field `<field>` is not null | Assert not null | field: string | requirements_runtime |
| Then the context field `<field>` is `<value>` | Assert context | field, value: strings | requirements_runtime |
| Then task `<name>` was executed | Assert ran | name: string | requirements_runtime |
| Then task `<name>` was skipped | Assert skipped | name: string | requirements_runtime |
| Then the extension `<name>` fired | Assert extension ran | name: string | requirements_runtime |

#### Workflow Status

| Step | Purpose | Parameters | Used by |
|------|---------|------------|---------|
| Then the workflow completes with status `<status>` | Assert terminal status | status: string | requirements_clawdflow |
| Then the workflow output field `<field>` equals `<value>` | Assert output field | field, value: strings | requirements_clawdflow |
| Then the workflow `<name>` was executed | Assert flow ran | name: string | requirements_clawdflow |

#### Infrastructure

| Step | Purpose | Parameters | Used by |
|------|---------|------------|---------|
| Then the HTTP endpoint is reachable at port `<port>` | Assert HTTP up | port: integer | requirements_clawdflow |
| Then the MCP endpoint is reachable at port `<port>` | Assert MCP up | port: integer | requirements_clawdflow |
| Then the flow `<name>` is retrievable via the API | Assert flow exists | name: string | requirements_clawdflow |
| Then the flow `<name>` is not found via the API | Assert flow absent | name: string | requirements_clawdflow |
| Then getting state key `<key>` returns `<value>` | Assert state value | key, value: strings | requirements_clawdflow |
| Then getting state key `<key>` returns null | Assert state absent | key: string | requirements_clawdflow |

#### MCP

| Step | Purpose | Parameters | Used by |
|------|---------|------------|---------|
| Then the tool list contains `<tool>` | Assert tool present | tool: string | requirements_clawdflow |
| Then the MCP result contains `<text>` | Assert result text | text: string | requirements_clawdflow |
| Then an MCP response is received on stdout | Assert stdio response | none | requirements_clawdflow |

#### Triggers

| Step | Purpose | Parameters | Used by |
|------|---------|------------|---------|
| Then GET /triggers returns a list containing `<name>` | Assert listed | name: string | requirements_clawdflow |
| Then GET /triggers does not contain `<name>` | Assert absent | name: string | requirements_clawdflow |
| Then the trigger `<name>` has status `<status>` | Assert status | name, status: strings | requirements_clawdflow |
| Then the trigger `<name>` has `<n>` target(s) | Assert target count | name: string, n: integer | requirements_clawdflow |
| Then the trigger delivers to `<n>` target(s) | Assert delivery | n: integer | requirements_clawdflow |

#### Templates

| Step | Purpose | Parameters | Used by |
|------|---------|------------|---------|
| Then GET /templates returns a list containing `<name>` | Assert listed | name: string | requirements_clawdflow |
| Then GET /templates does not contain `<name>` | Assert absent | name: string | requirements_clawdflow |
| Then the response contains template `<name>` at version `<version>` | Assert version | name, version: strings | requirements_clawdflow |
| Then GET /templates/`<name>` returns version `<version>` | Assert active version | name, version: strings | requirements_clawdflow |

#### Performance

| Step | Purpose | Parameters | Used by |
|------|---------|------------|---------|
| Then the p99 latency over `<n>` iterations is under `<duration>` for workflow `<name>` | Assert SLA | n: integer, duration, name: strings | requirements_clawdflow |

#### Security

| Step | Purpose | Parameters | Used by |
|------|---------|------------|---------|
| Then the captured logs do not contain `<text>` | Assert text absent | text: string | requirements_clawdflow |

#### Build

| Step | Purpose | Parameters | Used by |
|------|---------|------------|---------|
| Then the build completes with exit code 0 | Assert build OK | none | requirements_clawdflow |

---

## Tags

| Tag | Purpose |
|-----|---------|
| `@req-<id>` | Links scenario to requirement (REQUIRED on every scenario) |
| `@smoke` | Minimal suite, runs every commit |
| `@parser` | Parser-only test (no runtime) |
| `@runtime` | Requires runtime execution |
| `@conformance` | CNCF spec conformance |
| `@call-http` | Tests call:http |
| `@call-mcp` | Tests call:mcp |
| `@flow-control` | Tests do/for/switch/try/fork |
| `@data-flow` | Tests input/output/export |
| `@extensions` | Tests use.extensions |
| `@functions` | Tests use.functions |
| `@slow` | Excluded from fast feedback loop |
| `@integration` | Requires external services |

---

## Generation Rules

When generating scenarios from `requirements_*.yaml`, follow these rules:

1. Each scenario MUST be tagged with `@req-<id>` using the requirement's lowercase ID.
2. Each scenario MUST use ONLY step phrases from this vocabulary. No synonyms.
3. Every acceptance criterion SHOULD map to at least one assertion.
4. If a step is needed that doesn't exist, list it in a "Proposed New Steps" section — do NOT invent it inline.
5. For `kind: behavior`, generate happy-path + negative-path scenarios.
6. For `kind: sla`, generate pass + fail scenarios.
7. Use Scenario Outline with Examples for parametric variants.
8. Each feature file MUST start with `# requirements: requirements_<name>.yaml`.

---

## Contribution Rules

### Adding a new step

1. Check this vocabulary first — a synonym is not new.
2. PR must add: vocabulary row + step definition code + at least one scenario using it.
3. Reviewer checks: is this covered by an existing step? Is the parameter list minimal?

### Health signals

- Healthy projects: 40-80 steps and plateau.
- Current count: ~70 steps.
- Growth rate: 1-2 new steps per week during active development.
- Above 200: audit for synonyms.
