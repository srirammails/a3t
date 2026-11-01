//! In-memory engine store: the reference implementation of the engine
//! contracts, and today's behaviour (nothing survives the process). The SQLite
//! and Postgres stores must behave exactly like this one — the conformance
//! suite runs against all of them.

use std::collections::{HashMap, HashSet};
use std::sync::Mutex;
use std::time::Duration;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use futures::channel::mpsc::{unbounded, UnboundedSender};
use futures::stream::BoxStream;
use futures::StreamExt;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::engine::*;

struct TaskRow {
    task: Task,
    order: u64,
    dedupe_key: Option<String>,
    visible_at: DateTime<Utc>,
    lease_owner: Option<String>,
    lease_token: u64,
    lease_until: Option<DateTime<Utc>>,
}

#[derive(Default)]
struct Inner {
    tasks: HashMap<TaskId, TaskRow>,
    task_dedupe: HashMap<String, TaskId>,
    order: u64,
    runs: HashMap<RunId, RunRow>,
    run_dedupe: HashMap<String, (RunId, TaskId)>,
    owner_until: HashMap<RunId, DateTime<Utc>>,
    journal: HashMap<RunId, Vec<JournalEntry>>,
    identities: HashMap<RunId, HashSet<(String, &'static str, u32)>>,
    checkpoints: HashMap<RunId, Vec<(u64, Vec<u8>, DateTime<Utc>)>>,
    outbox: Vec<OutboxRow>,
    outbox_id: u64,
}

#[derive(Default)]
pub struct MemoryStore {
    inner: Mutex<Inner>,
    subscribers: Mutex<HashMap<String, Vec<UnboundedSender<Value>>>>,
}

fn span(d: Duration) -> chrono::Duration {
    chrono::Duration::from_std(d).unwrap_or(chrono::Duration::MAX)
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

    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().unwrap()
    }
}

impl Inner {
    fn enqueue(&mut self, t: NewTask) -> EnqueueOutcome {
        if let Some(key) = &t.dedupe_key {
            if let Some(id) = self.task_dedupe.get(key) {
                return EnqueueOutcome::Existing(*id);
            }
        }
        let task_id = Uuid::now_v7();
        self.order += 1;
        if let Some(key) = &t.dedupe_key {
            self.task_dedupe.insert(key.clone(), task_id);
        }
        self.tasks.insert(task_id, TaskRow {
            task: Task { task_id, pool: t.pool, kind: t.kind, run_id: t.run_id, namespace: t.namespace, payload: t.payload, attempts: 0 },
            order: self.order,
            dedupe_key: t.dedupe_key,
            visible_at: t.visible_at,
            lease_owner: None,
            lease_token: 0,
            lease_until: None,
        });
        EnqueueOutcome::Created(task_id)
    }

    fn remove_task(&mut self, id: TaskId) {
        if let Some(row) = self.tasks.remove(&id) {
            if let Some(key) = row.dedupe_key {
                self.task_dedupe.remove(&key);
            }
        }
    }

    fn remove_run_tasks(&mut self, run: RunId) {
        let ids: Vec<TaskId> = self.tasks.values().filter(|r| r.task.run_id == run && r.lease_owner.is_none()).map(|r| r.task.task_id).collect();
        for id in ids {
            self.remove_task(id);
        }
    }

    fn owned_now(&self, run: RunId, now: DateTime<Utc>) -> bool {
        self.runs.get(&run).is_some_and(|r| r.owner.is_some()) && self.owner_until.get(&run).is_some_and(|t| *t >= now)
    }

    fn check_lease(&self, lease: &Lease) -> StoreResult<()> {
        match self.tasks.get(&lease.task_id) {
            Some(row) if row.lease_token == lease.token && row.lease_owner.is_some() => Ok(()),
            _ => Err(StoreError::LeaseLost(lease.task_id)),
        }
    }

    fn check_epoch(&self, run: RunId, epoch: Epoch) -> StoreResult<()> {
        match self.runs.get(&run) {
            None => Err(StoreError::NotFound(format!("run {run}"))),
            Some(r) if r.epoch != epoch => Err(StoreError::Fenced(run)),
            Some(_) => Ok(()),
        }
    }

    /// Append without the epoch check (callers check it, or are the store
    /// itself recording an outside request).
    fn push(&mut self, run: RunId, epoch: Epoch, records: &[Record], now: DateTime<Utc>) -> AppendOutcome {
        let mut duplicates = 0;
        let row = self.runs.get_mut(&run).expect("checked");
        let entries = self.journal.entry(run).or_default();
        let ids = self.identities.entry(run).or_default();
        for rec in records {
            if let Some((k, kind, attempt)) = rec.identity() {
                if !ids.insert((k.to_string(), kind, attempt)) {
                    duplicates += 1;
                    continue;
                }
            }
            row.last_seq += 1;
            if let Some(k) = rec.step_key() {
                row.current_step = Some(k.to_string());
            }
            entries.push(JournalEntry { seq: row.last_seq, epoch, at: now, record: rec.clone() });
        }
        row.updated_at = now;
        AppendOutcome { last_seq: row.last_seq, duplicates }
    }
}

#[async_trait]
impl TaskQueue for MemoryStore {
    async fn enqueue(&self, t: NewTask) -> StoreResult<EnqueueOutcome> {
        let pool = t.pool.clone();
        let out = self.lock().enqueue(t);
        if matches!(out, EnqueueOutcome::Created(_)) {
            self.notify(&tasks_topic(&pool), json!({ "enqueued": out.task_id() }));
        }
        Ok(out)
    }

    async fn heartbeat(&self, leases: &[Lease], extend: Duration) -> StoreResult<Vec<LeaseStatus>> {
        let now = Utc::now();
        let mut inner = self.lock();
        Ok(leases
            .iter()
            .map(|l| match inner.tasks.get_mut(&l.task_id) {
                Some(row) if row.lease_token == l.token && row.lease_owner.is_some() => {
                    row.lease_until = Some(now + span(extend));
                    LeaseStatus::Renewed
                }
                _ => LeaseStatus::Lost,
            })
            .collect())
    }

    async fn ack(&self, lease: &Lease) -> StoreResult<()> {
        let mut inner = self.lock();
        inner.check_lease(lease)?;
        inner.remove_task(lease.task_id);
        Ok(())
    }

    async fn nack(&self, lease: &Lease, retry_at: Option<DateTime<Utc>>) -> StoreResult<()> {
        let pool = {
            let mut inner = self.lock();
            inner.check_lease(lease)?;
            let row = inner.tasks.get_mut(&lease.task_id).expect("checked");
            row.lease_owner = None;
            row.lease_until = None;
            row.visible_at = retry_at.unwrap_or_else(Utc::now);
            row.task.pool.clone()
        };
        self.notify(&tasks_topic(&pool), json!({ "nacked": lease.task_id }));
        Ok(())
    }

    async fn pending(&self, pool: &str) -> StoreResult<usize> {
        Ok(self.lock().tasks.values().filter(|r| r.task.pool == pool).count())
    }
}

#[async_trait]
impl RunStore for MemoryStore {
    async fn start(&self, run: NewRun, task: NewTask) -> StoreResult<(RunId, EnqueueOutcome)> {
        let pool = task.pool.clone();
        let out = {
            let mut inner = self.lock();
            if let Some(key) = &run.dedupe_key {
                if let Some((id, task_id)) = inner.run_dedupe.get(key) {
                    return Ok((*id, EnqueueOutcome::Existing(*task_id)));
                }
            }
            if inner.runs.contains_key(&run.run_id) {
                return Err(StoreError::Conflict(format!("run {} exists", run.run_id)));
            }
            let now = Utc::now();
            let run_id = run.run_id;
            inner.runs.insert(run_id, RunRow {
                run_id,
                namespace: run.namespace,
                flow: run.flow,
                version: run.version,
                def_hash: run.def_hash,
                parent_run_id: run.parent_run_id,
                durability: run.durability,
                debug: run.debug,
                breakpoints: run.breakpoints,
                principal: run.principal,
                trigger: run.trigger,
                input: run.input,
                output: None,
                error: None,
                phase: RunPhase::Pending,
                current_step: None,
                owner: None,
                epoch: 0,
                cancel_requested: false,
                suspend_requested: false,
                resume_count: 0,
                last_seq: 0,
                deadline_at: run.deadline_at,
                created_at: now,
                started_at: None,
                completed_at: None,
                updated_at: now,
            });
            let out = inner.enqueue(task);
            if let Some(key) = run.dedupe_key {
                inner.run_dedupe.insert(key, (run_id, out.task_id()));
            }
            (run_id, out)
        };
        self.notify(&tasks_topic(&pool), json!({ "enqueued": out.1.task_id() }));
        Ok(out)
    }

    async fn claim_runs(&self, pool: &str, worker: &str, n: usize, lease: Duration)
        -> StoreResult<Vec<(Lease, Task, Epoch)>> {
        let now = Utc::now();
        let until = now + span(lease);
        let mut inner = self.lock();
        let mut candidates: Vec<(DateTime<Utc>, u64, TaskId)> = inner
            .tasks
            .values()
            .filter(|r| r.task.pool == pool && r.visible_at <= now && r.lease_until.is_none_or(|t| t < now))
            .map(|r| (r.visible_at, r.order, r.task.task_id))
            .collect();
        candidates.sort();
        let mut out = Vec::new();
        for (_, _, id) in candidates {
            if out.len() >= n {
                break;
            }
            let run_id = inner.tasks[&id].task.run_id;
            let phase = inner.runs.get(&run_id).map(|r| r.phase);
            match phase {
                // A task for a run that is gone or finished has nothing to do.
                None => { inner.remove_task(id); continue; }
                Some(p) if p.is_terminal() => { inner.remove_task(id); continue; }
                _ => {}
            }
            if inner.owned_now(run_id, now) {
                // Owned elsewhere: try again shortly rather than spin on it.
                inner.tasks.get_mut(&id).unwrap().visible_at = now + chrono::Duration::seconds(1);
                continue;
            }
            let row = inner.tasks.get_mut(&id).unwrap();
            row.lease_owner = Some(worker.to_string());
            row.lease_token += 1;
            row.lease_until = Some(until);
            row.task.attempts += 1;
            let lease = Lease { task_id: id, token: row.lease_token, expires_at: until };
            let task = row.task.clone();
            let run = inner.runs.get_mut(&run_id).unwrap();
            run.owner = Some(worker.to_string());
            run.epoch += 1;
            run.phase = RunPhase::Running;
            run.started_at.get_or_insert(now);
            run.updated_at = now;
            if task.kind == TaskKind::ResumeRun {
                run.resume_count += 1;
            }
            let epoch = run.epoch;
            inner.owner_until.insert(run_id, until);
            out.push((lease, task, epoch));
        }
        Ok(out)
    }

    async fn renew(&self, run: RunId, epoch: Epoch, lease: Duration) -> StoreResult<bool> {
        let mut inner = self.lock();
        let ok = inner.runs.get(&run).is_some_and(|r| r.epoch == epoch && r.owner.is_some());
        if ok {
            inner.owner_until.insert(run, Utc::now() + span(lease));
        }
        Ok(ok)
    }

    async fn release(&self, run: RunId, epoch: Epoch, phase: RunPhase, outcome: RunOutcome, lease: &Lease)
        -> StoreResult<()> {
        {
            let now = Utc::now();
            let mut inner = self.lock();
            inner.check_epoch(run, epoch)?;
            inner.check_lease(lease)?;
            inner.remove_task(lease.task_id);
            inner.owner_until.remove(&run);
            let row = inner.runs.get_mut(&run).unwrap();
            row.owner = None;
            row.phase = phase;
            if outcome.output.is_some() {
                row.output = outcome.output;
            }
            if outcome.error.is_some() {
                row.error = outcome.error;
            }
            if phase == RunPhase::Suspended {
                row.suspend_requested = false;
            }
            if phase.is_terminal() {
                row.completed_at = Some(now);
            }
            row.updated_at = now;
        }
        self.notify(&run_topic(run), json!({ "phase": phase }));
        Ok(())
    }

    async fn request(&self, run: RunId, req: RunRequest) -> StoreResult<()> {
        let now = Utc::now();
        let mut wake: Option<String> = None;
        {
            let mut inner = self.lock();
            let row = inner.runs.get(&run).ok_or_else(|| StoreError::NotFound(format!("run {run}")))?;
            if row.phase.is_terminal() {
                return Ok(());
            }
            let owned = inner.owned_now(run, now);
            let (epoch, ns, phase, debug) = (row.epoch, row.namespace.clone(), row.phase, row.debug);
            match req {
                RunRequest::Cancel { by, reason } => {
                    inner.push(run, epoch, &[Record::CancelRequested { by, reason: reason.clone() }], now);
                    let row = inner.runs.get_mut(&run).unwrap();
                    row.cancel_requested = true;
                    if !owned {
                        // Nobody is running it: it ends here.
                        row.phase = RunPhase::Cancelled;
                        row.error = Some(reason.clone());
                        row.completed_at = Some(now);
                        row.owner = None;
                        inner.push(run, epoch, &[Record::RunCancelled { reason }], now);
                        inner.remove_run_tasks(run);
                    }
                }
                RunRequest::Suspend { .. } => {
                    let row = inner.runs.get_mut(&run).unwrap();
                    if owned {
                        row.suspend_requested = true;
                    } else {
                        row.phase = RunPhase::Suspended;
                        inner.push(run, epoch, &[Record::Suspended], now);
                        inner.remove_run_tasks(run);
                    }
                }
                RunRequest::Resume { by } => {
                    if phase != RunPhase::Suspended {
                        return Err(StoreError::Conflict(format!("run {run} is not suspended")));
                    }
                    inner.push(run, epoch, &[Record::Resumed { by }], now);
                    let row = inner.runs.get_mut(&run).unwrap();
                    row.phase = RunPhase::Pending;
                    let seq = row.last_seq;
                    inner.enqueue(NewTask::resume(run, &ns, Some(format!("{run}:resume:{seq}")), now));
                    wake = Some(DEFAULT_POOL.to_string());
                }
                RunRequest::Debug { command, by } => {
                    if !debug || phase != RunPhase::Suspended {
                        return Err(StoreError::Conflict(format!("run {run} is not a paused debug run")));
                    }
                    inner.push(run, epoch, &[Record::DebugCommand { command, by }], now);
                    let row = inner.runs.get_mut(&run).unwrap();
                    row.phase = RunPhase::Pending;
                    let seq = row.last_seq;
                    inner.enqueue(NewTask::resume(run, &ns, Some(format!("{run}:debug:{seq}")), now));
                    wake = Some(DEFAULT_POOL.to_string());
                }
                RunRequest::Breakpoints(bps) => {
                    if !debug {
                        return Err(StoreError::Conflict(format!("run {run} is not a debug run")));
                    }
                    inner.runs.get_mut(&run).unwrap().breakpoints = Some(bps);
                }
            }
            inner.runs.get_mut(&run).unwrap().updated_at = now;
        }
        if let Some(pool) = wake {
            self.notify(&tasks_topic(&pool), json!({ "resume": run }));
        }
        self.notify(&run_topic(run), json!({ "request": true }));
        Ok(())
    }

    async fn get(&self, run: RunId) -> StoreResult<Option<RunRow>> {
        Ok(self.lock().runs.get(&run).cloned())
    }

    async fn list(&self, q: RunQuery) -> StoreResult<Page<RunRow>> {
        let limit = if q.limit == 0 { 50 } else { q.limit };
        let after = q.cursor.as_deref().and_then(parse_cursor);
        let inner = self.lock();
        let mut rows: Vec<&RunRow> = inner
            .runs
            .values()
            .filter(|r| q.namespaces.as_ref().is_none_or(|ns| ns.contains(&r.namespace)))
            .filter(|r| q.phases.is_empty() || q.phases.contains(&r.phase))
            .filter(|r| q.flow.as_ref().is_none_or(|f| *f == r.flow))
            .filter(|r| q.trigger.as_ref().is_none_or(|t| *t == r.trigger))
            .filter(|r| q.parent.is_none_or(|p| r.parent_run_id == Some(p)))
            .filter(|r| q.from.is_none_or(|t| r.created_at >= t))
            .filter(|r| q.to.is_none_or(|t| r.created_at < t))
            .filter(|r| after.is_none_or(|c| (r.created_at, r.run_id) < c))
            .collect();
        rows.sort_by(|a, b| (b.created_at, b.run_id).cmp(&(a.created_at, a.run_id)));
        let more = rows.len() > limit;
        rows.truncate(limit);
        let next = if more { rows.last().map(|r| cursor_of(r)) } else { None };
        Ok(Page { items: rows.into_iter().cloned().collect(), next })
    }

    async fn purge_finished(&self, before: DateTime<Utc>) -> StoreResult<usize> {
        let mut inner = self.lock();
        let gone: Vec<RunId> = inner
            .runs
            .values()
            .filter(|r| r.phase.is_terminal() && r.completed_at.is_some_and(|c| c < before))
            .map(|r| r.run_id)
            .collect();
        for id in &gone {
            inner.runs.remove(id);
            inner.journal.remove(id);
            inner.identities.remove(id);
            inner.checkpoints.remove(id);
            inner.owner_until.remove(id);
        }
        inner.run_dedupe.retain(|_, (r, _)| !gone.contains(r));
        Ok(gone.len())
    }

    async fn orphaned(&self) -> StoreResult<Vec<RunRow>> {
        let now = Utc::now();
        let inner = self.lock();
        Ok(inner
            .runs
            .values()
            .filter(|r| !r.phase.is_terminal() && r.owner.is_some() && !inner.owned_now(r.run_id, now))
            .cloned()
            .collect())
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
        let out = {
            let mut inner = self.lock();
            inner.check_epoch(run, epoch)?;
            inner.push(run, epoch, records, Utc::now())
        };
        self.notify(&run_topic(run), json!({ "seq": out.last_seq }));
        Ok(out)
    }

    async fn read_from(&self, run: RunId, after_seq: u64) -> StoreResult<Vec<JournalEntry>> {
        Ok(self
            .lock()
            .journal
            .get(&run)
            .map(|e| e.iter().filter(|x| x.seq > after_seq).cloned().collect())
            .unwrap_or_default())
    }

    async fn commit_step(&self, run: RunId, epoch: Epoch, c: StepCommit) -> StoreResult<u64> {
        let seq = {
            let now = Utc::now();
            let mut inner = self.lock();
            inner.check_epoch(run, epoch)?;
            let seq = inner.push(run, epoch, &c.records, now).last_seq;
            if let Some(snap) = c.checkpoint {
                inner.checkpoints.entry(run).or_default().push((seq, snap, now));
            }
            for (topic, body) in c.outbox {
                inner.outbox_id += 1;
                let id = inner.outbox_id;
                inner.outbox.push(OutboxRow { id, topic, body });
            }
            seq
        };
        self.notify(&run_topic(run), json!({ "seq": seq }));
        Ok(seq)
    }

    async fn latest_checkpoint(&self, run: RunId) -> StoreResult<Option<(u64, Vec<u8>)>> {
        Ok(self.lock().checkpoints.get(&run).and_then(|c| c.last()).map(|(s, b, _)| (*s, b.clone())))
    }

    async fn checkpoints(&self, run: RunId) -> StoreResult<Vec<CheckpointMeta>> {
        Ok(self
            .lock()
            .checkpoints
            .get(&run)
            .map(|c| c.iter().map(|(seq, b, at)| CheckpointMeta { seq: *seq, size: b.len(), at: *at }).collect())
            .unwrap_or_default())
    }

    async fn outbox(&self, limit: usize) -> StoreResult<Vec<OutboxRow>> {
        Ok(self.lock().outbox.iter().take(limit).cloned().collect())
    }

    async fn outbox_done(&self, up_to_id: u64) -> StoreResult<()> {
        self.lock().outbox.retain(|r| r.id > up_to_id);
        Ok(())
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
