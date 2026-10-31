# A3T flows on ADB — architecture and cleanup (proposal, 31 Oct 2026)

Goal: every A3T agent uses all four ADB memory types and the graph, by default and
without the template author wiring it. Today the chat agent can *read* ADB, but
almost nothing it does is *written* back:

- the conversation's context doesn't go to WORKING;
- no execution checkpoints go to EPISODIC;
- nothing lands in SEMANTIC unless reflection is switched on by hand;
- no LINKs are ever made.

This doc covers:

1. what is in `a3t/backend` today and what is actually used;
2. what to delete;
3. the target structure for system flows, templates and policies;
4. the order to do it in.

---

## 1. What exists today

### 1.1 The live chat path

```
UI chat ──POST /chat/stream──▶ a3t/a3t-chat-engine        (registry only — NOT in the repo)
                                   └─ system/agent-engine  (backend/flows/system/agent-engine.yaml)
                                        └─ call: a3t:agent (Rust: call_agent.rs + agent_memory.rs)
                                             ├─ recall: WORKING, EPISODIC, SEMANTIC LIKE + graph, PROCEDURAL PATTERN
                                             └─ store:  WORKING ← user message, EPISODIC ← reply text
```

There is also a second, older engine. The UI still offers it when the template is named
`agent-template`:

```
a3t/a3t-chat-agent (chat_agent.yaml) ─▶ system/agent-template (370-line YAML engine)
```

That engine uses the `clawdb` handler and the `default/default-memory` resource. Its AQL
is pre-v0.5 (`content ~ "…"`, `SCOPE private`), which ADB doesn't accept. It also sets
no `memory_resource`, so it never touches memory at all.

### 1.2 Why the chat agent's memory stays empty

| Gap | Where | Effect |
|---|---|---|
| Memory is off unless the agent config has `memory_resource` | `call_agent.rs:529` | An agent created from a template without it gets no recall and no store. `a3t-chat-agent` never sets it; whether `a3t-chat-engine` does can only be checked in the live registry. |
| WORKING gets the user's message only | `agent_memory.rs:403` | Neither the assistant's reply, the tool calls, the tool results, nor the UI context reach WORKING. |
| EPISODIC gets the reply text only | `agent_memory.rs:409` | An episode has no goal, no steps, no tool outcomes and no result status, so it isn't a checkpoint you can resume or learn from. |
| FlowR's execution checkpoints stay in FlowR | `flowr-ce-storage/src/checkpoints.rs` (Lance) | A run's progress is never visible as memory. |
| SEMANTIC is written only by reflection, which is opt-in with no UI toggle | F-L3-a | By default, nothing. |
| No LINKs are ever made | — | Episodes, facts, sessions and users aren't connected, so `FOLLOW LINKS` and the graph recall (P1.5) have little to walk. |
| PROCEDURAL is read but never written | — | Procedures exist only if someone stores them by hand. |
| `memory:read` / `memory:write` policy actions are defined but never emitted | `flowr-policy/src/actions.rs:48` | Cedar has no say over memory. Isolation is the `NAMESPACE` plus an owner filter applied client-side in FlowR (F-L3-a). |

### 1.3 Inventory of `a3t/backend/flows` (34 files)

"Used" means it is called by the UI, the server or another flow. I checked this with a
grep over `a3t/ui/src`, `flowr-ce`, `flowr` and `shared`.

| File | Flow | Used? | Notes |
|---|---|---|---|
| `a3t_install.yaml` | a3t-install | yes (`flowr-server` names it as the seeding path) | Seeds both engines as templates |
| `a3t_schema_install.yaml` | a3t-schema-install | via install | 683 lines. **Seeds `usr-superadmin` with a fixed password hash committed to git.** |
| `a3t_deploy_init.yaml` | a3t-deploy-init | no | Its "hash-password" step passes the **plaintext password through as the hash**. Duplicates resource-setup. |
| `a3t_resource_setup.yaml` | a3t-resource-setup | no | Older `apiVersion/kind` format; registers the same DB resource as deploy-init |
| `a3t_triggers_setup.yaml` | a3t-triggers-setup | no | Creates webhooks for every CRUD flow **and for `a3t-schema-install`**. Check whether those webhooks are authenticated; a schema-install webhook should not exist. |
| `agent_dispatch.yaml` | a3t-agent-dispatch | no (roadmap only) | Passes the template's YAML as the system prompt, and hard-codes the two test skills |
| `chat_agent.yaml` | a3t-chat-agent | UI fallback for `agent-template` | Wrapper for the YAML engine |
| `system/agent-template.yaml` | system/agent-template | via `a3t-chat-agent` | Old engine; stale AQL |
| `system/agent-engine.yaml` | system/agent-engine | **yes: the live engine** | Hook points + `call: a3t:agent` |
| `system/agent-instance-schema.yaml` | — | no | A schema doc, not a flow; describes the old engine's input |
| `agents/support-agent.yaml` | support-agent | no | Example on the old engine; its tools (`search_kb`, `escalate`) don't exist |
| `agents/ops-agent.yaml` | ops-agent | no | Example on the old engine; the `ops-*` tool flows don't exist |
| `agents_crud.yaml` | a3t-agents | yes | |
| `templates_crud.yaml` | a3t-templates | yes | |
| `resources_crud.yaml` | a3t-resources | yes | |
| `users_crud.yaml` | a3t-users | yes | |
| `audit_crud.yaml` | a3t-audit | yes | |
| `settings_crud.yaml` | a3t-settings | yes | |
| `knowledge_crud.yaml` | a3t-knowledge | yes (Knowledge page) | Separate KG: `call: rotate` on its own `a3t-kg-db` Lance, **parallel to ADB SEMANTIC** |
| `canvas_crud.yaml` | a3t-canvas | no | The UI says it's retired (canvases are registry resources) |
| `test_resource.yaml` | a3t-test-resource | yes (Resources page "Test") | |
| `integrations/*.yaml` (6) | create-ticket, http-fetch, query-sql, search-lancedb, send-email, web-scrape | no | Generic tool flows that no agent binds. `search-lancedb` overlaps with ADB recall. |
| `test/adb_selftest.yaml` | adb-selftest | yes (manual e2e) | Keep |
| `test/canvas_dispatch.yaml`, `runtime_echo.yaml` | | runtime tests | Keep |
| `test/hook-logger.yaml`, `test-agent-full.yaml` | | no | `test-agent-full` uses `a3t/groq-llama70b`, which doesn't exist |
| `test/skill-*.yaml` (2) | a3t-skill-* | only via `agent_dispatch` | |

**Drift:** the UI also calls flows that only exist in the live registry and not in the
repo:

- `a3t-chat-engine` (the main chat path);
- per-action variants `a3t-agents-list`, `a3t-agents-create|update|delete` and
  `a3t-templates-list|create|update|delete`, which sit alongside the consolidated
  `a3t-agents` and `a3t-templates`;
- `a3t-early-access`.

`resources/a3t-resources.yaml` is gitignored (F-L7). So a fresh install can't be rebuilt
from the repo.

---

## 2. Cleanup

| Action | Items |
|---|---|
| **Export into the repo** (first, so the repo is the truth) | `a3t-chat-engine`, `a3t-early-access`, the per-action CRUD flows (to compare them, then delete), and a secret-free `a3t-resources.example.yaml` |
| **Delete** | `system/agent-template.yaml`, `chat_agent.yaml`, `system/agent-instance-schema.yaml`, `agent_dispatch.yaml`, `a3t_deploy_init.yaml`, `a3t_resource_setup.yaml`, `a3t_triggers_setup.yaml`, `canvas_crud.yaml`, `test/hook-logger.yaml`, `test/test-agent-full.yaml`, `test/skill-*.yaml`; then the per-action CRUD duplicates once the UI calls the consolidated flows. Also delete the UI's `agent-template` → `a3t-chat-agent` branches (`AgentsList.svelte:43`, `AgentTemplateEditor.svelte:264,348`) and the `tpl-agent-template` seed. |
| **Rewrite** | `agents/support-agent.yaml` and `agents/ops-agent.yaml` become **templates** on the one engine (§3.3), with tools that exist |
| **Fix** | `a3t_schema_install.yaml`: no superadmin row and no hash in git. The bootstrap creates the first admin from a Secret, as `flowr-server` `bootstrap::seed_platform` already does for credentials. |
| **Merge into ADB** | `a3t-knowledge` (§3.5): one knowledge graph, not two |
| **Move** | `test/*` to `a3t/backend/flows/test/` that is never seeded into production; `integrations/*` to `flows/tools/` once each one has an owner (§3.3) |

After this there are about 15 flow files instead of 34, in four folders: `platform/`,
`system/`, `templates/` and `tools/`, plus `test/`.

---

## 3. Target structure

```
a3t/backend/
  platform/   a3t-bootstrap · a3t-{agents,templates,resources,users,audit,settings}  (CRUD, policy-tagged)
  system/     agent-engine · agent-run · memory-*  (namespace: system)
  templates/  chat-assistant · support · ops · research · worker  (a template = profiles, §3.3)
  tools/      remember · recall · link · http-fetch · web-scrape · query-sql · create-ticket · send-email
  policies/   memory.cedar (+ baseline.cedar, unchanged)
  test/       adb-selftest · canvas-dispatch · runtime-echo
```

### 3.1 One engine, one entry flow

- **`system/agent-engine`** stays the only engine (`call: a3t:agent`), with the same hook
  points as now.
- **`a3t/agent-run`** replaces both `a3t-chat-engine` and `a3t-chat-agent`. Its input is
  `{agent_id, message, session_id, ui_context}`. It loads the agent config, resolves the
  template's profiles into a full `config` (including `memory_resource`, which defaults
  to the org's ADB) and runs the engine.
- Chat, webhooks, triggers and canvases all start agents through `agent-run`. The UI no
  longer chooses a wrapper by template name.

### 3.2 The memory lifecycle

There are two paths:

- the **hot path** runs in Rust inside the turn and adds no latency beyond one ADB write;
- the **slow path** runs as `system/memory-*` flows, triggered by an ADB topic (F5
  `adb_topic`). That keeps the background work editable as flows, retried, and off the
  chat's latency.

```
turn ─▶ recall (hot) ─▶ LLM/tools loop ─▶ capture (hot) ─▶ publish memory.turn ──▶ topic
                                                                     │
           ┌──────────────────────────────┬─────────────────────────┼──────────────────────┐
           ▼                              ▼                         ▼                      ▼
   memory-reflect (per turn       memory-link (per turn)   memory-procedure (on   memory-consolidate
   or every N turns) →            episode→facts,           success with ≥2 tools) (session end / idle) →
   SEMANTIC FACTs                 episode→previous,        → PROCEDURAL           EPISODIC summary,
                                  facts→user, session                             forget WORKING
```

| Stage | Memory | What is written | Today |
|---|---|---|---|
| **Capture** (hot) | WORKING | Each message of the turn as its own item: user, assistant, and each tool call with its result (truncated). Fields: `session_id`, `role`, `turn`, `owner`. TTL is the session TTL. | User message only |
| **Checkpoint** (hot) | EPISODIC | One episode per turn: `intent` (the user's message), `actions` (the tools called and whether each succeeded), `outcome` (`answered` / `tool_error` / `max_iterations` / `procedure`), `reply` (truncated), `run_id` (the FlowR instance id, which joins it to the execution checkpoint), `session_id`, `owner`, `turn`. A workflow run that isn't a chat (worker template) writes one episode per completed step group and one at the end. | Reply text only |
| **Reflect** (slow) | SEMANTIC | Facts (F-L3), plus batch mode: every N turns or at session end. On by default for the chat-assistant profile. | Opt-in, per turn |
| **Link** (slow) | graph | `LINK` episode → each fact it produced (`derived_from`); episode → previous episode (`followed_by`); fact subjects to `user:<owner>` and `session:<id>` (`mentioned_in`). This is what gives P1.5's graph recall something to walk. | Never |
| **Learn** (slow) | PROCEDURAL | When a turn succeeds using two or more tools and the user doesn't correct it on the next turn, store `pattern = intent` and `steps = tool sequence`, with confidence 0.5 and raising it on reuse. Read stays at "suggest" (F4) by default. | Never written |
| **Consolidate** (slow) | EPISODIC / WORKING | At session end or after idle: a summary episode linked to the turn episodes, then `FORGET` the session's WORKING items. | — |

Decisions behind this:

- **Hot path in Rust, slow path as flows.** The capture and checkpoint writes are cheap
  and must not be lost when a template is edited. Reflection, linking and learning cost
  LLM calls or several queries, so they belong off the request path. As flows, an org can
  replace them, for example with its own extraction prompt.
- **The topic is ADB's own.** The engine does a `STORE INTO EPISODIC` and publishes to
  topic `memory.turn` in the same call. The `memory-*` flows are `adb_topic`
  subscribers. That needs F-L6 fixed (trigger flows run as the agent's owner, not with no
  principal), so the writes are attributed and policy-checked.
- **Memory tools for the LLM itself.** `tools/remember`, `tools/recall` and `tools/link`
  are thin flows over AQL that every template gets unless it turns them off. F3 always
  intended "facts are written deliberately, by the `remember` tool"; that tool doesn't
  exist yet.

### 3.3 Templates = engine + profiles

A template stops being a YAML copy of the engine. It becomes a small document of
**profiles** that `agent-run` merges into the engine's `config`:

```yaml
kind: AgentTemplate
name: chat-assistant
prompt: |            # role and behaviour only; the memory section is added by the engine
  You are …
profiles:
  memory: personal   # none | session | personal | team | knowledge
  tools:  [remember, recall, link, http-fetch]
  policy: standard   # standard | restricted | ops
  llm:    { temperature: 0.4, max_tokens: 4096 }
```

| Memory profile | Recall | Capture / checkpoint | Reflect | Link | Learn | Scope |
|---|---|---|---|---|---|---|
| `none` | — | — | — | — | — | — |
| `session` | WORKING, EPISODIC | on | off | episodes | off | `user:<owner>` |
| `personal` (chat default) | all four + graph | on | batch | on | on | `user:<owner>` |
| `team` | all four + graph | on | batch | on | on | `team:<id>` (read), `user:` (write) |
| `knowledge` | SEMANTIC + graph only | episodes | per turn, curated | on | off | `org:<id>` |

Starting templates, each with tools that exist:

- **chat-assistant**: the platform assistant, `personal` memory.
- **support**: KB recall through `recall` over a `knowledge` namespace, plus `create-ticket`.
- **ops**: worker-style checkpoints, `restricted` policy, and destructive tools gated to
  a human (the baseline already does this).
- **research**: `web-scrape` and `http-fetch` with the `knowledge` profile, writing
  curated facts.
- **worker**: no chat. Triggered by a webhook or an ADB topic, and checkpoints every step
  group.

The engine adds a short, fixed **memory section** to every system prompt: what it
remembers, and when to use `remember`, `recall` and `link`. Template authors don't write
it, so prompts stay about behaviour.

### 3.4 Policies

Make memory a policed resource like tools and flows:

1. **Emit the actions that already exist.** `agent_memory.rs` and the `call: adb`
   handler raise `memory:read` / `memory:write`. The resource attributes are `tier`,
   `namespace`, `scope`, `owner` and `op` (`store`, `forget`, `merge`, `link`).
2. **`policies/memory.cedar`:**
   - Agents read and write memory only `on_behalf_of` a user (the same guardrail as
     tools).
   - `user:<x>` scope: only `x`, and agents acting for `x`.
   - `team:` / `org:` scopes: members read. Writes go through the `knowledge` profile only.
   - `FORGET` and `MERGE ENTITY` are human-only. Admins are exempt, matching the
     destructive-tool rule.
   - Reflection and learned procedures are writes by the agent for the owner. They are
     allowed, and audited.
3. **Scopes instead of the client-side owner filter.** Pass `IN SCOPE "user:<owner>"`
   on reads and writes. This closes F-L3-a's "other readers see every owner's facts"
   and E-L12-a's "no default scope", and the filter in `visible()` goes away. Namespace
   stays per agent; scope is per owner, team or org.

### 3.5 One knowledge graph

`a3t-knowledge` currently runs `call: rotate` (RotatE embeddings) over its own Lance
table, while agents build facts in ADB SEMANTIC. Two graphs never meet.

Proposal:

- The triples move to ADB SEMANTIC FACTs in an `org:<id>` scope, the `knowledge` profile.
- The Knowledge page reads ADB through the existing SQL/AQL console path.
- RotatE stays as a scoring job over a fact snapshot. It reads ADB's fact feed (GR-15)
  rather than its own store.

This is the central KG that the KBLaM goal needs.

---

## 4. Order of work

| Phase | Work | Visible result |
|---|---|---|
| **A0: repo truth** | Export the live-only flows. Do the deletes in §2. Fix the superadmin seed. Commit `a3t-resources.example.yaml`. | A fresh install builds from the repo. One engine. |
| **A1: memory on by default** | `agent-run`; `memory_resource` defaulted; full-turn capture into WORKING; checkpoint episodes; memory section in the prompt; `remember` / `recall` / `link` tools. | Chat history, tool use and turns show up in WORKING and EPISODIC. The agent remembers on request. |
| **A2: background memory** | `memory.turn` topic; `memory-reflect` (batch), `memory-link`, `memory-procedure`, `memory-consolidate` as `adb_topic` flows; F-L6 trigger principal. | Facts, links and procedures appear without anyone asking. The graph fills. |
| **A3: templates** | AgentTemplate profiles; the five starting templates; a Memory tab showing the profile (replaces the per-tier checkboxes). | New agents get full memory by picking a template. |
| **A4: policies and scopes** | Emit `memory:*`; `memory.cedar`; `IN SCOPE user:/team:/org:`; drop the client-side owner filter. | Isolation is enforced by policy and by ADB, not by FlowR's filter. |
| **A5: one KG** | `a3t-knowledge` on ADB SEMANTIC; RotatE reads the fact feed. | The Knowledge page and agents share a graph. |

A1 and A2 can be tested locally against ADB the same way as the F-L3 checks in
`tests/flowr-integration.mjs`. Each phase ends with your A3T pass on vc-dev.

## 5. Decisions needed

1. **Reflection default.** Batch reflection on for the `personal` profile, every 5 turns
   plus at session end. That is one extra LLM call per 5 turns. Or keep it opt-in?
2. **Where reflection and linking run.** `adb_topic` flows (proposed: editable,
   decoupled, needs F-L6), or keep them in Rust as background tasks (simpler, not
   editable)?
3. **CRUD flows.** Keep the consolidated `a3t-<entity>` flows and delete the per-action
   ones (proposed), or the reverse?
4. **Old engine.** Is anything outside this repo using `system/agent-template` or
   `a3t-chat-agent`? Deleting them breaks such agents.
5. **Scope model.** Is `user:` / `team:` / `org:` the right set, and should a namespace
   stay per agent?
