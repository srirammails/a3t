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

## Progress

| Stage | State | Commits |
|---|---|---|
| 0 | Code done; image `flowr:2026-11-01-ee21` built and pushed, **not rolled** (needs your OK) | `4d0fd2b` |
| 1 — U1 | Live on dash.dev: ▶ Run with inputs in the Flow Editor, Runs panel | `cc25944` |
| 1 — M1 | Contracts in `flowr-storage::engine` (`TaskQueue`, `RunStore`, `Journal`, `Notifier`, `Record`, `StoreError::Fenced`); `MemoryStore` reference store; conformance suite (`flowr-storage::conformance`, 16 checks, feature `conformance`) green on `MemoryStore`, and shown to fail when fencing is disabled | see git log |
| 2 — M2 | `SqliteStore` (`flowr-ce-storage::engine_sqlite`, rusqlite 0.28 bundled — the release on libsqlite3-sys 0.25, which `rustpython-stdlib` already links): conformance green in memory and on a WAL file; reopen test (committed state survives, dead owner's run is orphaned and reclaimed under a new epoch). **Benchmarks** (this pod's disk, debug build, `synchronous=FULL`): run start→claim→release 1.5 ms (≈660 runs/s); step commit p50 0.42 ms, p99 0.82 ms (≈2,250 steps/s serial); 8 concurrent runs ≈1,700 steps/s. Group commit not needed yet (budget: ≤ 5 ms added at p99). | see git log |
| 3 — M3/M4 | Engine worker: claims runs (`FLOWR_MAX_CONCURRENT_RUNS`, default 64), lease + ownership heartbeat, cancel requests stop a run at once, release with outcome; triggers enqueue (topic replies wait for the outcome); today's queue/execution traits are views over the engine (old LanceDB runs still readable, merged newest-first). Deferred: a `schedules` table for cron misfire/dedupe (cron ticks already enqueue durable runs), run/task timeouts. | see git log |
| 4 — M5 | Step hook in `swf-runtime` (keys from task paths, `#n` for goto revisits; replay returns recorded output, context, directive); journal hook in FlowR CE (pure steps batched, effects write `StepStarted` first, in-doubt policy fault/retry, context only when changed, version pinned at start, ephemeral flows via `metadata.a3t.durability`); kill points `FLOWR_KILL_AT`, test lease `FLOWR_RUN_LEASE_MS`. Crash tests F2, F3, F4, F4-retry, F4 caught by try/catch, F5, F14, F16 green with a counting HTTP server; F17 in swf-runtime. **Engine store is now the default** (`FLOWR_STORE=lance` opts out). Spec §5.16 refined: values are masked on read, not before journaling. | see git log |
| 5 — M6 | Durable timers (waits > 5 s let go of the run, wake on time on any worker, survive a restart); debug runs (breakpoints before/after/on error with jq condition and hit count; step over/into/out, run to, continue, stop; durable pause; edited input); `RuntimeError::Halted` (pauses, timers, lost runs, kills are never caught by `try`/`catch`; nothing written after). Run tools: `list_runs`, `get_run`, `get_run_journal`, `start_run`, `debug_run`, `set_breakpoints`, `edit_step_input`, `cancel_run`, `rerun_run` (gated, masked). Step inputs journaled (≤ 64 KiB). | see git log |
| 6 — M7 (part) | Retention: finished runs + journals purged after `FLOWR_RUN_RETENTION_DAYS` (30), in the contract and conformance suite. Pure steps flushed at least every 250 ms for live views. | see git log |
| U2–U4 | Flow Editor 🐞 Debug + breakpoints; Runs panel with RunView (live status, debug controls, edit input, Steps + inspector, Timeline, Journal, Checkpoints with re-run from here, Input/Output); Executions on the run store (server-side filters, cursor). Verified in a browser against a local FlowR CE. Falls back to the execution list on servers without the run store. | see git log |

**Deferred (not in this CE checkpoint):**
- Durable `listen` and human approvals (still in-process; a restart loses a pending approval) — needs the `InputManager` moved onto `waits` + `RunRequest` wake-ups.
- Subflows with their own journal scope (a `run: workflow` step is one effect step in its parent; a crash inside it is an in-doubt parent step).
- Cron `schedules` table (misfire policy, dedupe across restarts); cron ticks already enqueue durable runs.
- Run/task timeouts (`timeout` in the document / task).
- Outbox drain to metering and lifecycle CloudEvents (the outbox exists; nothing publishes yet).
- Console: breakpoints by clicking graph nodes and a status overlay on the graph (breakpoints are set in the Debug dialog's step list); live updates by SSE (the views poll: 1 s live, 3 s paused).
- M5b interpreter snapshots (replay is fast enough per the benchmarks).


**Changed from the stage table:** deleting the unwired executors moves from M1 to M3. `flowr-ext/flowr-adcp-tasks` re-exports `flowr-ce-core`'s task traits, and `shared/flowr-core` (whose expression engine FlowR CE uses) carries its own unused executor, so removal touches three workspaces; it is done when the worker is replaced and nothing can call them.

**Contract notes for Stage 2:** `RunRow` carries `input`/`output` values; a SQL store spills large ones to `blobs` internally (inline limit 64 KiB). Outside writes (cancel, suspend, resume, debug commands, wake-ups) go through `RunStore::request`, which journals them without an epoch — the owner's epoch fences only the owner's own writes.

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
