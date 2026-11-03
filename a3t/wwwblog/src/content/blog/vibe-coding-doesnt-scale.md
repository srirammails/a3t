---
title: Vibe Coding Doesn't Scale. Here's What Does.
description: LLMs have no memory of your project, so every session starts from zero. Prose rots; formal artifacts checked in CI keep a codebase coherent across sessions.
date: 2026-04-19
updated: 2026-06-05
author: Sriram
tags: [engineering, llm]
image: /covers/vibe-coding.png
imageAlt: Cover image for "Vibe Coding Doesn't Scale. Here's What Does."
---

### The problem nobody names

You open Claude Code, Cursor, or a fresh ChatGPT window. You describe what you want. Code appears. It runs. You ship.

Next week you open a new session to extend the feature. The LLM doesn't remember last week. It reads your repo, makes assumptions, and produces code that works locally but subtly contradicts what you built before. Terminology drifts. Test style drifts. Architectural decisions you made get silently overturned.

Six weeks in, your codebase is a patchwork of ten LLM sessions that never talked to each other. Each session was productive in isolation. The aggregate is incoherent.

> This is the scaling problem of LLM-assisted development, and it has a specific cause: **LLMs have no memory of your project**. Every session starts from zero. The only context they have is what's in the files.

So the question becomes: what files can you put in the repo that let any LLM session reconstruct your project's intent accurately?

### Why README.md isn't enough

The intuitive answer is "write a good spec." People create spec.md and describe what they're building. This helps on the first session. It doesn't help on the tenth.

The reason is that prose has no enforcement mechanism. An LLM reading spec.md can skim it, misinterpret it, or outright contradict it, and nothing catches the drift. There's no check that runs on every PR saying "this change violates section 3.2 of the spec."

Free-form text survives if everyone reads it carefully every time. That worked when humans were the only contributors. It doesn't work when your primary contributor is a stateless model that reads differently each session.

### The kitchen analogy

Think about how a professional kitchen operates.

A recipe book (spec.md) tells you what to cook. But kitchens don't run on recipe books alone. They run on:

- A **glossary** of techniques every cook knows ("julienne", "reduce", "temper")
- A **pantry** of approved ingredients with consistent names
- **Standard recipes** written in a format every cook recognizes
- A **line check** before service that verifies every station is ready

The glossary, pantry, and format aren't written in prose — they're physical realities of the kitchen. You can't "forget" that the walk-in has specific shelves. You can't accidentally invent a new word for "dice" because the expediter will call it out.

LLM-driven development needs the same physical reality for code. Not prose documents describing how things should be, but **formal artifacts the build system checks on every commit**.

### A Hello World example

Let's build the smallest project that shows the full discipline. A HTTP server with one endpoint:

```text
GET /greet?name=<name>  →  { "message": "Hello, <name>!" }
```

Trivial to implement. Useful to specify rigorously.

### Step 1: The requirements schema

Before writing any code, we define what a requirement *is*. This is a one-time organizational artifact — you design it once and reuse it across projects.

```yaml
# .spec/schema/requirements.schema.yaml
$schema: http://json-schema.org/draft-07/schema#
type: object
required: [project, version, requirements]
properties:
  project: { type: string }
  version: { type: string }
  requirements:
    type: array
    items:
      type: object
      required: [id, kind, priority, statement, acceptance_criteria]
      properties:
        id:
          type: string
          pattern: "^REQ-[A-Z]+-[0-9]{3}$"
        kind:
          enum: [behavior, invariant, sla, interface, security]
        priority:
          enum: [must, should, may]
        statement:
          type: string
          maxLength: 300
        acceptance_criteria:
          type: array
          minItems: 1
          items: { type: string }
        tests:
          type: array
          items: { type: string }
```

This schema is the **grammar for requirements**. Any requirements file that doesn't match it gets rejected by the validator. Senior engineer designs it; everyone else uses it.

### Step 2: The requirements file

Now we write actual requirements for Hello World:

```yaml
# .spec/requirements.yaml
project: hello-greeter
version: "0.1"
requirements:

  - id: REQ-GREET-001
    kind: behavior
    priority: must
    statement: >
      GET /greet with a name parameter returns a JSON greeting.
    acceptance_criteria:
      - "Response status is 200"
      - "Response body is JSON with a 'message' field"
      - "Message contains the provided name"
    tests: []

  - id: REQ-GREET-002
    kind: behavior
    priority: must
    statement: >
      GET /greet without a name parameter returns a 400 error.
    acceptance_criteria:
      - "Response status is 400"
      - "Response body contains an error description"
    tests: []

  - id: REQ-GREET-003
    kind: sla
    priority: must
    statement: >
      Greeting responses MUST return within 50ms at p99.
    acceptance_criteria:
      - "p99 latency over 1000 requests is under 50ms"
    tests: []
```

Three requirements, all testable. Notice what's different from prose:

- Each has a unique ID you can reference (REQ-GREET-001)
- Each has explicit acceptance criteria — the observations that prove it works
- Each has a kind and priority — you can filter, prioritize, report
- The tests field is empty for now; it'll be filled as we build

A CI job validates this file against the schema on every commit. Malformed requirements can't enter the repo.

### Step 3: The vocabulary

Now we define the language our tests will speak. This is the kitchen's glossary of techniques:

```markdown
# .spec/allowed-bdd-steps.md

## Given — Preconditions

| Step | Purpose | Parameters |
|------|---------|------------|
| Given the server is running on port `<port>` | Start test server | port: integer |

## When — Actions

| Step | Purpose | Parameters |
|------|---------|------------|
| When I GET `<path>` | Make HTTP GET request | path: string |
| When I GET `<path>` `<n>` times | Repeated request for SLA tests | path: string; n: integer |

## Then — Assertions

| Step | Purpose | Parameters |
|------|---------|------------|
| Then the response status is `<code>` | Assert HTTP status | code: integer |
| Then the response body is JSON | Assert content type | none |
| Then the response body field `<field>` contains `<text>` | Assert JSON field | field, text: strings |
| Then the p99 latency is under `<ms>` ms | Assert SLA | ms: integer |

## Tags

| Tag | Purpose |
|-----|---------|
| `@req-<id>` | Links scenario to requirement (required) |
| `@smoke` | Minimal suite, runs every commit |
```

Eight steps total for Hello World. Real projects reach 40-80. The vocabulary is **project-specific** — a workflow engine and a chat app would have completely different vocabularies. This is an important lesson I learned the hard way: you can't reuse a vocabulary across projects. Each project earns its own.

The vocabulary is derived from the requirements (what actions and assertions do these acceptance criteria need?), reviewed by a human for consolidation, then committed as a project artifact.

### Step 4: The scenarios

Now we compose scenarios from the vocabulary. Scenarios are specific stories that verify specific requirements:

```gherkin
# features/greet.feature
Feature: Greeting endpoint

  Background:
    Given the server is running on port 8080

  @req-greet-001 @smoke
  Scenario: Happy path greeting
    When I GET "/greet?name=Sriram"
    Then the response status is 200
    And the response body is JSON
    And the response body field "message" contains "Sriram"

  @req-greet-002 @smoke
  Scenario: Missing name returns 400
    When I GET "/greet"
    Then the response status is 400
    And the response body field "error" contains "name is required"

  @req-greet-003
  Scenario: Latency SLA under load
    When I GET "/greet?name=test" 1000 times
    Then the p99 latency is under 50 ms
```

The kitchen parallel snaps into place here:

- **A step** (one Given/When/Then line) is one technique from your glossary. "Julienne." "Reduce by half."
- **A scenario** (several steps together) is a dish. "Coq au vin" — specific techniques in specific order producing a specific outcome.
- **The vocabulary file** is the approved glossary — the palette of allowed moves.
- **The feature file** is the recipe book — specific dishes built from the glossary.

Your 8 steps compose into unlimited scenarios. Without vocabulary discipline, you'd end up with 3 near-synonymous ways to say "make an HTTP GET" and no coherent test style.

### Step 5: The step definitions

Scenarios are declarative. Something has to actually execute them. That something is step definitions in code:

```python
# tests/step_defs/http_steps.py
import time
import requests
from pytest_bdd import given, when, then, parsers

@given(parsers.parse('the server is running on port {port:d}'))
def server_running(port, test_server):
    test_server.ensure_running(port)

@when(parsers.parse('I GET "{path}"'))
def http_get(context, path):
    context.response = requests.get(f"http://localhost:8080{path}")

@when(parsers.parse('I GET "{path}" {n:d} times'))
def http_get_repeated(context, path, n):
    latencies = []
    for _ in range(n):
        start = time.perf_counter()
        requests.get(f"http://localhost:8080{path}")
        latencies.append((time.perf_counter() - start) * 1000)
    context.latencies = sorted(latencies)

@then(parsers.parse('the response status is {code:d}'))
def check_status(context, code):
    assert context.response.status_code == code

@then('the response body is JSON')
def check_json(context):
    context.json = context.response.json()

@then(parsers.parse('the response body field "{field}" contains "{text}"'))
def check_field(context, field, text):
    body = context.response.json()
    assert text in body[field]

@then(parsers.parse('the p99 latency is under {ms:d} ms'))
def check_p99(context, ms):
    p99 = context.latencies[int(len(context.latencies) * 0.99)]
    assert p99 < ms
```

One step definition per vocabulary phrase. Never two. This 1:1 relationship is what prevents the step-definition graveyard — the pile of near-duplicate functions that accumulate when teams let step phrasing drift.

### Step 6: The implementation

Only now do we write the actual server code. The tests exist first, and the tests must pass:

```python
# src/server.py
from fastapi import FastAPI, HTTPException

app = FastAPI()

@app.get("/greet")
def greet(name: str | None = None):
    if not name:
        raise HTTPException(status_code=400, detail="name is required")
    return {"message": f"Hello, {name}!"}
```

Run the test suite. All three scenarios pass. We have a verified implementation of three requirements.

### The CI gates

Here's where prose fails and formal artifacts win. Every commit runs:

```yaml
# .github/workflows/spec-check.yml
name: Spec and test checks
on: [pull_request, push]

jobs:
  spec:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: actions/setup-python@v5
        with: { python-version: "3.12" }
      - run: pip install pyyaml jsonschema pytest pytest-bdd requests

      - name: Schema validation
        run: python scripts/validate_requirements.py

      - name: Coverage check
        run: python scripts/check_coverage.py

      - name: Run scenarios
        run: pytest tests/
```

Three gates, each catching a different class of drift:

**Schema validation** rejects malformed requirements. If an LLM session adds a requirement without acceptance criteria, the PR fails.

**Coverage check** is the 40-line script that pairs every REQ-ID with scenario tags:

```python
# scripts/check_coverage.py
import sys, yaml, re, glob

req_ids = {r["id"].lower() for r in
           yaml.safe_load(open(".spec/requirements.yaml"))["requirements"]}

tagged = set()
for path in glob.glob("features/**/*.feature", recursive=True):
    for match in re.finditer(r"@req-([a-z0-9-]+)", open(path).read()):
        tagged.add(f"req-{match.group(1)}")

normalized = {r.replace("_", "-") for r in req_ids}
uncovered = normalized - tagged
orphans = tagged - normalized

if uncovered:
    print(f"Uncovered requirements: {uncovered}")
    sys.exit(1)
if orphans:
    print(f"Scenarios tag non-existent requirements: {orphans}")
    sys.exit(1)
print(f"All {len(req_ids)} requirements have scenarios")
```

This is the check that matters most. It ensures requirements and tests stay synchronized. Add a requirement without scenarios → CI fails. Delete a requirement without removing scenarios → CI fails. This mechanical traceability is the core discipline.

**Run scenarios** is the traditional test gate. Everything actually passes.

Add more gates as the project grows: vocabulary conformance (no invented step phrasings), consistency checks (no duplicate IDs, no dependency cycles), performance regression (SLA scenarios actually meet their bounds).

### Who owns what

The pipeline only works if roles are clear. Here's how the responsibilities distribute:

**Staff/principal engineer — designs the schema.** This is a one-time artifact per organization or project family. It encodes opinions about what a requirement is, what kinds exist, what metadata matters. Hard to get right, easy to reuse once done.

**Tech lead or architect — owns requirements extraction and review.** Prose from product managers or stakeholders gets turned into requirements.yaml. LLM does the draft; tech lead reviews for completeness, correctness, and atomicity. This is the irreducible human step — it requires judgment about what the project actually needs.

**Tech lead — also owns vocabulary consolidation.** LLM proposes steps from requirements; tech lead consolidates duplicates, parameterizes awkward ones, and groups by subsystem. The output is a small, coherent vocabulary that the whole team will use.

**Developers — own step definitions and implementation code.** Once vocabulary and scenarios exist, developers (LLM-assisted) implement step definitions and production code. Scenarios run continuously; red → green → refactor per scenario. This is the daily work.

**LLM — does the drafting at every stage.** Drafts requirements from prose, drafts vocabulary from requirements, drafts scenarios from requirements and vocabulary, drafts code from scenarios and architecture. Humans review at each stage. The LLM is a force multiplier, not a decision-maker.

**CI — enforces the discipline.** Every gate fails loudly. No exceptions, no "I'll fix it in the next PR." The build is the law.

### The shift this represents

Traditional software engineering assumed a human team with accumulated context. People remembered last month's decisions, knew which approaches were tried and rejected, understood the module boundaries. This context lived in their heads. Documentation was a nice-to-have; the real knowledge was tacit.

LLM-assisted development breaks this assumption. Your primary contributor has zero accumulated context. Each session is a new hire with amnesia.

That means the context has to be externalized — and not as prose, which drifts, but as formal artifacts with mechanical enforcement. Every architectural decision becomes a structured record. Every behavioral expectation becomes a tagged scenario. Every PR runs through gates that catch drift before merge.

This feels like ceremony when you start. After three projects, it feels like oxygen. The alternative is the six-weeks-in incoherent patchwork, and you already know how that ends.

### The minimum viable discipline

Five artifacts, fewer than 200 lines of CI scripting, and you have a project that any LLM session can join without drifting:

```text
.spec/
├── schema/
│   └── requirements.schema.yaml     # Grammar for requirements
├── requirements.yaml                 # What the project does
└── allowed-bdd-steps.md              # Vocabulary for tests

features/
└── *.feature                         # Scenarios using the vocabulary

tests/
└── step_defs/                        # Vocabulary bound to code

scripts/
├── validate_requirements.py          # Schema gate
└── check_coverage.py                 # Traceability gate

.github/workflows/
└── spec-check.yml                    # CI orchestration
```

Hello World used eight steps and three scenarios to demonstrate this. Real projects reach 60 requirements and 200 scenarios. The artifacts scale; the discipline doesn't change.

The insight that matters: **prose rots, artifacts endure**. A README that describes the system will be out of date in six months. A schema-validated requirements file paired with executing scenarios stays synchronized because the build fails when it doesn't.

LLMs accelerated the production of code. They did nothing for the production of coherent systems. Coherence comes from discipline that survives across sessions, contributors, and months. Formal artifacts with mechanical CI enforcement are how you build that discipline into the project itself.

Start with Hello World if you need to. The ceremony feels absurd for three requirements. That's the point — you learn the shape on something trivial, then scale it to the projects where the ceremony saves your sanity.

---

*Originally published on [LinkedIn](https://www.linkedin.com/pulse/vibe-coding-doesnt-scale-heres-what-does-sriram-reddy-mryaf/) on 19 April 2026.*
