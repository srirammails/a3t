# FlowR engine and console — known limitations

**Started:** 1 Nov 2026, at the CE checkpoint (image `flowr:2026-11-01-ee22`, commit `d8ea254`).
**Related:** [spec](FLOWR_QUEUE_CHECKPOINT_SPEC.md) · [console spec](FLOWR_CONSOLE_RUN_DEBUG_UI_SPEC.md) · [plan](FLOWR_QUEUE_EXECUTION_PLAN.md)

How to use this file: one row per limitation. When work starts, set **Status** to `in progress`; when it ships, set `fixed` with the commit and date, and keep the row (the history is useful). New limitations found later get the next id.

Status values: `open` · `in progress` · `fixed (<commit>, <date>)` · `won't fix (<why>)`.

---

## Engine — durability gaps

| ID | Limitation | Impact today | Workaround | Planned fix | Status |
|---|---|---|---|---|---|
| L1 | `listen` and human approvals (`InputManager`, `authz` approvals) are held in memory. | A restart while a run waits for an event or an approval loses the wait; the run is faulted or stays stuck. | Avoid restarting the console while approvals are pending. | Move waits onto the `waits` table with `RunRequest` wake-ups (spec §5.9). Plan stage 5 remainder. | open |
| L2 | A subflow (`run: workflow`, agent tool flows) is one effect step in its parent's journal; it has no journal of its own. | A crash inside a subflow leaves the parent step in doubt: the parent faults (or retries if idempotent) instead of the child resuming. | Mark subflow steps `idempotent` + `inDoubt: retry` where re-running the child is safe. | Child runs with their own `run_id` and journal scope (spec §5.11). | open |
| L3 | Cron triggers still fire from the per-minute loop in `TriggerManager`; there is no `schedules` table. | No misfire policy; a restart near a tick can skip it or, rarely, fire it twice. Each tick does enqueue a durable run. | — | `schedules` table with lease, dedupe `(schedule_id, fire_time)` and misfire policy (spec §5.10). | open |
| L4 | Run and task timeouts (`timeout` in the document or on a task) are not enforced. | A hung step holds its run until someone cancels it. | Cancel from the console (Cancel / Stop) or `cancel_run`. | `deadline_at` on the run and a task-level race (spec §5.14). | open |
| L5 | The outbox exists but nothing publishes from it. | No metering events and no lifecycle CloudEvents from runs. | — | Outbox drain → metering, lifecycle CloudEvents, optional LanceDB/ADB projection (spec §7.5, M7). | open |
| L6 | The idempotency key (`idem_key`) is journaled but not passed to HTTP or MCP calls. | An in-doubt step marked `retry` is re-sent without a key the receiver can dedupe on. | Only mark steps `retry` when the receiver is naturally idempotent. | Send `Idempotency-Key` on HTTP calls and an `idempotency_key` argument to MCP tools that declare it (spec §5.5). | open |
| L7 | `inDoubt: approve` and `inDoubt: reconcile` are not implemented. | In-doubt steps either fault (default) or retry. | Handle the fault with `try`/`catch`. | Approval request on in-doubt; reconciliation task (spec §5.6). Depends on L1. | open |
| L8 | Fork branches run one after another. | No parallelism inside `fork`. | — | Parallel branches with a per-run journal writer (spec open question 2). | open |
| L9 | A run that is idle-waiting on a timer releases its worker, but a wait of 5 s or less (`FLOWR_INLINE_WAIT_MAX_MS`) still sleeps in place holding a slot. | Many short waits can fill the 64 concurrent slots. | Raise `FLOWR_MAX_CONCURRENT_RUNS`. | Tune per deployment; revisit if seen in practice. | open |
| L10 | No interpreter snapshots (M5b): resume replays the run from the start against its journal. | Resume time grows with run length (fine at measured sizes). | — | Snapshots when benchmarks show replay is too slow (spec §5.7). | open |

## Engine — semantics to know

| ID | Limitation | Impact today | Workaround | Planned fix | Status |
|---|---|---|---|---|---|
| L11 | Breakpoint hit counts reset on each resume of a debug run. | `hit: n` counts only within one attempt (between pauses). | — | Replayed steps count their hits, so counts carry across resumes. | fixed (see git log, 1 Nov 2026) — found on vc-dev: *Pause on start* paused again after every Continue |
| L12 | Breakpoints changed while a debug run is running apply from its next resume (next pause, timer or restart). | A breakpoint added mid-run may be passed. | Use **Pause on start** or step commands. | Re-read breakpoints on each step when they changed (via the run topic). | open |
| L13 | A replayed failure is re-raised as a plain task error with the recorded message. | A `catch` that filters by error type may not match a replayed failure. (Catch filters are not implemented yet either, so no flow depends on this today.) | — | Record the error type and status; re-raise the same kind. | open |
| L14 | Step inputs over 64 KiB are journaled as `{_truncated, bytes}`. | The step inspector cannot show very large inputs. Replay is unaffected (it never reads inputs). | Look at the previous step's output. | Store large values as blobs (spec §5.7). | open |
| L15 | Journaled values are stored as they are and masked only when read; no encryption at rest. | Secrets that pass through step outputs or context sit in `flowr.db` in clear (as they did in the executions table before). | Keep secrets in resources (`*_ref`), not in step outputs. | Encryption at rest per namespace (spec open question 8). | open |
| L16 | Masking hides fields whose names contain key, secret, password, token, credential or authorization — including innocent ones such as `cache_key`. | Some ordinary values show as `****` in the console. | Read the raw value through an authorised path if needed. | Allow-list of known-safe field names. | open |
| L17 | The dash's own platform flows (`a3t-*` CRUD and list flows) are journaled as durable runs. | About a millisecond extra per call step; journal grows with dash traffic (purged after 30 days). | — | Declare read/list flows `metadata.a3t.durability: ephemeral` (spec open question 9). | open |
| L18 | Debug runs execute real side effects. | Debugging a flow that charges, sends or writes does it for real. | Debug against test resources. | Mock effects from a chosen earlier run (console spec U5). | open |
| L19 | Re-run from a checkpoint copies the journal up to that point; the new run's task becomes visible 1.5 s later. | A short delay before a re-run starts. | — | Seed the journal in the same transaction as the start (`RunStore::start_seeded`). | open |

## Distributed (EE)

| ID | Limitation | Impact today | Workaround | Planned fix | Status |
|---|---|---|---|---|---|
| L20 | Every run still executes in the console pod; runtime pods (`rt-worker`, `cv-*`) run their own empty engine and do no work. | Isolated runtimes are a placement rule, not real isolation; the console pod carries all load. | — | Plan stage 7 (control plane, worker protocol, runtimes pull work). | open |
| L21 | Runtime pods keep their store in `/tmp`. | Anything a runtime pod stored is lost when it restarts (it holds no runs today, see L20). | — | Stage 7: runtimes never touch storage directly. | open |
| L22 | Not yet verified: whether the Cedar policy lets non-superadmin users call the new run tools (`list_runs`, `get_run`, `debug_run`, …). | Developers who are not superadmin might see the old execution list or "denied". | — | Check after rolling ee22; add Cedar permits if needed. | open |
| L23 | No Postgres store yet (phase 2 backend). | EE runs on the same SQLite file as CE, single console replica. | — | Stage 7; decide open question 7 (Postgres vs single control-plane replica). | open |

## Console

| ID | Limitation | Impact today | Workaround | Planned fix | Status |
|---|---|---|---|---|---|
| L24 | Breakpoints are set from the step list in the Debug dialog, not by clicking nodes in the flow graph; the graph shows no run status overlay. | Less direct than the spec's design. | Use the step list; the Steps tab shows status. | Node click to toggle; status overlay on graph nodes (console spec §3, §5.1). | open |
| L25 | Run views poll (1 s while running, 3 s while paused, list every 5 s) instead of streaming. | Slight lag; extra requests while a view is open. | — | SSE from the run topic (`/v1/runs/{id}/events`, console spec §7). | open |
| L26 | Breakpoints are kept per flow in the browser's local storage. | They don't follow you to another browser or device. | — | Store per user on the server (console spec open question 1). | open |
| L27 | No Logs or Policy tab in the step inspector; no dead-letter view; no run presets shared in the project. | Logs and policy decisions are only in the pod logs and audit log. | Audit log page; pod logs. | Console spec §6 (Logs, Policy), §9 (dead-letter), §4 (presets). | open |
| L28 | "Run to" lists only the flow's top-level and nested step names, not individual loop iterations. | Cannot run to iteration n of a loop. | Breakpoint with a `condition` on the step input. | Iteration picker. | open |
