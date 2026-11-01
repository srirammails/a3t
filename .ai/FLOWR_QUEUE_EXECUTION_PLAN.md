# FlowR queue, checkpointing and console run/debug — execution plan

**Status:** active (1 Nov 2026) · **Specs:** [engine v0.2](FLOWR_QUEUE_CHECKPOINT_SPEC.md) · [console UI v0.1](FLOWR_CONSOLE_RUN_DEBUG_UI_SPEC.md)
**Branch:** `adb-integration` (feature work in stage-sized commits; the user pushes) · **Owner:** Sri

---

## How we work

- **One stage at a time, each shippable.** A stage ends with tests green, a CE build, an EE image, and a short report. Rolling an image to vc-dev needs your explicit OK each time.
- **Tests first where the contract is clear.** Store contracts get the conformance suite before the SQLite store; crash scenarios get kill-point tests before we claim durability.
- **Feature flag until proven.** The new engine runs behind `FLOWR_STORE=sqlite` (default `lance` = today) until Stage 4's crash tests pass; then it becomes the CE default, then EE's.
- **Build budget.** Cargo `-j4`, target dirs under `/var/tmp/a3t-build` (≤ 100 GB); `target-flowr-ce` for CE tests, `target-shared` for shared crates, `target-flowr-ee` for the EE image.
- **No data loss on vc-dev.** The new store starts empty beside LanceDB; old execution rows stay readable (read-only) until retention removes them.

---

## Stages

| Stage | Engine | Console | Exit criteria | Size |
|---|---|---|---|---|
| **0 — Quick fixes** | M0: execution lists newest-first; mark runs left `Queued`/`Running`/`Suspended` at startup as failed `interrupted by restart` | U0: Executions page shows current runs | Storage tests; live list shows today's runs; the 16 Sep run shows interrupted | S |
| **1 — Contracts** | M1: `TaskQueue`, `RunStore`, `Journal`, `Notifier` and records in `flowr-storage`; `MemoryStore` reproducing today; delete `flowr-ce-core` executor and unwired `flowr/flowr-core` executor | U1: ▶ Run with inputs in the flow editor (schema form, last input), result in a Run panel, flow's Runs list — on today's engine | Everything compiles on the contracts; CE + EE tests green; Run from editor works on vc-dev | M |
| **2 — SQLite store** | M2: `SqliteStore` (schema §7, writer thread, group commit); conformance suite (`flowr-store-conformance`) run against Memory and SQLite | — | Conformance green; benchmarks recorded (commit latency, claim throughput) | M |
| **3 — Durable queue** | M3: claim loop on `claim_runs` with concurrency limit, run/task timeouts, ops admission at claim, `execute_flow` waits on `Notifier`; lists read `runs`. M4: webhook/cron/topic/event triggers through `RunStore::start`; cron in `schedules` | U2a: Executions app on `/v1/runs` (server-side project/status/time filters, cursor, live ●) | No direct-spawn path left; restart test: queued runs survive and run; dash latency unchanged | M |
| **4 — Journal and replay** | M5: step keys from the parser; `StepHook`; write-ahead protocol; replay index; durability classes; handlers declare `step_class`; masking of journaled values. M8 (part): kill-point hook + F1–F5, F14, F16, F17 | U2b: Monitor — graph overlay, Steps list, step inspector (Input, Output, Context, Journal), Timeline, Events, SSE live updates | Crash tests green; `FLOWR_STORE=sqlite` becomes CE default | L |
| **5 — Durable waits and debug** | M6: durable `wait`/`listen`/approvals (replacing in-process sleeps and `InputManager` state), subflows with own journal scope, debug runs (§5.17), cancel/suspend via `RunStore::request` | U3: breakpoints (all kinds), durable pause, stepping, edit input, effect warnings | F6–F13, F15 green; a paused debug run survives a console restart | L |
| **6 — Outbox and checkpoints view** | M7: outbox drain → metering, lifecycle CloudEvents, optional LanceDB/ADB projection; retention; M8 complete in CI | U4: Checkpoints tab, re-run from here, dead-letter fold, Logs and Policy tabs | All F-tests in CI; EE image on vc-dev with the new store (your OK) | M |
| **7 — Distributed (EE)** | M9: Postgres store (or SQLite single control plane, open question 7), `WorkerGateway` (Tonic) incl. `Stream`, runtime pods pull work, console stops executing runs | Runtime page shows per-pool claims and in-flight runs | Runs execute on `rt-worker`; chat streams from a remote runtime; fencing test across two workers | L |
| **8 — Pools (EE)** | M10: pools and labels, placement metadata, `remote_step`, runtime policy as pool eligibility, controller CRs as pool registrations | Placement shown in Run dialog and run views | Isolated org's runs only on its pool; per-step placement works | L |
| **Later** | M5b interpreter snapshots (when benchmarks show replay too slow); parallel fork branches | U5 mock effects in debug runs | — | — |

Sizes: S ≤ 1 day, M a few days, L about a week of focused work.

---

## Stage 0 detail (in progress)

1. `LanceExecutionStore::list_recent` / `list_by_workflow`: read `instance_id` + `updated_at` only for the matching rows, sort newest-first, then load the top `limit` states. (LanceDB applies `limit` before any ordering, which is why the list shows August.)
2. `ExecutionStore::list_open()` (default: none) and its Lance implementation; `run_worker` marks each open run `Failed` with `interrupted by restart` before taking new work.
3. Tests in `flowr-ce-storage`: newest-first with more rows than the limit; open runs found; terminal ones not.
4. CE tests, EE build; live check after your OK to roll.

---

## Risks and how we handle them

| Risk | Handling |
|---|---|
| Per-step fsync slows the dash | Ephemeral class for read/list flows (spec §5.15); group commit; benchmark gate in Stage 2 and dash latency check in Stage 3 |
| Replay diverges from the first run | Every completed step journaled with output and context (R2); F16/F17 tests |
| Executor refactor breaks existing flows | Hook-based (no rewrite); the full swf-parser/runtime and a3t flow tests run every stage; `FLOWR_STORE=lance` fallback until Stage 4 |
| Secrets end up in the journal | Masking at the hook (§5.16) with a test that journals a secret-bearing step and asserts it is masked |
| Phase 2 store choice blocks Stage 7 | Decide open question 7 during Stage 5 |
| Disk budget on the build pod | `build-budget.sh` before each EE image; slim profiles |

---

## Decisions needed from you (not blocking Stage 0–2)

1. Stage 7 store on vc-dev: Postgres, or SQLite with a single control-plane replica first? (Needed by Stage 5.)
2. Which a3t flows are `ephemeral` — proposal: `a3t-*` read/list actions. (Needed by Stage 4.)
3. Breakpoints personal only in v1? (Proposal: yes. Needed by Stage 5.)
