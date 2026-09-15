# Gherkin Steps — FlowR EE

Canonical step vocabulary and generation rules for the FlowR
Enterprise Edition.

**For developers:** use only phrases from the Vocabulary section when
writing `.feature` files.

**For LLMs:** use the Generation Rules section as your system prompt
when generating scenarios from `.spec/requirements.yaml`. Use only
phrases from the Vocabulary section.

**For reviewers:** use the Contribution Rules section when approving
PRs that add or change steps.

---

## Conventions

- Third person, declarative. "The workflow completes" not "I see it complete".
- Present tense throughout, including in Given.
- Placeholders in angle brackets: `<name>`, `<duration>`, `<event_type>`.
- One concept per step. No "and" inside a single step.
- Every scenario carries at least one `@req-<id>` tag.

---

## Vocabulary

### Given — Preconditions

#### Runtime & Infrastructure

| Step | Purpose | Parameters |
|------|---------|------------|
| Given FlowR is started on port `<port>` | Start FlowR API server | port: integer (default 8000) |
| Given FlowR is started with config `<path>` | Start with config file | path: string |
| Given Iggy is available | Assert Iggy TCP/HTTP connection | none |
| Given Dragonfly is available | Assert Dragonfly connection on 6379 | none |
| Given a Kubernetes cluster is available | Assert K8s API reachable | none |
| Given the AgentCanvas CRD is installed | Assert CRD registered in cluster | none |
| Given the FlowR controller is running | Assert reconciliation controller active | none |
| Given the system clock is set to `<timestamp>` | Freeze time | timestamp: ISO-8601 |

#### Workflow & Registry

| Step | Purpose | Parameters |
|------|---------|------------|
| Given a workflow `<name>` is registered with durability `<mode>` | Register workflow with durability choice | name: string; mode: Completion \| Checkpoint |
| Given the workflow definition at `<path>` | Load workflow YAML from fixture | path: relative to tests/fixtures/ |
| Given the flow registry is empty | Reset registry state in Dragonfly | none |
| Given a checkpoint exists for flow `<name>` at step `<step_id>` | Preload checkpoint in Iggy | name: string; step_id: string |
| Given a suspended flow instance `<instance_id>` awaiting signal `<signal_name>` | Park a flow for signal tests | instance_id: UUID; signal_name: string |
| Given a suspended flow instance `<instance_id>` awaiting timer `<duration>` | Park a flow for timer tests | instance_id: UUID; duration: string |

#### Agent Canvas

| Step | Purpose | Parameters |
|------|---------|------------|
| Given an AgentCanvas `<name>` is applied with spec: (docstring) | Apply Canvas YAML spec | name: string; docstring: YAML |
| Given an AgentCanvas `<name>` with `<count>` replicas of agent `<agent_ref>` | Apply simple Canvas | name: string; count: integer; agent_ref: string |
| Given an AgentCanvas `<name>` with agents: (table) | Apply Canvas with multiple agents | name: string; table columns: ref, template, replicas, trafficShare |
| Given an AgentCanvas `<name>` with HTTPTrigger on path `<path>` | Apply Canvas with HTTP trigger | name: string; path: string |
| Given an AgentCanvas `<name>` with output sink `<sink_uri>` | Apply Canvas with output sink | name: string; sink_uri: string (e.g. kafka://topic) |
| Given no AgentCanvas exists | Assert no canvases deployed | none |

#### Traffic Management

| Step | Purpose | Parameters |
|------|---------|------------|
| Given an AgentCanvas `<name>` with traffic split: (table) | Apply Canvas with traffic shares | name: string; table columns: ref, trafficShare |
| Given an AgentCanvas `<name>` with header routing rule `<header>` = `<value>` to agent `<ref>` | Apply Canvas with header routing | name: string; header: string; value: string; ref: string |
| Given an AgentCanvas `<name>` with cookie routing rule `<cookie>` = `<value>` to agent `<ref>` | Apply Canvas with cookie routing | name: string; cookie: string; value: string; ref: string |

#### Output Sinks

| Step | Purpose | Parameters |
|------|---------|------------|
| Given a Kafka topic `<topic>` exists | Assert Kafka/Redpanda topic available | topic: string |
| Given a mock HTTP server is listening on port `<port>` | Start mock server for webhook sinks | port: integer |
| Given an S3 bucket `<bucket>` exists | Assert S3 bucket available | bucket: string |
| Given a ClickHouse table `<table>` exists in database `<db>` | Assert ClickHouse table | table: string; db: string |

#### Scaling

| Step | Purpose | Parameters |
|------|---------|------------|
| Given an AgentCanvas `<name>` with HPA min `<min>` max `<max>` | Apply Canvas with HPA bounds | name: string; min: integer; max: integer |
| Given an AgentCanvas `<name>` with scale-to-zero after `<duration>` idle | Apply Canvas with idle scale-down | name: string; duration: string |

#### Agent YAML & Loading

| Step | Purpose | Parameters |
|------|---------|------------|
| Given an agent YAML file `<path>` | Load agent YAML fixture | path: relative to tests/fixtures/ |
| Given an agent YAML file `<path>` with content: (docstring) | Create agent YAML inline | path: string; docstring: YAML |
| Given an agent YAML file `<path>` referencing flow `<flow_path>` | Agent with flow reference | path: string; flow_path: string |
| Given a flow YAML file `<path>` with content: (docstring) | Create flow YAML fixture | path: string; docstring: YAML |
| Given the ADB default is `<address>` | Set default ADB/ClawDB address | address: string (e.g. localhost:6379) |
| Given no agents are loaded | Assert clean runtime | none |

#### Agent Templates & Parameters

| Step | Purpose | Parameters |
|------|---------|------------|
| Given a template YAML file `<path>` with content: (docstring) | Create template fixture | path: string; docstring: YAML |
| Given a template `<name>` is registered at version `<version>` | Assert template exists | name: string; version: string |
| Given an agent YAML `<path>` referencing template `<template_path>` with parameters: (table) | Agent from template | path: string; template_path: string; table: name, value |

#### UI

| Step | Purpose | Parameters |
|------|---------|------------|
| Given an agent template `<name>` with input JSON Schema: (docstring) | Register template with input schema | name: string; docstring: JSON Schema |
| Given an agent template `<name>` with output HTML template: (docstring) | Register template with output template | name: string; docstring: HTML |

### When — Actions

#### Agent YAML Loading

| Step | Purpose | Parameters |
|------|---------|------------|
| When I run `clawdflow run <path>` | Load and start agent from YAML | path: string |
| When I run `clawdflow validate <path>` | Validate agent YAML without running | path: string |
| When I run `clawdflow run <path>` with `--adb=<address>` | Load agent with ADB override | path: string; address: string |

#### Agent Admin API

| Step | Purpose | Parameters |
|------|---------|------------|
| When I POST /agents with the agent YAML at `<path>` | Register agent via API | path: string |
| When I GET /agents | List all agents | none |
| When I GET /agents/`<name>` | Get agent detail | name: string |
| When I PATCH /agents/`<name>` with `<json>` | Update agent | name: string; json: inline JSON |
| When I DELETE /agents/`<name>` | Remove agent | name: string |
| When I POST /agents/`<name>`/pause | Pause agent triggers | name: string |
| When I POST /agents/`<name>`/resume | Resume agent triggers | name: string |
| When I GET /agents/`<name>`/status | Get detailed runtime status | name: string |
| When I GET /agents/`<name>`/events | Get lifecycle events | name: string |
| When I GET /agents/`<name>`/history | Get conversation history | name: string |
| When I POST /templates/`<name>`/render with parameters `<json>` | Render template with params | name: string; json: inline JSON |

#### Workflow Execution

| Step | Purpose | Parameters |
|------|---------|------------|
| When I execute workflow `<name>` with input `<json>` | Trigger workflow via Admin API | name: string; json: inline JSON |
| When I execute workflow `<name>` with the fixture `<path>` | Trigger with named input fixture | name: string; path: relative path |
| When the runtime crashes mid-flow at step `<step_id>` | Simulate node failure | step_id: string |
| When the runtime restarts | Resume runtime process | none |
| When I deliver signal `<signal_name>` to instance `<instance_id>` | Resume suspended flow | signal_name: string; instance_id: UUID |
| When `<duration>` elapses | Advance time | duration: e.g. "5s", "1h", "60 seconds" |

#### Admin API

| Step | Purpose | Parameters |
|------|---------|------------|
| When I POST /flows with the fixture `<path>` | Register workflow definition | path: string |
| When I GET /flows | List all workflows | none |
| When I GET /flows/`<ns>`/`<name>` | Retrieve workflow | ns: string; name: string |
| When I PATCH /flows/`<ns>`/`<name>` with `<json>` | Toggle or swap version | ns: string; name: string; json: inline JSON |
| When I DELETE /flows/`<ns>`/`<name>`/`<version>` | Remove version | ns: string; name: string; version: string |
| When I POST /flows/`<ns>`/`<name>`/execute with input `<json>` | Trigger execution | ns: string; name: string; json: inline JSON |
| When I GET /flows/`<ns>`/`<name>`/events | Get workflow events | ns: string; name: string |
| When I GET /flows/`<ns>`/`<name>`/instances/`<id>`/events | Get instance events | ns: string; name: string; id: UUID |
| When I GET /health/ready | Check readiness | none |
| When I GET /health/live | Check liveness | none |
| When I disable flow `<name>` via the Admin API | Toggle flow off | name: string |
| When I swap flow `<name>` to version `<version>` | Hot-swap version | name: string; version: string |

#### Agent Canvas CRUD

| Step | Purpose | Parameters |
|------|---------|------------|
| When I apply AgentCanvas `<name>` with spec: (docstring) | kubectl apply Canvas | name: string; docstring: YAML |
| When I update AgentCanvas `<name>` setting replicas of `<agent_ref>` to `<count>` | Scale agent replicas | name: string; agent_ref: string; count: integer |
| When I update AgentCanvas `<name>` setting trafficShare of `<agent_ref>` to `<share>` | Adjust traffic share | name: string; agent_ref: string; share: float |
| When I delete AgentCanvas `<name>` | kubectl delete Canvas | name: string |
| When I rollback AgentCanvas `<name>` to previous version | Restore last-known-good spec | name: string |

#### Drift Simulation

| Step | Purpose | Parameters |
|------|---------|------------|
| When I manually delete pod `<pod>` in namespace `<ns>` | Simulate pod loss | pod: string; ns: string |
| When I manually delete IngressRoute for Canvas `<name>` | Simulate route loss | name: string |
| When one reconciliation cycle completes | Wait for controller loop | none |

#### Traffic

| Step | Purpose | Parameters |
|------|---------|------------|
| When I send `<count>` requests to path `<path>` | Generate traffic volume | count: integer; path: string |
| When I send a request to path `<path>` with header `<header>`: `<value>` | Send request with header | path: string; header: string; value: string |
| When I send a request to path `<path>` with cookie `<cookie>`=`<value>` | Send request with cookie | path: string; cookie: string; value: string |

#### Scaling

| Step | Purpose | Parameters |
|------|---------|------------|
| When the average request latency exceeds `<duration>` | Simulate latency spike | duration: string |
| When the queue depth exceeds `<count>` | Simulate queue buildup | count: integer |
| When metrics return to normal | Simulate load decrease | none |
| When no traffic arrives for `<duration>` | Simulate idle period | duration: string |
| When a request arrives after scale-to-zero | Trigger cold start | none |

#### Output Sinks

| Step | Purpose | Parameters |
|------|---------|------------|
| When an agent in Canvas `<name>` produces output `<json>` | Simulate agent output | name: string; json: inline JSON |

#### UI

| Step | Purpose | Parameters |
|------|---------|------------|
| When I open the input form for template `<name>` | Render input form in UI | name: string |
| When I submit the form with input `<json>` | Submit form data | json: inline JSON |
| When I open the output view for agent `<name>` | Render output in UI | name: string |
| When I open the topology view for Canvas `<name>` | Render topology graph | name: string |
| When I open the reconciliation dashboard for Canvas `<name>` | Render dashboard | name: string |
| When I open the log stream for pod `<pod>` | Start log streaming | pod: string |

#### Build & Compatibility

| Step | Purpose | Parameters |
|------|---------|------------|
| When I run cargo check on crate `<crate>` | Compile-check a Rust crate | crate: string |
| When I inspect the dependency tree for crate `<crate>` | Inspect cargo dep tree | crate: string |
| When I execute the same workflow on ClawdFlow CE | Run workflow on CE for comparison | none |

### Then — Completion and output

| Step | Purpose | Parameters |
|------|---------|------------|
| Then the workflow completes with status `<status>` | Assert terminal status | status: completed \| failed \| cancelled |
| Then the workflow output equals `<json>` | Assert exact output | json: inline JSON |
| Then the workflow output matches the fixture `<path>` | Assert output against fixture | path: relative path |
| Then execution fails at step `<step_id>` with error type `<error_type>` | Assert structured failure | step_id: string; error_type: string |
| Then no workflow instance is running | Assert runtime idle | none |

### Then — Iggy events

| Step | Purpose | Parameters |
|------|---------|------------|
| Then exactly `<count>` event of type `<event_type>` is appended to Iggy | Assert event count | count: integer; event_type: CloudEvent type |
| Then the Iggy event sequence is `<type_list>` | Assert event ordering | type_list: comma-separated event types |
| Then no Iggy write occurs before the response is returned | Assert Completion semantics | none |
| Then the Iggy checkpoint for step `<step_id>` exists | Assert checkpoint durability | step_id: string |
| Then the Iggy topic `<topic>` exists | Assert topic created on startup | topic: string |

### Then — Performance and timing

| Step | Purpose | Parameters |
|------|---------|------------|
| Then the response latency is under `<duration>` | Assert SLA bound | duration: e.g. "5ms" |
| Then the p99 latency over `<n>` iterations is under `<duration>` | Assert distributional SLA | n: integer; duration: e.g. "10ms" |
| Then the Iggy write timestamp is after the response timestamp | Assert Completion ordering | none |
| Then the p99 Iggy async append latency over `<n>` iterations is under `<duration>` | Assert Iggy async SLA | n: integer; duration: string |
| Then the p99 Iggy sync write latency over `<n>` iterations is under `<duration>` | Assert Iggy sync SLA | n: integer; duration: string |
| Then the p99 Dragonfly GET latency over `<n>` iterations is under `<duration>` | Assert Dragonfly read SLA | n: integer; duration: string |

### Then — Registry state

| Step | Purpose | Parameters |
|------|---------|------------|
| Then flow `<name>` is registered at version `<version>` | Assert registry state | name: string; version: string |
| Then flow `<name>` is disabled | Assert toggle state | name: string |
| Then the Dragonfly key `<key>` has value `<value>` | Assert raw registry value | key: string; value: string |

### Then — Admin API assertions

| Step | Purpose | Parameters |
|------|---------|------------|
| Then the response status is `<code>` | Assert HTTP status code | code: integer |
| Then the response body contains `<text>` | Assert HTTP body substring | text: string |
| Then the response is a JSON array with at least `<count>` entries | Assert array min length | count: integer |
| Then the response field `<field>` equals `<value>` | Assert JSON field value | field: string; value: any |
| Then GET /health/ready returns status 200 | Assert readiness | none |
| Then GET /health/live returns status 200 | Assert liveness | none |
| Then the events list contains `<event_type>` | Assert event in list | event_type: string |
| Then the events list has `<count>` entries | Assert event count | count: integer |

### Then — Suspension assertions

| Step | Purpose | Parameters |
|------|---------|------------|
| Then the flow is suspended | Assert flow in suspended state | none |
| Then the flow resumes after `<duration>` | Assert timer-based resume | duration: string |
| Then the flow resumes when signal `<signal_name>` is delivered | Assert signal-based resume | signal_name: string |
| Then the suspended flow is persisted in Dragonfly | Assert Dragonfly persistence | none |
| Then the suspended flow is recovered after restart | Assert restart recovery | none |

### Then — Agent Canvas assertions

| Step | Purpose | Parameters |
|------|---------|------------|
| Then `<count>` agent pods are running for Canvas `<name>` | Assert pod count | count: integer; name: string |
| Then `<count>` agent pods are running for agent `<agent_ref>` in Canvas `<name>` | Assert per-agent pod count | count: integer; agent_ref: string; name: string |
| Then kubectl get agentcanvas lists `<name>` | Assert CRD visible | name: string |
| Then kubectl get agentcanvas does not list `<name>` | Assert CRD removed | name: string |
| Then no pods exist for Canvas `<name>` | Assert cleanup complete | name: string |
| Then an IngressRoute exists for path `<path>` | Assert trigger route created | path: string |
| Then no IngressRoute exists for Canvas `<name>` | Assert routes cleaned up | name: string |
| Then requests to path `<path>` are routed to agent pods | Assert traffic reaches pods | path: string |
| Then the output sink `<sink_uri>` is bound for Canvas `<name>` | Assert sink wired | sink_uri: string; name: string |

### Then — Traffic assertions

| Step | Purpose | Parameters |
|------|---------|------------|
| Then agent `<agent_ref>` receives approximately `<percent>`% of traffic | Assert proportional distribution | agent_ref: string; percent: number |
| Then traffic shares in Canvas `<name>` sum to 1.0 | Assert share validity | name: string |
| Then the request is routed to agent `<agent_ref>` | Assert header/cookie routing | agent_ref: string |
| Then agent `<ref_a>` receives 0% of traffic | Assert traffic zeroed (rollback) | ref_a: string |
| Then agent `<ref_b>` receives 100% of traffic | Assert full cutover | ref_b: string |

### Then — Drift assertions

| Step | Purpose | Parameters |
|------|---------|------------|
| Then drift is detected for Canvas `<name>` | Assert drift logged | name: string |
| Then the drift log includes expected `<expected>` and actual `<actual>` | Assert drift details | expected: string; actual: string |
| Then the controller recreates the deleted pod | Assert auto-reconciliation | none |
| Then the controller corrects the traffic routes | Assert route reconciliation | none |
| Then `<count>` agent pods are running for Canvas `<name>` | Assert restored count | count: integer; name: string |
| Then a drift alert event is emitted for Canvas `<name>` | Assert alert emitted | name: string |
| Then the alert includes canvas `<name>`, drift type `<type>`, and action `<action>` | Assert alert fields | name: string; type: string; action: string |
| Then the Canvas `<name>` spec matches the previous version | Assert rollback applied | name: string |
| Then the Canvas `<name>` has `<count>` spec versions in history | Assert history depth | name: string; count: integer |

### Then — Scaling assertions

| Step | Purpose | Parameters |
|------|---------|------------|
| Then replicas for agent `<agent_ref>` in Canvas `<name>` scale up | Assert scale-up occurred | agent_ref: string; name: string |
| Then replicas for agent `<agent_ref>` in Canvas `<name>` scale down | Assert scale-down occurred | agent_ref: string; name: string |
| Then an HPA exists for Canvas `<name>` with min `<min>` max `<max>` | Assert HPA created | name: string; min: integer; max: integer |
| Then agent `<agent_ref>` in Canvas `<name>` scales to 0 replicas | Assert scale-to-zero | agent_ref: string; name: string |
| Then agent `<agent_ref>` in Canvas `<name>` scales up from 0 | Assert cold start scale-up | agent_ref: string; name: string |
| Then the first request latency after scale-up is under `<duration>` | Assert cold-start bound | duration: string |

### Then — Output sink assertions

| Step | Purpose | Parameters |
|------|---------|------------|
| Then a message is produced to Kafka topic `<topic>` | Assert Kafka produce | topic: string |
| Then the Kafka message payload contains `<text>` | Assert Kafka message body | text: string |
| Then the Kafka message headers include `<header>` | Assert Kafka header | header: string |
| Then the mock server at port `<port>` received a POST with body containing `<text>` | Assert webhook forward | port: integer; text: string |
| Then the webhook sink logs HTTP status `<code>` | Assert sink observability | code: integer |
| Then an object is written to S3 bucket `<bucket>` | Assert S3 write | bucket: string |
| Then the S3 object key contains `<text>` | Assert S3 key format | text: string |
| Then a row is inserted into ClickHouse table `<table>` | Assert ClickHouse insert | table: string |
| Then the ClickHouse row contains field `<field>` with value `<value>` | Assert ClickHouse field | field: string; value: string |

### Then — UI assertions

| Step | Purpose | Parameters |
|------|---------|------------|
| Then a form is rendered with fields from the JSON Schema | Assert form generation | none |
| Then the form submission sends validated input to the agent | Assert form→agent binding | none |
| Then the output is rendered using the HTML template | Assert template rendering | none |
| Then template variables are populated from agent output | Assert variable interpolation | none |
| Then the topology graph shows `<count>` agent nodes | Assert graph nodes | count: integer |
| Then the topology graph shows `<count>` edges | Assert graph edges | count: integer |
| Then the topology graph contains node `<name>` of type `<type>` | Assert specific node | name: string; type: agent \| trigger \| sink |
| Then the dashboard shows `<agent_ref>` with desired `<desired>` and actual `<actual>` replicas | Assert dashboard row | agent_ref: string; desired: integer; actual: integer |
| Then the dashboard shows a drift event with timestamp | Assert drift in dashboard | none |
| Then the log stream displays new entries without page refresh | Assert live streaming | none |

### Then — Agent YAML & schema assertions

| Step | Purpose | Parameters |
|------|---------|------------|
| Then the agent `<name>` is running | Assert agent loaded and active | name: string |
| Then the agent `<name>` is running in mode `<mode>` | Assert agent mode | name: string; mode: ephemeral \| session \| persistent \| reactive |
| Then the agent `<name>` has `<count>` procedural actions | Assert bound flow count | name: string; count: integer |
| Then the agent `<name>` has aql_namespace `<namespace>` | Assert AQL identity | name: string; namespace: string |
| Then the agent `<name>` has `<count>` triggers | Assert trigger count | name: string; count: integer |
| Then the agent `<name>` has trigger of type `<type>` | Assert trigger type exists | name: string; type: http \| cron \| iggy \| event |
| Then the agent `<name>` has durability `<mode>` | Assert durability default | name: string; mode: completion \| checkpoint |
| Then validation succeeds with exit code 0 | Assert validate passes | none |
| Then validation fails with error containing `<text>` | Assert validate error | text: string |
| Then the error message contains file path `<path>` | Assert file reference in error | path: string |
| Then the error message contains line number | Assert line reference in error | none |
| Then the agent YAML is rejected with `<reason>` | Assert schema rejection | reason: string |
| Then the runtime rejects the agent with error `<text>` | Assert runtime rejection | text: string |
| Then the effective spec has field `<field>` equal to `<value>` | Assert rendered spec field | field: JSON path; value: any |
| Then the rendered template output matches the inline agent YAML | Assert template equivalence | none |
| Then the agent `<name>` has parameter `<param>` with value `<value>` | Assert runtime parameter | name: string; param: string; value: any |
| Then procedural action `<action>` resolves to flow at `<path>` | Assert flow reference resolved | action: string; path: string |
| Then procedural action `<action>` is marked as `<ownership>` | Assert flow ownership | action: string; ownership: agent \| shared |
| Then the API response includes agent status `<status>` | Assert agent status in API | status: string |
| Then the events list includes event type `<type>` | Assert lifecycle event | type: spawned \| turn_completed \| stopped |

### Then — Compatibility assertions

| Step | Purpose | Parameters |
|------|---------|------------|
| Then all crate checks pass with exit code 0 | Assert cargo checks pass | none |
| Then the dependency tree does not contain `<dependency>` | Assert crate independence | dependency: string |
| Then the FlowR EE output equals the ClawdFlow CE output | Assert cross-edition parity | none |
| Then all 22 task types produce the same results on both editions | Assert task parity | none |

### Tags

| Tag | Purpose |
|-----|---------|
| `@req-<id>` | Links scenario to requirement ID (required on every scenario) |
| `@smoke` | Minimal sanity suite, runs on every commit |
| `@integration` | Requires Iggy + Dragonfly running |
| `@k8s` | Requires Kubernetes cluster (Canvas, HPA, Ingress tests) |
| `@kafka` | Requires Kafka/Redpanda broker |
| `@s3` | Requires S3-compatible object store |
| `@clickhouse` | Requires ClickHouse instance |
| `@ui` | Browser/UI rendering tests |
| `@slow` | Excluded from fast feedback loop |
| `@flaky` | Known intermittent; excluded from blocking CI |
| `@wip` | In progress; excluded from all CI suites |

---

## Generation Rules

*This section is the system prompt when an LLM generates Gherkin
scenarios from `.spec/requirements.yaml`.*

You are generating Gherkin scenarios for the FlowR EE project with a
formal requirements specification. Your inputs are
`.spec/requirements.yaml` (the requirements to verify) and this
document (the step vocabulary).

For each requirement in requirements.yaml, produce one or more Gherkin
scenarios that verify its acceptance_criteria. Follow these rules:

1. Each scenario MUST be tagged with `@req-<id>` using the requirement's
   lowercase ID (e.g. `@req-flowr-001`).

2. Each scenario MUST use only step phrases that appear verbatim in
   the Vocabulary section above. Placeholders in angle brackets are
   the substitution points — replace them with concrete values.

3. Every acceptance criterion SHOULD map to at least one assertion.
   If a criterion cannot be expressed with existing steps, do NOT
   invent a new step. Instead, list proposed new steps in a
   "Proposed New Steps" section at the end of your output.

4. For requirements with `kind: behavior`, generate a happy-path
   scenario and at least one negative-path scenario.

5. For requirements with `kind: sla`, generate a scenario asserting
   the bound holds under normal conditions and a scenario asserting
   it fails when the bound is violated.

6. For requirements with `kind: invariant`, generate a scenario that
   attempts to violate the invariant and asserts it still holds.

7. Use Scenario Outline with Examples when a requirement has
   parametric variants (e.g. multiple durability modes, multiple
   sink types).

8. Group scenarios by requirement area into separate `.feature`
   files under `features/`. File naming: `<area>.feature`.

9. Canvas scenarios MUST be tagged `@k8s`. Sink scenarios MUST carry
   the appropriate infrastructure tag (`@kafka`, `@s3`, `@clickhouse`).
   UI scenarios MUST be tagged `@ui`.

### Output format

For each .feature file, produce the complete file content in a code
block preceded by its filename:

    File: features/durability.feature
```gherkin
    Feature: Durability modes

      @req-flowr-002 @smoke
      Scenario: Completion mode defers Iggy write until after response
        Given a workflow "simple" is registered with durability "Completion"
        When I execute workflow "simple" with input {}
        Then the workflow completes with status "completed"
        And no Iggy write occurs before the response is returned
        And exactly 1 event of type "workflow.completed.v1" is appended to Iggy
```

After all files, include a "Proposed New Steps" section if needed.
For each proposed step, provide: the phrase, its purpose, its
parameters, and which requirement drove the need.

### What NOT to do

- Do not invent step phrasings that are synonyms of existing steps.
- Do not add scenarios for requirements outside the file you were given.
- Do not include prose commentary in .feature files — only Feature,
  Scenario, Given/When/Then, tags, and comments (`#`).
- Do not generate step definition code. That is a separate task.

---

## Contribution Rules

*This section governs how to add, change, or remove steps.*

### Adding a new step

1. Check the Vocabulary section first. A synonym of an existing step
   is not new — use the existing phrase.

2. If genuinely new, open a PR that adds:
   - A row in the appropriate Vocabulary subsection.
   - The step definition code in `tests/step_defs/`.
   - At least one scenario using the new step.

3. PR reviewer checks:
   - Is this covered by an existing step?
   - Is the parameter list minimal?
   - Does it follow the conventions above?
   - Is the step definition code tested?

4. LLM-generated scenarios that use phrases not in the Vocabulary
   must be flagged in a "Proposed New Steps" section. Review these
   before merging scenarios that depend on them.

### Deprecating a step

1. Mark the row with `~~strikethrough~~` and add a `(deprecated
   YYYY-MM-DD)` note.
2. Add a `replaced_by:` note pointing to the successor step.
3. Update existing `.feature` files over the following sprint.
4. Remove the row and step definition after the grace period.

### Health signals

- Vocabulary plateau: healthy projects reach 40-80 steps and stop.
- Growth rate: 1-2 new steps per week during active development is
  normal. 10+ per week means the granularity is too fine or the
  discipline has broken down.
- Above 200 steps: audit for synonyms. Most large vocabularies are
  smaller vocabularies with drift.
