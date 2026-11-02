//! The engine store's wire format (FlowR's gateway, ADB's `do_action("engine")`,
//! ADB `.ai/H_ENGINE_STORE.md` H6): one op per store method, `{op, args}` in,
//! the result as JSON out, and errors that keep their kind (`fenced`,
//! `lease_lost`, `not_found`, `conflict`, `backend`), so a fenced writer is
//! told it is fenced wherever the store runs.

use std::time::Duration;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use futures::stream::BoxStream;
use serde::de::DeserializeOwned;
use serde_json::{json, Value};

use crate::engine::*;


/// A store error as the wire carries it.
pub fn err_json(e: &StoreError) -> Value {
    match e {
        StoreError::Fenced(r) => json!({ "kind": "fenced", "id": r, "message": e.to_string() }),
        StoreError::LeaseLost(t) => json!({ "kind": "lease_lost", "id": t, "message": e.to_string() }),
        StoreError::NotFound(m) => json!({ "kind": "not_found", "message": m }),
        StoreError::Conflict(m) => json!({ "kind": "conflict", "message": m }),
        StoreError::Backend(m) => json!({ "kind": "backend", "message": m }),
    }
}

/// A store error from the wire.
pub fn err_from(v: &Value) -> StoreError {
    let id = || v.get("id").and_then(|i| i.as_str()).and_then(|i| i.parse().ok()).unwrap_or_default();
    let msg = v.get("message").and_then(|m| m.as_str()).unwrap_or("").to_string();
    match v.get("kind").and_then(|k| k.as_str()).unwrap_or("") {
        "fenced" => StoreError::Fenced(id()),
        "lease_lost" => StoreError::LeaseLost(id()),
        "not_found" => StoreError::NotFound(msg),
        "conflict" => StoreError::Conflict(msg),
        _ => StoreError::Backend(msg),
    }
}

fn arg<T: DeserializeOwned>(args: &Value, k: &str) -> StoreResult<T> {
    serde_json::from_value(args.get(k).cloned().unwrap_or(Value::Null)).map_err(|e| StoreError::Backend(format!("bad argument `{k}`: {e}")))
}

fn ms(args: &Value, k: &str) -> StoreResult<Duration> {
    Ok(Duration::from_millis(arg::<u64>(args, k)?))
}

fn out<T: serde::Serialize>(v: StoreResult<T>) -> StoreResult<Value> {
    v.map(|x| serde_json::to_value(x).unwrap_or(Value::Null))
}

/// Run one op against `store`; unknown ops are a `backend` error.
pub async fn dispatch(store: &dyn EngineStore, op: &str, a: &Value) -> StoreResult<Value> {
    match op {
        "enqueue" => out(store.enqueue(arg(a, "task")?).await),
        "heartbeat" => out(store.heartbeat(&arg::<Vec<Lease>>(a, "leases")?, ms(a, "extend_ms")?).await),
        "ack" => out(store.ack(&arg(a, "lease")?).await),
        "nack" => out(store.nack(&arg(a, "lease")?, arg(a, "retry_at")?).await),
        "pending" => out(store.pending(&arg::<String>(a, "pool")?).await),
        "live_pools" => out(store.live_pools(ms(a, "within_ms")?).await),
        "start" => out(store.start(arg(a, "run")?, arg(a, "task")?).await),
        "claim_runs" => out(store.claim_runs(&arg::<String>(a, "pool")?, &arg::<String>(a, "worker")?, arg(a, "n")?, ms(a, "lease_ms")?).await),
        "renew" => out(store.renew(arg(a, "run")?, arg(a, "epoch")?, ms(a, "lease_ms")?).await),
        "release" => out(store.release(arg(a, "run")?, arg(a, "epoch")?, arg(a, "phase")?, arg(a, "outcome")?, &arg(a, "lease")?).await),
        "request" => out(store.request(arg(a, "run")?, arg(a, "req")?).await),
        "get" => out(store.get(arg(a, "run")?).await),
        "list" => out(store.list(arg(a, "q")?).await),
        "orphaned" => out(store.orphaned().await),
        "purge_finished" => out(store.purge_finished(arg(a, "before")?).await),
        "open_wait" => out(store.open_wait(arg(a, "run")?, arg(a, "epoch")?, &arg::<String>(a, "wait_id")?, arg(a, "correlation")?).await),
        "close_wait" => out(store.close_wait(arg(a, "run")?, arg(a, "epoch")?, &arg::<String>(a, "wait_id")?).await),
        "deliver_event" => out(store.deliver_event(&arg::<String>(a, "namespace")?, arg(a, "event")?).await),
        "open_approval" => out(store.open_approval(arg(a, "run")?, arg(a, "epoch")?, &arg::<String>(a, "wait_id")?, arg(a, "request")?).await),
        "approvals" => out(store.approvals().await),
        "decide_approval" => out(store.decide_approval(&arg::<String>(a, "wait_id")?, arg(a, "decision")?, &arg::<String>(a, "by")?).await),
        "append" => out(store.append(arg(a, "run")?, arg(a, "epoch")?, &arg::<Vec<Record>>(a, "records")?).await),
        "read_from" => out(store.read_from(arg(a, "run")?, arg(a, "after_seq")?).await),
        "commit_step" => out(store.commit_step(arg(a, "run")?, arg(a, "epoch")?, arg(a, "commit")?).await),
        "latest_checkpoint" => out(store.latest_checkpoint(arg(a, "run")?).await),
        "checkpoints" => out(store.checkpoints(arg(a, "run")?).await),
        "outbox" => out(store.outbox(arg(a, "limit")?).await),
        "outbox_done" => out(store.outbox_done(arg(a, "up_to_id")?).await),
        "publish" => out(store.publish(&arg::<String>(a, "topic")?, arg(a, "msg")?).await),
        other => Err(StoreError::Backend(format!("unknown op `{other}`"))),
    }
}

// ── Client ──

/// How a [`Client`] reaches a store: one op (the `{ok}` / `{err}` envelope
/// already unwrapped, see [`unwrap`]) and a topic's notices.
#[async_trait]
pub trait Transport: Send + Sync {
    async fn call(&self, op: &str, args: Value) -> StoreResult<Value>;
    /// Notices on `topic`, best effort (claimers also poll).
    fn subscribe(&self, topic: &str) -> BoxStream<'static, Value>;
}

/// An answer in the wire's envelope as a store result.
pub fn unwrap(body: Value) -> StoreResult<Value> {
    match body.get("err") {
        Some(e) => Err(err_from(e)),
        None => Ok(body.get("ok").cloned().unwrap_or(Value::Null)),
    }
}

/// The engine store traits over a [`Transport`]: FlowR's gateway client (HTTP)
/// and ADB's (Flight) share this encoding with [`dispatch`].
pub struct Client<T: Transport> {
    transport: T,
}

impl<T: Transport> Client<T> {
    pub fn new(transport: T) -> Self {
        Self { transport }
    }

    pub fn transport(&self) -> &T {
        &self.transport
    }

    async fn op<R: DeserializeOwned>(&self, op: &str, args: Value) -> StoreResult<R> {
        let v = self.transport.call(op, args).await?;
        serde_json::from_value(v).map_err(|e| StoreError::Backend(format!("{op}: {e}")))
    }
}

#[async_trait]
impl<T: Transport> TaskQueue for Client<T> {
    async fn enqueue(&self, t: NewTask) -> StoreResult<EnqueueOutcome> {
        self.op("enqueue", json!({ "task": t })).await
    }
    async fn heartbeat(&self, leases: &[Lease], extend: Duration) -> StoreResult<Vec<LeaseStatus>> {
        self.op("heartbeat", json!({ "leases": leases, "extend_ms": extend.as_millis() as u64 })).await
    }
    async fn ack(&self, lease: &Lease) -> StoreResult<()> {
        self.op("ack", json!({ "lease": lease })).await
    }
    async fn nack(&self, lease: &Lease, retry_at: Option<DateTime<Utc>>) -> StoreResult<()> {
        self.op("nack", json!({ "lease": lease, "retry_at": retry_at })).await
    }
    async fn pending(&self, pool: &str) -> StoreResult<usize> {
        self.op("pending", json!({ "pool": pool })).await
    }
    async fn live_pools(&self, within: Duration) -> StoreResult<Vec<String>> {
        self.op("live_pools", json!({ "within_ms": within.as_millis() as u64 })).await
    }
}

#[async_trait]
impl<T: Transport> RunStore for Client<T> {
    async fn start(&self, run: NewRun, task: NewTask) -> StoreResult<(RunId, EnqueueOutcome)> {
        self.op("start", json!({ "run": run, "task": task })).await
    }
    async fn claim_runs(&self, pool: &str, worker: &str, n: usize, lease: Duration) -> StoreResult<Vec<(Lease, Task, Epoch)>> {
        self.op("claim_runs", json!({ "pool": pool, "worker": worker, "n": n, "lease_ms": lease.as_millis() as u64 })).await
    }
    async fn renew(&self, run: RunId, epoch: Epoch, lease: Duration) -> StoreResult<bool> {
        self.op("renew", json!({ "run": run, "epoch": epoch, "lease_ms": lease.as_millis() as u64 })).await
    }
    async fn release(&self, run: RunId, epoch: Epoch, phase: RunPhase, outcome: RunOutcome, lease: &Lease) -> StoreResult<()> {
        self.op("release", json!({ "run": run, "epoch": epoch, "phase": phase, "outcome": outcome, "lease": lease })).await
    }
    async fn request(&self, run: RunId, req: RunRequest) -> StoreResult<()> {
        self.op("request", json!({ "run": run, "req": req })).await
    }
    async fn get(&self, run: RunId) -> StoreResult<Option<RunRow>> {
        self.op("get", json!({ "run": run })).await
    }
    async fn list(&self, q: RunQuery) -> StoreResult<Page<RunRow>> {
        self.op("list", json!({ "q": q })).await
    }
    async fn orphaned(&self) -> StoreResult<Vec<RunRow>> {
        self.op("orphaned", json!({})).await
    }
    async fn purge_finished(&self, before: DateTime<Utc>) -> StoreResult<usize> {
        self.op("purge_finished", json!({ "before": before })).await
    }
    async fn open_wait(&self, run: RunId, epoch: Epoch, wait_id: &str, correlation: Value) -> StoreResult<()> {
        self.op("open_wait", json!({ "run": run, "epoch": epoch, "wait_id": wait_id, "correlation": correlation })).await
    }
    async fn close_wait(&self, run: RunId, epoch: Epoch, wait_id: &str) -> StoreResult<bool> {
        self.op("close_wait", json!({ "run": run, "epoch": epoch, "wait_id": wait_id })).await
    }
    async fn deliver_event(&self, namespace: &str, event: Value) -> StoreResult<Vec<RunId>> {
        self.op("deliver_event", json!({ "namespace": namespace, "event": event })).await
    }
    async fn open_approval(&self, run: RunId, epoch: Epoch, wait_id: &str, request: Value) -> StoreResult<()> {
        self.op("open_approval", json!({ "run": run, "epoch": epoch, "wait_id": wait_id, "request": request })).await
    }
    async fn approvals(&self) -> StoreResult<Vec<(RunId, String, Value)>> {
        self.op("approvals", json!({})).await
    }
    async fn decide_approval(&self, wait_id: &str, decision: Value, by: &str) -> StoreResult<bool> {
        self.op("decide_approval", json!({ "wait_id": wait_id, "decision": decision, "by": by })).await
    }
}

#[async_trait]
impl<T: Transport> Journal for Client<T> {
    async fn append(&self, run: RunId, epoch: Epoch, records: &[Record]) -> StoreResult<AppendOutcome> {
        self.op("append", json!({ "run": run, "epoch": epoch, "records": records })).await
    }
    async fn read_from(&self, run: RunId, after_seq: u64) -> StoreResult<Vec<JournalEntry>> {
        self.op("read_from", json!({ "run": run, "after_seq": after_seq })).await
    }
    async fn commit_step(&self, run: RunId, epoch: Epoch, c: StepCommit) -> StoreResult<u64> {
        self.op("commit_step", json!({ "run": run, "epoch": epoch, "commit": c })).await
    }
    async fn latest_checkpoint(&self, run: RunId) -> StoreResult<Option<(u64, Vec<u8>)>> {
        self.op("latest_checkpoint", json!({ "run": run })).await
    }
    async fn checkpoints(&self, run: RunId) -> StoreResult<Vec<CheckpointMeta>> {
        self.op("checkpoints", json!({ "run": run })).await
    }
    async fn outbox(&self, limit: usize) -> StoreResult<Vec<OutboxRow>> {
        self.op("outbox", json!({ "limit": limit })).await
    }
    async fn outbox_done(&self, up_to_id: u64) -> StoreResult<()> {
        self.op("outbox_done", json!({ "up_to_id": up_to_id })).await
    }
}

#[async_trait]
impl<T: Transport> Notifier for Client<T> {
    async fn publish(&self, topic: &str, msg: Value) -> StoreResult<()> {
        self.op("publish", json!({ "topic": topic, "msg": msg })).await
    }

    fn subscribe(&self, topic: &str) -> BoxStream<'static, Value> {
        self.transport.subscribe(topic)
    }
}
