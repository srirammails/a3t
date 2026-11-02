//! In-memory engine store: the reference implementation of the engine
//! contracts — [`EngineCore`] (the rules, `engine_core.rs`) behind a mutex,
//! with notices delivered to in-process subscribers. Nothing survives the
//! process. ADB, FlowR's durable store, runs the same core; the conformance
//! suite runs against both. (The SQLite store this used to be compared with is
//! kept on the branch `engine-sqlite`.)

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Duration;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use futures::channel::mpsc::{unbounded, UnboundedSender};
use futures::stream::BoxStream;
use futures::StreamExt;
use serde_json::Value;

use crate::engine::*;
use crate::engine_core::EngineCore;

pub struct MemoryStore {
    core: Mutex<EngineCore>,
    subscribers: Mutex<HashMap<String, Vec<UnboundedSender<Value>>>>,
}

impl Default for MemoryStore {
    fn default() -> Self {
        Self { core: Mutex::new(EngineCore::new().with_router(pool_for)), subscribers: Mutex::new(HashMap::new()) }
    }
}

impl MemoryStore {
    pub fn new() -> Self {
        Self::default()
    }

    fn notify(&self, topic: &str, msg: Value) {
        let mut subs = self.subscribers.lock().unwrap();
        if let Some(list) = subs.get_mut(topic) {
            list.retain(|tx| tx.unbounded_send(msg.clone()).is_ok());
        }
    }

    /// Run `f` on the core, then deliver the notices it left.
    fn op<T>(&self, f: impl FnOnce(&mut EngineCore) -> T) -> T {
        let (out, fx) = {
            let mut core = self.core.lock().unwrap();
            let out = f(&mut core);
            (out, core.take_effects())
        };
        for (topic, msg) in fx.notices {
            self.notify(&topic, msg);
        }
        out
    }

    fn read<T>(&self, f: impl FnOnce(&EngineCore) -> T) -> T {
        f(&self.core.lock().unwrap())
    }
}

#[async_trait]
impl TaskQueue for MemoryStore {
    async fn enqueue(&self, t: NewTask) -> StoreResult<EnqueueOutcome> {
        self.op(|c| c.enqueue(t))
    }
    async fn heartbeat(&self, leases: &[Lease], extend: Duration) -> StoreResult<Vec<LeaseStatus>> {
        self.op(|c| c.heartbeat(Utc::now(), leases, extend))
    }
    async fn ack(&self, lease: &Lease) -> StoreResult<()> {
        self.op(|c| c.ack(lease))
    }
    async fn nack(&self, lease: &Lease, retry_at: Option<DateTime<Utc>>) -> StoreResult<()> {
        self.op(|c| c.nack(Utc::now(), lease, retry_at))
    }
    async fn pending(&self, pool: &str) -> StoreResult<usize> {
        Ok(self.read(|c| c.pending(pool)))
    }
    async fn live_pools(&self, within: Duration) -> StoreResult<Vec<String>> {
        Ok(self.read(|c| c.live_pools(Utc::now(), within)))
    }
}

#[async_trait]
impl RunStore for MemoryStore {
    async fn start(&self, run: NewRun, task: NewTask) -> StoreResult<(RunId, EnqueueOutcome)> {
        self.op(|c| c.start(Utc::now(), run, task))
    }
    async fn claim_runs(&self, pool: &str, worker: &str, n: usize, lease: Duration) -> StoreResult<Vec<(Lease, Task, Epoch)>> {
        self.op(|c| c.claim_runs(Utc::now(), pool, worker, n, lease))
    }
    async fn renew(&self, run: RunId, epoch: Epoch, lease: Duration) -> StoreResult<bool> {
        self.op(|c| c.renew(Utc::now(), run, epoch, lease))
    }
    async fn release(&self, run: RunId, epoch: Epoch, phase: RunPhase, outcome: RunOutcome, lease: &Lease) -> StoreResult<()> {
        self.op(|c| c.release(Utc::now(), run, epoch, phase, outcome, lease))
    }
    async fn request(&self, run: RunId, req: RunRequest) -> StoreResult<()> {
        self.op(|c| c.request(Utc::now(), run, req))
    }
    async fn get(&self, run: RunId) -> StoreResult<Option<RunRow>> {
        Ok(self.read(|c| c.get(run)))
    }
    async fn list(&self, q: RunQuery) -> StoreResult<Page<RunRow>> {
        Ok(self.read(|c| c.list(&q)))
    }
    async fn orphaned(&self) -> StoreResult<Vec<RunRow>> {
        Ok(self.read(|c| c.orphaned(Utc::now())))
    }
    async fn purge_finished(&self, before: DateTime<Utc>) -> StoreResult<usize> {
        self.op(|c| c.purge_finished(before))
    }
    async fn open_wait(&self, run: RunId, epoch: Epoch, wait_id: &str, correlation: Value) -> StoreResult<()> {
        self.op(|c| c.open_wait(Utc::now(), run, epoch, wait_id, correlation))
    }
    async fn close_wait(&self, run: RunId, epoch: Epoch, wait_id: &str) -> StoreResult<bool> {
        self.op(|c| c.close_wait(run, epoch, wait_id))
    }
    async fn deliver_event(&self, namespace: &str, event: Value) -> StoreResult<Vec<RunId>> {
        self.op(|c| c.deliver_event(Utc::now(), namespace, event))
    }
    async fn open_approval(&self, run: RunId, epoch: Epoch, wait_id: &str, request: Value) -> StoreResult<()> {
        self.op(|c| c.open_approval(Utc::now(), run, epoch, wait_id, request))
    }
    async fn approvals(&self) -> StoreResult<Vec<(RunId, String, Value)>> {
        Ok(self.read(|c| c.approvals()))
    }
    async fn decide_approval(&self, wait_id: &str, decision: Value, by: &str) -> StoreResult<bool> {
        self.op(|c| c.decide_approval(Utc::now(), wait_id, decision, by))
    }
}

/// A page cursor: the last row's creation time and id.
pub fn cursor_of(r: &RunRow) -> String {
    format!("{}_{}", r.created_at.timestamp_nanos_opt().unwrap_or(0), r.run_id)
}

pub fn parse_cursor(c: &str) -> Option<(DateTime<Utc>, RunId)> {
    let (t, id) = c.split_once('_')?;
    Some((DateTime::from_timestamp_nanos(t.parse().ok()?), id.parse().ok()?))
}

#[async_trait]
impl Journal for MemoryStore {
    async fn append(&self, run: RunId, epoch: Epoch, records: &[Record]) -> StoreResult<AppendOutcome> {
        self.op(|c| c.append(Utc::now(), run, epoch, records))
    }
    async fn read_from(&self, run: RunId, after_seq: u64) -> StoreResult<Vec<JournalEntry>> {
        Ok(self.read(|c| c.read_from(run, after_seq)).unwrap_or_default())
    }
    async fn commit_step(&self, run: RunId, epoch: Epoch, commit: StepCommit) -> StoreResult<u64> {
        self.op(|c| c.commit_step(Utc::now(), run, epoch, commit))
    }
    async fn latest_checkpoint(&self, run: RunId) -> StoreResult<Option<(u64, Vec<u8>)>> {
        Ok(self.read(|c| c.latest_checkpoint(run)))
    }
    async fn checkpoints(&self, run: RunId) -> StoreResult<Vec<CheckpointMeta>> {
        Ok(self.read(|c| c.checkpoints(run)))
    }
    async fn outbox(&self, limit: usize) -> StoreResult<Vec<OutboxRow>> {
        Ok(self.read(|c| c.outbox(limit)))
    }
    async fn outbox_done(&self, up_to_id: u64) -> StoreResult<()> {
        self.op(|c| c.outbox_done(up_to_id))
    }
}

#[async_trait]
impl Notifier for MemoryStore {
    async fn publish(&self, topic: &str, msg: Value) -> StoreResult<()> {
        self.notify(topic, msg);
        Ok(())
    }

    fn subscribe(&self, topic: &str) -> BoxStream<'static, Value> {
        let (tx, rx) = unbounded();
        self.subscribers.lock().unwrap().entry(topic.to_string()).or_default().push(tx);
        rx.boxed()
    }
}

#[cfg(test)]
mod tests {
    use super::MemoryStore;

    #[tokio::test]
    async fn conformance() {
        crate::conformance::run_all(|| async { MemoryStore::new() }).await;
    }
}
