//! The engine worker (`.ai/FLOWR_QUEUE_CHECKPOINT_SPEC.md` §5.2, plan stage 3):
//! claims runs from the durable engine store, runs each under a lease it keeps
//! alive, and releases it with its outcome.
//!
//! - **Claim:** up to the free slots (`FLOWR_MAX_CONCURRENT_RUNS`, default 64)
//!   at a time; woken by the store's notification, polling 250 ms → 2 s.
//! - **Heartbeat:** every 10 s the queue lease and run ownership are renewed
//!   together; if either is lost the run is stopped at once — another worker
//!   now owns it and this one must not act for it.
//! - **Cancel:** a cancel request (`RunStore::request`) wakes the owner through
//!   the run's topic; the run stops at its next await point, as Terminate does.
//! - **Journal:** a durable run (the default) records every step
//!   ([`crate::journal_hook`]) and is pinned to the definition version it
//!   started on.
//! - **Recovery:** a run whose earlier claim never finished (its owner died) is
//!   claimed again once that lease expires. A durable run resumes: completed
//!   steps replay from the journal, a step left in doubt follows its in-doubt
//!   policy. An ephemeral run (`metadata.a3t.durability: ephemeral`) keeps no
//!   journal, so it is faulted `interrupted`, never silently re-run.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use futures::StreamExt;
use tracing::{error, info, warn};

use flowr_storage::engine::*;
use flowr_storage::StorageBackend;
use flowr_types::{ExecutionRequest, ExecutionStatus};

use crate::journal_hook::{Halt, JournalHook};

/// Run lease (30 s; `FLOWR_RUN_LEASE_MS` for tests), renewed every third of it.
fn lease() -> Duration {
    Duration::from_millis(std::env::var("FLOWR_RUN_LEASE_MS").ok().and_then(|v| v.parse().ok()).unwrap_or(30_000))
}

/// The pools this process claims from (spec §10): `FLOWR_WORKER_POOLS`
/// (comma-separated; empty = claim nothing, a pure control plane), else a
/// canvas sandbox serves `canvas/<ns>/<name>`, a runtime pod (remote engine)
/// its runtime (`rt-<name>` → `<name>`), and a console `default`.
pub(crate) fn worker_pools() -> Vec<String> {
    if let Ok(v) = std::env::var("FLOWR_WORKER_POOLS") {
        return v.split(',').map(str::trim).filter(|p| !p.is_empty()).map(String::from).collect();
    }
    if let Some(c) = std::env::var("A3T_CANVAS").ok().filter(|c| !c.is_empty()) {
        return vec![format!("canvas/{c}")];
    }
    if std::env::var("FLOWR_ENGINE_URL").map(|u| !u.is_empty()).unwrap_or(false) {
        let host = std::env::var("HOSTNAME").unwrap_or_default();
        let name = host.strip_prefix("rt-").unwrap_or(&host);
        if !name.is_empty() {
            return vec![name.to_string()];
        }
    }
    vec![DEFAULT_POOL.to_string()]
}

/// Where a run of `namespace/flow` goes (console side): its canvas sandbox,
/// else the runtime it is deployed on, else `FLOWR_DEFAULT_POOL` — the first
/// with a live worker — else `default`. The platform's own flows (`a3t`,
/// `system`) always run on the console: they read and write the platform
/// store, which only the console holds.
pub(crate) fn route(namespace: &str, flow: &str) -> String {
    if is_platform(namespace) {
        return DEFAULT_POOL.to_string();
    }
    let flow_ref = format!("{namespace}/{flow}");
    let mut candidates = vec![format!("canvas/{flow_ref}")];
    for d in crate::ops::deployments_of_flow(&flow_ref) {
        if !d.runtime.is_empty() && !d.runtime.starts_with('(') {
            candidates.push(d.runtime.clone());
        }
    }
    if let Some(p) = std::env::var("FLOWR_DEFAULT_POOL").ok().filter(|p| !p.is_empty()) {
        candidates.push(p);
    }
    candidates.into_iter().find(|p| crate::engine_remote::pool_is_live(p)).unwrap_or_else(|| DEFAULT_POOL.to_string())
}

fn is_platform(namespace: &str) -> bool {
    matches!(namespace, "a3t" | "system") || namespace.starts_with("a3t-") || namespace.starts_with("system-")
}

fn max_concurrent() -> usize {
    std::env::var("FLOWR_MAX_CONCURRENT_RUNS").ok().and_then(|v| v.parse().ok()).filter(|n| *n > 0).unwrap_or(64)
}

pub(crate) async fn run(storage: Arc<dyn StorageBackend>, engine: Arc<dyn EngineStore>, pools: Vec<String>) {
    let worker = format!(
        "{}:{}",
        std::env::var("HOSTNAME").unwrap_or_else(|_| "flowr".into()),
        &uuid::Uuid::now_v7().simple().to_string()[24..]
    );
    let max = max_concurrent();
    info!(%worker, max_concurrent = max, ?pools, "engine worker started");
    let slots = Arc::new(tokio::sync::Semaphore::new(max));
    let mut wake = futures::stream::select_all(pools.iter().map(|p| engine.subscribe(&tasks_topic(p))));
    // Keep the wake stream pending (not ended) when there are no pools.
    let mut idle_only = pools.is_empty();
    let mut turn = 0usize;
    let mut backoff = Duration::from_millis(250);

    // LanceDB maintenance as before: only while nothing is running.
    let interval = Duration::from_secs(
        std::env::var("FLOWR_COMPACT_INTERVAL_SECS").ok().and_then(|v| v.parse().ok()).unwrap_or(600),
    );
    let mut next_compact = std::time::Instant::now() + Duration::from_secs(30);

    loop {
        let free = slots.available_permits();
        if free == 0 {
            // Every slot is busy: wait for one to come back.
            drop(slots.clone().acquire_owned().await);
            continue;
        }
        // Pools in turn, so none starves another.
        let claimed = if idle_only {
            Ok(Vec::new())
        } else {
            let mut got = Ok(Vec::new());
            for i in 0..pools.len() {
                let pool = &pools[(turn + i) % pools.len()];
                match engine.claim_runs(pool, &worker, free.min(16), lease()).await {
                    Ok(c) if c.is_empty() => continue,
                    other => {
                        got = other;
                        break;
                    }
                }
            }
            turn = turn.wrapping_add(1);
            got
        };
        match claimed {
            Ok(claimed) if !claimed.is_empty() => {
                backoff = Duration::from_millis(250);
                for (lease, task, epoch) in claimed {
                    let permit = slots.clone().acquire_owned().await.expect("semaphore open");
                    let (storage, engine) = (storage.clone(), engine.clone());
                    tokio::spawn(async move {
                        run_claimed(storage, engine, lease, task, epoch).await;
                        drop(permit);
                    });
                }
            }
            Ok(_) => {
                if std::time::Instant::now() >= next_compact && slots.available_permits() == max {
                    match storage.maintain().await {
                        Ok(summary) if !summary.is_empty() => info!(%summary, "storage maintenance"),
                        Ok(_) => {}
                        Err(e) => warn!(error = %e, "storage maintenance failed"),
                    }
                    // Retention: finished runs and their journals.
                    let days = std::env::var("FLOWR_RUN_RETENTION_DAYS").ok().and_then(|v| v.parse().ok()).unwrap_or(30i64);
                    match engine.purge_finished(chrono::Utc::now() - chrono::Duration::days(days)).await {
                        Ok(n) if n > 0 => info!(removed = n, days, "run retention"),
                        Ok(_) => {}
                        Err(e) => warn!(error = %e, "run retention failed"),
                    }
                    next_compact = std::time::Instant::now() + interval;
                }
                tokio::select! {
                    w = wake.next(), if !idle_only => if w.is_none() { idle_only = pools.is_empty(); },
                    _ = tokio::time::sleep(backoff) => backoff = (backoff * 2).min(Duration::from_secs(2)),
                }
            }
            Err(e) => {
                error!(error = %e, "claiming runs failed");
                tokio::time::sleep(Duration::from_millis(500)).await;
            }
        }
    }
}

async fn run_claimed(storage: Arc<dyn StorageBackend>, engine: Arc<dyn EngineStore>, lease: Lease, task: Task, epoch: Epoch) {
    let run_id = task.run_id;
    let row = match engine.get(run_id).await {
        Ok(Some(r)) => r,
        other => {
            warn!(%run_id, ?other, "claimed a task for a run that is not there");
            let _ = engine.ack(&lease).await;
            return;
        }
    };
    let release = |phase: RunPhase, outcome: RunOutcome| {
        let engine = engine.clone();
        let lease = lease.clone();
        async move {
            match engine.release(run_id, epoch, phase, outcome, &lease).await {
                Ok(()) => {}
                Err(e @ (StoreError::Fenced(_) | StoreError::LeaseLost(_))) => {
                    warn!(%run_id, error = %e, "run was taken over before it finished here; its outcome is not recorded")
                }
                Err(e) => error!(%run_id, error = %e, "could not record the run's outcome"),
            }
        }
    };

    if row.cancel_requested {
        release(RunPhase::Cancelled, RunOutcome { output: None, error: Some("cancelled before it started".into()) }).await;
        return;
    }

    // What already happened, and the version it is pinned to.
    let hook = match JournalHook::load(engine.clone(), &row, epoch).await {
        Ok(h) => h,
        Err(e) => {
            release(RunPhase::Faulted, RunOutcome { output: None, error: Some(format!("could not read the run journal: {e}")) }).await;
            return;
        }
    };
    let version = hook.pinned_version().or_else(|| Some(row.version.clone()).filter(|v| !v.is_empty()));
    let record = match version.as_deref() {
        Some(v) => storage.workflows().get_version(&row.namespace, &row.flow, v).await,
        None => storage.workflows().get(&row.namespace, &row.flow).await,
    };
    let record = match record {
        Ok(r) => r,
        Err(e) => {
            release(RunPhase::Faulted, RunOutcome { output: None, error: Some(format!("Failed to load workflow: {e}")) }).await;
            return;
        }
    };
    // A debug run always keeps its journal: pauses live there.
    let ephemeral = !row.debug && (row.durability == Durability::Ephemeral || declared_ephemeral(&record));
    let resumed = task.attempts > 1 || !hook.is_fresh();
    if ephemeral && task.attempts > 1 {
        warn!(%run_id, attempts = task.attempts, "ephemeral run interrupted: its previous worker stopped before it finished");
        release(RunPhase::Faulted, RunOutcome { output: None, error: Some("interrupted: the process running it stopped before it finished".into()) }).await;
        return;
    }
    if !ephemeral && hook.is_fresh() {
        let started = Record::RunStarted {
            def_hash: format!("{:016x}", {
                use std::hash::{Hash, Hasher};
                let mut h = std::collections::hash_map::DefaultHasher::new();
                record.raw_yaml.hash(&mut h);
                h.finish()
            }),
            trigger: row.trigger.clone(),
            durability: Durability::Durable,
            version: record.version().to_string(),
        };
        if let Err(e) = hook.record(started).await {
            warn!(%run_id, error = %e, "could not record the run start; another worker owns it");
            return;
        }
    }
    info!(%run_id, namespace = %row.namespace, flow = %row.flow, epoch, resumed, ephemeral, "run claimed");
    let done = Arc::new(AtomicBool::new(false));
    let keeper = tokio::spawn(keep_alive(engine.clone(), lease.clone(), run_id, epoch, done.clone()));

    let request = ExecutionRequest {
        instance_id: run_id,
        namespace: row.namespace.clone(),
        name: row.flow.clone(),
        version: Some(record.version().to_string()),
        input: row.input.clone(),
        durability: Some(match row.durability {
            Durability::Durable => "durable".to_string(),
            Durability::Ephemeral => "ephemeral".to_string(),
        }),
        requested_at: row.created_at,
        caller: Some(row.trigger.clone()),
        principal: serde_json::from_value(row.principal.clone()).ok(),
    };
    // The document's `timeout`: counted from the run's first start, so time
    // spent waiting on timers counts too. Debug runs have none — a paused
    // debugger must not time out.
    let limit = if row.debug { None } else { record.parse().ok().and_then(|d| d.timeout).and_then(|t| t.duration()) };
    let mut deadline_timer = None;
    let deadline = limit.map(|l| row.started_at.unwrap_or_else(chrono::Utc::now) + chrono::Duration::from_std(l).unwrap_or(chrono::Duration::MAX));
    if let (Some(limit), Some(deadline)) = (limit, deadline) {
        let left = deadline - chrono::Utc::now();
        let why = format!("timeout: the run took longer than its {}s limit", limit.as_secs());
        match left.to_std() {
            Ok(left) if !left.is_zero() => {
                deadline_timer = Some(tokio::spawn(async move {
                    tokio::time::sleep(left).await;
                    crate::ops::stop_run(run_id, &why);
                }));
            }
            _ => {
                release(RunPhase::Faulted, RunOutcome { output: None, error: Some(why) }).await;
                return;
            }
        }
    }

    let hook = Arc::new(hook);
    let step_hook: Option<Arc<dyn swf_runtime::StepHook>> = if ephemeral { None } else { Some(hook.clone()) };
    let result = crate::run_request(storage, request, false, step_hook).await;
    if let Some(t) = deadline_timer {
        t.abort();
    }
    done.store(true, Ordering::SeqCst);
    keeper.abort();
    match hook.halt() {
        Some(Halt::Killed) => {
            // Crash test: behave as a dead process — no more writes; the lease runs out.
            warn!(%run_id, "worker stopped at a kill point (crash test)");
            return;
        }
        Some(Halt::Lost) => {
            warn!(%run_id, "the run is no longer this worker's; leaving it");
            return;
        }
        Some(Halt::Paused) => {
            info!(%run_id, "debug run paused");
            release(RunPhase::Suspended, RunOutcome::default()).await;
            return;
        }
        Some(Halt::Timer(fire_at)) => {
            // Wake it when the timer fires — or at its deadline, if that comes
            // first (it then faults on waking) — on whichever worker is free.
            let wake_at = deadline.map_or(fire_at, |d| d.min(fire_at));
            let wake = NewTask::resume_for(&row, Some(format!("{run_id}:timer:{}", wake_at.timestamp_millis())), wake_at);
            if let Err(e) = engine.enqueue(wake).await {
                error!(%run_id, error = %e, "could not schedule the timer; the run stays owned until its lease runs out and is retried");
                return;
            }
            info!(%run_id, %fire_at, "run waiting on a timer");
            release(RunPhase::Waiting, RunOutcome::default()).await;
            return;
        }
        Some(Halt::Listen(timeout_at)) => {
            // Woken by a matching event (the store enqueues that), or at the
            // step's timeout or the run's deadline, whichever comes first.
            if let Some(wake_at) = [timeout_at, deadline].into_iter().flatten().min() {
                let wake = NewTask::resume_for(&row, Some(format!("{run_id}:listen:{}", wake_at.timestamp_millis())), wake_at);
                if let Err(e) = engine.enqueue(wake).await {
                    error!(%run_id, error = %e, "could not schedule the listen timeout; the run stays owned until its lease runs out and is retried");
                    return;
                }
            }
            info!(%run_id, ?timeout_at, "run waiting for events");
            release(RunPhase::Waiting, RunOutcome::default()).await;
            return;
        }
        None => {}
    }

    let (phase, outcome) = match result {
        Ok(state) => match state.status {
            ExecutionStatus::Completed => (RunPhase::Completed, RunOutcome { output: state.output, error: None }),
            // Stopped by its deadline: a failure, not a cancel.
            ExecutionStatus::Cancelled if state.error.as_deref().is_some_and(|e| e.starts_with("timeout:")) => {
                (RunPhase::Faulted, RunOutcome { output: None, error: state.error })
            }
            ExecutionStatus::Cancelled => (RunPhase::Cancelled, RunOutcome { output: None, error: state.error }),
            _ => (RunPhase::Faulted, RunOutcome { output: None, error: state.error.or_else(|| Some("failed".into())) }),
        },
        Err(e) => (RunPhase::Faulted, RunOutcome { output: None, error: Some(e.to_string()) }),
    };
    if !ephemeral {
        let end = match phase {
            RunPhase::Completed => Record::RunCompleted { output: outcome.output.clone().unwrap_or(serde_json::Value::Null) },
            RunPhase::Cancelled => Record::RunCancelled { reason: outcome.error.clone().unwrap_or_default() },
            _ => Record::RunFaulted { error: serde_json::Value::String(outcome.error.clone().unwrap_or_default()) },
        };
        let _ = hook.record(end).await;
    }
    release(phase, outcome).await;
}

/// `metadata.a3t.durability: ephemeral` on the flow's document.
fn declared_ephemeral(record: &flowr_types::WorkflowRecord) -> bool {
    record
        .parse()
        .ok()
        .and_then(|d| d.metadata)
        .and_then(|m| m.pointer("/a3t/durability").and_then(|v| v.as_str()).map(|s| s == "ephemeral"))
        .unwrap_or(false)
}

/// Renew the lease and ownership; stop the run if either is lost or a cancel
/// arrives.
async fn keep_alive(engine: Arc<dyn EngineStore>, lease: Lease, run: RunId, epoch: Epoch, done: Arc<AtomicBool>) {
    let mut news = engine.subscribe(&run_topic(run));
    let mut tick = tokio::time::interval(self::lease() / 3);
    tick.tick().await;
    loop {
        tokio::select! {
            _ = tick.tick() => {
                if done.load(Ordering::SeqCst) {
                    return;
                }
                let lease_ok = matches!(engine.heartbeat(std::slice::from_ref(&lease), self::lease()).await.as_deref(), Ok([LeaseStatus::Renewed]));
                let owned = engine.renew(run, epoch, self::lease()).await.unwrap_or(false);
                if !(lease_ok && owned) {
                    warn!(%run, "lost ownership of the run; stopping it here");
                    crate::ops::stop_run(run, "lost ownership of the run");
                    return;
                }
            }
            msg = news.next() => {
                if msg.is_none() {
                    // The store stopped publishing; keep heartbeating.
                    news = futures::stream::pending().boxed();
                    continue;
                }
                if let Ok(Some(r)) = engine.get(run).await {
                    if r.cancel_requested {
                        crate::ops::stop_run(run, "cancelled");
                    }
                }
            }
        }
    }
}
