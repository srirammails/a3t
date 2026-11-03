# Console: run, debug, monitor and checkpoints — UI specification

**Status:** draft v0.1 (1 Nov 2026) · **Scope:** A3T dash (`a3t/ui`) flow editor, Executions app, run views · **Owner:** Sri
**Depends on:** [FLOWR_QUEUE_CHECKPOINT_SPEC.md](FLOWR_QUEUE_CHECKPOINT_SPEC.md) v0.2 — journal (§5.5), debug runs (§5.17), run listing (§7.4), checkpoints (§5.7 M5b).

---

## 1. Goals

From the flow editor a developer can:

1. **Run** a flow with inputs, and re-run it with the same or edited inputs.
2. **Debug** it: set breakpoints on any step, pause there, inspect the step's input, output and context, edit the input, then continue or step.
3. **Monitor** any run they can see — their own or a production one — live, step by step, on the flow graph.
4. **See checkpoints**: what was recorded after each step, what changed, and start a new debug run from any of them.

The same run views open from the Executions app, an agent's activity, a trigger's history and a chat message.

### Non-goals

- Editing a production run's data. Only debug runs accept edits.
- Debugging code inside handlers (Rust or Python). The unit is a workflow step; fine-mode agent effects (§5.12) show as child steps.
- A separate "debugger" app. Everything lives in the flow editor and a shared run view.

---

## 2. Where things live today

- **Flow editor:** `FlowEditorPane.svelte` — a graph (`@xyflow/svelte`, nodes from `workflowParser.ts`: `StepNode`, `ControlNode`, `SubFlowNode`, `AgentNode`, …), a YAML editor (`YamlEditor.svelte`), and a form view (`SchemaForm`). It has no run action.
- **Executions app:** `ExecutionsPage.svelte` — a table of runs (namespace, name, status, step, timings), a detail panel with input, output and error, and a 4 s auto-refresh. Its list is currently stale (engine M0 fixes it), and the step column is always empty.
- **Chat:** streams tokens and tool progress per session (`token_stream`).

The new views reuse the editor's graph and the dash's existing building blocks (SlidePanel, FoldSection, EntityCard, the project context and pills).

---

## 3. Flow editor layout

A **Run** bar is added to the editor header, and a bottom **Run panel** opens under the graph/YAML when a run is active or selected.

```
┌ Flow: a3t-examples / order-intake   v1.4.0   [Graph | YAML | Form]        ▶ Run ▾   🐞 Debug   ⧉ Runs (12) ┐
├───────────────────────────────────────────────────────────────────────────────────────────────────────────┤
│  graph                                                                                                    │
│   ┌─────────┐    ┌─────────────┐    ┌───────────────┐    ┌──────────┐                                     │
│   │ validate│──▶ │● chargeCard │──▶ │ for each item │──▶ │ notify   │        ● breakpoint                 │
│   │   ✓ 3ms │    │  ⏸ paused   │    │   ✓ 12/40     │    │          │        ⏸ current step               │
│   └─────────┘    └─────────────┘    └───────────────┘    └──────────┘        ✓ ✗ ↻ step status            │
├──────────────────────────────── Run panel ────────────────────────────────────────────────────────────────┤
│ run 01a19…f7d4 · debug · paused before /do/1/chargeCard · 1.2 s        [▶ Continue] [⤼ Over] [⤵ Into]     │
│                                                                          [⤴ Out] [⇥ Run to] [■ Stop]       │
│ ┌ Steps ───────────────┐ ┌ Step: chargeCard ─────────────────────────────────────────────────────────────┐│
│ │ ✓ validate      3 ms │ │ [Input] [Task] [Output] [Context] [Journal] [Logs] [Policy]                    ││
│ │ ⏸ chargeCard         │ │  { "amount": 42.5, "currency": "EUR", "customer": "c_19" }      ✎ Edit input   ││
│ │   for each item      │ │                                                                               ││
│ │   notify             │ │  class: effect · in-doubt: fault · idem key 7f3a…                             ││
│ └──────────────────────┘ └───────────────────────────────────────────────────────────────────────────────┘│
│ [Timeline] [Checkpoints] [Output] [Events]                                                                │
└───────────────────────────────────────────────────────────────────────────────────────────────────────────┘
```

- The graph overlays each node with its status in the selected run: not reached, running, paused, completed (with duration), failed, skipped (replayed from journal is shown as completed with a ↻ mark), and for loops a progress count (`12/40`).
- Clicking a node selects that step in the Run panel. Clicking a step in the Steps list selects and centres the node.
- The YAML view shows the same: status icons and breakpoint dots in the gutter, the current step's lines highlighted.
- **Runs (n)** opens the flow's recent runs (newest first) to pick one to view; the panel then shows that run, live or finished.

---

## 4. Run with inputs

**▶ Run** opens the input dialog; **▶ Run ▾** offers *Run with last input* and saved presets.

- **Input form:** generated from the flow's `input.schema` with `SchemaForm` when present; otherwise a JSON editor. Validation uses the same schema the engine checks, so errors show before the run starts.
- **Sources:** last input, a saved preset, or *Use input of run…* (any visible run of this flow).
- **Presets:** saved per flow in the project as a resource of type `run-preset` (name, input, notes), so a team shares them. Presets never hold secrets; fields the schema marks `format: password` are refused in a preset.
- **Version:** runs the version open in the editor. Unsaved edits ask to *Save and run* or *Run saved version* — a run never executes text that isn't stored, because it is pinned by `def_hash` (engine §5.8).
- **Where it runs:** shows the target (`default` pool in phase 1; the placement result in phase 3). Isolated-runtime rules apply exactly as for any run.
- After start the Run panel opens on the new run and follows it live.

---

## 5. Debug: breakpoints and stepping

**🐞 Debug** starts a debug run (engine §5.17) with the input dialog, the current breakpoints, and an option *Pause on start*.

### 5.1 Breakpoints

- Toggle by clicking a node's left edge in the graph, or the YAML gutter. A breakpoint is a step key (`/do/1/chargeCard`), so it survives layout changes; editing the YAML so a key no longer exists marks the breakpoint *orphaned* (hollow) instead of moving it silently.
- Kinds (right-click / breakpoint popover):
  - **Before step** (default) — pause with the computed input, before anything runs.
  - **After step** — pause with the output and the context after exports, before the next step.
  - **On error** — pause when the step fails, before `try`/`catch` or retry handling.
  - **Condition** — a jq expression over `{input, context, iteration}`; pause only when true. Evaluated by the engine, never in the browser.
  - **Hit count** — pause on the nth hit (useful inside `for`).
  - **Pattern** — every step of a kind (`call:http`, any `effect`), from the Breakpoints list.
- Breakpoints belong to the user and the flow (stored per user, not in the YAML) and are listed in a **Breakpoints** fold in the Run panel, where they can be disabled without removing them.
- Changing breakpoints while a debug run is live updates it (engine accepts breakpoint updates).

### 5.2 Paused

- The run is `suspended` at the breakpoint and released by the runtime; the pause is durable. Closing the tab, reloading, or a restart leaves it paused; reopening the flow (or the run link) shows it paused.
- Controls, mapped to engine debug commands:
  - **Continue** — run to the next breakpoint or the end.
  - **Step over** — run this step (including all of a subflow or agent call), pause after it.
  - **Step into** — enter a subflow's first step, a `for` loop's first iteration, a `try` block, or a fine-mode agent call's first effect.
  - **Step out** — finish the current subflow / loop / block, pause after it.
  - **Run to here** — from a node's context menu: continue until that step.
  - **Stop** — cancel the run.
- **Edit input** (before-step pauses only): edit the computed input in a JSON editor; *Apply* journals a `DebugEdit` with your identity and the step then runs with it. Edited steps carry an ✎ mark in every view of the run, for good.
- A debug run that stays paused longer than `debug_pause_ttl` (default 24 h) is cancelled with the reason `debug session expired`.

### 5.3 Effects in debug runs

Debug runs execute real effects. Before **Continue**/**Step** passes an `effect` step that has no idempotency key, the panel names it ("This will call `POST /payments` for real") once per run, with *Don't ask again for this run*. Mocking effects is a later phase (§10, U5).

---

## 6. Step inspector

Selecting a step (in any run view) shows tabs:

| Tab | Content |
|---|---|
| **Input** | The step's input after `input.from`, as the engine journaled it (masked, engine §5.16). For a before-step pause, the input about to be used. |
| **Task** | The task definition with expressions evaluated (`with:` after evaluation), plus the raw YAML. |
| **Output** | Output after `output.as`; for a failed step, the error as Problem Details (type, title, status, detail, instance). |
| **Context** | `$context` after the step's `export.as`, with a diff against the previous step (added / changed / removed keys). |
| **Journal** | The step's records: `StepStarted`, `StepCompleted`/`StepFailed`, attempts with timestamps and durations, idempotency key, step class, in-doubt resolution. |
| **Logs** | Log lines from the step's `flow_step` span (captured per run and step, last N lines). |
| **Policy** | The Cedar/AESOP decision for the step: action, resource, principal, decision, reason. |

- Large values (over 64 KiB) show a size and *Load* button; values are fetched from the blob store on demand.
- **Copy as input** on any output or context value creates a run input or preset from it.
- For a `for` step, an iteration picker (`each[17]`); for a goto revisit, a visit picker (`#3`).
- For fine-mode agent steps, child rows for each model and tool effect, with prompt/response and tool input/output.

---

## 7. Monitor (attach to a run)

Any run the user can see opens in the same Run panel, read-only unless it is their debug run.

- **Live:** subscribes to the run's events (journal appends and status changes; engine `Notifier` topic `run:<id>` over SSE). The graph overlay, Steps list and Timeline update as records arrive; *Follow* keeps the current step centred.
- **Attach** from: the flow editor's *Runs* list, the Executions app, an agent's activity, a trigger's history, a chat message (*View run*). A run that is running now shows a pulsing ● in all those lists.
- **Timeline tab:** a Gantt of steps (start, duration, waits as hatched bars, retries as segments), subflows as nested lanes, with the run deadline as a line when set.
- **Events tab:** the raw journal stream (seq, time, kind, step key), filterable by kind.
- **Chat runs:** a link to the session's token stream; tokens are not journaled.
- **Actions** on a production run, by permission: Suspend, Resume, Cancel (with reason). No breakpoints, no edits.
- A finished run shows the same views from the journal, without the live subscription.

---

## 8. Checkpoints

**Checkpoints tab** in the Run panel.

- **Phase 1 (replay engine):** each committed step is a resume point. The list shows seq, step key, time, and the context size after it. Selecting one shows the position (step key), the context at that point, and a diff to the previous one.
- **M5b (snapshots):** entries also show snapshot size and the frame stack (for example `do[3] › for each[17] › do[0]`), and *Download snapshot* (JSON; superadmin and namespace admins only).
- **Re-run from here** (debug, Write access): starts a **new** debug run of the same `def_hash` whose journal is seeded with the original run's records up to that point. Steps before it replay; steps after it execute again. The confirm dialog lists the `effect` steps that will execute again. The new run links back to its source run, and the source run is not changed.
- In-doubt steps (a `StepStarted` without completion) show as ⚠ with the policy applied (fault / retry / approval / reconcile) and, for `approve`, the approval request inline.

---

## 9. Executions app changes

- Reads the engine's run listing (§7.4 of the engine spec): newest first, filtered on the server by the selected project (or all visible projects), status, flow, time range and trigger; cursor paging (*Load more*), no input/output in the list.
- Columns: status, flow (with project pill), trigger, started by, started, duration, **current/last step** (now populated), and a ● for live runs.
- Row click opens the run in the shared Run panel as a slide-in (same views as §6–§8), with *Open in editor* to jump to the flow editor with that run selected.
- Bulk actions for operators: cancel selected, retry failed (a new run with the same input).
- **Dead-letter** fold: runs faulted as `poisoned` or `interrupted`, and runs with unresolved in-doubt steps.

---

## 10. Phasing (tracks the engine milestones)

| UI | Delivers | Needs (engine) |
|---|---|---|
| **U0** | Executions list fixed (fresh, newest first) | M0 |
| **U1** | ▶ Run with inputs from the editor (schema form, last input, presets); run result in the Run panel (input, output, error, duration); flow's Runs list | Works on today's engine + M0; M3 for server-side listing |
| **U2** | Monitor: graph overlay, Steps list, step inspector (Input, Output, Context, Journal), Timeline, Events, live updates; Executions app on the run listing | M3 (run listing, `Notifier`), M5 (journal) |
| **U3** | Debug runs: breakpoints (all kinds), durable pause, stepping, edit input, effect warnings | M5 (step hook), M6 (debug runs §5.17) |
| **U4** | Checkpoints tab, re-run from here, dead-letter fold, Logs and Policy tabs | M5 (+ M5b for snapshots), log capture per step |
| **U5** | Mock effects in debug runs: answer `effect` steps from a chosen earlier run's recorded outputs instead of executing them | Engine support for a replay source on a debug run (later) |

U1 can ship before the new engine: it uses `execute_flow` with an input and the existing run status call.

---

## 11. API the console needs

REST under `/v1` (MCP tools mirror them for agents and the CLI):

| Method | Path | Purpose |
|---|---|---|
| POST | `/runs` | Start: `{flow, version?, input, debug?, breakpoints?, pause_on_start?}` → `{run_id}` |
| GET | `/runs` | List: `namespace[]`, `status[]`, `flow`, `trigger`, `from`, `to`, `cursor`, `limit` |
| GET | `/runs/{id}` | Run row: status, phase, current step, timings, principal, trigger, parent, debug flag |
| GET | `/runs/{id}/journal?after=seq` | Journal records |
| GET | `/runs/{id}/steps/{step_key}` | Step view: input, task (evaluated), output, context, attempts, class, policy decision |
| GET | `/runs/{id}/checkpoints` · `/checkpoints/{seq}` | Resume points / snapshots |
| GET | `/runs/{id}/events` | SSE: journal appends and status changes |
| GET | `/blobs/{hash}` | Large values on demand (same access check as the run) |
| PUT | `/runs/{id}/breakpoints` | Update a debug run's breakpoints |
| POST | `/runs/{id}/debug` | `{command: continue \| step_over \| step_into \| step_out \| run_to, step_key?}` |
| POST | `/runs/{id}/debug/edit` | `{step_key, input}` while paused before that step |
| POST | `/runs/{id}/cancel` · `/suspend` · `/resume` | Operations |
| POST | `/runs/{id}/rerun` | `{from_seq?, input?, debug?}` → new run linked to the source |
| GET/PUT | `/flows/{ns}/{name}/presets` | Run presets (stored as `run-preset` resources) |

All responses carry values already masked (engine §5.16).

---

## 12. Permissions

| Action | Needs |
|---|---|
| See a run, its steps, journal, checkpoints, logs | Read on the run's namespace, and the `can_see_run` rule (a platform flow's runs only to whoever started them) |
| Run a flow | Run on the namespace (today's guard) |
| Debug run: start, breakpoints, step, edit input, re-run from here | Write on the namespace |
| Suspend / resume / cancel a production run | Write on the namespace, or the org's operator role |
| Download a snapshot | Namespace admin or superadmin |
| Logs and Policy tabs | Read; the Policy tab hides other principals' details unless securityadmin |

Every debug command, edit and operation is audit-logged with the principal (the existing audit log).

---

## 13. Open questions

1. Should breakpoints be shareable (saved with the flow for the team), or stay personal? Proposal: personal, with *Share with project* later.
2. Allow a time-boxed breakpoint on a production run (an operator pauses the next run of a flow)? Proposal: no in v1; use a debug run with a copied input.
3. Log capture per step: keep the last N lines per step in the journal store, or query the log backend by `run_id`/`step_key`? Proposal: last 200 lines per step in the store; full logs stay in the cluster log backend.
4. Where do presets live for personal projects? Proposal: same `run-preset` resource, in the personal project.

---

## 14. Studio shell (reusable) and Flow Studio — added 1 Nov 2026

> Extended by **`A3T_STUDIO_ARCHITECTURE.md`** (2 Nov 2026): the Studio as a
> developer channel for web and VS Code, built from kinds, a workspace and
> adapters. The run, debug, monitor and checkpoint behaviour in this spec is
> unchanged and is what the Studio's Run documents and Runs panel implement.

A reusable IDE-style shell (`a3t/ui/src/lib/components/studio/`) for A3T apps — Flow Studio first; DB Console, Logs and Runtime next.

- **`StudioShell`**: optional left and right regions, the editor area, and an optional bottom region under it. Each region is a tabbed panel group (`Region`) that resizes by dragging its splitter, **minimizes to a stack** on its edge (vertical labels left/right, a strip at the bottom; click a label to bring that panel back), and **maximizes** to fill the studio (⤢ or double-click a tab; Esc restores). The editor area maximizes the same way.
- **`Studio`** (`layout.svelte.ts`): the layout per app (`new Studio('<app>')`), kept per browser under `a3t.studio.<app>`.
- **`DocTabs`**: open documents with close buttons (● for unsaved), middle-click to close, double-click to maximize the editor.
- An app supplies its panels (id, title, what they render, an optional badge) and its editor area.

**Flow Studio** (replaces the Flow Editor page; the shell's flat flow list is gone):
- **Explorer** (left): per project — **Flows**, **Resources** with a folder per kind (LLM, Databases, MCP Servers, Secrets, …), **Triggers**, **Templates**, **Canvases**; counts, a filter, remembered folder state; one node per project when several are in view; **+** on Flows starts a new flow in that project.
- **Editor area**: flows open as tabs (graph / code / form, the Run/Debug split button and Deploy per tab); other items open as read-only previews with *Open in <app>*. Open tabs are remembered (texts reload from the server).
- **Bottom**: **Runs** (the run panel and run view for the active flow; Run/Debug brings it up) and **Problems** (YAML errors, a missing `document` field, no steps, duplicate step names; the count shows on the tab).
