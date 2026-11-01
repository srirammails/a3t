//! Durable execution contracts: the run queue, run ownership, the journal and
//! wake-up notifications (`.ai/FLOWR_QUEUE_CHECKPOINT_SPEC.md` §6).
//!
//! These replace [`ExecutionQueue`](crate::ExecutionQueue),
//! [`CheckpointStore`](crate::CheckpointStore) and
//! [`ExecutionStore`](crate::ExecutionStore) as the engine moves onto them
//! (plan stage 3). A backend implements all four traits over one store so the
//! operations that must be atomic — claim + ownership, release + ack, a step's
//! completion + its outbox rows — are single transactions.
//!
//! The rules every backend keeps (checked by [`crate::conformance`]):
//! - a run has at most one owner, carried by a monotonically increasing epoch;
//! - every journal write names the writer's epoch, and a stale epoch is refused
//!   with [`StoreError::Fenced`] — the old owner must stop;
//! - a journal record is unique on (step key, kind, attempt): re-appending it is
//!   a no-op, so a retried write never duplicates history;
//! - a queue task is held under a lease token; only the current holder may ack.

use std::time::Duration;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use futures::stream::BoxStream;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

pub type RunId = Uuid;
pub type TaskId = Uuid;
/// The fencing token: bumped each time a run changes owner.
pub type Epoch = u64;

#[derive(Debug, Clone, thiserror::Error, PartialEq)]
pub enum StoreError {
    /// The writer no longer owns the run. It must stop the run and perform no
    /// further side effects.
    #[error("fenced: run {0} has a newer owner")]
    Fenced(RunId),
    /// The queue task's lease was lost (expired and claimed by someone else).
    #[error("lease lost on task {0}")]
    LeaseLost(TaskId),
    #[error("not found: {0}")]
    NotFound(String),
    #[error("conflict: {0}")]
    Conflict(String),
    #[error("store: {0}")]
    Backend(String),
}

pub type StoreResult<T> = Result<T, StoreError>;

// ── Queue ──

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskKind {
    StartRun,
    ResumeRun,
    /// Phase 3: one step of a run, executed by another pool.
    RemoteStep,
}

#[derive(Debug, Clone)]
pub struct NewTask {
    pub pool: String,
    pub kind: TaskKind,
    pub run_id: RunId,
    pub namespace: String,
    pub payload: Value,
    /// A second enqueue with the same key returns the first task.
    pub dedupe_key: Option<String>,
    /// Not claimable before this time (timers, retry backoff).
    pub visible_at: DateTime<Utc>,
}

impl NewTask {
    pub fn start(run: &NewRun) -> Self {
        Self {
            pool: DEFAULT_POOL.to_string(),
            kind: TaskKind::StartRun,
            run_id: run.run_id,
            namespace: run.namespace.clone(),
            payload: Value::Null,
            dedupe_key: None,
            visible_at: Utc::now(),
        }
    }

    pub fn resume(run_id: RunId, namespace: &str, dedupe_key: Option<String>, visible_at: DateTime<Utc>) -> Self {
        Self {
            pool: DEFAULT_POOL.to_string(),
            kind: TaskKind::ResumeRun,
            run_id,
            namespace: namespace.to_string(),
            payload: Value::Null,
            dedupe_key,
            visible_at,
        }
    }
}

pub const DEFAULT_POOL: &str = "default";

#[derive(Debug, Clone, PartialEq)]
pub struct Task {
    pub task_id: TaskId,
    pub pool: String,
    pub kind: TaskKind,
    pub run_id: RunId,
    pub namespace: String,
    pub payload: Value,
    /// Times this task has been claimed, this claim included.
    pub attempts: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Lease {
    pub task_id: TaskId,
    pub token: u64,
    pub expires_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnqueueOutcome {
    Created(TaskId),
    /// The dedupe key (or run id, for [`RunStore::start`]) was already taken.
    Existing(TaskId),
}

impl EnqueueOutcome {
    pub fn task_id(&self) -> TaskId {
        match self {
            Self::Created(t) | Self::Existing(t) => *t,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LeaseStatus {
    Renewed,
    Lost,
}

#[async_trait]
pub trait TaskQueue: Send + Sync {
    async fn enqueue(&self, t: NewTask) -> StoreResult<EnqueueOutcome>;
    async fn heartbeat(&self, leases: &[Lease], extend: Duration) -> StoreResult<Vec<LeaseStatus>>;
    /// Remove the task. Refused with `LeaseLost` unless `lease` is current.
    async fn ack(&self, lease: &Lease) -> StoreResult<()>;
    /// Give the task back, visible again at `retry_at` (now if none).
    async fn nack(&self, lease: &Lease, retry_at: Option<DateTime<Utc>>) -> StoreResult<()>;
    /// Tasks not yet acked, for operators and tests.
    async fn pending(&self, pool: &str) -> StoreResult<usize>;
}

// ── Runs ──

/// Open Workflow status phases.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunPhase {
    Pending,
    Running,
    Waiting,
    Suspended,
    Cancelled,
    Faulted,
    Completed,
}

impl RunPhase {
    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Cancelled | Self::Faulted | Self::Completed)
    }

    /// The status name today's API reports (`ExecutionStatus` Debug names),
    /// kept alongside `phase` for one release.
    pub fn legacy_status(self) -> &'static str {
        match self {
            Self::Pending => "Queued",
            Self::Running => "Running",
            Self::Waiting | Self::Suspended => "Suspended",
            Self::Cancelled => "Cancelled",
            Self::Faulted => "Failed",
            Self::Completed => "Completed",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Durability {
    #[default]
    Durable,
    /// Run record only; faulted `interrupted` after a crash, never resumed.
    Ephemeral,
}

#[derive(Debug, Clone)]
pub struct NewRun {
    pub run_id: RunId,
    pub namespace: String,
    pub flow: String,
    pub version: String,
    pub def_hash: String,
    pub parent_run_id: Option<RunId>,
    pub durability: Durability,
    pub debug: bool,
    pub breakpoints: Option<Value>,
    /// The run's identity. Restored from here on resume, never from stored context.
    pub principal: Value,
    /// api | chat | agent | webhook:<id> | cron:<id> | topic:<id> | subflow
    pub trigger: String,
    pub input: Value,
    pub deadline_at: Option<DateTime<Utc>>,
    /// A second start with the same key returns the first run.
    pub dedupe_key: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RunRow {
    pub run_id: RunId,
    pub namespace: String,
    pub flow: String,
    pub version: String,
    pub def_hash: String,
    pub parent_run_id: Option<RunId>,
    pub durability: Durability,
    pub debug: bool,
    pub breakpoints: Option<Value>,
    pub principal: Value,
    pub trigger: String,
    pub input: Value,
    pub output: Option<Value>,
    pub error: Option<String>,
    pub phase: RunPhase,
    pub current_step: Option<String>,
    pub owner: Option<String>,
    pub epoch: Epoch,
    pub cancel_requested: bool,
    pub suspend_requested: bool,
    pub resume_count: u32,
    pub last_seq: u64,
    pub deadline_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub updated_at: DateTime<Utc>,
}

/// What a run ends a turn with when its owner lets go.
#[derive(Debug, Clone, Default)]
pub struct RunOutcome {
    pub output: Option<Value>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct RunQuery {
    /// Only these namespaces (the caller's visible projects); `None` = all.
    pub namespaces: Option<Vec<String>>,
    pub phases: Vec<RunPhase>,
    pub flow: Option<String>,
    pub trigger: Option<String>,
    pub parent: Option<RunId>,
    pub from: Option<DateTime<Utc>>,
    pub to: Option<DateTime<Utc>>,
    /// From a previous page's `next`.
    pub cursor: Option<String>,
    pub limit: usize,
}

#[derive(Debug, Clone)]
pub struct Page<T> {
    pub items: Vec<T>,
    /// Cursor for the next (older) page; `None` at the end.
    pub next: Option<String>,
}

/// Requests from outside the owner: operators, the console, wake-ups.
#[derive(Debug, Clone, PartialEq)]
pub enum RunRequest {
    Cancel { by: String, reason: String },
    Suspend { by: String },
    Resume { by: String },
    Debug { command: Value, by: String },
    Breakpoints(Value),
}

#[async_trait]
pub trait RunStore: Send + Sync {
    /// Create the run and its StartRun task in one transaction. A repeated
    /// `dedupe_key` returns the existing run's task.
    async fn start(&self, run: NewRun, task: NewTask) -> StoreResult<(RunId, EnqueueOutcome)>;
    /// Lease up to `n` visible tasks of `pool` and take ownership of their
    /// runs, atomically. A task whose run is owned elsewhere stays queued.
    async fn claim_runs(&self, pool: &str, worker: &str, n: usize, lease: Duration)
        -> StoreResult<Vec<(Lease, Task, Epoch)>>;
    /// Extend ownership; false once it is lost.
    async fn renew(&self, run: RunId, epoch: Epoch, lease: Duration) -> StoreResult<bool>;
    /// Let go of the run with a new phase and ack its task — one transaction.
    async fn release(&self, run: RunId, epoch: Epoch, phase: RunPhase, outcome: RunOutcome, lease: &Lease)
        -> StoreResult<()>;
    async fn request(&self, run: RunId, req: RunRequest) -> StoreResult<()>;
    async fn get(&self, run: RunId) -> StoreResult<Option<RunRow>>;
    /// Newest first.
    async fn list(&self, q: RunQuery) -> StoreResult<Page<RunRow>>;
    /// Runs left owned or pending by a process that is gone (owner lease
    /// expired), for startup recovery.
    async fn orphaned(&self) -> StoreResult<Vec<RunRow>>;
}

// ── Journal ──

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StepClass {
    Pure,
    Read,
    Idempotent,
    Effect,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Record {
    RunStarted {
        def_hash: String,
        trigger: String,
        durability: Durability,
        /// The definition version the run is pinned to.
        #[serde(default)]
        version: String,
    },
    StepStarted { step_key: String, attempt: u32, idem_key: String, input_hash: String },
    /// `context` is the run context after the step, or null when unchanged
    /// from the previous step; `directive` is the flow directive it took.
    StepCompleted {
        step_key: String,
        attempt: u32,
        output: Value,
        context: Value,
        class: StepClass,
        duration_us: u64,
        #[serde(default)]
        directive: String,
    },
    StepFailed { step_key: String, attempt: u32, error: Value, retryable: bool, duration_us: u64 },
    StepScheduled { step_key: String, pool: String },
    TimerSet { wait_id: String, fire_at: DateTime<Utc> },
    WaitForEvents { wait_id: String, correlation: Value },
    EventReceived { wait_id: String, events: Value },
    ApprovalRequested { wait_id: String, request: Value },
    ApprovalDecided { wait_id: String, decision: Value, by: String },
    SubflowStarted { step_key: String, child_run: RunId },
    SubflowCompleted { step_key: String, child_run: RunId, output: Value },
    Paused { step_key: String, when: String, reason: String },
    DebugCommand { command: Value, by: String },
    DebugEdit { step_key: String, input: Value, by: String },
    Suspended,
    Resumed { by: String },
    CancelRequested { by: String, reason: String },
    RunCompleted { output: Value },
    RunFaulted { error: Value },
    RunCancelled { reason: String },
}

impl Record {
    /// The record's identity within a run, for step records: re-appending a
    /// record with the same identity is a no-op. Other records always append.
    pub fn identity(&self) -> Option<(&str, &'static str, u32)> {
        match self {
            Self::StepStarted { step_key, attempt, .. } => Some((step_key, "step_started", *attempt)),
            Self::StepCompleted { step_key, attempt, .. } => Some((step_key, "step_completed", *attempt)),
            Self::StepFailed { step_key, attempt, .. } => Some((step_key, "step_failed", *attempt)),
            Self::StepScheduled { step_key, .. } => Some((step_key, "step_scheduled", 0)),
            Self::SubflowStarted { step_key, .. } => Some((step_key, "subflow_started", 0)),
            Self::SubflowCompleted { step_key, .. } => Some((step_key, "subflow_completed", 0)),
            _ => None,
        }
    }

    pub fn step_key(&self) -> Option<&str> {
        self.identity().map(|(k, _, _)| k).or(match self {
            Self::Paused { step_key, .. } | Self::DebugEdit { step_key, .. } => Some(step_key.as_str()),
            _ => None,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct JournalEntry {
    pub seq: u64,
    pub epoch: Epoch,
    pub at: DateTime<Utc>,
    pub record: Record,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AppendOutcome {
    /// The run's last sequence number after the append.
    pub last_seq: u64,
    /// Records skipped because an identical one was already journaled.
    pub duplicates: usize,
}

/// A step's completion and what goes with it, committed together.
#[derive(Debug, Clone, Default)]
pub struct StepCommit {
    pub records: Vec<Record>,
    /// M5b: an interpreter snapshot.
    pub checkpoint: Option<Vec<u8>>,
    /// (topic, body) rows for the outbox drain.
    pub outbox: Vec<(String, Value)>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CheckpointMeta {
    pub seq: u64,
    pub size: usize,
    pub at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct OutboxRow {
    pub id: u64,
    pub topic: String,
    pub body: Value,
}

#[async_trait]
pub trait Journal: Send + Sync {
    /// Append in order. `Fenced` if `epoch` is not the run's current epoch;
    /// records already journaled (same identity) are skipped.
    async fn append(&self, run: RunId, epoch: Epoch, records: &[Record]) -> StoreResult<AppendOutcome>;
    async fn read_from(&self, run: RunId, after_seq: u64) -> StoreResult<Vec<JournalEntry>>;
    /// Records + checkpoint + outbox rows atomically; returns the last seq.
    async fn commit_step(&self, run: RunId, epoch: Epoch, c: StepCommit) -> StoreResult<u64>;
    async fn latest_checkpoint(&self, run: RunId) -> StoreResult<Option<(u64, Vec<u8>)>>;
    async fn checkpoints(&self, run: RunId) -> StoreResult<Vec<CheckpointMeta>>;
    /// Oldest first, up to `limit`; the drain deletes what it has published.
    async fn outbox(&self, limit: usize) -> StoreResult<Vec<OutboxRow>>;
    async fn outbox_done(&self, up_to_id: u64) -> StoreResult<()>;
}

// ── Notifications ──

/// Wake-ups and live views. Best effort: losing a message only adds latency,
/// because claimers also poll.
#[async_trait]
pub trait Notifier: Send + Sync {
    async fn publish(&self, topic: &str, msg: Value) -> StoreResult<()>;
    fn subscribe(&self, topic: &str) -> BoxStream<'static, Value>;
}

/// Topic for a pool's new work.
pub fn tasks_topic(pool: &str) -> String {
    format!("tasks:{pool}")
}

/// Topic for one run's journal appends and status changes.
pub fn run_topic(run: RunId) -> String {
    format!("run:{run}")
}

/// A complete engine backend.
pub trait EngineStore: TaskQueue + RunStore + Journal + Notifier {}
impl<T: TaskQueue + RunStore + Journal + Notifier> EngineStore for T {}
