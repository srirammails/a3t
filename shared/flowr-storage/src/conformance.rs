//! Conformance suite for engine stores (`.ai/FLOWR_QUEUE_CHECKPOINT_SPEC.md` §12).
//!
//! Every backend runs the same checks: `run_all(|| async { MyStore::new() })`
//! from its own tests. Each check gets a fresh store. Lease-expiry checks use
//! short real leases, so the suite takes about a second.

use std::future::Future;
use std::time::Duration;

use chrono::Utc;
use futures::StreamExt;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::engine::*;

const LEASE: Duration = Duration::from_secs(30);
const SHORT: Duration = Duration::from_millis(60);

pub async fn run_all<S, F, Fut>(make: F)
where
    S: EngineStore,
    F: Fn() -> Fut,
    Fut: Future<Output = S>,
{
    start_then_claim_takes_ownership(&make().await).await;
    start_dedupe_returns_the_first_run(&make().await).await;
    enqueue_dedupe_until_acked(&make().await).await;
    delayed_visibility(&make().await).await;
    lease_expiry_reclaim_and_fencing(&make().await).await;
    stale_lease_cannot_ack_or_nack(&make().await).await;
    owned_run_is_not_claimed_twice(&make().await).await;
    journal_appends_are_idempotent_and_ordered(&make().await).await;
    commit_step_is_atomic_under_fencing(&make().await).await;
    release_sets_phase_and_acks(&make().await).await;
    list_is_newest_first_with_filters_and_cursor(&make().await).await;
    cancel_and_suspend_requests(&make().await).await;
    debug_requests_only_on_paused_debug_runs(&make().await).await;
    notifier_reports_appends(&make().await).await;
    orphaned_runs_after_owner_lease_expiry(&make().await).await;
    outbox_drains_in_order(&make().await).await;
    purge_removes_only_old_finished_runs(&make().await).await;
}

fn new_run(ns: &str, flow: &str) -> NewRun {
    NewRun {
        run_id: Uuid::now_v7(),
        namespace: ns.into(),
        flow: flow.into(),
        version: "1.0.0".into(),
        def_hash: "h".into(),
        parent_run_id: None,
        durability: Durability::Durable,
        debug: false,
        breakpoints: None,
        principal: json!({ "kind": "user", "id": "u1" }),
        trigger: "api".into(),
        input: json!({ "x": 1 }),
        deadline_at: None,
        dedupe_key: None,
    }
}

async fn start<S: EngineStore>(s: &S, run: NewRun) -> RunId {
    let task = NewTask::start(&run);
    s.start(run, task).await.expect("start").0
}

fn completed(key: &str) -> Record {
    Record::StepCompleted {
        step_key: key.into(),
        attempt: 0,
        output: json!(1),
        context: json!({}),
        class: StepClass::Pure,
        duration_us: 1, directive: String::new(), input: Value::Null, at: None,
    }
}

async fn start_then_claim_takes_ownership<S: EngineStore>(s: &S) {
    let id = start(s, new_run("p", "f")).await;
    let row = s.get(id).await.unwrap().unwrap();
    assert_eq!(row.phase, RunPhase::Pending, "a started run is pending");
    assert_eq!(row.principal["id"], "u1", "the principal is stored with the run");

    let claimed = s.claim_runs(DEFAULT_POOL, "w1", 10, LEASE).await.unwrap();
    assert_eq!(claimed.len(), 1);
    let (_, task, epoch) = &claimed[0];
    assert_eq!(task.run_id, id);
    assert_eq!(task.kind, TaskKind::StartRun);
    assert_eq!(*epoch, 1, "first owner gets epoch 1");
    let row = s.get(id).await.unwrap().unwrap();
    assert_eq!(row.phase, RunPhase::Running);
    assert_eq!(row.owner.as_deref(), Some("w1"));
    assert!(row.started_at.is_some());
    assert!(s.claim_runs(DEFAULT_POOL, "w2", 10, LEASE).await.unwrap().is_empty(), "a leased task is not claimed again");
}

async fn start_dedupe_returns_the_first_run<S: EngineStore>(s: &S) {
    let mut a = new_run("p", "hook");
    a.dedupe_key = Some("webhook:abc".into());
    let mut b = new_run("p", "hook");
    b.dedupe_key = Some("webhook:abc".into());
    let first = start(s, a).await;
    let task = NewTask::start(&b);
    let (second, out) = s.start(b, task).await.unwrap();
    assert_eq!(second, first, "a duplicate delivery returns the existing run");
    assert!(matches!(out, EnqueueOutcome::Existing(_)));
    assert_eq!(s.pending(DEFAULT_POOL).await.unwrap(), 1, "no second task");
}

async fn enqueue_dedupe_until_acked<S: EngineStore>(s: &S) {
    let id = start(s, new_run("p", "f")).await;
    let (lease, _, _) = s.claim_runs(DEFAULT_POOL, "w", 1, LEASE).await.unwrap().remove(0);
    s.ack(&lease).await.unwrap();
    let t = || NewTask::resume(id, "p", Some(format!("{id}:wait-1")), Utc::now());
    let a = s.enqueue(t()).await.unwrap();
    let b = s.enqueue(t()).await.unwrap();
    assert!(matches!(a, EnqueueOutcome::Created(_)));
    assert_eq!(b, EnqueueOutcome::Existing(a.task_id()), "same dedupe key → same task");
}

async fn delayed_visibility<S: EngineStore>(s: &S) {
    let run = new_run("p", "f");
    let mut task = NewTask::start(&run);
    task.visible_at = Utc::now() + chrono::Duration::milliseconds(80);
    s.start(run, task).await.unwrap();
    assert!(s.claim_runs(DEFAULT_POOL, "w", 1, LEASE).await.unwrap().is_empty(), "not visible yet");
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(s.claim_runs(DEFAULT_POOL, "w", 1, LEASE).await.unwrap().len(), 1, "visible after visible_at");
}

async fn lease_expiry_reclaim_and_fencing<S: EngineStore>(s: &S) {
    let id = start(s, new_run("p", "f")).await;
    let (old_lease, _, old_epoch) = s.claim_runs(DEFAULT_POOL, "w1", 1, SHORT).await.unwrap().remove(0);
    s.append(id, old_epoch, &[completed("/do/0/a")]).await.expect("owner appends");

    tokio::time::sleep(SHORT * 2).await;
    let (new_lease, _, new_epoch) = s.claim_runs(DEFAULT_POOL, "w2", 1, LEASE).await.unwrap().pop().expect("reclaimed after expiry");
    assert_eq!(new_epoch, old_epoch + 1, "a new owner bumps the epoch");
    assert_ne!(new_lease.token, old_lease.token);

    assert_eq!(s.append(id, old_epoch, &[completed("/do/1/b")]).await, Err(StoreError::Fenced(id)), "the old owner is fenced");
    assert!(!s.renew(id, old_epoch, LEASE).await.unwrap(), "the old owner cannot renew");
    assert!(s.renew(id, new_epoch, LEASE).await.unwrap());
    assert_eq!(s.ack(&old_lease).await, Err(StoreError::LeaseLost(old_lease.task_id)), "the old lease cannot ack");
    assert_eq!(
        s.release(id, old_epoch, RunPhase::Completed, RunOutcome::default(), &old_lease).await,
        Err(StoreError::Fenced(id)),
        "the old owner cannot release"
    );
    let rows = s.read_from(id, 0).await.unwrap();
    assert_eq!(rows.len(), 1, "nothing from the fenced write");
    assert_eq!(rows[0].epoch, old_epoch);
}

async fn stale_lease_cannot_ack_or_nack<S: EngineStore>(s: &S) {
    start(s, new_run("p", "f")).await;
    let (lease, _, _) = s.claim_runs(DEFAULT_POOL, "w", 1, LEASE).await.unwrap().remove(0);
    let forged = Lease { token: lease.token + 7, ..lease.clone() };
    assert!(s.ack(&forged).await.is_err());
    assert!(s.nack(&forged, None).await.is_err());
    s.nack(&lease, None).await.unwrap();
    assert!(s.ack(&lease).await.is_err(), "a nacked lease is no longer held");
    let st = s.heartbeat(&[lease.clone()], LEASE).await.unwrap();
    assert_eq!(st, vec![LeaseStatus::Lost]);
}

async fn owned_run_is_not_claimed_twice<S: EngineStore>(s: &S) {
    let id = start(s, new_run("p", "f")).await;
    s.claim_runs(DEFAULT_POOL, "w1", 1, LEASE).await.unwrap();
    // A wake-up for the same run while it is owned must wait.
    s.enqueue(NewTask::resume(id, "p", None, Utc::now())).await.unwrap();
    assert!(s.claim_runs(DEFAULT_POOL, "w2", 5, LEASE).await.unwrap().is_empty(), "the owned run is not claimed by another worker");
    assert_eq!(s.pending(DEFAULT_POOL).await.unwrap(), 2, "the resume task stays queued");
}

async fn journal_appends_are_idempotent_and_ordered<S: EngineStore>(s: &S) {
    let id = start(s, new_run("p", "f")).await;
    let (_, _, epoch) = s.claim_runs(DEFAULT_POOL, "w", 1, LEASE).await.unwrap().remove(0);
    let started = Record::StepStarted { step_key: "/do/0/a".into(), attempt: 0, idem_key: "k".into(), input_hash: "i".into() };
    let out = s.append(id, epoch, &[started.clone(), completed("/do/0/a")]).await.unwrap();
    assert_eq!((out.last_seq, out.duplicates), (2, 0));
    let again = s.append(id, epoch, &[started, completed("/do/0/a"), completed("/do/1/b")]).await.unwrap();
    assert_eq!((again.last_seq, again.duplicates), (3, 2), "re-appending a record is a no-op");
    // Non-step records are not deduplicated.
    s.append(id, epoch, &[Record::Suspended, Record::Suspended]).await.unwrap();

    let all = s.read_from(id, 0).await.unwrap();
    assert_eq!(all.iter().map(|e| e.seq).collect::<Vec<_>>(), vec![1, 2, 3, 4, 5]);
    assert_eq!(s.read_from(id, 3).await.unwrap().len(), 2, "read_from is exclusive");
    let row = s.get(id).await.unwrap().unwrap();
    assert_eq!(row.last_seq, 5);
    assert_eq!(row.current_step.as_deref(), Some("/do/1/b"));
    assert!(matches!(s.append(Uuid::now_v7(), 1, &[Record::Suspended]).await, Err(StoreError::NotFound(_))), "unknown run");
}

async fn commit_step_is_atomic_under_fencing<S: EngineStore>(s: &S) {
    let id = start(s, new_run("p", "f")).await;
    let (_, _, epoch) = s.claim_runs(DEFAULT_POOL, "w", 1, LEASE).await.unwrap().remove(0);
    let commit = |key: &str| StepCommit {
        records: vec![completed(key)],
        checkpoint: Some(vec![1, 2, 3]),
        outbox: vec![("usage".into(), json!({ "step": key }))],
    };
    let seq = s.commit_step(id, epoch, commit("/do/0/a")).await.unwrap();
    assert_eq!(seq, 1);
    assert_eq!(s.latest_checkpoint(id).await.unwrap(), Some((1, vec![1, 2, 3])));
    assert_eq!(s.outbox(10).await.unwrap().len(), 1);

    assert_eq!(s.commit_step(id, epoch + 1, commit("/do/1/b")).await, Err(StoreError::Fenced(id)));
    assert_eq!(s.read_from(id, 0).await.unwrap().len(), 1, "no record from the fenced commit");
    assert_eq!(s.checkpoints(id).await.unwrap().len(), 1, "no checkpoint from the fenced commit");
    assert_eq!(s.outbox(10).await.unwrap().len(), 1, "no outbox row from the fenced commit");
}

async fn release_sets_phase_and_acks<S: EngineStore>(s: &S) {
    let id = start(s, new_run("p", "f")).await;
    let (lease, _, epoch) = s.claim_runs(DEFAULT_POOL, "w", 1, LEASE).await.unwrap().remove(0);
    s.release(id, epoch, RunPhase::Completed, RunOutcome { output: Some(json!({ "ok": true })), error: None }, &lease)
        .await
        .unwrap();
    let row = s.get(id).await.unwrap().unwrap();
    assert_eq!(row.phase, RunPhase::Completed);
    assert_eq!(row.output, Some(json!({ "ok": true })));
    assert!(row.owner.is_none());
    assert!(row.completed_at.is_some());
    assert_eq!(s.pending(DEFAULT_POOL).await.unwrap(), 0, "the task is acked");

    // A wait: released without finishing, woken later by a resume task.
    let w = start(s, new_run("p", "g")).await;
    let (lease, _, epoch) = s.claim_runs(DEFAULT_POOL, "w", 1, LEASE).await.unwrap().remove(0);
    s.release(w, epoch, RunPhase::Waiting, RunOutcome::default(), &lease).await.unwrap();
    s.enqueue(NewTask::resume(w, "p", None, Utc::now())).await.unwrap();
    let (_, task, epoch2) = s.claim_runs(DEFAULT_POOL, "w", 1, LEASE).await.unwrap().remove(0);
    assert_eq!(task.kind, TaskKind::ResumeRun);
    assert_eq!(epoch2, epoch + 1);
    assert_eq!(s.get(w).await.unwrap().unwrap().resume_count, 1);
}

async fn list_is_newest_first_with_filters_and_cursor<S: EngineStore>(s: &S) {
    let mut ids = Vec::new();
    for i in 0..7 {
        ids.push(start(s, new_run(if i % 2 == 0 { "a" } else { "b" }, "f")).await);
        tokio::time::sleep(Duration::from_millis(2)).await;
    }
    let page = s.list(RunQuery { limit: 3, ..Default::default() }).await.unwrap();
    assert_eq!(page.items.iter().map(|r| r.run_id).collect::<Vec<_>>(), vec![ids[6], ids[5], ids[4]], "newest first");
    let page2 = s.list(RunQuery { limit: 3, cursor: page.next.clone(), ..Default::default() }).await.unwrap();
    assert_eq!(page2.items.iter().map(|r| r.run_id).collect::<Vec<_>>(), vec![ids[3], ids[2], ids[1]], "the cursor continues");
    let page3 = s.list(RunQuery { limit: 3, cursor: page2.next.clone(), ..Default::default() }).await.unwrap();
    assert_eq!(page3.items.len(), 1);
    assert!(page3.next.is_none(), "no cursor at the end");

    let only_a = s.list(RunQuery { namespaces: Some(vec!["a".into()]), limit: 10, ..Default::default() }).await.unwrap();
    assert_eq!(only_a.items.len(), 4);
    assert!(only_a.items.iter().all(|r| r.namespace == "a"));
    let none = s.list(RunQuery { namespaces: Some(vec![]), limit: 10, ..Default::default() }).await.unwrap();
    assert!(none.items.is_empty(), "no visible namespaces → no runs");
    let running = s.list(RunQuery { phases: vec![RunPhase::Running], limit: 10, ..Default::default() }).await.unwrap();
    assert!(running.items.is_empty());
}

async fn cancel_and_suspend_requests<S: EngineStore>(s: &S) {
    // Not running anywhere: cancelled at once, its task removed.
    let queued = start(s, new_run("p", "f")).await;
    s.request(queued, RunRequest::Cancel { by: "u1".into(), reason: "not needed".into() }).await.unwrap();
    let row = s.get(queued).await.unwrap().unwrap();
    assert_eq!(row.phase, RunPhase::Cancelled);
    assert_eq!(row.error.as_deref(), Some("not needed"));
    assert_eq!(s.pending(DEFAULT_POOL).await.unwrap(), 0);

    // Running: only flagged; the owner stops it between steps.
    let running = start(s, new_run("p", "g")).await;
    s.claim_runs(DEFAULT_POOL, "w", 1, LEASE).await.unwrap();
    s.request(running, RunRequest::Cancel { by: "u1".into(), reason: "stop".into() }).await.unwrap();
    let row = s.get(running).await.unwrap().unwrap();
    assert_eq!(row.phase, RunPhase::Running);
    assert!(row.cancel_requested);

    // Suspend while not running, then resume: a resume task appears.
    let parked = start(s, new_run("p", "h")).await;
    s.request(parked, RunRequest::Suspend { by: "u1".into() }).await.unwrap();
    assert_eq!(s.get(parked).await.unwrap().unwrap().phase, RunPhase::Suspended);
    assert!(s.claim_runs(DEFAULT_POOL, "w", 5, LEASE).await.unwrap().is_empty(), "a suspended run is not claimable");
    s.request(parked, RunRequest::Resume { by: "u1".into() }).await.unwrap();
    let claimed = s.claim_runs(DEFAULT_POOL, "w", 5, LEASE).await.unwrap();
    assert_eq!(claimed.len(), 1);
    assert_eq!(claimed[0].1.kind, TaskKind::ResumeRun);

    // A finished run ignores requests.
    s.request(queued, RunRequest::Suspend { by: "u1".into() }).await.unwrap();
    assert_eq!(s.get(queued).await.unwrap().unwrap().phase, RunPhase::Cancelled);
}

async fn debug_requests_only_on_paused_debug_runs<S: EngineStore>(s: &S) {
    let plain = start(s, new_run("p", "f")).await;
    assert!(s.request(plain, RunRequest::Breakpoints(json!([]))).await.is_err(), "breakpoints need a debug run");

    let mut d = new_run("p", "dbg");
    d.debug = true;
    let id = start(s, d).await;
    s.request(id, RunRequest::Breakpoints(json!([{ "step_key": "/do/1/x", "when": "before" }]))).await.unwrap();
    assert!(s.request(id, RunRequest::Debug { command: json!("continue"), by: "u1".into() }).await.is_err(), "not paused yet");
    // Claim the debug run (the plain one is claimed too); pause it.
    let claimed = s.claim_runs(DEFAULT_POOL, "w", 5, LEASE).await.unwrap();
    let (lease, _, epoch) = claimed.into_iter().find(|(_, t, _)| t.run_id == id).unwrap();
    s.append(id, epoch, &[Record::Paused { step_key: "/do/1/x".into(), when: "before".into(), reason: "breakpoint".into(), input: Value::Null }]).await.unwrap();
    s.release(id, epoch, RunPhase::Suspended, RunOutcome::default(), &lease).await.unwrap();
    s.request(id, RunRequest::Debug { command: json!({ "step_over": true }), by: "u1".into() }).await.unwrap();
    let kinds: Vec<_> = s.read_from(id, 0).await.unwrap().into_iter().map(|e| e.record).collect();
    assert!(matches!(kinds.last(), Some(Record::DebugCommand { .. })), "the command is journaled");
    let claimed = s.claim_runs(DEFAULT_POOL, "w", 5, LEASE).await.unwrap();
    assert!(claimed.iter().any(|(_, t, _)| t.run_id == id && t.kind == TaskKind::ResumeRun), "the command wakes the run");
}

async fn notifier_reports_appends<S: EngineStore>(s: &S) {
    let id = start(s, new_run("p", "f")).await;
    let mut sub = s.subscribe(&run_topic(id));
    let (_, _, epoch) = s.claim_runs(DEFAULT_POOL, "w", 1, LEASE).await.unwrap().remove(0);
    s.append(id, epoch, &[completed("/do/0/a")]).await.unwrap();
    let msg = tokio::time::timeout(Duration::from_secs(1), sub.next()).await.expect("a notification").expect("open");
    assert_eq!(msg["seq"], 1);
}

async fn orphaned_runs_after_owner_lease_expiry<S: EngineStore>(s: &S) {
    let id = start(s, new_run("p", "f")).await;
    s.claim_runs(DEFAULT_POOL, "w", 1, SHORT).await.unwrap();
    assert!(s.orphaned().await.unwrap().is_empty(), "owned and alive");
    tokio::time::sleep(SHORT * 2).await;
    let orphans = s.orphaned().await.unwrap();
    assert_eq!(orphans.iter().map(|r| r.run_id).collect::<Vec<_>>(), vec![id]);
}

async fn outbox_drains_in_order<S: EngineStore>(s: &S) {
    let id = start(s, new_run("p", "f")).await;
    let (_, _, epoch) = s.claim_runs(DEFAULT_POOL, "w", 1, LEASE).await.unwrap().remove(0);
    for i in 0..3 {
        s.commit_step(id, epoch, StepCommit { records: vec![completed(&format!("/do/{i}/s"))], checkpoint: None, outbox: vec![("lifecycle".into(), json!(i))] })
            .await
            .unwrap();
    }
    let rows = s.outbox(2).await.unwrap();
    assert_eq!(rows.iter().map(|r| r.body.clone()).collect::<Vec<_>>(), vec![json!(0), json!(1)]);
    s.outbox_done(rows[1].id).await.unwrap();
    let rest = s.outbox(10).await.unwrap();
    assert_eq!(rest.len(), 1);
    assert_eq!(rest[0].body, json!(2));
}

async fn purge_removes_only_old_finished_runs<S: EngineStore>(s: &S) {
    let done = start(s, new_run("p", "f")).await;
    let (lease, _, epoch) = s.claim_runs(DEFAULT_POOL, "w", 1, LEASE).await.unwrap().remove(0);
    s.append(done, epoch, &[completed("/do/0/a")]).await.unwrap();
    s.release(done, epoch, RunPhase::Completed, RunOutcome::default(), &lease).await.unwrap();
    let open = start(s, new_run("p", "g")).await;
    assert_eq!(s.purge_finished(Utc::now() - chrono::Duration::days(1)).await.unwrap(), 0, "nothing that old");
    assert_eq!(s.purge_finished(Utc::now() + chrono::Duration::seconds(1)).await.unwrap(), 1);
    assert!(s.get(done).await.unwrap().is_none());
    assert!(s.read_from(done, 0).await.unwrap().is_empty(), "its journal goes too");
    assert!(s.get(open).await.unwrap().is_some(), "unfinished runs stay");
}
