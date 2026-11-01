# FlowR queue and checkpointing — specification and implementation plan

**Status:** draft v0.2 (reviewed against the code, 1 Nov 2026) · **Scope:** FlowR CE and EE execution engine · **Owner:** Sri
**Companion:** [FLOWR_CONSOLE_RUN_DEBUG_UI_SPEC.md](FLOWR_CONSOLE_RUN_DEBUG_UI_SPEC.md) — run, debug, monitor and checkpoint views in the console.

---

## 0. Review: changes from v0.1

v0.1 was checked against `flowr-ce` (worker, queue, stores), `swf-runtime` (executor) and the live vc-dev system. The direction holds. These changes make it fit the code as it is, and fill gaps:

| # | Change | Why | Sections |
|---|---|---|---|
| R1 | **Phase 1 resumes by journal replay**, not an interpreter snapshot. The executor re-runs the flow from the start; every step with a completion record returns its recorded output and context instead of executing. Interpreter snapshots become an optimisation (M5b). | `swf-runtime` is a recursive async interpreter (`execute_task_list` → `execute_for`/`execute_try`/`execute_fork`); it has no explicit frame stack to serialise. Replay needs a hook around each step, not a rewrite into a state machine. | 5.7, 13 |
| R2 | **Every completed step is journaled, pure steps included**, with its output *and* the context after exports. Pure-step records ride in the next commit (no extra fsync). | Replay must restore `$context` (`export.as`) exactly, and jq `now` / input-dependent ordering would otherwise diverge on re-evaluation. | 5.4, 5.5 |
| R3 | **Durability classes per flow**: `durable` (default) and `ephemeral`. Ephemeral runs get a `runs` row and a `StartRun` task but no step journal; after a crash they fault as `interrupted`, they are not resumed. | Most runs today are the dash's own CRUD/list flows (`a3t-agents`, `a3t-templates`, `a3t-projects`: ~200 runs per 9 h). Journaling each of their steps would add an fsync per step to every page load for no benefit. | 5.15 |
| R4 | **Keep UUIDv7 `instance_id` as `run_id`**, no ULID. | Already time-ordered, already the id in MCP tools, the UI, logs and the token stream. | 3 |
| R5 | **Evolve `flowr-storage` traits instead of a parallel crate.** Replace `ExecutionQueue`, `CheckpointStore` and `ExecutionStore`; delete the unwired executors (`flowr-ce-core`, `flowr/flowr-core`) and `flowr-worker` once M9 has taken what it needs. | Two trait sets side by side is how the current dead code happened. | 6, 13 |
| R6 | **Claim + ownership is one store operation** (`RunStore::claim_runs`), not two traits joined by the caller. | §5.2 needs both in one transaction; separate traits can't promise that across backends. | 5.2, 6 |
| R7 | **`runs` is the execution history.** It carries flow ref, version, `def_hash`, principal, trigger, input/output refs, error and timestamps, indexed by `(namespace, created_at)` and `status`. Lists read it directly; the LanceDB projection becomes optional. | Today's execution list returns runs from August because LanceDB applies `limit` before any ordering. The new table must answer "newest runs in these projects" with an index. | 7, 7.4 |
| R8 | **Identity and secrets never come from a snapshot.** `$context._principal` is restored from the `runs` row; `$secrets` and resolved credentials are never serialised; journal values pass the same masking as `masked_config`. | A tampered or leaked snapshot must not change who a run acts as or expose a secret. | 5.7, 5.16 |
| R9 | **Live streams are not journaled but must travel.** Chat tokens and agent progress go through `token_stream` (an in-process ring per session). Phase 2 adds a `Stream` message to the worker protocol so a remote runtime can feed the console's ring. | Otherwise chat breaks the day a run leaves the console pod. | 5.12, 9 |
| R10 | **Operations controls map onto the run model.** The in-memory ops ledger (`admit`, `register_run`, Terminate, pause) becomes claim-time admission plus `cancel_requested`; the owner also gets an in-process notification for an immediate stop. | Pause and Terminate exist today and must keep working. | 5.13, 5.14 |
| R11 | **Fork branches are sequential today.** Phase 1 keeps them sequential with separate branch keys; parallel branches come later. Open question 2 is deferred. | `execute_fork` iterates branches in order. | 5.3, 15 |
| R12 | **Agent fine mode is a handler API**, `StepContext::effect(key, fut)`, used first by the Rust agent engine (`call:a3t:agent`, `call_agent.rs`), later by the Python SDK. | The agent loop is native Rust, not the SDK. | 5.12 |
| R13 | **Run and task timeouts are enforced.** The SWF document `timeout` and task `timeout` are not implemented today. | A hung step currently holds its task until someone terminates it. | 5.14 |
| R14 | **Debug runs** (breakpoints, stepping, durable pause) are engine features: records `Paused`/`DebugCommand`/`DebugEdit` and a `debug` flag on the run. | The console spec needs them; they share the step hook with journaling. | 5.17 |
| R15 | **M0 quick fixes first**: correct ordering in the current execution list, and mark runs left `Running`/`Queued` after a restart as faulted (`interrupted by restart`). | Independent of the new store and visible to users now (a run from 16 Sep still shows Running). | 13 |
| R16 | **Why not LanceDB for the queue** is stated: no multi-row transactions, no compare-and-set. | Avoids the question coming back. | 7 |

---

## 1. Why this exists

Today CE and EE schedule and execute runs identically:

- Every entry point (MCP `execute_flow`, chat, agent messages) enqueues into `InMemoryExecutionQueue` (a `VecDeque`). Triggers (webhook, cron, topic, event) bypass the queue and `tokio::spawn` `execute_workflow` directly.
- A single `run_worker` loop dequeues every ~100 ms and spawns one task per run. No claim, lease, ack or concurrency limit.
- `swf-runtime` executes all steps of a run in one in-memory task. Subflows run inline in a child executor.
- Waits sleep in the process (`tokio::time::sleep`); approvals and human input live in an in-memory map. Queued and running runs live only in memory. LanceDB gets one execution row at start and one at end; `current_step` is never set.
- **A restart loses everything queued or running**, and leaves its record `Running` forever.
- EE adds auth, Cedar, the project guard and the controller, but every run still executes in the console pod. Runtime pods (`rt-worker`, `cv-*`) have their own empty queues and do no work (0 runs vs 202 on the console in a 9 h sample).
- The distributed pieces in `flowr/` (`flowr-core`, `flowr-worker`, `flowr-suspend`, the Dragonfly queue/storage) are not wired into `flowr-server`.

The product claims checkpoint-after-every-task, resume without repeating side effects, and placement onto runtimes. This spec closes that gap.

---

## 2. Goals and non-goals

### Goals

1. **Durability.** No accepted run is lost on process crash, restart or redeploy.
2. **Resume from where it stopped.** A run resumes at the step where it stopped; completed steps are not executed again.
3. **No duplicated completed side effects.** A step journaled as completed is never re-executed.
4. **Explicit handling of in-doubt side effects.** A step that may or may not have happened is resolved by policy, never silently retried.
5. **One durable path.** API calls, chat, triggers, cron, waits, approvals and subflows all go through the same queue and journal.
6. **Pluggable storage.** Queue, journal and notifier are contracts with swappable backends (SQLite for CE; Postgres for EE; Iggy and ADB where they fit).
7. **Ready for distribution and pools.** Phase 1 design makes phases 2 and 3 additive, not rewrites.
8. **Debuggable.** The same step hook that journals a step can pause it (breakpoints) and report it live (monitor).

### Non-goals (for now)

- Exactly-once side effects against arbitrary external systems (impossible in general; we provide at-most-once-after-completion plus idempotency keys).
- Live migration of in-flight runs to a new workflow definition version.
- Cross-region replication.
- Parallel fork branches (R11).

---

## 3. Terms

| Term | Meaning |
|---|---|
| **Run** | One execution of a workflow definition. Identified by `run_id` = today's `instance_id` (UUIDv7, time-ordered). |
| **Definition** | An immutable, content-addressed workflow document (`def_hash` = SHA-256 of the stored YAML). |
| **Step** | One task execution inside a run, identified by a deterministic `step_key`. |
| **Queue task** | A unit of work in the `TaskQueue` (start a run, resume a run, or in phase 3 a remote step). Not the same as a workflow task. |
| **Lease** | Time-bounded exclusive claim on a queue task. Extended by heartbeat. |
| **Run ownership** | Exclusive right to advance a run, carried by a monotonically increasing `epoch` (the fencing token). |
| **Journal** | Append-only, per-run record of what happened. Source of truth for resume. |
| **Replay** | Resume by re-running the interpreter from the start, returning recorded results for completed steps (phase 1). |
| **Checkpoint** | A serialised interpreter snapshot; makes resume O(1) instead of a replay (M5b). |
| **Durability class** | `durable` (journaled, resumable) or `ephemeral` (run record only, faulted on crash). |
| **Pool** | A named group of runtimes (`namespace/pool`). Phase 1 has one implicit pool, `default`. |

---

## 4. Architecture by phase

```
Phase 1 (CE and EE console, one process)    Phase 2 (EE, distributed)                 Phase 3 (EE, pools)
┌──────────────────────────────┐           ┌──────────────┐  gRPC stream (pull)   ┌──────────────┐
│ entry points ──► TaskQueue   │           │ control plane│◄──────────────────────│ runtime pool A│
│                  │           │           │  WorkerGW    │◄──────────────────────│ runtime pool B│
│        claim loop (N slots)  │           │  Queue/Journal backends (Postgres;   │ customer pool │
│        → execute → Journal   │           │  Iggy/ADB for events & projection)   └──────────────┘
│ SQLite file: queue, runs,    │           └──────────────┘
│ journal, schedules, waits    │           Runtimes never touch storage directly. Steps with a different
└──────────────────────────────┘           `pool` become remote queue tasks for that pool.
```

- **Phase 1:** durable queue and journal in one SQLite file. A claim loop with a concurrency limit claims runs, executes, journals. Everything goes through the queue. In EE the console pod runs phase 1 on its persistent volume; runtime pods stay idle until phase 2.
- **Phase 2:** the same contracts behind a control plane. Runtimes connect over a Tonic gRPC stream and pull work. Backend: Postgres (to be provisioned on vc-dev; open question 7).
- **Phase 3:** pools with capability labels. Per-step `pool` routing turns a step into a remote queue task.

---

## 5. Execution model

### 5.1 Run lifecycle

```
          enqueue StartRun
                │
   ┌────────────▼────────────┐   claim + acquire ownership (epoch+1)
   │        pending          │───────────────────────────────┐
   └─────────────────────────┘                               ▼
                                                   ┌──────────────────┐
         wake-up (timer / signal / event /         │     running      │──── step loop:
         approval / remote step / debug command)   └───┬───────┬──────┘     journal (+ checkpoint)
         enqueues ResumeRun ◄──────────────────────────┘       │
   ┌─────────────────────────┐   release ownership,           │ terminal
   │   waiting / suspended   │◄──ack queue task               ▼
   └─────────────────────────┘                    completed / faulted / cancelled
   suspended: only an explicit resume (or a debug command) enqueues ResumeRun.
```

Status values follow the Open Workflow status phases: `pending`, `running`, `waiting`, `suspended`, `cancelled`, `faulted`, `completed`.

Compatibility with today's `ExecutionStatus` (API responses keep the old names for one release, alongside the new `phase` field):

| Today | New phase |
|---|---|
| `Queued` | `pending` |
| `Running` | `running` |
| `Suspended` | `waiting` or `suspended` |
| `Completed` | `completed` |
| `Failed` | `faulted` |
| `Cancelled` | `cancelled` |

### 5.2 Ownership and the queue

1. A runtime calls `RunStore::claim_runs(pool, worker, n, lease)`. In **one transaction** the store leases up to `n` visible queue tasks and, for each, acquires run ownership:
   `UPDATE runs SET owner = ?, epoch = epoch + 1, owner_lease_until = ? WHERE run_id = ? AND (owner IS NULL OR owner_lease_until < now) RETURNING epoch`.
   A task whose run is owned elsewhere is released with a short delay inside the same transaction.
2. Admission runs inside the claim (R10): a task for a paused deployment or an over-limit namespace (§5.14) stays queued.
3. The runtime heartbeats the queue lease and run ownership on one timer, default every `lease/3`.
4. Every journal append and checkpoint write carries `epoch`. The store rejects writes where `epoch != runs.epoch` (**fencing**). A rejected write means ownership was lost: the runtime stops the run immediately and performs no further side effects.
5. When the run reaches a wait, a pause or a terminal state, the runtime writes the journal record, releases ownership (`owner = NULL`) and acks the queue task in one transaction.

**Stickiness** is a cache optimisation only. Any runtime must be able to resume any run from storage alone.

### 5.3 Step keys

`step_key` is deterministic and stable across resumes, built from the task reference (JSON pointer) plus dynamic segments. The parser already knows each task's position; `swf-parser` gains a `reference` on every `NamedTask` so the executor and the console editor (breakpoints) use the same key.

| Construct | Key segment |
|---|---|
| Top-level task | `/do/2/chargeCard` |
| `for` iteration | `/do/3/each[17]/do/0/sendMail` |
| `fork` branch | `/do/4/fork/branches/1/do/0/x` |
| Subflow (`run: workflow`) | `<parent_key>/sub/<child_run_id>` (child has its own journal scope) |
| `try` → `catch.do` | `/do/5/try/catch/do/0/notify` |
| Agent effect (fine mode) | `<agent_step_key>/fx/<tool_use_id>` or `/fx/model/<seq>` |
| Retry attempt | not part of the key; `attempt` is a separate field |
| `then: <name>` (goto) revisit | `#<visit>` suffix on the second and later visits to the same task: `/do/1/poll#3` |

Loop indices, fork branch indices and goto visit counts come from the replayed run, so keys are reproducible on resume.

### 5.4 Step classes

Each task is classified at load time (overridable in `metadata.a3t.stepClass`):

| Class | Examples | On resume |
|---|---|---|
| `pure` | `set`, `switch`, `do`, `for`, `fork`, `raise`, transforms | Journaled when completed (R2): recorded output and context are used. Re-evaluated only if after the last commit. |
| `read` | `call` with `GET`, memory `RECALL`, `listen` | Completed: recorded. In doubt: re-execute. |
| `idempotent` | Effects that accept an idempotency key (`Idempotency-Key` header, MCP tools declaring it) | Completed: recorded. In doubt: re-execute with the **same** idempotency key. |
| `effect` | Everything else that changes the world | Completed: recorded. In doubt: **in-doubt policy** (§5.6). |

Default for `call`, `run`, `emit` and MCP tools is `effect` unless declared otherwise. Built-in handlers declare their class in Rust (`TaskHandler::step_class`), next to the existing `policy_action`.

### 5.5 Step execution protocol (write-ahead)

The executor calls one hook around every task (`StepHook`, the same one the debugger uses, §5.17):

```
1. rec = journal.get(run, step_key)            // from the in-memory replay index, not a DB read per step
   - Completed → return recorded output + context, skip execution
   - Failed (terminal) → apply recorded failure, skip execution
   - Started without Completed → in-doubt → §5.6
   - none → continue
2. pure step → execute; buffer StepCompleted{output, context_after}; go to next step
3. non-pure step →
   journal.append(pending pure records + StepStarted{ step_key, attempt, idem_key, input_hash }, epoch)   // durable before the effect
4. execute (pass idem_key = hash(run_id, step_key) to the tool / HTTP call where supported)
5. txn {
     journal.append(StepCompleted{ step_key, output_ref, context_ref } | StepFailed{ error }, epoch)
     [M5b] checkpoints.put(run, seq, interpreter_snapshot, epoch)
     outbox.put(usage_event, lifecycle_event)
   }
```

`idem_key` is stable across attempts of the same logical step, so a retry after a crash presents the same key to the external system. `context_ref` points at the context after the step's `export.as`, stored as a blob (deduplicated, so an unchanged context costs nothing).

### 5.6 In-doubt side effects

A step with `StepStarted` but no completion record may or may not have happened. Policy per task (`metadata.a3t.inDoubt`), default `fault`:

| Policy | Behaviour |
|---|---|
| `fault` (default) | Fault the step with `https://open-workflow-specification.org/spec/1.0.0/errors/runtime`, detail `in-doubt side effect`. Normal `try`/`catch` applies. |
| `retry` | Re-execute with the same `idem_key`. Allowed only for `idempotent` steps. |
| `approve` | Move the run to `waiting` with an approval request: an operator confirms whether the effect happened. |
| `reconcile` | Call a declared reconciliation task (e.g. `GET /payments?idem=...`) and use its result as the step output. |

### 5.7 Resume: replay (phase 1) and checkpoints (M5b)

**Phase 1 — replay.** Resume loads the run's journal into an in-memory index keyed by `step_key`, then calls `executor.execute` from the start with the pinned definition and the original input. The step hook (§5.5) returns recorded results for every completed step, so no effect repeats and expressions are not re-evaluated for completed steps. The first step without a completion record runs normally (or goes through §5.6). Cost: one pass of the interpreter over the completed prefix, with no I/O; acceptable for flows up to thousands of steps.

**M5b — snapshots.** When replay time matters (long agent runs, big loops), add interpreter snapshots so resume starts at the last checkpoint:

```rust
struct Checkpoint {
    run_id: RunId,
    seq: u64,                 // journal sequence this snapshot reflects
    def_hash: DefHash,        // pinned definition
    frames: Vec<Frame>,       // position stack: do/for/fork/try/subflow frames
    context: BlobRef,         // $context without _principal and $secrets (§5.16)
    last_output: BlobRef,     // input to the next task
    loop_state: Vec<LoopState>,
    fork_state: Vec<ForkState>,
    pending_waits: Vec<WaitRef>,
}
```

This needs the executor's recursion turned into an explicit frame stack, which is why it is not phase 1. Checkpoints are written in the same transaction as a `StepCompleted`, at most every `checkpoint_every` effect steps (default 1). Resume = latest checkpoint + journal records after its `seq`.

In both modes:

- Large values go to `blobs` (content-addressed). Inline limit 64 KiB; hard limit per value 8 MiB (configurable). A value over the hard limit faults the step with `value too large`.
- On resume `$context._principal` is rebuilt from `runs.principal`, never from a stored context (R8).

### 5.8 Definition pinning

A run records `def_hash` at start and resumes against that exact definition, even if a newer version is published. On first use the stored YAML is copied into `definitions`. Today's version registry (`active_version`, `run: workflow version:`) keeps deciding *which* version a new run starts on; `def_hash` makes it immutable for that run.

### 5.9 Waits, timers, signals, approvals

All long waits are durable. When a run reaches one, the runtime journals it, releases ownership and acks.

| Construct | Journal record | Wake-up |
|---|---|---|
| `wait` | `TimerSet{ fire_at }` | `ResumeRun` enqueued with `visible_at = fire_at` |
| `listen` | `WaitForEvents{ correlation }` + row in `waits` | Matching event appends `EventReceived`, enqueues `ResumeRun` |
| Human approval / input (`InputManager`, `authz` approvals) | `ApprovalRequested{ id, approver }` + row in `waits` | Decision appends `ApprovalDecided`, enqueues `ResumeRun` |
| Timeout on any wait | Second `ResumeRun` with `visible_at = timeout` | First wake-up wins (see dedupe) |
| Debug pause (§5.17) | `Paused{ step_key, reason }` | A debug command appends `DebugCommand`, enqueues `ResumeRun` |
| Remote step (phase 3) | `StepScheduled{ pool }` | Result appends `StepCompleted`, enqueues `ResumeRun` |

**Short waits** (`wait` under `inline_wait_max`, default 5 s) sleep in place while the run keeps ownership and heartbeats; they are still journaled (`TimerSet` + completion) so a crash mid-wait resumes correctly.

**Dedupe:** `ResumeRun` tasks carry `dedupe_key = (run_id, wait_id)`. The wake-up handler appends the wake record only if the wait is still open (conditional update on `waits.state`), so a timeout racing an event resolves exactly once.

The approval answer path keeps today's rule: only the principal named in the request may answer an `authz` request, and anyone else is told it does not exist.

### 5.10 Triggers and schedules

- **API, chat, agent message, webhook:** insert `runs` row + `StartRun` task in one transaction, return `run_id`. Webhooks accept an optional `Idempotency-Key` header mapped to `dedupe_key`; a duplicate returns the existing `run_id`.
- **Cron / `every` / `after`:** rows in `schedules` with `next_fire_at`, replacing the per-minute `cron_matches` loop in the agents crate's `TriggerManager`. A scheduler loop claims due schedules (`UPDATE … WHERE next_fire_at <= now AND (lease_until IS NULL OR lease_until < now)`), enqueues `StartRun` with `dedupe_key = (schedule_id, fire_time)` and advances `next_fire_at` in the same transaction. Misfire policy per schedule: `skip` (default), `fire_once`, `fire_all`.
- **Topic and event triggers (`schedule.on`, `topic_triggers`):** the consumer acks the source message only after `StartRun` is committed.
- **MCP `execute_flow`** keeps answering synchronously within `FLOWR_MCP_EXEC_WAIT_SECS`: it enqueues, then waits on `Notifier` topic `run:<id>` (with a poll fallback) instead of polling the execution store.

No trigger spawns execution directly any more. A paused target is still refused at trigger time with the paused response, as today, and again at claim time as the backstop.

### 5.11 Subflows

Phase 1 keeps executing subflows inline in the parent's runtime (`run_workflow.rs` and agent tool flows in `call_agent.rs` create a child executor), but each child gets its own `run_id` (with `parent_run_id`), journal scope and durability class. The parent journals `SubflowStarted{ child_run_id }` before starting the child and `SubflowCompleted{ output_ref }` after. On resume, an unfinished child resumes from its own journal instead of restarting. The child inherits the parent's principal and template ceiling exactly as today.

### 5.12 Agents (`call:a3t:agent`)

- **Coarse mode (phase 1 default):** the whole agent call is one `effect` step.
- **Fine mode:** handlers get `StepContext::effect(key_suffix, class, fut)`. The Rust agent engine (`call_agent.rs`) wraps each model call and tool call with it, keys `<agent_step_key>/fx/model/<seq>` and `/fx/<tool_use_id>`. These go through §5.5, so a resumed agent replays recorded model and tool results and continues from the first unrecorded effect. The Python SDK uses the same API through the runtime later.
- **Streaming:** token and tool progress (`token_stream::publish`, keyed by session) is not journaled; only each effect's final result is. On replay, recorded effects do not re-stream. Phase 2 forwards stream chunks over the worker protocol (§9).

### 5.13 Cancellation, suspension and operations controls

- **Cancel / Terminate:** set `runs.cancel_requested` and notify the owner (`Notifier` topic `run:<id>`; in-process today via `ops::register_run`'s stop signal). The owner checks between steps and at heartbeat; in-flight effects finish (or are aborted where the tool supports it, as Terminate does today by dropping the future), then the run moves to `cancelled` with the reason. A waiting run is cancelled directly by closing its waits.
- **Suspend/resume:** suspend sets `suspend_requested`; the owner parks the run at the next step boundary as `suspended`. Resume enqueues `ResumeRun`.
- **Deployment pause** (Runtime console): new runs of a paused deployment are refused at entry with the paused response; already-queued tasks stay queued until resume (claim-time admission).

### 5.14 Concurrency limits, timeouts and poison runs

- Per-pool and per-namespace `max_concurrent_runs`, enforced at claim time. Default pool limit in CE: `FLOWR_MAX_CONCURRENT_RUNS` (default 64).
- **Run timeout:** the document's `timeout` (SWF `timeout.after`) is stored on the run as `deadline_at`; the owner faults the run at the deadline (`errors/timeout`). **Task timeout:** a task's `timeout` races its execution; expiry is a step failure that `try`/`catch` can handle. Waiting time counts toward the run deadline.
- Each `ResumeRun` increments `runs.resume_count`. Beyond `max_resumes` (default 20) without journal progress, the run is faulted with a `poisoned` detail and shows in the dead-letter view.

### 5.15 Durability classes (R3)

| Class | Journal | On crash | Default for |
|---|---|---|---|
| `durable` | Every step (§5.5) | Resumed | All flows unless declared otherwise; always for triggers, agents, chat and anything with `wait`/`listen` |
| `ephemeral` | None (run row only) | Faulted `interrupted`, not resumed | Flows declaring `metadata.a3t.durability: ephemeral` — intended for the dash's read/list flows |

An ephemeral flow that reaches a `wait` longer than `inline_wait_max`, a `listen` or an approval is promoted to `durable` for that run (journal starts there), so it never silently loses a wait.

### 5.16 Security of stored state (R8)

- `$secrets`, resolved resource credentials and `_principal` are never written to journal, checkpoint or blob.
- Step inputs and outputs pass the same masking as `masked_config` (secret-typed resource values and key/password/token fields) before they are journaled. The masked form is what the console shows.
- Reading a run's journal, steps or checkpoints requires Read access to the run's namespace and passes the same `can_see_run` rule as `get_execution_status` (platform-flow runs only to their initiator).
- EE: blobs and journal bodies may be encrypted at rest per namespace (open question 8).

### 5.17 Debug runs (R14)

Used by the console run/debug UI. A run started with `debug: true`:

- carries `breakpoints: [{ step_key | step_pattern, when: before|after|on_error, condition?: jq, hit?: n }]`, updatable while it runs;
- at a matching breakpoint the step hook journals `Paused{ step_key, when, reason }`, releases ownership and acks (a durable pause: the browser can close);
- resumes on `DebugCommand{ continue | step_over | step_into | step_out | run_to(step_key) | stop }`, which enqueues `ResumeRun`;
- may receive `DebugEdit{ step_key, input }` while paused before a step; the edited input is journaled with the editor's principal and used instead of the computed one;
- is always `durable`, is never started by a trigger, and requires Write access to the namespace.

Production (non-debug) runs cannot hit breakpoints; they can be monitored read-only, suspended, resumed or cancelled.

---

## 6. Contracts (`flowr-storage`, replacing `ExecutionQueue` / `CheckpointStore` / `ExecutionStore`)

```rust
pub struct Lease { pub task_id: TaskId, pub token: u64, pub expires_at: SystemTime }

#[async_trait]
pub trait TaskQueue: Send + Sync {
    async fn enqueue(&self, t: NewTask) -> Result<EnqueueOutcome>;          // dedupe_key → Existing(id)
    async fn heartbeat(&self, leases: &[Lease], extend: Duration) -> Result<Vec<LeaseStatus>>;
    async fn ack(&self, lease: &Lease) -> Result<()>;
    async fn nack(&self, lease: &Lease, retry_at: Option<SystemTime>) -> Result<()>;
}

#[async_trait]
pub trait RunStore: Send + Sync {
    /// Create the run row and its StartRun task in one transaction.
    async fn start(&self, run: NewRun, task: NewTask) -> Result<EnqueueOutcome>;
    /// Lease up to n tasks AND acquire ownership of their runs, atomically (§5.2).
    async fn claim_runs(&self, pool: &PoolId, worker: &WorkerId, n: usize, lease: Duration)
        -> Result<Vec<(Lease, Task, Epoch)>>;
    async fn renew(&self, run: RunId, epoch: Epoch, lease: Duration) -> Result<bool>;
    /// Release ownership, set status, ack the task — one transaction.
    async fn release(&self, run: RunId, epoch: Epoch, status: RunStatus, lease: &Lease) -> Result<()>;
    async fn request(&self, run: RunId, req: RunRequest) -> Result<()>;   // cancel | suspend | resume | debug command
    async fn get(&self, run: RunId) -> Result<Option<RunRow>>;
    async fn list(&self, q: RunQuery) -> Result<Page<RunRow>>;             // namespaces[], status[], flow, from/to, cursor
}

#[async_trait]
pub trait Journal: Send + Sync {
    /// Idempotent on (run, step_key, kind, attempt). Fails with Fenced if epoch is stale.
    async fn append(&self, run: RunId, epoch: Epoch, recs: &[Record]) -> Result<AppendOutcome>;
    async fn read_from(&self, run: RunId, after_seq: u64) -> Result<Vec<(u64, Record)>>;
    /// Atomic: completion record (+ buffered pure records) + optional checkpoint + outbox rows.
    async fn commit_step(&self, run: RunId, epoch: Epoch, c: StepCommit) -> Result<u64>;
    async fn latest_checkpoint(&self, run: RunId) -> Result<Option<Checkpoint>>;   // M5b
    async fn checkpoints(&self, run: RunId) -> Result<Vec<CheckpointMeta>>;        // console
    async fn blob(&self, hash: &BlobHash) -> Result<Option<Bytes>>;
}

#[async_trait]
pub trait Notifier: Send + Sync {          // wake-ups and live views only; loss is tolerated
    async fn publish(&self, topic: &str, msg: Bytes) -> Result<()>;
    fn subscribe(&self, topic: &str) -> BoxStream<'static, Bytes>;
}
```

`Notifier` is an optimisation: claimers also poll (250 ms backoff to 2 s), so a lost notification only adds latency. Topics: `tasks:<pool>`, `run:<id>` (journal appends, status changes — feeds the console monitor).

### Backend requirements

| Contract | Must provide |
|---|---|
| `TaskQueue` | Atomic claim with lease; delayed visibility; dedupe on `dedupe_key`; out-of-order ack |
| `RunStore` + `Journal` | **Conditional write on epoch** (compare-and-set) in the same transaction as the append; claim + ownership in one transaction; durable commit before returning; indexed run listing |
| `Notifier` | Best effort |

LanceDB (today's store) provides neither multi-row transactions nor compare-and-set, so it cannot back `TaskQueue`, `RunStore` or `Journal` (R16). It keeps definitions, resources, memory and the optional execution projection.

### Records

```rust
enum Record {
    RunStarted { def_hash, input_ref, trigger, durability },
    StepStarted { step_key, attempt, idem_key, input_hash },
    StepCompleted { step_key, attempt, output_ref, context_ref, class },
    StepFailed { step_key, attempt, error: ProblemDetails, retryable: bool },
    StepScheduled { step_key, pool },                     // phase 3
    TimerSet { wait_id, fire_at },
    WaitForEvents { wait_id, correlation },
    EventReceived { wait_id, events_ref },
    ApprovalRequested { wait_id, request_ref },
    ApprovalDecided { wait_id, decision, by },
    SubflowStarted { step_key, child_run },
    SubflowCompleted { step_key, child_run, output_ref },
    Paused { step_key, when, reason },                    // debug (§5.17)
    DebugCommand { command, by },
    DebugEdit { step_key, input_ref, by },
    Suspended, Resumed { by }, CancelRequested { by, reason },
    RunCompleted { output_ref }, RunFaulted { error }, RunCancelled { reason },
}
```

Each record also stores `at` (wall clock) and, for step records, `duration_us`, so the console can draw a timeline from the journal alone.

---

## 7. Storage: CE (and EE phase 1) on SQLite

One file, `<data_dir>/flowr.db` (on the console's persistent volume in EE). LanceDB keeps definitions, resources, platform tables and memory.

### 7.1 Settings

```sql
PRAGMA journal_mode = WAL;
PRAGMA synchronous = FULL;      -- NORMAL can lose the last commits on power loss
PRAGMA busy_timeout = 5000;
PRAGMA foreign_keys = ON;
```

One writer (all writes `BEGIN IMMEDIATE`) on a dedicated thread fed by a channel, batching commits from many runs per fsync (group commit, open question 1); a small reader pool. `rusqlite` (bundled), behind the writer thread / `spawn_blocking`.

### 7.2 Schema

```sql
CREATE TABLE tasks (
  task_id      TEXT PRIMARY KEY,           -- UUIDv7
  pool         TEXT NOT NULL DEFAULT 'default',
  kind         TEXT NOT NULL,              -- start_run | resume_run | remote_step
  run_id       TEXT NOT NULL,
  namespace    TEXT NOT NULL,              -- per-namespace limits at claim time
  payload      BLOB,
  dedupe_key   TEXT UNIQUE,
  visible_at   INTEGER NOT NULL,           -- unix ms
  lease_owner  TEXT,
  lease_token  INTEGER NOT NULL DEFAULT 0,
  lease_until  INTEGER,
  attempts     INTEGER NOT NULL DEFAULT 0,
  created_at   INTEGER NOT NULL
);
CREATE INDEX tasks_claim ON tasks(pool, visible_at) WHERE lease_until IS NULL;

CREATE TABLE runs (
  run_id            TEXT PRIMARY KEY,      -- = instance_id (UUIDv7)
  namespace         TEXT NOT NULL,
  flow              TEXT NOT NULL,
  version           TEXT NOT NULL,
  def_hash          TEXT NOT NULL,
  parent_run_id     TEXT,
  durability        TEXT NOT NULL DEFAULT 'durable',
  debug             INTEGER NOT NULL DEFAULT 0,
  breakpoints       TEXT,                  -- JSON, debug runs only
  principal         TEXT NOT NULL,         -- JSON; the run's identity (§5.16)
  trigger           TEXT,                  -- api | chat | agent | webhook:<id> | cron:<id> | topic:<id> | subflow
  status            TEXT NOT NULL,
  current_step      TEXT,
  input_ref         TEXT, output_ref TEXT, error TEXT,
  owner             TEXT,
  epoch             INTEGER NOT NULL DEFAULT 0,
  owner_lease_until INTEGER,
  cancel_requested  INTEGER NOT NULL DEFAULT 0,
  suspend_requested INTEGER NOT NULL DEFAULT 0,
  resume_count      INTEGER NOT NULL DEFAULT 0,
  deadline_at       INTEGER,
  last_seq          INTEGER NOT NULL DEFAULT 0,
  created_at INTEGER NOT NULL, started_at INTEGER, completed_at INTEGER, updated_at INTEGER NOT NULL
);
CREATE INDEX runs_ns_time   ON runs(namespace, created_at DESC);
CREATE INDEX runs_flow_time ON runs(namespace, flow, created_at DESC);
CREATE INDEX runs_status    ON runs(status, created_at DESC);
CREATE INDEX runs_parent    ON runs(parent_run_id);

CREATE TABLE journal (
  run_id   TEXT NOT NULL,
  seq      INTEGER NOT NULL,
  step_key TEXT,
  kind     TEXT NOT NULL,
  attempt  INTEGER NOT NULL DEFAULT 0,
  epoch    INTEGER NOT NULL,
  body     BLOB NOT NULL,                  -- encoded Record
  at       INTEGER NOT NULL,
  PRIMARY KEY (run_id, seq),
  UNIQUE (run_id, step_key, kind, attempt)
);

CREATE TABLE checkpoints (                 -- M5b
  run_id TEXT NOT NULL, seq INTEGER NOT NULL, snapshot BLOB NOT NULL, size INTEGER NOT NULL,
  PRIMARY KEY (run_id, seq)
);

CREATE TABLE blobs (hash TEXT PRIMARY KEY, bytes BLOB NOT NULL, size INTEGER NOT NULL, refs INTEGER NOT NULL DEFAULT 1);

CREATE TABLE definitions (def_hash TEXT PRIMARY KEY, namespace TEXT, name TEXT, version TEXT, doc BLOB);

CREATE TABLE waits (
  wait_id TEXT PRIMARY KEY, run_id TEXT NOT NULL, kind TEXT NOT NULL,
  correlation TEXT, state TEXT NOT NULL DEFAULT 'open', created_at INTEGER NOT NULL
);

CREATE TABLE schedules (
  schedule_id TEXT PRIMARY KEY, namespace TEXT, def_ref TEXT, spec TEXT NOT NULL,
  misfire TEXT NOT NULL DEFAULT 'skip', next_fire_at INTEGER NOT NULL,
  lease_owner TEXT, lease_until INTEGER, enabled INTEGER NOT NULL DEFAULT 1
);

CREATE TABLE outbox (
  id INTEGER PRIMARY KEY AUTOINCREMENT, topic TEXT NOT NULL,   -- usage | lifecycle | projection
  body BLOB NOT NULL, created_at INTEGER NOT NULL
);
```

### 7.3 Key statements

Claim + ownership (inside `BEGIN IMMEDIATE`):

```sql
UPDATE tasks
SET lease_owner = :worker, lease_token = lease_token + 1,
    lease_until = :now + :lease_ms, attempts = attempts + 1
WHERE task_id IN (
  SELECT task_id FROM tasks
  WHERE pool = :pool AND visible_at <= :now
    AND (lease_until IS NULL OR lease_until < :now)
  ORDER BY visible_at LIMIT :n)
RETURNING task_id, kind, run_id, namespace, payload, lease_token, lease_until;

-- per returned task, same transaction:
UPDATE runs SET owner = :worker, epoch = epoch + 1, owner_lease_until = :now + :lease_ms,
                status = 'running', started_at = COALESCE(started_at, :now)
WHERE run_id = :run AND (owner IS NULL OR owner_lease_until < :now)
RETURNING epoch;          -- none → release the task with visible_at = :now + 1000
```

Fenced append (same transaction as the epoch check):

```sql
-- abort the transaction with Fenced if this returns 0 rows
UPDATE runs SET last_seq = last_seq + 1, current_step = :step_key, updated_at = :now
WHERE run_id = :run AND epoch = :epoch
RETURNING last_seq;

INSERT INTO journal(run_id, seq, step_key, kind, attempt, epoch, body, at)
VALUES (:run, :seq, :step_key, :kind, :attempt, :epoch, :body, :now)
ON CONFLICT(run_id, step_key, kind, attempt) DO NOTHING;   -- idempotent re-append
```

Ack and nack check `lease_token` so a stale holder cannot ack another holder's lease.

### 7.4 Execution history and retention

- `list_executions`, `get_execution_status` and the console's runs list read `runs` directly through `RunStore::list` (newest first, filtered by the caller's visible namespaces in SQL, cursor-paged, without input/output bodies).
- Retention per namespace (default 30 days for terminal runs; journal and blobs follow their run; blobs are reference-counted). Never affected by memory `FORGET`.
- The outbox drain feeds metering, lifecycle CloudEvents and, optionally, a LanceDB/ADB projection for analytics and agent memory. It is no longer the source for lists.

### 7.5 Outbox drain

A background task reads `outbox` in id order, publishes, then deletes the rows. Consumers tolerate duplicates.

---

## 8. Storage: EE backends

### Postgres (phase 2 default)

Same schema with native types (`timestamptz`, `bytea`, `jsonb` for indexes). Claim with:

```sql
WITH c AS (
  SELECT task_id FROM tasks
  WHERE pool = $1 AND visible_at <= now() AND (lease_until IS NULL OR lease_until < now())
  ORDER BY visible_at LIMIT $2
  FOR UPDATE SKIP LOCKED)
UPDATE tasks t SET lease_owner = $3, lease_token = lease_token + 1,
                   lease_until = now() + $4, attempts = attempts + 1
FROM c WHERE t.task_id = c.task_id
RETURNING t.*;
```

`Notifier` via `LISTEN/NOTIFY` on `tasks:<pool>` and `run:<id>`. Partition `journal` by `run_id` hash once volume needs it.

### Iggy

Good fit for append-only streams: **outbox, lifecycle events, `Notifier`** and audit export. It has no compare-and-set on append, so it cannot enforce fencing alone; Iggy-as-journal stays an open question.

### ADB

Natural role: the **projection** — episodic memory and the execution graph built from the outbox, readable by agents (KBLaM direction). Pluggable for a contract once it passes the conformance suite (§12). Checkpoints kept in episodic memory must be exempt from TTL, `FORGET` and consolidation; the journal of record stays in the queue store.

### Dragonfly

Retire the Dragonfly-backed queue and storage (`flowr-storage-dragonfly`). It may remain a `Notifier` option.

---

## 9. Phase 2: control plane and worker protocol

Runtimes and customer-hosted workers connect outbound to the control plane. They never talk to the store directly, so tenancy, Cedar, metering and fencing are enforced in one place.

```proto
service WorkerGateway {
  rpc Connect(stream WorkerMsg) returns (stream ServerMsg);
}
message WorkerMsg {
  oneof msg {
    Register    register  = 1;  // worker_id, build_version, pools, labels, max_concurrency
    Claim       claim     = 2;  // pool, n
    Heartbeat   heartbeat = 3;  // leases + run epochs
    Append      append    = 4;  // run_id, epoch, records
    CommitStep  commit    = 5;  // run_id, epoch, completion (+ checkpoint)
    Release     release   = 6;  // run_id, epoch, status
    Ack         ack       = 7;
    Nack        nack      = 8;
    Stream      stream    = 9;  // run_id, session_id, kind, data — token/tool progress (R9), not journaled
  }
}
message ServerMsg {
  oneof msg {
    Assigned     assigned  = 1;  // lease, task, epoch, def blob ref, breakpoints
    LeaseStatus  lease     = 2;  // renewed | lost
    AppendResult result    = 3;  // ok | fenced | duplicate
    Cancel       cancel    = 4;  // run_id, reason
    Drain        drain     = 5;  // stop claiming, finish current
    Breakpoints  bps       = 6;  // run_id, updated breakpoints (debug runs)
  }
}
```

- **Joining:** the pool issues a short-lived join token; the worker exchanges it for an mTLS client certificate (auto-rotated). The A3T controller's Runtime CR provisions the token.
- **Worker versioning:** workers report `build_version`. A run resumes only on workers whose build can execute its `def_hash` and engine version. New runs prefer the newest build.
- **Secrets:** resolved inside the worker's pool from its secret store; the control plane holds references only.
- **Identity:** the control plane sends the run's principal with the assignment; the worker's Cedar hook evaluates steps against it as today.

The console pod stops executing runs; it becomes the control plane plus an optional embedded `default` pool for small installs.

---

## 10. Phase 3: pools and per-step placement

- A pool is `namespace/pool` with capability labels (`runtime=python3.12`, `gpu=true`, `region=eu-west-1`, `framework=langgraph`). Today's runtime policy (org/project `isolated` vs `shared`, `a3t.io/org` labels) becomes pool eligibility: an isolated org's runs are only claimable by its own pools.
- A task may declare placement in metadata:

```yaml
- triage:
    call: a3t:agent
    metadata:
      a3t.placement:
        pool: secops/sandboxed
        requires: { runtime: python3.12, framework: langgraph }
```

- When the owning runtime reaches a step whose placement differs from its own pool, it journals `StepScheduled{ pool }`, enqueues a `remote_step` task to that pool, releases ownership and acks. The remote worker claims the task, runs **only that step** under §5.5 (its journal records are fenced by the task's `lease_token`), and commits the result. The commit enqueues `ResumeRun` for the parent.
- Cedar decides which namespaces may schedule onto which pools (`Action::"pool:schedule"`).
- The controller's Runtime and Canvas CRs become pool registrations; runtime pods start doing real work.

---

## 11. Failure scenarios (all covered by tests)

| # | Failure | Expected outcome |
|---|---|---|
| F1 | Crash after `enqueue`, before claim | Task stays visible; another claimer starts it |
| F2 | Crash after claim, before first step | Lease expires; run resumes from `RunStarted` |
| F3 | Crash after `StepStarted`, before the external call | In-doubt → policy (§5.6). With `retry` + idempotency key, the effect happens once |
| F4 | Crash after the external call, before `StepCompleted` | In-doubt → policy. Never silently re-executed for `effect` steps |
| F5 | Crash after `StepCompleted`, before ack | Resume replays; the step is skipped |
| F6 | Owner paused past its lease (GC, partition), new owner takes over | Old owner's next append is `Fenced`; it stops; no further effects from it |
| F7 | Duplicate webhook delivery | Same `dedupe_key` returns the existing `run_id` |
| F8 | Restart during cron window | `dedupe_key = (schedule_id, fire_time)` prevents double fire; misfire policy applies |
| F9 | Timer and event wake-up race | Exactly one wake record; the other `ResumeRun` finds the wait closed and acks |
| F10 | Run crashes every resume (poison) | Faulted after `max_resumes`; visible in dead-letter view |
| F11 | Disk full / store unavailable | Claims stop; in-flight runs fail their next append and pause; nothing is acked without a durable commit |
| F12 | Cancel while a step is in flight | Step finishes or aborts; run ends `cancelled`; no new steps start |
| F13 | Subflow crashes mid-way | Child resumes from its own journal; parent does not restart the child |
| F14 | Crash while an ephemeral run is running | Run faulted `interrupted`; not resumed; no partial journal |
| F15 | Crash while a debug run is paused | Run stays `suspended` at the breakpoint; the console shows it paused after restart |
| F16 | Replay meets a changed expression result (`now`) | Recorded output and context are used; no divergence |
| F17 | Goto loop (`then:` back-edge) crashes on the 3rd visit | Visits 1–2 replay from the journal (`#2` keys); visit 3 runs |

---

## 12. Conformance suite and benchmarks

Every backend runs the same suite (crate `flowr-store-conformance`):

- lease expiry and reclaim; stale `ack` rejected by `lease_token`
- claim + ownership atomic; owned run not double-claimed
- fencing: stale-epoch append rejected inside the transaction
- duplicate `append` is a no-op; duplicate `enqueue` with `dedupe_key` returns the existing task
- delayed visibility honoured; out-of-order ack
- `commit_step` atomicity (inject failure between statements)
- run listing: newest first, namespace filter, cursor stability under concurrent inserts
- crash injection for F1–F17 using a kill-point hook in the runtime (`FLOWR_KILL_AT=after_step_started:/do/2/chargeCard`)

**Benchmarks** (CE on a laptop SSD, published): steps/sec for a 10-step pure flow (durable and ephemeral); p50/p99 per-step commit latency with `synchronous=FULL`, with and without group commit; replay time vs run length (decides when M5b is needed); claim throughput with 1, 4, 16 claim loops; dash page-load latency before/after (ephemeral CRUD flows must not regress).

---

## 13. Migration from the current code

| Step | Change | Notes |
|---|---|---|
| **M0** | Quick fixes on today's store: order `list_recent`/`list_by_workflow` newest-first before the limit; at startup mark `Queued`/`Running` executions as failed `interrupted by restart`. | Independent; fixes the stale Executions page now |
| M1 | Contracts in `flowr-storage` (§6) with a `MemoryStore` that reproduces today's behaviour; delete `flowr-ce-core`'s executor and the unwired `flowr/flowr-core` executor | Everything compiles against the contracts first |
| M2 | `SqliteStore` (§7) with the writer thread and group commit | Behind `FLOWR_STORE=sqlite`; default in CE once M5 passes |
| M3 | Replace `InMemoryExecutionQueue` and `run_worker`/`spawn_worker` with a claim loop over `RunStore::claim_runs`; concurrency semaphore; run/task timeouts; ops admission at claim; `execute_flow` waits on `Notifier` | Lists read `runs` (§7.4) |
| M4 | Route webhook, cron, topic and event triggers through `RunStore::start`; move cron into `schedules` | Removes every direct-spawn path |
| M5 | `swf-runtime`: step keys from the parser, `StepHook` around every task, §5.5 protocol, replay index, durability classes; handlers declare `step_class` | Start with `do`/`set`/`switch`/`call`, then `for`, `try`, `fork`, goto, `wait`, `listen` |
| M5b | Interpreter snapshots (explicit frame stack) and checkpoint writes | Only when replay time needs it (benchmarks) |
| M6 | Durable waits and approvals (§5.9) replacing in-process sleeps and `InputManager` state; subflows with their own journal scope (§5.11); debug runs (§5.17) | Review `flowr-suspend` for reuse |
| M7 | Outbox drain → metering, lifecycle CloudEvents (`org.open-workflow-specification.*` plus deprecated `io.serverlessworkflow.*`), optional LanceDB/ADB projection | Replaces the two LanceDB execution rows |
| M8 | Conformance suite + kill-point crash tests in CI | Gate for claiming durability on the website |
| M9 | Phase 2: `PostgresStore`, `WorkerGateway` (Tonic) incl. `Stream`, console stops executing runs | EE; take what is reusable from `flowr-worker`, then delete it |
| M10 | Phase 3: pools, placement metadata, `remote_step`, controller CRs as pool registrations, runtime policy as pool eligibility | EE |

Console work tracks these milestones; see the companion spec §10.

---

## 14. Defaults

| Setting | Default |
|---|---|
| Queue lease / heartbeat | 30 s / 10 s |
| Run ownership lease | same as queue lease, renewed together |
| Claim poll backoff | 250 ms → 2 s (reset on notify) |
| `FLOWR_MAX_CONCURRENT_RUNS` (CE pool) | 64 |
| In-doubt policy | `fault` |
| `inline_wait_max` | 5 s |
| `max_resumes` without progress | 20 |
| Inline value limit / hard value limit | 64 KiB / 8 MiB |
| `checkpoint_every` (M5b) | 1 effect step |
| Terminal run retention | 30 days (per namespace policy) |
| Journal retention | follows its run; never affected by memory `FORGET` |

---

## 15. Open questions

1. **Group commit in CE:** batch commits from many runs per fsync — what latency budget per step (proposal: ≤ 5 ms added at p99)?
2. **Fork concurrency** (deferred, R11): when branches run in parallel, per-run writer task serialising journal commits (proposal) or shared epoch across branch tasks?
3. **Iggy as journal:** worth control-plane serialisation, or keep Iggy to events/outbox/notifier?
4. **Remote step leases in phase 3:** reuse `tasks.lease_token` recorded in the step's journal rows (proposal) or a separate per-step token?
5. **Context size:** cap `$context` per run or always spill to blobs (proposal: spill above 64 KiB, fault above 8 MiB)?
6. **Definition migration:** opt-in migration of waiting runs to a new `def_hash` at a safe point, later?
7. **EE phase 2 store on vc-dev:** provision Postgres (operator or managed), or run phase 2 first on SQLite with a single control-plane replica?
8. **Encryption at rest** for journal bodies and blobs per namespace in EE: required for phase 2, or later?
9. **Which a3t flows are `ephemeral`:** proposal — every `a3t-*` CRUD flow whose actions are only `list`/`get`, declared in their metadata; writes stay `durable`.
