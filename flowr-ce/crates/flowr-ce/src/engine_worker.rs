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
//! - **Recovery:** a run whose earlier claim never finished (its owner died) is
//!   claimed again once that lease expires. Until runs are journaled (stage 4)
//!   it cannot continue without repeating its steps, so it is faulted
//!   `interrupted`, never silently re-run.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use futures::StreamExt;
use tracing::{error, info, warn};

use flowr_storage::engine::*;
use flowr_storage::StorageBackend;
use flowr_types::{ExecutionRequest, ExecutionStatus};

const LEASE: Duration = Duration::from_secs(30);
const HEARTBEAT: Duration = Duration::from_secs(10);

fn max_concurrent() -> usize {
    std::env::var("FLOWR_MAX_CONCURRENT_RUNS").ok().and_then(|v| v.parse().ok()).filter(|n| *n > 0).unwrap_or(64)
}

pub(crate) async fn run(storage: Arc<dyn StorageBackend>, engine: Arc<dyn EngineStore>) {
    let worker = format!(
        "{}:{}",
        std::env::var("HOSTNAME").unwrap_or_else(|_| "flowr".into()),
        &uuid::Uuid::now_v7().simple().to_string()[24..]
    );
    let max = max_concurrent();
    info!(%worker, max_concurrent = max, "engine worker started");
    let slots = Arc::new(tokio::sync::Semaphore::new(max));
    let mut wake = engine.subscribe(&tasks_topic(DEFAULT_POOL));
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
        match engine.claim_runs(DEFAULT_POOL, &worker, free.min(16), LEASE).await {
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
                    next_compact = std::time::Instant::now() + interval;
                }
                tokio::select! {
                    _ = wake.next() => {}
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

    if task.attempts > 1 {
        warn!(%run_id, attempts = task.attempts, "run interrupted: its previous worker stopped before it finished");
        release(RunPhase::Faulted, RunOutcome { output: None, error: Some("interrupted: the process running it stopped before it finished".into()) }).await;
        return;
    }
    if row.cancel_requested {
        release(RunPhase::Cancelled, RunOutcome { output: None, error: Some("cancelled before it started".into()) }).await;
        return;
    }

    info!(%run_id, namespace = %row.namespace, flow = %row.flow, epoch, "run claimed");
    let done = Arc::new(AtomicBool::new(false));
    let keeper = tokio::spawn(keep_alive(engine.clone(), lease.clone(), run_id, epoch, done.clone()));

    let request = ExecutionRequest {
        instance_id: run_id,
        namespace: row.namespace.clone(),
        name: row.flow.clone(),
        version: Some(row.version.clone()).filter(|v| !v.is_empty()),
        input: row.input.clone(),
        durability: Some(match row.durability {
            Durability::Durable => "durable".to_string(),
            Durability::Ephemeral => "ephemeral".to_string(),
        }),
        requested_at: row.created_at,
        caller: Some(row.trigger.clone()),
        principal: serde_json::from_value(row.principal.clone()).ok(),
    };
    let result = crate::run_request(storage, request, false).await;
    done.store(true, Ordering::SeqCst);
    keeper.abort();

    let (phase, outcome) = match result {
        Ok(state) => match state.status {
            ExecutionStatus::Completed => (RunPhase::Completed, RunOutcome { output: state.output, error: None }),
            ExecutionStatus::Cancelled => (RunPhase::Cancelled, RunOutcome { output: None, error: state.error }),
            _ => (RunPhase::Faulted, RunOutcome { output: None, error: state.error.or_else(|| Some("failed".into())) }),
        },
        Err(e) => (RunPhase::Faulted, RunOutcome { output: None, error: Some(e.to_string()) }),
    };
    release(phase, outcome).await;
}

/// Renew the lease and ownership; stop the run if either is lost or a cancel
/// arrives.
async fn keep_alive(engine: Arc<dyn EngineStore>, lease: Lease, run: RunId, epoch: Epoch, done: Arc<AtomicBool>) {
    let mut news = engine.subscribe(&run_topic(run));
    let mut tick = tokio::time::interval(HEARTBEAT);
    tick.tick().await;
    loop {
        tokio::select! {
            _ = tick.tick() => {
                if done.load(Ordering::SeqCst) {
                    return;
                }
                let lease_ok = matches!(engine.heartbeat(std::slice::from_ref(&lease), LEASE).await.as_deref(), Ok([LeaseStatus::Renewed]));
                let owned = engine.renew(run, epoch, LEASE).await.unwrap_or(false);
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
