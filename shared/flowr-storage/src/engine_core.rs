//! The engine's rules, once (ADB `.ai/H_ENGINE_STORE.md` H2): tasks, runs, the
//! journal, waits, events seen, checkpoints and the outbox, with no I/O and no
//! clock of its own (`now` is passed in).
//!
//! Every operation validates first, then **emits changes**: each change is
//! applied at once, and the list is what a host makes durable — ADB logs it,
//! replays it after a restart, and ships it to followers. [`EngineCore::apply`]
//! is the only code that mutates state, so applying an operation's changes to
//! a copy of the state gives the same state, by construction. A refused
//! operation emits nothing.
//!
//! Lease extensions (`heartbeat`, `renew`) are changes that are **not durable**
//! ([`Change::durable`]): a host doesn't log them; after a restart it gives every
//! lease a fresh period ([`EngineCore::reset_leases`]). The epoch check keeps
//! that safe.
//!
//! [`crate::engine_memory::MemoryStore`] is this core behind a mutex.

use std::collections::{HashMap, HashSet};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use uuid::Uuid;

use crate::engine::*;

/// A task as the core keeps it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TaskRow {
    pub task: Task,
    pub order: u64,
    pub dedupe_key: Option<String>,
    pub visible_at: DateTime<Utc>,
    pub lease_owner: Option<String>,
    pub lease_token: u64,
    pub lease_until: Option<DateTime<Utc>>,
}

/// A run as the core keeps it: the row and what only the store needs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RunState {
    pub row: RunRow,
    pub owner_until: Option<DateTime<Utc>>,
    /// The pool its last task went to (resumes go there without a router).
    pub pool: String,
    pub dedupe_key: Option<String>,
    pub start_task: TaskId,
}

/// An open wait: a `listen` (`event`) or an approval (`approval`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WaitRow {
    pub kind: String,
    pub wait_id: String,
    pub run: RunId,
    pub namespace: String,
    pub correlation: Value,
    pub received: Vec<Value>,
}

/// One state change. `None` rows delete.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "change", rename_all = "snake_case")]
pub enum Change {
    Task { id: TaskId, row: Option<TaskRow> },
    /// A run and, when deleted, its journal, checkpoints and waits.
    Run { id: RunId, state: Option<RunState> },
    /// Entries appended to a run's journal (sequence numbers already assigned).
    Journal { run: RunId, entries: Vec<JournalEntry> },
    /// A run's journal leaves memory (its history is kept by the host).
    JournalEvicted { run: RunId },
    Wait { run: RunId, wait_id: String, row: Option<WaitRow> },
    EventSeen { key: String, at: DateTime<Utc> },
    EventsPruned { before: DateTime<Utc> },
    Checkpoint { run: RunId, seq: u64, snapshot: Vec<u8>, at: DateTime<Utc> },
    Outbox { row: OutboxRow },
    OutboxDone { up_to: u64 },
    /// Not durable: a task lease extended.
    TaskLease { id: TaskId, until: DateTime<Utc> },
    /// Not durable: a run's ownership extended.
    RunOwner { id: RunId, until: DateTime<Utc> },
}

impl Change {
    /// Whether a host must make this change durable before answering.
    pub fn durable(&self) -> bool {
        !matches!(self, Self::TaskLease { .. } | Self::RunOwner { .. })
    }
}

/// What an operation did besides its answer.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Effects {
    pub changes: Vec<Change>,
    /// (topic, message) for the notifier — hints, never durable.
    pub notices: Vec<(String, Value)>,
}

/// Everything the engine holds. `Serialize` is the host's snapshot.
#[derive(Clone, Default, Serialize, Deserialize)]
pub struct EngineCore {
    tasks: HashMap<TaskId, TaskRow>,
    task_dedupe: HashMap<String, TaskId>,
    order: u64,
    runs: HashMap<RunId, RunState>,
    run_dedupe: HashMap<String, RunId>,
    journal: HashMap<RunId, Vec<JournalEntry>>,
    identities: HashMap<RunId, HashSet<(String, String, u32)>>,
    evicted: HashSet<RunId>,
    checkpoints: HashMap<RunId, Vec<(u64, Vec<u8>, DateTime<Utc>)>>,
    outbox: Vec<OutboxRow>,
    outbox_id: u64,
    /// Open waits, oldest first.
    waits: Vec<WaitRow>,
    /// `namespace \n id` → when first delivered.
    events_seen: HashMap<String, DateTime<Utc>>,
    /// Last claim per pool (not durable: a pool is live if it claimed lately).
    #[serde(skip)]
    live: HashMap<String, DateTime<Utc>>,
    #[serde(skip)]
    router: Option<fn(&str, &str) -> String>,
    #[serde(skip)]
    fx: Effects,
}

fn span(d: std::time::Duration) -> chrono::Duration {
    chrono::Duration::from_std(d).unwrap_or(chrono::Duration::MAX)
}

fn seen_key(namespace: &str, id: &str) -> String {
    format!("{namespace}\n{id}")
}

impl EngineCore {
    pub fn new() -> Self {
        Self::default()
    }

    /// Pick pools for resumes and wake-ups with this router (FlowR's
    /// `pool_for`); without one, a run's tasks go to the pool its last task used.
    pub fn with_router(mut self, router: fn(&str, &str) -> String) -> Self {
        self.router = Some(router);
        self
    }

    pub fn set_router(&mut self, router: Option<fn(&str, &str) -> String>) {
        self.router = router;
    }

    /// The changes and notices of the operations since the last call.
    pub fn take_effects(&mut self) -> Effects {
        std::mem::take(&mut self.fx)
    }

    /// After a restart: every lease and ownership gets one fresh period from `now`.
    pub fn reset_leases(&mut self, now: DateTime<Utc>, lease: std::time::Duration) {
        let until = now + span(lease);
        for t in self.tasks.values_mut() {
            if t.lease_owner.is_some() {
                t.lease_until = Some(until);
            }
        }
        for r in self.runs.values_mut() {
            if r.row.owner.is_some() {
                r.owner_until = Some(until);
            }
        }
    }

    // ── Applying changes (the only mutation) ──

    pub fn apply(&mut self, change: &Change) {
        match change {
            Change::Task { id, row } => {
                if let Some(old) = self.tasks.remove(id) {
                    if let Some(k) = old.dedupe_key {
                        self.task_dedupe.remove(&k);
                    }
                }
                if let Some(row) = row {
                    self.order = self.order.max(row.order);
                    if let Some(k) = &row.dedupe_key {
                        self.task_dedupe.insert(k.clone(), *id);
                    }
                    self.tasks.insert(*id, row.clone());
                }
            }
            Change::Run { id, state } => match state {
                Some(s) => {
                    if let Some(k) = &s.dedupe_key {
                        self.run_dedupe.insert(k.clone(), *id);
                    }
                    self.runs.insert(*id, s.clone());
                }
                None => {
                    if let Some(old) = self.runs.remove(id) {
                        if let Some(k) = old.dedupe_key {
                            self.run_dedupe.remove(&k);
                        }
                    }
                    self.journal.remove(id);
                    self.identities.remove(id);
                    self.evicted.remove(id);
                    self.checkpoints.remove(id);
                    self.waits.retain(|w| w.run != *id);
                }
            },
            Change::Journal { run, entries } => {
                let ids = self.identities.entry(*run).or_default();
                for e in entries {
                    if let Some((k, kind, a)) = e.record.identity() {
                        ids.insert((k.to_string(), kind.to_string(), a));
                    }
                }
                if let Some(s) = self.runs.get_mut(run) {
                    for e in entries {
                        s.row.last_seq = s.row.last_seq.max(e.seq);
                        if let Some(k) = e.record.step_key() {
                            s.row.current_step = Some(k.to_string());
                        }
                        s.row.updated_at = e.at;
                    }
                }
                if !self.evicted.contains(run) {
                    self.journal.entry(*run).or_default().extend(entries.iter().cloned());
                }
            }
            Change::JournalEvicted { run } => {
                self.journal.remove(run);
                self.evicted.insert(*run);
            }
            Change::Wait { run, wait_id, row } => {
                let at = self.waits.iter().position(|w| w.run == *run && w.wait_id == *wait_id);
                match (at, row) {
                    (Some(i), Some(r)) => self.waits[i] = r.clone(),
                    (None, Some(r)) => self.waits.push(r.clone()),
                    (Some(i), None) => {
                        self.waits.remove(i);
                    }
                    (None, None) => {}
                }
            }
            Change::EventSeen { key, at } => {
                self.events_seen.insert(key.clone(), *at);
            }
            Change::EventsPruned { before } => self.events_seen.retain(|_, at| *at >= *before),
            Change::Checkpoint { run, seq, snapshot, at } => {
                self.checkpoints.entry(*run).or_default().push((*seq, snapshot.clone(), *at));
            }
            Change::Outbox { row } => {
                self.outbox_id = self.outbox_id.max(row.id);
                self.outbox.push(row.clone());
            }
            Change::OutboxDone { up_to } => self.outbox.retain(|r| r.id > *up_to),
            Change::TaskLease { id, until } => {
                if let Some(t) = self.tasks.get_mut(id) {
                    t.lease_until = Some(*until);
                }
            }
            Change::RunOwner { id, until } => {
                if let Some(r) = self.runs.get_mut(id) {
                    r.owner_until = Some(*until);
                }
            }
        }
    }

    fn emit(&mut self, change: Change) {
        self.apply(&change);
        self.fx.changes.push(change);
    }

    fn notice(&mut self, topic: String, msg: Value) {
        self.fx.notices.push((topic, msg));
    }

    // ── Helpers ──

    fn put_task(&mut self, row: TaskRow) {
        self.emit(Change::Task { id: row.task.task_id, row: Some(row) });
    }

    fn del_task(&mut self, id: TaskId) {
        if self.tasks.contains_key(&id) {
            self.emit(Change::Task { id, row: None });
        }
    }

    fn put_run(&mut self, s: RunState) {
        self.emit(Change::Run { id: s.row.run_id, state: Some(s) });
    }

    fn run(&self, id: RunId) -> StoreResult<&RunState> {
        self.runs.get(&id).ok_or_else(|| StoreError::NotFound(format!("run {id}")))
    }

    fn pool_of(&self, s: &RunState) -> String {
        match self.router {
            Some(route) => route(&s.row.namespace, &s.row.flow),
            None => s.pool.clone(),
        }
    }

    fn new_task(&mut self, t: NewTask) -> EnqueueOutcome {
        if let Some(key) = &t.dedupe_key {
            if let Some(id) = self.task_dedupe.get(key) {
                return EnqueueOutcome::Existing(*id);
            }
        }
        let task_id = Uuid::now_v7();
        let order = self.order + 1;
        let pool = t.pool.clone();
        let run = t.run_id;
        self.put_task(TaskRow {
            task: Task { task_id, pool: t.pool, kind: t.kind, run_id: t.run_id, namespace: t.namespace, payload: t.payload, attempts: 0 },
            order,
            dedupe_key: t.dedupe_key,
            visible_at: t.visible_at,
            lease_owner: None,
            lease_token: 0,
            lease_until: None,
        });
        if let Some(mut s) = self.runs.get(&run).cloned() {
            if s.pool != pool {
                s.pool = pool;
                self.put_run(s);
            }
        }
        EnqueueOutcome::Created(task_id)
    }

    /// Enqueue a resume of `run` and notice the pool.
    fn wake(&mut self, run: RunId, dedupe: String, now: DateTime<Utc>) {
        let Some(s) = self.runs.get(&run).cloned() else { return };
        let pool = self.pool_of(&s);
        self.new_task(NewTask { pool: pool.clone(), ..NewTask::resume(run, &s.row.namespace, Some(dedupe), now) });
        self.notice(tasks_topic(&pool), json!({ "resume": run }));
    }

    fn owned_now(&self, run: RunId, now: DateTime<Utc>) -> bool {
        self.runs.get(&run).is_some_and(|s| s.row.owner.is_some() && s.owner_until.is_some_and(|t| t >= now))
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
            Some(s) if s.row.epoch != epoch => Err(StoreError::Fenced(run)),
            Some(_) => Ok(()),
        }
    }

    /// Append to a journal (epoch already checked, or an outside request).
    fn push(&mut self, run: RunId, epoch: Epoch, records: &[Record], now: DateTime<Utc>) -> AppendOutcome {
        let mut duplicates = 0;
        let mut seq = self.runs.get(&run).map(|s| s.row.last_seq).unwrap_or(0);
        let mut seen: HashSet<(String, String, u32)> = HashSet::new();
        let ids = self.identities.get(&run);
        let mut entries = Vec::new();
        for rec in records {
            if let Some((k, kind, a)) = rec.identity() {
                let id = (k.to_string(), kind.to_string(), a);
                if ids.is_some_and(|s| s.contains(&id)) || !seen.insert(id) {
                    duplicates += 1;
                    continue;
                }
            }
            seq += 1;
            entries.push(JournalEntry { seq, epoch, at: now, record: rec.clone() });
        }
        if !entries.is_empty() {
            self.emit(Change::Journal { run, entries });
        } else if let Some(mut s) = self.runs.get(&run).cloned() {
            s.row.updated_at = now;
            self.put_run(s);
        }
        AppendOutcome { last_seq: seq, duplicates }
    }

    fn set_phase(&mut self, run: RunId, f: impl FnOnce(&mut RunRow)) {
        if let Some(mut s) = self.runs.get(&run).cloned() {
            f(&mut s.row);
            self.put_run(s);
        }
    }

    fn drop_unleased_tasks(&mut self, run: RunId) {
        let ids: Vec<TaskId> = self.tasks.values().filter(|r| r.task.run_id == run && r.lease_owner.is_none()).map(|r| r.task.task_id).collect();
        for id in ids {
            self.del_task(id);
        }
    }

    fn drop_waits(&mut self, run: RunId) {
        let ids: Vec<String> = self.waits.iter().filter(|w| w.run == run).map(|w| w.wait_id.clone()).collect();
        for wait_id in ids {
            self.emit(Change::Wait { run, wait_id, row: None });
        }
    }

    // ── TaskQueue ──

    pub fn enqueue(&mut self, t: NewTask) -> StoreResult<EnqueueOutcome> {
        let pool = t.pool.clone();
        let out = self.new_task(t);
        if matches!(out, EnqueueOutcome::Created(_)) {
            self.notice(tasks_topic(&pool), json!({ "enqueued": out.task_id() }));
        }
        Ok(out)
    }

    pub fn heartbeat(&mut self, now: DateTime<Utc>, leases: &[Lease], extend: std::time::Duration) -> StoreResult<Vec<LeaseStatus>> {
        let until = now + span(extend);
        let mut out = Vec::with_capacity(leases.len());
        for l in leases {
            if self.check_lease(l).is_ok() {
                self.emit(Change::TaskLease { id: l.task_id, until });
                out.push(LeaseStatus::Renewed);
            } else {
                out.push(LeaseStatus::Lost);
            }
        }
        Ok(out)
    }

    pub fn ack(&mut self, lease: &Lease) -> StoreResult<()> {
        self.check_lease(lease)?;
        self.del_task(lease.task_id);
        Ok(())
    }

    pub fn nack(&mut self, now: DateTime<Utc>, lease: &Lease, retry_at: Option<DateTime<Utc>>) -> StoreResult<()> {
        self.check_lease(lease)?;
        let mut row = self.tasks[&lease.task_id].clone();
        row.lease_owner = None;
        row.lease_until = None;
        row.visible_at = retry_at.unwrap_or(now);
        let pool = row.task.pool.clone();
        self.put_task(row);
        self.notice(tasks_topic(&pool), json!({ "nacked": lease.task_id }));
        Ok(())
    }

    pub fn pending(&self, pool: &str) -> usize {
        self.tasks.values().filter(|r| r.task.pool == pool).count()
    }

    /// Pools that claimed within `within` of `now`, sorted.
    pub fn live_pools(&self, now: DateTime<Utc>, within: std::time::Duration) -> Vec<String> {
        let since = now - span(within);
        let mut v: Vec<String> = self.live.iter().filter(|(_, t)| **t >= since).map(|(p, _)| p.clone()).collect();
        v.sort();
        v
    }

    // ── RunStore ──

    pub fn start(&mut self, now: DateTime<Utc>, run: NewRun, task: NewTask) -> StoreResult<(RunId, EnqueueOutcome)> {
        if let Some(key) = &run.dedupe_key {
            if let Some(id) = self.run_dedupe.get(key) {
                let s = &self.runs[id];
                return Ok((*id, EnqueueOutcome::Existing(s.start_task)));
            }
        }
        if self.runs.contains_key(&run.run_id) {
            return Err(StoreError::Conflict(format!("run {} exists", run.run_id)));
        }
        let run_id = run.run_id;
        let pool = task.pool.clone();
        let dedupe_key = run.dedupe_key.clone();
        self.put_run(RunState {
            row: RunRow {
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
            },
            owner_until: None,
            pool: pool.clone(),
            dedupe_key: None,
            start_task: Uuid::nil(),
        });
        let out = self.new_task(task);
        let mut s = self.runs[&run_id].clone();
        s.dedupe_key = dedupe_key;
        s.start_task = out.task_id();
        self.put_run(s);
        self.notice(tasks_topic(&pool), json!({ "enqueued": out.task_id() }));
        Ok((run_id, out))
    }

    pub fn claim_runs(&mut self, now: DateTime<Utc>, pool: &str, worker: &str, n: usize, lease: std::time::Duration) -> StoreResult<Vec<(Lease, Task, Epoch)>> {
        self.live.insert(pool.to_string(), now);
        let until = now + span(lease);
        let mut candidates: Vec<(DateTime<Utc>, u64, TaskId)> = self
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
            let run_id = self.tasks[&id].task.run_id;
            match self.runs.get(&run_id).map(|s| s.row.phase) {
                // A task for a run that is gone or finished has nothing to do.
                None => {
                    self.del_task(id);
                    continue;
                }
                Some(p) if p.is_terminal() => {
                    self.del_task(id);
                    continue;
                }
                _ => {}
            }
            if self.owned_now(run_id, now) {
                // Owned elsewhere: try again shortly rather than spin on it.
                let mut row = self.tasks[&id].clone();
                row.visible_at = now + chrono::Duration::seconds(1);
                self.put_task(row);
                continue;
            }
            let mut row = self.tasks[&id].clone();
            row.lease_owner = Some(worker.to_string());
            row.lease_token += 1;
            row.lease_until = Some(until);
            row.task.attempts += 1;
            let lease = Lease { task_id: id, token: row.lease_token, expires_at: until };
            let task = row.task.clone();
            self.put_task(row);
            let mut s = self.runs[&run_id].clone();
            s.row.owner = Some(worker.to_string());
            s.row.epoch += 1;
            s.row.phase = RunPhase::Running;
            s.row.started_at.get_or_insert(now);
            s.row.updated_at = now;
            if task.kind == TaskKind::ResumeRun {
                s.row.resume_count += 1;
            }
            s.owner_until = Some(until);
            let epoch = s.row.epoch;
            self.put_run(s);
            out.push((lease, task, epoch));
        }
        Ok(out)
    }

    pub fn renew(&mut self, now: DateTime<Utc>, run: RunId, epoch: Epoch, lease: std::time::Duration) -> StoreResult<bool> {
        let ok = self.runs.get(&run).is_some_and(|s| s.row.epoch == epoch && s.row.owner.is_some());
        if ok {
            self.emit(Change::RunOwner { id: run, until: now + span(lease) });
        }
        Ok(ok)
    }

    pub fn release(&mut self, now: DateTime<Utc>, run: RunId, epoch: Epoch, phase: RunPhase, outcome: RunOutcome, lease: &Lease) -> StoreResult<()> {
        self.check_epoch(run, epoch)?;
        self.check_lease(lease)?;
        self.del_task(lease.task_id);
        // A cancel asked for while the run was still owned, and the owner lets go
        // to wait (a timer, an event, an answer, a pause): it ends here, not when
        // the wait would have woken it.
        if self.runs[&run].row.cancel_requested && !phase.is_terminal() {
            let reason = self.cancel_reason(run);
            self.push(run, epoch, &[Record::RunCancelled { reason: reason.clone() }], now);
            let mut s = self.runs[&run].clone();
            s.owner_until = None;
            s.row.owner = None;
            s.row.phase = RunPhase::Cancelled;
            s.row.error = Some(reason);
            s.row.completed_at = Some(now);
            s.row.updated_at = now;
            self.put_run(s);
            self.drop_unleased_tasks(run);
            self.drop_waits(run);
            self.notice(run_topic(run), json!({ "phase": RunPhase::Cancelled }));
            return Ok(());
        }
        let mut s = self.runs[&run].clone();
        s.owner_until = None;
        s.row.owner = None;
        s.row.phase = phase;
        if outcome.output.is_some() {
            s.row.output = outcome.output;
        }
        if outcome.error.is_some() {
            s.row.error = outcome.error;
        }
        if phase == RunPhase::Suspended {
            s.row.suspend_requested = false;
        }
        if phase.is_terminal() {
            s.row.completed_at = Some(now);
        }
        s.row.updated_at = now;
        self.put_run(s);
        self.notice(run_topic(run), json!({ "phase": phase }));
        Ok(())
    }

    /// Why a cancel was asked for (its journal record), for the run's error.
    fn cancel_reason(&self, run: RunId) -> String {
        self.journal
            .get(&run)
            .and_then(|j| {
                j.iter().rev().find_map(|e| match &e.record {
                    Record::CancelRequested { reason, .. } => Some(reason.clone()),
                    _ => None,
                })
            })
            .unwrap_or_else(|| "cancelled".into())
    }

    pub fn request(&mut self, now: DateTime<Utc>, run: RunId, req: RunRequest) -> StoreResult<()> {
        let s = self.run(run)?.clone();
        if s.row.phase.is_terminal() {
            return Ok(());
        }
        let owned = self.owned_now(run, now);
        let epoch = s.row.epoch;
        match req {
            RunRequest::Cancel { by, reason } => {
                self.push(run, epoch, &[Record::CancelRequested { by, reason: reason.clone() }], now);
                self.set_phase(run, |r| r.cancel_requested = true);
                if !owned {
                    // Nobody is running it: it ends here.
                    self.push(run, epoch, &[Record::RunCancelled { reason: reason.clone() }], now);
                    let mut s = self.runs[&run].clone();
                    s.row.phase = RunPhase::Cancelled;
                    s.row.error = Some(reason);
                    s.row.completed_at = Some(now);
                    s.row.owner = None;
                    s.owner_until = None;
                    self.put_run(s);
                    self.drop_unleased_tasks(run);
                    self.drop_waits(run);
                }
            }
            RunRequest::Suspend { .. } => {
                if owned {
                    self.set_phase(run, |r| r.suspend_requested = true);
                } else {
                    self.push(run, epoch, &[Record::Suspended], now);
                    self.set_phase(run, |r| r.phase = RunPhase::Suspended);
                    self.drop_unleased_tasks(run);
                }
            }
            RunRequest::Resume { by } => {
                if s.row.phase != RunPhase::Suspended {
                    return Err(StoreError::Conflict(format!("run {run} is not suspended")));
                }
                let seq = self.push(run, epoch, &[Record::Resumed { by }], now).last_seq;
                self.set_phase(run, |r| r.phase = RunPhase::Pending);
                self.wake(run, format!("{run}:resume:{seq}"), now);
            }
            RunRequest::Debug { command, by } => {
                if !s.row.debug || s.row.phase != RunPhase::Suspended {
                    return Err(StoreError::Conflict(format!("run {run} is not a paused debug run")));
                }
                let seq = self.push(run, epoch, &[Record::DebugCommand { command, by }], now).last_seq;
                self.set_phase(run, |r| r.phase = RunPhase::Pending);
                self.wake(run, format!("{run}:debug:{seq}"), now);
            }
            RunRequest::Breakpoints(bps) => {
                if !s.row.debug {
                    return Err(StoreError::Conflict(format!("run {run} is not a debug run")));
                }
                self.set_phase(run, |r| r.breakpoints = Some(bps));
            }
        }
        self.set_phase(run, |r| r.updated_at = now);
        self.notice(run_topic(run), json!({ "request": true }));
        Ok(())
    }

    pub fn get(&self, run: RunId) -> Option<RunRow> {
        self.runs.get(&run).map(|s| s.row.clone())
    }

    pub fn list(&self, q: &RunQuery) -> Page<RunRow> {
        let limit = if q.limit == 0 { 50 } else { q.limit };
        let after = q.cursor.as_deref().and_then(crate::engine_memory::parse_cursor);
        let mut rows: Vec<&RunRow> = self
            .runs
            .values()
            .map(|s| &s.row)
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
        let next = if more { rows.last().map(|r| crate::engine_memory::cursor_of(r)) } else { None };
        Page { items: rows.into_iter().cloned().collect(), next }
    }

    pub fn orphaned(&self, now: DateTime<Utc>) -> Vec<RunRow> {
        self.runs
            .values()
            .filter(|s| !s.row.phase.is_terminal() && s.row.owner.is_some() && !self.owned_now(s.row.run_id, now))
            .map(|s| s.row.clone())
            .collect()
    }

    pub fn purge_finished(&mut self, before: DateTime<Utc>) -> StoreResult<usize> {
        let gone: Vec<RunId> = self
            .runs
            .values()
            .filter(|s| s.row.phase.is_terminal() && s.row.completed_at.is_some_and(|c| c < before))
            .map(|s| s.row.run_id)
            .collect();
        for id in &gone {
            let tasks: Vec<TaskId> = self.tasks.values().filter(|t| t.task.run_id == *id).map(|t| t.task.task_id).collect();
            for t in tasks {
                self.del_task(t);
            }
            self.emit(Change::Run { id: *id, state: None });
        }
        if self.events_seen.values().any(|at| *at < before) {
            self.emit(Change::EventsPruned { before });
        }
        Ok(gone.len())
    }

    // ── Waits ──

    fn open(&mut self, now: DateTime<Utc>, run: RunId, epoch: Epoch, kind: &str, wait_id: &str, correlation: Value, record: Record) -> StoreResult<()> {
        self.check_epoch(run, epoch)?;
        if self.waits.iter().any(|w| w.run == run && w.wait_id == wait_id) {
            return Ok(());
        }
        let namespace = self.runs[&run].row.namespace.clone();
        self.push(run, epoch, &[record], now);
        self.emit(Change::Wait {
            run,
            wait_id: wait_id.to_string(),
            row: Some(WaitRow { kind: kind.into(), wait_id: wait_id.to_string(), run, namespace, correlation, received: Vec::new() }),
        });
        Ok(())
    }

    pub fn open_wait(&mut self, now: DateTime<Utc>, run: RunId, epoch: Epoch, wait_id: &str, correlation: Value) -> StoreResult<()> {
        let rec = Record::WaitForEvents { wait_id: wait_id.to_string(), correlation: correlation.clone() };
        self.open(now, run, epoch, "event", wait_id, correlation, rec)
    }

    pub fn open_approval(&mut self, now: DateTime<Utc>, run: RunId, epoch: Epoch, wait_id: &str, request: Value) -> StoreResult<()> {
        let rec = Record::ApprovalRequested { wait_id: wait_id.to_string(), request: request.clone() };
        self.open(now, run, epoch, "approval", wait_id, request, rec)
    }

    pub fn close_wait(&mut self, run: RunId, epoch: Epoch, wait_id: &str) -> StoreResult<bool> {
        self.check_epoch(run, epoch)?;
        if !self.waits.iter().any(|w| w.run == run && w.wait_id == wait_id) {
            return Ok(false);
        }
        self.emit(Change::Wait { run, wait_id: wait_id.to_string(), row: None });
        Ok(true)
    }

    pub fn approvals(&self) -> Vec<(RunId, String, Value)> {
        self.waits
            .iter()
            .filter(|w| w.kind == "approval" && self.runs.get(&w.run).is_some_and(|s| !s.row.phase.is_terminal()))
            .map(|w| (w.run, w.wait_id.clone(), w.correlation.clone()))
            .collect()
    }

    pub fn decide_approval(&mut self, now: DateTime<Utc>, wait_id: &str, decision: Value, by: &str) -> StoreResult<bool> {
        let Some(w) = self.waits.iter().find(|w| w.kind == "approval" && w.wait_id == wait_id).cloned() else {
            return Ok(false);
        };
        self.emit(Change::Wait { run: w.run, wait_id: w.wait_id.clone(), row: None });
        let Some(s) = self.runs.get(&w.run).cloned() else { return Ok(false) };
        if s.row.phase.is_terminal() {
            return Ok(false);
        }
        let run = w.run;
        self.push(run, s.row.epoch, &[Record::ApprovalDecided { wait_id: wait_id.to_string(), decision, by: by.to_string() }], now);
        if !self.owned_now(run, now) {
            self.set_phase(run, |r| r.phase = RunPhase::Pending);
        }
        self.wake(run, wait_wake_key(run, wait_id), now);
        self.notice(run_topic(run), json!({ "approval": true }));
        Ok(true)
    }

    pub fn deliver_event(&mut self, now: DateTime<Utc>, namespace: &str, event: Value) -> StoreResult<Vec<RunId>> {
        if let Some(id) = event.get("id").and_then(|i| i.as_str()) {
            let key = seen_key(namespace, id);
            if self.events_seen.contains_key(&key) {
                return Ok(Vec::new());
            }
            self.emit(Change::EventSeen { key, at: now });
        }
        let candidates: Vec<WaitRow> = self.waits.iter().filter(|w| w.namespace == namespace && w.kind == "event").cloned().collect();
        let mut woke = Vec::new();
        for mut w in candidates {
            let run = w.run;
            let Some(s) = self.runs.get(&run).cloned().filter(|s| !s.row.phase.is_terminal()) else {
                self.emit(Change::Wait { run, wait_id: w.wait_id.clone(), row: None });
                continue;
            };
            let (taken, done) = offer_event(&w.correlation, &mut w.received, &event);
            if !taken {
                continue;
            }
            if !done {
                self.emit(Change::Wait { run, wait_id: w.wait_id.clone(), row: Some(w) });
                continue;
            }
            self.emit(Change::Wait { run, wait_id: w.wait_id.clone(), row: None });
            self.push(run, s.row.epoch, &[Record::EventReceived { wait_id: w.wait_id.clone(), events: received_events(&w.received) }], now);
            if !self.owned_now(run, now) {
                self.set_phase(run, |r| r.phase = RunPhase::Pending);
            }
            self.wake(run, wait_wake_key(run, &w.wait_id), now);
            self.notice(run_topic(run), json!({ "event": true }));
            woke.push(run);
        }
        Ok(woke)
    }

    // ── Journal ──

    pub fn append(&mut self, now: DateTime<Utc>, run: RunId, epoch: Epoch, records: &[Record]) -> StoreResult<AppendOutcome> {
        self.check_epoch(run, epoch)?;
        let out = self.push(run, epoch, records, now);
        self.notice(run_topic(run), json!({ "seq": out.last_seq }));
        Ok(out)
    }

    /// `None` when the run's journal was evicted: the host reads its history.
    pub fn read_from(&self, run: RunId, after_seq: u64) -> Option<Vec<JournalEntry>> {
        if self.evicted.contains(&run) {
            return None;
        }
        Some(self.journal.get(&run).map(|e| e.iter().filter(|x| x.seq > after_seq).cloned().collect()).unwrap_or_default())
    }

    /// Let a run's journal leave memory (its history is the host's to keep).
    pub fn evict_journal(&mut self, run: RunId) {
        if self.runs.contains_key(&run) && !self.evicted.contains(&run) {
            self.emit(Change::JournalEvicted { run });
        }
    }

    pub fn commit_step(&mut self, now: DateTime<Utc>, run: RunId, epoch: Epoch, c: StepCommit) -> StoreResult<u64> {
        self.check_epoch(run, epoch)?;
        let seq = self.push(run, epoch, &c.records, now).last_seq;
        if let Some(snapshot) = c.checkpoint {
            self.emit(Change::Checkpoint { run, seq, snapshot, at: now });
        }
        for (topic, body) in c.outbox {
            let id = self.outbox_id + 1;
            self.emit(Change::Outbox { row: OutboxRow { id, topic, body } });
        }
        self.notice(run_topic(run), json!({ "seq": seq }));
        Ok(seq)
    }

    pub fn latest_checkpoint(&self, run: RunId) -> Option<(u64, Vec<u8>)> {
        self.checkpoints.get(&run).and_then(|c| c.last()).map(|(s, b, _)| (*s, b.clone()))
    }

    pub fn checkpoints(&self, run: RunId) -> Vec<CheckpointMeta> {
        self.checkpoints
            .get(&run)
            .map(|c| c.iter().map(|(seq, b, at)| CheckpointMeta { seq: *seq, size: b.len(), at: *at }).collect())
            .unwrap_or_default()
    }

    pub fn outbox(&self, limit: usize) -> Vec<OutboxRow> {
        self.outbox.iter().take(limit).cloned().collect()
    }

    pub fn outbox_done(&mut self, up_to_id: u64) -> StoreResult<()> {
        if self.outbox.iter().any(|r| r.id <= up_to_id) {
            self.emit(Change::OutboxDone { up_to: up_to_id });
        }
        Ok(())
    }

    /// Durable state as JSON, for comparing two cores (tests): leases and
    /// ownership times, which aren't durable, are left out.
    pub fn durable_view(&self) -> Value {
        let mut c = self.clone();
        for t in c.tasks.values_mut() {
            t.lease_until = None;
        }
        for r in c.runs.values_mut() {
            r.owner_until = None;
        }
        let mut v = serde_json::to_value(&c).unwrap_or(Value::Null);
        sort_sets(&mut v);
        v
    }
}

/// Sets and maps serialize in hash order: sort arrays of sets for comparison.
fn sort_sets(v: &mut Value) {
    match v {
        Value::Object(m) => m.values_mut().for_each(sort_sets),
        Value::Array(a) => {
            a.iter_mut().for_each(sort_sets);
            if a.iter().all(|x| x.is_array()) {
                a.sort_by_key(|x| x.to_string());
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    const LEASE: Duration = Duration::from_secs(30);

    fn new_run(ns: &str, flow: &str, debug: bool) -> NewRun {
        NewRun {
            run_id: Uuid::now_v7(),
            namespace: ns.into(),
            flow: flow.into(),
            version: "1".into(),
            def_hash: "h".into(),
            parent_run_id: None,
            durability: Durability::Durable,
            debug,
            breakpoints: None,
            principal: json!({ "id": "u" }),
            trigger: "api".into(),
            input: json!({ "x": 1 }),
            deadline_at: None,
            dedupe_key: None,
        }
    }

    fn step(key: &str, n: u64) -> Record {
        Record::StepCompleted {
            step_key: key.into(),
            attempt: 0,
            output: json!(n),
            context: Value::Null,
            class: StepClass::Pure,
            duration_us: 1,
            directive: String::new(),
            input: Value::Null,
            at: None,
        }
    }

    /// A small deterministic generator (no extra dependency).
    struct Lcg(u64);
    impl Lcg {
        fn next(&mut self, n: u64) -> u64 {
            self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            (self.0 >> 33) % n
        }
    }

    /// Many operations of every kind on one core, collecting what it emitted.
    fn exercise(core: &mut EngineCore, seed: u64) -> Vec<Change> {
        let mut rng = Lcg(seed);
        let mut log = Vec::new();
        let mut now = Utc::now();
        let mut runs: Vec<RunId> = Vec::new();
        let mut held: Vec<(Lease, RunId, Epoch)> = Vec::new();
        for i in 0..400u64 {
            now += chrono::Duration::milliseconds(50);
            match rng.next(14) {
                0 | 1 => {
                    let mut r = new_run(if rng.next(2) == 0 { "p" } else { "q" }, "f", rng.next(4) == 0);
                    if rng.next(5) == 0 {
                        r.dedupe_key = Some(format!("d{}", rng.next(3)));
                    }
                    let t = NewTask::start(&r);
                    if let Ok((id, _)) = core.start(now, r, t) {
                        if !runs.contains(&id) {
                            runs.push(id);
                        }
                    }
                }
                2 | 3 => {
                    for (lease, task, epoch) in core.claim_runs(now, DEFAULT_POOL, "w", 2, LEASE).unwrap() {
                        held.push((lease, task.run_id, epoch));
                    }
                }
                4 | 5 if !held.is_empty() => {
                    let (_, run, epoch) = held[rng.next(held.len() as u64) as usize].clone();
                    let c = StepCommit {
                        records: vec![step(&format!("/s{}", rng.next(6)), i)],
                        checkpoint: (rng.next(8) == 0).then(|| vec![1, 2, 3]),
                        outbox: if rng.next(6) == 0 { vec![("usage".into(), json!({ "i": i }))] } else { vec![] },
                    };
                    let _ = core.commit_step(now, run, epoch, c);
                }
                6 if !held.is_empty() => {
                    let (lease, run, epoch) = held.remove(rng.next(held.len() as u64) as usize);
                    let phase = [RunPhase::Completed, RunPhase::Waiting, RunPhase::Suspended, RunPhase::Faulted][rng.next(4) as usize];
                    let _ = core.release(now, run, epoch, phase, RunOutcome { output: Some(json!(i)), error: None }, &lease);
                }
                7 if !held.is_empty() => {
                    let (lease, run, epoch) = held[rng.next(held.len() as u64) as usize].clone();
                    let _ = core.heartbeat(now, &[lease], LEASE);
                    let _ = core.renew(now, run, epoch, LEASE);
                    if rng.next(2) == 0 {
                        let _ = core.open_wait(now, run, epoch, &format!("/w{}", rng.next(3)), json!({ "mode": "any", "filters": [{ "type": format!("t{}", rng.next(3)) }] }));
                    } else {
                        let _ = core.open_approval(now, run, epoch, &format!("a{i}"), json!({ "tool": "x" }));
                    }
                }
                8 => {
                    let ev = json!({ "id": format!("e{}", rng.next(20)), "type": format!("t{}", rng.next(3)) });
                    let _ = core.deliver_event(now, if rng.next(2) == 0 { "p" } else { "q" }, ev);
                }
                9 => {
                    if let Some((_, id, _)) = core.approvals().first().cloned() {
                        let _ = core.decide_approval(now, &id, json!({ "allow": rng.next(2) == 0 }), "u");
                    }
                }
                10 if !runs.is_empty() => {
                    let run = runs[rng.next(runs.len() as u64) as usize];
                    let req = match rng.next(5) {
                        0 => RunRequest::Cancel { by: "u".into(), reason: "r".into() },
                        1 => RunRequest::Suspend { by: "u".into() },
                        2 => RunRequest::Resume { by: "u".into() },
                        3 => RunRequest::Debug { command: json!("continue"), by: "u".into() },
                        _ => RunRequest::Breakpoints(json!([])),
                    };
                    let _ = core.request(now, run, req);
                }
                11 if !held.is_empty() => {
                    let (lease, _, _) = held.remove(rng.next(held.len() as u64) as usize);
                    let _ = if rng.next(2) == 0 { core.ack(&lease) } else { core.nack(now, &lease, None) };
                }
                12 => {
                    let _ = core.outbox_done(rng.next(5));
                    if rng.next(4) == 0 {
                        let _ = core.purge_finished(now - chrono::Duration::seconds(5));
                    }
                }
                _ => {
                    let _ = core.enqueue(NewTask { dedupe_key: Some(format!("k{}", rng.next(4))), ..NewTask::resume(Uuid::now_v7(), "p", None, now) });
                }
            }
            log.extend(core.take_effects().changes);
        }
        log
    }

    #[test]
    fn applying_the_durable_changes_rebuilds_the_same_state() {
        for seed in [1, 7, 42, 2026] {
            let mut leader = EngineCore::new();
            let log = exercise(&mut leader, seed);
            assert!(log.iter().any(|c| !c.durable()), "the run includes lease extensions");
            let mut follower = EngineCore::new();
            for c in log.iter().filter(|c| c.durable()) {
                follower.apply(c);
            }
            assert_eq!(follower.durable_view(), leader.durable_view(), "seed {seed}");
        }
    }

    #[test]
    fn a_snapshot_restores_the_state() {
        let mut core = EngineCore::new();
        exercise(&mut core, 99);
        let snap = serde_json::to_vec(&core).unwrap();
        let back: EngineCore = serde_json::from_slice(&snap).unwrap();
        assert_eq!(back.durable_view(), core.durable_view());
        // Changes still apply on the restored core.
        let mut a = core.clone();
        let mut b = back;
        let log = exercise(&mut a, 100);
        for c in log.iter().filter(|c| c.durable()) {
            b.apply(c);
        }
        assert_eq!(b.durable_view(), a.durable_view());
    }

    #[test]
    fn a_refused_operation_emits_nothing() {
        let mut core = EngineCore::new();
        let now = Utc::now() + chrono::Duration::seconds(1);
        let r = new_run("p", "f", false);
        let t = NewTask::start(&r);
        core.start(now, r, t).unwrap();
        let (_, task, epoch) = core.claim_runs(now, DEFAULT_POOL, "w", 1, LEASE).unwrap().remove(0);
        core.take_effects();
        assert!(matches!(core.append(now, task.run_id, epoch + 1, &[step("/a", 1)]), Err(StoreError::Fenced(_))));
        assert!(core.request(now, task.run_id, RunRequest::Resume { by: "u".into() }).is_err());
        assert_eq!(core.take_effects(), Effects::default());
    }

    #[test]
    fn leases_reset_after_a_restart_and_epochs_still_fence() {
        let mut core = EngineCore::new();
        let t0 = Utc::now() + chrono::Duration::seconds(1);
        let r = new_run("p", "f", false);
        let t = NewTask::start(&r);
        core.start(t0, r, t).unwrap();
        let (_, task, epoch) = core.claim_runs(t0, DEFAULT_POOL, "w", 1, Duration::from_millis(10)).unwrap().remove(0);
        // Restart much later: the dead worker's run is not up for grabs at once.
        let later = t0 + chrono::Duration::seconds(60);
        core.reset_leases(later, LEASE);
        assert!(core.orphaned(later).is_empty(), "owned for one more period");
        assert!(core.claim_runs(later, DEFAULT_POOL, "w2", 1, LEASE).unwrap().is_empty());
        // After it, the run is claimed again under a new epoch; the old one is fenced.
        let after = later + chrono::Duration::seconds(31);
        assert_eq!(core.orphaned(after).len(), 1);
        core.enqueue(NewTask::resume(task.run_id, "p", None, after)).unwrap();
        let (_, _, e2) = core.claim_runs(after, DEFAULT_POOL, "w2", 5, LEASE).unwrap().into_iter().find(|(_, t, _)| t.run_id == task.run_id).unwrap();
        assert!(e2 > epoch);
        assert!(matches!(core.append(after, task.run_id, epoch, &[step("/a", 1)]), Err(StoreError::Fenced(_))));
    }

    #[test]
    fn an_evicted_journal_is_read_by_the_host() {
        let mut core = EngineCore::new();
        let now = Utc::now() + chrono::Duration::seconds(1);
        let r = new_run("p", "f", false);
        let id = r.run_id;
        let t = NewTask::start(&r);
        core.start(now, r, t).unwrap();
        let (lease, _, epoch) = core.claim_runs(now, DEFAULT_POOL, "w", 1, LEASE).unwrap().remove(0);
        core.append(now, id, epoch, &[step("/a", 1), step("/b", 2)]).unwrap();
        core.release(now, id, epoch, RunPhase::Completed, RunOutcome::default(), &lease).unwrap();
        assert_eq!(core.read_from(id, 0).unwrap().len(), 2);
        core.evict_journal(id);
        assert_eq!(core.read_from(id, 0), None, "the host answers from its archive");
        assert_eq!(core.get(id).unwrap().last_seq, 2, "the run row stays");
        // A record appended after eviction still dedupes and numbers on.
        let mut follower = EngineCore::new();
        for c in core.take_effects().changes.iter().filter(|c| c.durable()) {
            follower.apply(c);
        }
        assert_eq!(follower.read_from(id, 0), None);
        assert_eq!(follower.durable_view(), core.durable_view());
    }
}
