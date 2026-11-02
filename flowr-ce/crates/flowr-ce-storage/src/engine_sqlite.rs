//! SQLite engine store: the durable queue, runs, journal, checkpoints and
//! outbox for CE (and the EE console in phase 1) in one file
//! (`.ai/FLOWR_QUEUE_CHECKPOINT_SPEC.md` §7). Behaves exactly like
//! [`flowr_storage::engine_memory::MemoryStore`]; both pass
//! `flowr_storage::conformance`.
//!
//! One connection, used from blocking threads; every write is a
//! `BEGIN IMMEDIATE` transaction committed with `synchronous = FULL`, so an
//! acknowledged write survives power loss. Notifications are in-process — the
//! store serves one process.

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use futures::channel::mpsc::{unbounded, UnboundedSender};
use futures::stream::BoxStream;
use futures::StreamExt;
use rusqlite::{params, Connection, OptionalExtension, Row, ToSql, Transaction, TransactionBehavior};
use serde::{de::DeserializeOwned, Serialize};
use serde_json::{json, Value};
use uuid::Uuid;

use flowr_storage::engine::*;
use flowr_storage::engine_memory::{cursor_of, parse_cursor};

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS tasks (
  task_id     TEXT PRIMARY KEY,
  pool        TEXT NOT NULL,
  kind        TEXT NOT NULL,
  run_id      TEXT NOT NULL,
  namespace   TEXT NOT NULL,
  payload     TEXT NOT NULL,
  dedupe_key  TEXT UNIQUE,
  visible_at  INTEGER NOT NULL,
  lease_owner TEXT,
  lease_token INTEGER NOT NULL DEFAULT 0,
  lease_until INTEGER,
  attempts    INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX IF NOT EXISTS tasks_claim ON tasks(pool, visible_at);
CREATE INDEX IF NOT EXISTS tasks_run ON tasks(run_id);

CREATE TABLE IF NOT EXISTS runs (
  run_id            TEXT PRIMARY KEY,
  namespace         TEXT NOT NULL,
  flow              TEXT NOT NULL,
  version           TEXT NOT NULL,
  def_hash          TEXT NOT NULL,
  parent_run_id     TEXT,
  durability        TEXT NOT NULL,
  debug             INTEGER NOT NULL DEFAULT 0,
  breakpoints       TEXT,
  principal         TEXT NOT NULL,
  trigger_kind      TEXT NOT NULL,
  input             TEXT NOT NULL,
  output            TEXT,
  error             TEXT,
  phase             TEXT NOT NULL,
  current_step      TEXT,
  owner             TEXT,
  epoch             INTEGER NOT NULL DEFAULT 0,
  owner_lease_until INTEGER,
  cancel_requested  INTEGER NOT NULL DEFAULT 0,
  suspend_requested INTEGER NOT NULL DEFAULT 0,
  resume_count      INTEGER NOT NULL DEFAULT 0,
  last_seq          INTEGER NOT NULL DEFAULT 0,
  deadline_at       INTEGER,
  dedupe_key        TEXT UNIQUE,
  start_task_id     TEXT,
  created_at        INTEGER NOT NULL,
  started_at        INTEGER,
  completed_at      INTEGER,
  updated_at        INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS runs_ns_time   ON runs(namespace, created_at DESC);
CREATE INDEX IF NOT EXISTS runs_flow_time ON runs(namespace, flow, created_at DESC);
CREATE INDEX IF NOT EXISTS runs_phase     ON runs(phase, created_at DESC);
CREATE INDEX IF NOT EXISTS runs_parent    ON runs(parent_run_id);
CREATE INDEX IF NOT EXISTS runs_time      ON runs(created_at DESC);

CREATE TABLE IF NOT EXISTS journal (
  run_id   TEXT NOT NULL,
  seq      INTEGER NOT NULL,
  ident    TEXT,
  step_key TEXT,
  kind     TEXT NOT NULL,
  epoch    INTEGER NOT NULL,
  body     TEXT NOT NULL,
  at       INTEGER NOT NULL,
  PRIMARY KEY (run_id, seq),
  UNIQUE (run_id, ident)
);

CREATE TABLE IF NOT EXISTS checkpoints (
  run_id TEXT NOT NULL, seq INTEGER NOT NULL, snapshot BLOB NOT NULL, at INTEGER NOT NULL,
  PRIMARY KEY (run_id, seq)
);

CREATE TABLE IF NOT EXISTS waits (
  run_id      TEXT NOT NULL,
  wait_id     TEXT NOT NULL,
  namespace   TEXT NOT NULL,
  correlation TEXT NOT NULL,
  received    TEXT NOT NULL DEFAULT '[]',
  created_at  INTEGER NOT NULL,
  PRIMARY KEY (run_id, wait_id)
);
CREATE INDEX IF NOT EXISTS waits_ns ON waits(namespace, created_at);

CREATE TABLE IF NOT EXISTS outbox (
  id INTEGER PRIMARY KEY AUTOINCREMENT, topic TEXT NOT NULL, body TEXT NOT NULL, created_at INTEGER NOT NULL
);
"#;

pub struct SqliteStore {
    conn: Arc<Mutex<Connection>>,
    subscribers: Mutex<HashMap<String, Vec<UnboundedSender<Value>>>>,
}

fn be<E: std::fmt::Display>(e: E) -> StoreError {
    StoreError::Backend(e.to_string())
}

fn nanos(t: DateTime<Utc>) -> i64 {
    t.timestamp_nanos_opt().unwrap_or(i64::MAX)
}

fn time(n: i64) -> DateTime<Utc> {
    DateTime::from_timestamp_nanos(n)
}

fn span(d: Duration) -> i64 {
    i64::try_from(d.as_nanos()).unwrap_or(i64::MAX / 2)
}

/// A unit enum as its serde name (`"start_run"` → `start_run`).
fn tag<T: Serialize>(v: &T) -> String {
    serde_json::to_value(v).ok().and_then(|v| v.as_str().map(str::to_string)).unwrap_or_default()
}

fn untag<T: DeserializeOwned>(s: &str) -> rusqlite::Result<T> {
    serde_json::from_value(Value::String(s.to_string())).map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))
}

fn js(v: &Value) -> String {
    v.to_string()
}

fn unjs(s: &str) -> Value {
    serde_json::from_str(s).unwrap_or(Value::Null)
}

fn uuid(s: &str) -> rusqlite::Result<Uuid> {
    s.parse().map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))
}

const RUN_COLS: &str = "run_id, namespace, flow, version, def_hash, parent_run_id, durability, debug, breakpoints, principal, \
    trigger_kind, input, output, error, phase, current_step, owner, epoch, cancel_requested, suspend_requested, resume_count, \
    last_seq, deadline_at, created_at, started_at, completed_at, updated_at";

fn run_row(r: &Row) -> rusqlite::Result<RunRow> {
    Ok(RunRow {
        run_id: uuid(&r.get::<_, String>(0)?)?,
        namespace: r.get(1)?,
        flow: r.get(2)?,
        version: r.get(3)?,
        def_hash: r.get(4)?,
        parent_run_id: r.get::<_, Option<String>>(5)?.map(|s| uuid(&s)).transpose()?,
        durability: untag(&r.get::<_, String>(6)?)?,
        debug: r.get::<_, i64>(7)? != 0,
        breakpoints: r.get::<_, Option<String>>(8)?.map(|s| unjs(&s)),
        principal: unjs(&r.get::<_, String>(9)?),
        trigger: r.get(10)?,
        input: unjs(&r.get::<_, String>(11)?),
        output: r.get::<_, Option<String>>(12)?.map(|s| unjs(&s)),
        error: r.get(13)?,
        phase: untag(&r.get::<_, String>(14)?)?,
        current_step: r.get(15)?,
        owner: r.get(16)?,
        epoch: r.get::<_, i64>(17)? as u64,
        cancel_requested: r.get::<_, i64>(18)? != 0,
        suspend_requested: r.get::<_, i64>(19)? != 0,
        resume_count: r.get::<_, i64>(20)? as u32,
        last_seq: r.get::<_, i64>(21)? as u64,
        deadline_at: r.get::<_, Option<i64>>(22)?.map(time),
        created_at: time(r.get(23)?),
        started_at: r.get::<_, Option<i64>>(24)?.map(time),
        completed_at: r.get::<_, Option<i64>>(25)?.map(time),
        updated_at: time(r.get(26)?),
    })
}

fn get_run(tx: &Connection, run: RunId) -> StoreResult<Option<RunRow>> {
    tx.query_row(&format!("SELECT {RUN_COLS} FROM runs WHERE run_id = ?1"), [run.to_string()], run_row)
        .optional()
        .map_err(be)
}

fn enqueue_tx(tx: &Transaction, t: &NewTask) -> StoreResult<EnqueueOutcome> {
    if let Some(key) = &t.dedupe_key {
        let existing: Option<String> = tx
            .query_row("SELECT task_id FROM tasks WHERE dedupe_key = ?1", [key], |r| r.get(0))
            .optional()
            .map_err(be)?;
        if let Some(id) = existing {
            return Ok(EnqueueOutcome::Existing(id.parse().map_err(be)?));
        }
    }
    let id = Uuid::now_v7();
    tx.execute(
        "INSERT INTO tasks(task_id, pool, kind, run_id, namespace, payload, dedupe_key, visible_at) VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",
        params![id.to_string(), t.pool, tag(&t.kind), t.run_id.to_string(), t.namespace, js(&t.payload), t.dedupe_key, nanos(t.visible_at)],
    )
    .map_err(be)?;
    Ok(EnqueueOutcome::Created(id))
}

fn check_epoch(tx: &Connection, run: RunId, epoch: Epoch) -> StoreResult<()> {
    let cur: Option<i64> = tx
        .query_row("SELECT epoch FROM runs WHERE run_id = ?1", [run.to_string()], |r| r.get(0))
        .optional()
        .map_err(be)?;
    match cur {
        None => Err(StoreError::NotFound(format!("run {run}"))),
        Some(e) if e as u64 != epoch => Err(StoreError::Fenced(run)),
        Some(_) => Ok(()),
    }
}

fn check_lease(tx: &Connection, lease: &Lease) -> StoreResult<()> {
    let ok: Option<i64> = tx
        .query_row(
            "SELECT 1 FROM tasks WHERE task_id = ?1 AND lease_token = ?2 AND lease_owner IS NOT NULL",
            params![lease.task_id.to_string(), lease.token as i64],
            |r| r.get(0),
        )
        .optional()
        .map_err(be)?;
    ok.map(|_| ()).ok_or(StoreError::LeaseLost(lease.task_id))
}

/// Append records without the epoch check (callers check it, or are recording
/// an outside request).
fn push(tx: &Transaction, run: RunId, epoch: Epoch, records: &[Record], now: i64) -> StoreResult<AppendOutcome> {
    let rid = run.to_string();
    let mut last: i64 = tx.query_row("SELECT last_seq FROM runs WHERE run_id = ?1", [&rid], |r| r.get(0)).map_err(be)?;
    let mut duplicates = 0;
    let mut current: Option<String> = None;
    for rec in records {
        let ident = rec.identity().map(|(k, kind, a)| format!("{k}|{kind}|{a}"));
        if let Some(id) = &ident {
            let seen: Option<i64> = tx
                .query_row("SELECT 1 FROM journal WHERE run_id = ?1 AND ident = ?2", params![rid, id], |r| r.get(0))
                .optional()
                .map_err(be)?;
            if seen.is_some() {
                duplicates += 1;
                continue;
            }
        }
        last += 1;
        let body = serde_json::to_value(rec).map_err(be)?;
        let kind = body.get("kind").and_then(|k| k.as_str()).unwrap_or("").to_string();
        let step = rec.step_key().map(str::to_string);
        if step.is_some() {
            current = step.clone();
        }
        tx.execute(
            "INSERT INTO journal(run_id, seq, ident, step_key, kind, epoch, body, at) VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",
            params![rid, last, ident, step, kind, epoch as i64, body.to_string(), now],
        )
        .map_err(be)?;
    }
    tx.execute(
        "UPDATE runs SET last_seq = ?2, current_step = COALESCE(?3, current_step), updated_at = ?4 WHERE run_id = ?1",
        params![rid, last, current, now],
    )
    .map_err(be)?;
    Ok(AppendOutcome { last_seq: last as u64, duplicates })
}

impl SqliteStore {
    /// Open (or create) the store file.
    pub fn open(path: impl AsRef<Path>) -> StoreResult<Self> {
        if let Some(dir) = path.as_ref().parent() {
            std::fs::create_dir_all(dir).map_err(be)?;
        }
        Self::init(Connection::open(path).map_err(be)?, true)
    }

    /// A private in-memory database (tests).
    pub fn in_memory() -> StoreResult<Self> {
        Self::init(Connection::open_in_memory().map_err(be)?, false)
    }

    fn init(conn: Connection, file: bool) -> StoreResult<Self> {
        if file {
            conn.pragma_update(None, "journal_mode", "WAL").map_err(be)?;
        }
        conn.pragma_update(None, "synchronous", "FULL").map_err(be)?;
        conn.pragma_update(None, "busy_timeout", 5000).map_err(be)?;
        conn.pragma_update(None, "foreign_keys", "ON").map_err(be)?;
        conn.execute_batch(SCHEMA).map_err(be)?;
        Ok(Self { conn: Arc::new(Mutex::new(conn)), subscribers: Mutex::new(HashMap::new()) })
    }

    /// Run `f` on the connection on a blocking thread.
    async fn with<T, F>(&self, f: F) -> StoreResult<T>
    where
        T: Send + 'static,
        F: FnOnce(&mut Connection) -> StoreResult<T> + Send + 'static,
    {
        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || {
            let mut c = conn.lock().map_err(be)?;
            f(&mut c)
        })
        .await
        .map_err(be)?
    }

    /// Run `f` in a `BEGIN IMMEDIATE` transaction; commit on Ok.
    async fn write<T, F>(&self, f: F) -> StoreResult<T>
    where
        T: Send + 'static,
        F: FnOnce(&Transaction) -> StoreResult<T> + Send + 'static,
    {
        self.with(move |c| {
            let tx = c.transaction_with_behavior(TransactionBehavior::Immediate).map_err(be)?;
            let out = f(&tx)?;
            tx.commit().map_err(be)?;
            Ok(out)
        })
        .await
    }

    fn notify(&self, topic: &str, msg: Value) {
        let mut subs = self.subscribers.lock().unwrap();
        if let Some(list) = subs.get_mut(topic) {
            list.retain(|tx| tx.unbounded_send(msg.clone()).is_ok());
        }
    }
}

#[async_trait]
impl TaskQueue for SqliteStore {
    async fn enqueue(&self, t: NewTask) -> StoreResult<EnqueueOutcome> {
        let pool = t.pool.clone();
        let out = self.write(move |tx| enqueue_tx(tx, &t)).await?;
        if matches!(out, EnqueueOutcome::Created(_)) {
            self.notify(&tasks_topic(&pool), json!({ "enqueued": out.task_id() }));
        }
        Ok(out)
    }

    async fn heartbeat(&self, leases: &[Lease], extend: Duration) -> StoreResult<Vec<LeaseStatus>> {
        let leases = leases.to_vec();
        self.write(move |tx| {
            let until = nanos(Utc::now()) + span(extend);
            leases
                .iter()
                .map(|l| {
                    let n = tx
                        .execute(
                            "UPDATE tasks SET lease_until = ?3 WHERE task_id = ?1 AND lease_token = ?2 AND lease_owner IS NOT NULL",
                            params![l.task_id.to_string(), l.token as i64, until],
                        )
                        .map_err(be)?;
                    Ok(if n == 1 { LeaseStatus::Renewed } else { LeaseStatus::Lost })
                })
                .collect()
        })
        .await
    }

    async fn ack(&self, lease: &Lease) -> StoreResult<()> {
        let lease = lease.clone();
        self.write(move |tx| {
            check_lease(tx, &lease)?;
            tx.execute("DELETE FROM tasks WHERE task_id = ?1", [lease.task_id.to_string()]).map_err(be)?;
            Ok(())
        })
        .await
    }

    async fn nack(&self, lease: &Lease, retry_at: Option<DateTime<Utc>>) -> StoreResult<()> {
        let lease = lease.clone();
        let pool = self
            .write(move |tx| {
                check_lease(tx, &lease)?;
                tx.execute(
                    "UPDATE tasks SET lease_owner = NULL, lease_until = NULL, visible_at = ?2 WHERE task_id = ?1",
                    params![lease.task_id.to_string(), nanos(retry_at.unwrap_or_else(Utc::now))],
                )
                .map_err(be)?;
                tx.query_row("SELECT pool FROM tasks WHERE task_id = ?1", [lease.task_id.to_string()], |r| r.get::<_, String>(0))
                    .map_err(be)
            })
            .await?;
        self.notify(&tasks_topic(&pool), json!({ "nacked": true }));
        Ok(())
    }

    async fn pending(&self, pool: &str) -> StoreResult<usize> {
        let pool = pool.to_string();
        self.with(move |c| {
            c.query_row("SELECT COUNT(*) FROM tasks WHERE pool = ?1", [pool], |r| r.get::<_, i64>(0)).map(|n| n as usize).map_err(be)
        })
        .await
    }
}

#[async_trait]
impl RunStore for SqliteStore {
    async fn start(&self, run: NewRun, task: NewTask) -> StoreResult<(RunId, EnqueueOutcome)> {
        let pool = task.pool.clone();
        let out = self
            .write(move |tx| {
                if let Some(key) = &run.dedupe_key {
                    let existing: Option<(String, Option<String>)> = tx
                        .query_row("SELECT run_id, start_task_id FROM runs WHERE dedupe_key = ?1", [key], |r| Ok((r.get(0)?, r.get(1)?)))
                        .optional()
                        .map_err(be)?;
                    if let Some((rid, tid)) = existing {
                        let tid = tid.and_then(|t| t.parse().ok()).unwrap_or_default();
                        return Ok((rid.parse().map_err(be)?, EnqueueOutcome::Existing(tid)));
                    }
                }
                let exists: Option<i64> = tx
                    .query_row("SELECT 1 FROM runs WHERE run_id = ?1", [run.run_id.to_string()], |r| r.get(0))
                    .optional()
                    .map_err(be)?;
                if exists.is_some() {
                    return Err(StoreError::Conflict(format!("run {} exists", run.run_id)));
                }
                let now = nanos(Utc::now());
                let out = enqueue_tx(tx, &task)?;
                tx.execute(
                    "INSERT INTO runs(run_id, namespace, flow, version, def_hash, parent_run_id, durability, debug, breakpoints, principal, \
                     trigger_kind, input, phase, deadline_at, dedupe_key, start_task_id, created_at, updated_at) \
                     VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?17)",
                    params![
                        run.run_id.to_string(),
                        run.namespace,
                        run.flow,
                        run.version,
                        run.def_hash,
                        run.parent_run_id.map(|p| p.to_string()),
                        tag(&run.durability),
                        run.debug as i64,
                        run.breakpoints.as_ref().map(js),
                        js(&run.principal),
                        run.trigger,
                        js(&run.input),
                        tag(&RunPhase::Pending),
                        run.deadline_at.map(nanos),
                        run.dedupe_key,
                        out.task_id().to_string(),
                        now,
                    ],
                )
                .map_err(be)?;
                Ok((run.run_id, out))
            })
            .await?;
        self.notify(&tasks_topic(&pool), json!({ "enqueued": out.1.task_id() }));
        Ok(out)
    }

    async fn claim_runs(&self, pool: &str, worker: &str, n: usize, lease: Duration)
        -> StoreResult<Vec<(Lease, Task, Epoch)>> {
        let (pool, worker) = (pool.to_string(), worker.to_string());
        self.write(move |tx| {
            let now = nanos(Utc::now());
            let until = now + span(lease);
            let candidates: Vec<(String, String)> = {
                let mut st = tx
                    .prepare(
                        "SELECT task_id, run_id FROM tasks WHERE pool = ?1 AND visible_at <= ?2 \
                         AND (lease_until IS NULL OR lease_until < ?2) ORDER BY visible_at, rowid",
                    )
                    .map_err(be)?;
                let rows = st.query_map(params![pool, now], |r| Ok((r.get(0)?, r.get(1)?))).map_err(be)?;
                rows.collect::<Result<_, _>>().map_err(be)?
            };
            let mut out = Vec::new();
            for (task_id, run_id) in candidates {
                if out.len() >= n {
                    break;
                }
                let run: Option<(String, Option<String>, Option<i64>)> = tx
                    .query_row("SELECT phase, owner, owner_lease_until FROM runs WHERE run_id = ?1", [&run_id], |r| {
                        Ok((r.get(0)?, r.get(1)?, r.get(2)?))
                    })
                    .optional()
                    .map_err(be)?;
                let Some((phase, owner, owner_until)) = run else {
                    tx.execute("DELETE FROM tasks WHERE task_id = ?1", [&task_id]).map_err(be)?;
                    continue;
                };
                let phase: RunPhase = untag(&phase).map_err(be)?;
                if phase.is_terminal() {
                    tx.execute("DELETE FROM tasks WHERE task_id = ?1", [&task_id]).map_err(be)?;
                    continue;
                }
                if owner.is_some() && owner_until.is_some_and(|t| t >= now) {
                    tx.execute("UPDATE tasks SET visible_at = ?2 WHERE task_id = ?1", params![task_id, now + 1_000_000_000])
                        .map_err(be)?;
                    continue;
                }
                let task: (Task, i64) = tx
                    .query_row(
                        "UPDATE tasks SET lease_owner = ?2, lease_token = lease_token + 1, lease_until = ?3, attempts = attempts + 1 \
                         WHERE task_id = ?1 RETURNING pool, kind, namespace, payload, attempts, lease_token",
                        params![task_id, worker, until],
                        |r| {
                            Ok((
                                Task {
                                    task_id: uuid(&task_id)?,
                                    pool: r.get(0)?,
                                    kind: untag(&r.get::<_, String>(1)?)?,
                                    run_id: uuid(&run_id)?,
                                    namespace: r.get(2)?,
                                    payload: unjs(&r.get::<_, String>(3)?),
                                    attempts: r.get::<_, i64>(4)? as u32,
                                },
                                r.get(5)?,
                            ))
                        },
                    )
                    .map_err(be)?;
                let resumed = (task.0.kind == TaskKind::ResumeRun) as i64;
                let epoch: i64 = tx
                    .query_row(
                        "UPDATE runs SET owner = ?2, epoch = epoch + 1, owner_lease_until = ?3, phase = ?4, \
                         started_at = COALESCE(started_at, ?5), resume_count = resume_count + ?6, updated_at = ?5 \
                         WHERE run_id = ?1 RETURNING epoch",
                        params![run_id, worker, until, tag(&RunPhase::Running), now, resumed],
                        |r| r.get(0),
                    )
                    .map_err(be)?;
                let lease = Lease { task_id: task.0.task_id, token: task.1 as u64, expires_at: time(until) };
                out.push((lease, task.0, epoch as u64));
            }
            Ok(out)
        })
        .await
    }

    async fn renew(&self, run: RunId, epoch: Epoch, lease: Duration) -> StoreResult<bool> {
        self.write(move |tx| {
            let n = tx
                .execute(
                    "UPDATE runs SET owner_lease_until = ?3 WHERE run_id = ?1 AND epoch = ?2 AND owner IS NOT NULL",
                    params![run.to_string(), epoch as i64, nanos(Utc::now()) + span(lease)],
                )
                .map_err(be)?;
            Ok(n == 1)
        })
        .await
    }

    async fn release(&self, run: RunId, epoch: Epoch, phase: RunPhase, outcome: RunOutcome, lease: &Lease)
        -> StoreResult<()> {
        let lease = lease.clone();
        self.write(move |tx| {
            check_epoch(tx, run, epoch)?;
            check_lease(tx, &lease)?;
            let now = nanos(Utc::now());
            tx.execute("DELETE FROM tasks WHERE task_id = ?1", [lease.task_id.to_string()]).map_err(be)?;
            tx.execute(
                "UPDATE runs SET owner = NULL, owner_lease_until = NULL, phase = ?2, \
                 output = COALESCE(?3, output), error = COALESCE(?4, error), \
                 suspend_requested = CASE WHEN ?2 = 'suspended' THEN 0 ELSE suspend_requested END, \
                 completed_at = CASE WHEN ?5 THEN ?6 ELSE completed_at END, updated_at = ?6 WHERE run_id = ?1",
                params![run.to_string(), tag(&phase), outcome.output.as_ref().map(js), outcome.error, phase.is_terminal(), now],
            )
            .map_err(be)?;
            Ok(())
        })
        .await?;
        self.notify(&run_topic(run), json!({ "phase": phase }));
        Ok(())
    }

    async fn request(&self, run: RunId, req: RunRequest) -> StoreResult<()> {
        let woke: Option<String> = self
            .write(move |tx| {
                let now_t = Utc::now();
                let now = nanos(now_t);
                let row = get_run(tx, run)?.ok_or_else(|| StoreError::NotFound(format!("run {run}")))?;
                if row.phase.is_terminal() {
                    return Ok(None);
                }
                let owner_until: Option<i64> = tx
                    .query_row("SELECT owner_lease_until FROM runs WHERE run_id = ?1", [run.to_string()], |r| r.get(0))
                    .map_err(be)?;
                let owned = row.owner.is_some() && owner_until.is_some_and(|t| t >= now);
                let rid = run.to_string();
                let drop_tasks = |tx: &Transaction| {
                    tx.execute("DELETE FROM tasks WHERE run_id = ?1 AND lease_owner IS NULL", [&rid]).map_err(be)
                };
                let mut woke = None;
                match req {
                    RunRequest::Cancel { by, reason } => {
                        push(tx, run, row.epoch, &[Record::CancelRequested { by, reason: reason.clone() }], now)?;
                        tx.execute("UPDATE runs SET cancel_requested = 1 WHERE run_id = ?1", [&rid]).map_err(be)?;
                        if !owned {
                            push(tx, run, row.epoch, &[Record::RunCancelled { reason: reason.clone() }], now)?;
                            tx.execute(
                                "UPDATE runs SET phase = ?2, error = ?3, completed_at = ?4, owner = NULL WHERE run_id = ?1",
                                params![rid, tag(&RunPhase::Cancelled), reason, now],
                            )
                            .map_err(be)?;
                            drop_tasks(tx)?;
                            tx.execute("DELETE FROM waits WHERE run_id = ?1", [&rid]).map_err(be)?;
                        }
                    }
                    RunRequest::Suspend { .. } => {
                        if owned {
                            tx.execute("UPDATE runs SET suspend_requested = 1 WHERE run_id = ?1", [&rid]).map_err(be)?;
                        } else {
                            push(tx, run, row.epoch, &[Record::Suspended], now)?;
                            tx.execute("UPDATE runs SET phase = ?2 WHERE run_id = ?1", params![rid, tag(&RunPhase::Suspended)]).map_err(be)?;
                            drop_tasks(tx)?;
                        }
                    }
                    RunRequest::Resume { by } => {
                        if row.phase != RunPhase::Suspended {
                            return Err(StoreError::Conflict(format!("run {run} is not suspended")));
                        }
                        let seq = push(tx, run, row.epoch, &[Record::Resumed { by }], now)?.last_seq;
                        tx.execute("UPDATE runs SET phase = ?2 WHERE run_id = ?1", params![rid, tag(&RunPhase::Pending)]).map_err(be)?;
                        enqueue_tx(tx, &NewTask::resume_for(&row, Some(format!("{run}:resume:{seq}")), now_t))?;
                        woke = Some(pool_for(&row.namespace, &row.flow));
                    }
                    RunRequest::Debug { command, by } => {
                        if !row.debug || row.phase != RunPhase::Suspended {
                            return Err(StoreError::Conflict(format!("run {run} is not a paused debug run")));
                        }
                        let seq = push(tx, run, row.epoch, &[Record::DebugCommand { command, by }], now)?.last_seq;
                        tx.execute("UPDATE runs SET phase = ?2 WHERE run_id = ?1", params![rid, tag(&RunPhase::Pending)]).map_err(be)?;
                        enqueue_tx(tx, &NewTask::resume_for(&row, Some(format!("{run}:debug:{seq}")), now_t))?;
                        woke = Some(pool_for(&row.namespace, &row.flow));
                    }
                    RunRequest::Breakpoints(bps) => {
                        if !row.debug {
                            return Err(StoreError::Conflict(format!("run {run} is not a debug run")));
                        }
                        tx.execute("UPDATE runs SET breakpoints = ?2 WHERE run_id = ?1", params![rid, js(&bps)]).map_err(be)?;
                    }
                }
                tx.execute("UPDATE runs SET updated_at = ?2 WHERE run_id = ?1", params![rid, now]).map_err(be)?;
                Ok(woke)
            })
            .await?;
        if let Some(pool) = woke {
            self.notify(&tasks_topic(&pool), json!({ "resume": run }));
        }
        self.notify(&run_topic(run), json!({ "request": true }));
        Ok(())
    }

    async fn get(&self, run: RunId) -> StoreResult<Option<RunRow>> {
        self.with(move |c| get_run(c, run)).await
    }

    async fn list(&self, q: RunQuery) -> StoreResult<Page<RunRow>> {
        if q.namespaces.as_ref().is_some_and(|n| n.is_empty()) {
            return Ok(Page { items: Vec::new(), next: None });
        }
        self.with(move |c| {
            let limit = if q.limit == 0 { 50 } else { q.limit };
            let mut sql = format!("SELECT {RUN_COLS} FROM runs WHERE 1=1");
            let mut args: Vec<Box<dyn ToSql>> = Vec::new();
            let mut bind = |sql: &mut String, frag: &str, v: Box<dyn ToSql>| {
                args.push(v);
                sql.push_str(&frag.replace('?', &format!("?{}", args.len())));
            };
            if let Some(ns) = &q.namespaces {
                sql.push_str(" AND namespace IN (");
                for (i, n) in ns.iter().enumerate() {
                    if i > 0 {
                        sql.push(',');
                    }
                    bind(&mut sql, "?", Box::new(n.clone()));
                }
                sql.push(')');
            }
            if !q.phases.is_empty() {
                sql.push_str(" AND phase IN (");
                for (i, p) in q.phases.iter().enumerate() {
                    if i > 0 {
                        sql.push(',');
                    }
                    bind(&mut sql, "?", Box::new(tag(p)));
                }
                sql.push(')');
            }
            if let Some(f) = &q.flow {
                bind(&mut sql, " AND flow = ?", Box::new(f.clone()));
            }
            if let Some(t) = &q.trigger {
                bind(&mut sql, " AND trigger_kind = ?", Box::new(t.clone()));
            }
            if let Some(p) = q.parent {
                bind(&mut sql, " AND parent_run_id = ?", Box::new(p.to_string()));
            }
            if let Some(t) = q.from {
                bind(&mut sql, " AND created_at >= ?", Box::new(nanos(t)));
            }
            if let Some(t) = q.to {
                bind(&mut sql, " AND created_at < ?", Box::new(nanos(t)));
            }
            if let Some((t, id)) = q.cursor.as_deref().and_then(parse_cursor) {
                let (t, id) = (nanos(t), id.to_string());
                args.push(Box::new(t));
                let a = args.len();
                args.push(Box::new(id));
                let b = args.len();
                sql.push_str(&format!(" AND (created_at < ?{a} OR (created_at = ?{a} AND run_id < ?{b}))"));
            }
            sql.push_str(&format!(" ORDER BY created_at DESC, run_id DESC LIMIT {}", limit + 1));
            let mut st = c.prepare(&sql).map_err(be)?;
            let refs: Vec<&dyn ToSql> = args.iter().map(|b| b.as_ref()).collect();
            let mut items: Vec<RunRow> = st.query_map(refs.as_slice(), run_row).map_err(be)?.collect::<Result<_, _>>().map_err(be)?;
            let more = items.len() > limit;
            items.truncate(limit);
            let next = if more { items.last().map(cursor_of) } else { None };
            Ok(Page { items, next })
        })
        .await
    }

    async fn purge_finished(&self, before: DateTime<Utc>) -> StoreResult<usize> {
        self.write(move |tx| {
            let cut = nanos(before);
            let done = "phase IN ('completed','faulted','cancelled') AND completed_at IS NOT NULL AND completed_at < ?1";
            tx.execute(&format!("DELETE FROM journal WHERE run_id IN (SELECT run_id FROM runs WHERE {done})"), [cut]).map_err(be)?;
            tx.execute(&format!("DELETE FROM checkpoints WHERE run_id IN (SELECT run_id FROM runs WHERE {done})"), [cut]).map_err(be)?;
            tx.execute(&format!("DELETE FROM tasks WHERE run_id IN (SELECT run_id FROM runs WHERE {done})"), [cut]).map_err(be)?;
            tx.execute(&format!("DELETE FROM waits WHERE run_id IN (SELECT run_id FROM runs WHERE {done})"), [cut]).map_err(be)?;
            let n = tx.execute(&format!("DELETE FROM runs WHERE {done}"), [cut]).map_err(be)?;
            Ok(n)
        })
        .await
    }

    async fn open_wait(&self, run: RunId, epoch: Epoch, wait_id: &str, correlation: Value) -> StoreResult<()> {
        let wait_id = wait_id.to_string();
        self.write(move |tx| {
            check_epoch(tx, run, epoch)?;
            let now = nanos(Utc::now());
            let n = tx
                .execute(
                    "INSERT OR IGNORE INTO waits(wait_id, run_id, namespace, correlation, created_at) \
                     SELECT ?1, run_id, namespace, ?3, ?4 FROM runs WHERE run_id = ?2",
                    params![wait_id, run.to_string(), js(&correlation), now],
                )
                .map_err(be)?;
            if n > 0 {
                push(tx, run, epoch, &[Record::WaitForEvents { wait_id, correlation }], now)?;
            }
            Ok(())
        })
        .await
    }

    async fn close_wait(&self, run: RunId, epoch: Epoch, wait_id: &str) -> StoreResult<bool> {
        let wait_id = wait_id.to_string();
        self.write(move |tx| {
            check_epoch(tx, run, epoch)?;
            let n = tx.execute("DELETE FROM waits WHERE wait_id = ?1 AND run_id = ?2", params![wait_id, run.to_string()]).map_err(be)?;
            Ok(n > 0)
        })
        .await
    }

    async fn deliver_event(&self, namespace: &str, event: Value) -> StoreResult<Vec<RunId>> {
        let namespace = namespace.to_string();
        let woke: Vec<(RunId, String)> = self
            .write(move |tx| {
                let now_t = Utc::now();
                let now = nanos(now_t);
                let open: Vec<(String, String, String, String)> = {
                    let mut st = tx
                        .prepare("SELECT wait_id, run_id, correlation, received FROM waits WHERE namespace = ?1 ORDER BY created_at")
                        .map_err(be)?;
                    let rows = st.query_map([&namespace], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))).map_err(be)?;
                    rows.collect::<Result<_, _>>().map_err(be)?
                };
                let mut woke = Vec::new();
                for (wait_id, rid, correlation, received) in open {
                    let run = uuid(&rid).map_err(be)?;
                    let row = match get_run(tx, run)? {
                        Some(r) if !r.phase.is_terminal() => r,
                        _ => {
                            tx.execute("DELETE FROM waits WHERE run_id = ?1 AND wait_id = ?2", params![rid, wait_id]).map_err(be)?;
                            continue;
                        }
                    };
                    let mut got: Vec<Value> = serde_json::from_str(&received).unwrap_or_default();
                    let (taken, done) = offer_event(&unjs(&correlation), &mut got, &event);
                    if !taken {
                        continue;
                    }
                    if !done {
                        tx.execute("UPDATE waits SET received = ?3 WHERE run_id = ?1 AND wait_id = ?2", params![rid, wait_id, js(&Value::Array(got))]).map_err(be)?;
                        continue;
                    }
                    tx.execute("DELETE FROM waits WHERE run_id = ?1 AND wait_id = ?2", params![rid, wait_id]).map_err(be)?;
                    push(tx, run, row.epoch, &[Record::EventReceived { wait_id: wait_id.clone(), events: received_events(&got) }], now)?;
                    let owner_until: Option<i64> = tx
                        .query_row("SELECT owner_lease_until FROM runs WHERE run_id = ?1", [&rid], |r| r.get(0))
                        .map_err(be)?;
                    if !(row.owner.is_some() && owner_until.is_some_and(|t| t >= now)) {
                        tx.execute("UPDATE runs SET phase = ?2 WHERE run_id = ?1", params![rid, tag(&RunPhase::Pending)]).map_err(be)?;
                    }
                    enqueue_tx(tx, &NewTask::resume_for(&row, Some(wait_wake_key(run, &wait_id)), now_t))?;
                    woke.push((run, pool_for(&row.namespace, &row.flow)));
                }
                Ok(woke)
            })
            .await?;
        for (run, pool) in &woke {
            self.notify(&tasks_topic(pool), json!({ "resume": run }));
            self.notify(&run_topic(*run), json!({ "event": true }));
        }
        Ok(woke.into_iter().map(|(r, _)| r).collect())
    }

    async fn orphaned(&self) -> StoreResult<Vec<RunRow>> {
        self.with(|c| {
            let now = nanos(Utc::now());
            let mut st = c
                .prepare(&format!(
                    "SELECT {RUN_COLS} FROM runs WHERE phase NOT IN ('completed','faulted','cancelled') \
                     AND owner IS NOT NULL AND (owner_lease_until IS NULL OR owner_lease_until < ?1)"
                ))
                .map_err(be)?;
            let rows = st.query_map([now], run_row).map_err(be)?.collect::<Result<Vec<_>, _>>().map_err(be)?;
            Ok(rows)
        })
        .await
    }
}

#[async_trait]
impl Journal for SqliteStore {
    async fn append(&self, run: RunId, epoch: Epoch, records: &[Record]) -> StoreResult<AppendOutcome> {
        let records = records.to_vec();
        let out = self
            .write(move |tx| {
                check_epoch(tx, run, epoch)?;
                push(tx, run, epoch, &records, nanos(Utc::now()))
            })
            .await?;
        self.notify(&run_topic(run), json!({ "seq": out.last_seq }));
        Ok(out)
    }

    async fn read_from(&self, run: RunId, after_seq: u64) -> StoreResult<Vec<JournalEntry>> {
        self.with(move |c| {
            let mut st = c
                .prepare("SELECT seq, epoch, at, body FROM journal WHERE run_id = ?1 AND seq > ?2 ORDER BY seq")
                .map_err(be)?;
            let rows = st
                .query_map(params![run.to_string(), after_seq as i64], |r| {
                    let body: String = r.get(3)?;
                    Ok(JournalEntry {
                        seq: r.get::<_, i64>(0)? as u64,
                        epoch: r.get::<_, i64>(1)? as u64,
                        at: time(r.get(2)?),
                        record: serde_json::from_str(&body).map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?,
                    })
                })
                .map_err(be)?;
            rows.collect::<Result<Vec<_>, _>>().map_err(be)
        })
        .await
    }

    async fn commit_step(&self, run: RunId, epoch: Epoch, c: StepCommit) -> StoreResult<u64> {
        let seq = self
            .write(move |tx| {
                check_epoch(tx, run, epoch)?;
                let now = nanos(Utc::now());
                let seq = push(tx, run, epoch, &c.records, now)?.last_seq;
                if let Some(snap) = c.checkpoint {
                    tx.execute(
                        "INSERT OR REPLACE INTO checkpoints(run_id, seq, snapshot, at) VALUES (?1,?2,?3,?4)",
                        params![run.to_string(), seq as i64, snap, now],
                    )
                    .map_err(be)?;
                }
                for (topic, body) in c.outbox {
                    tx.execute("INSERT INTO outbox(topic, body, created_at) VALUES (?1,?2,?3)", params![topic, js(&body), now])
                        .map_err(be)?;
                }
                Ok(seq)
            })
            .await?;
        self.notify(&run_topic(run), json!({ "seq": seq }));
        Ok(seq)
    }

    async fn latest_checkpoint(&self, run: RunId) -> StoreResult<Option<(u64, Vec<u8>)>> {
        self.with(move |c| {
            c.query_row(
                "SELECT seq, snapshot FROM checkpoints WHERE run_id = ?1 ORDER BY seq DESC LIMIT 1",
                [run.to_string()],
                |r| Ok((r.get::<_, i64>(0)? as u64, r.get::<_, Vec<u8>>(1)?)),
            )
            .optional()
            .map_err(be)
        })
        .await
    }

    async fn checkpoints(&self, run: RunId) -> StoreResult<Vec<CheckpointMeta>> {
        self.with(move |c| {
            let mut st = c
                .prepare("SELECT seq, length(snapshot), at FROM checkpoints WHERE run_id = ?1 ORDER BY seq")
                .map_err(be)?;
            let rows = st
                .query_map([run.to_string()], |r| {
                    Ok(CheckpointMeta { seq: r.get::<_, i64>(0)? as u64, size: r.get::<_, i64>(1)? as usize, at: time(r.get(2)?) })
                })
                .map_err(be)?;
            rows.collect::<Result<Vec<_>, _>>().map_err(be)
        })
        .await
    }

    async fn outbox(&self, limit: usize) -> StoreResult<Vec<OutboxRow>> {
        self.with(move |c| {
            let mut st = c.prepare("SELECT id, topic, body FROM outbox ORDER BY id LIMIT ?1").map_err(be)?;
            let rows = st
                .query_map([limit as i64], |r| {
                    Ok(OutboxRow { id: r.get::<_, i64>(0)? as u64, topic: r.get(1)?, body: unjs(&r.get::<_, String>(2)?) })
                })
                .map_err(be)?;
            rows.collect::<Result<Vec<_>, _>>().map_err(be)
        })
        .await
    }

    async fn outbox_done(&self, up_to_id: u64) -> StoreResult<()> {
        self.write(move |tx| {
            tx.execute("DELETE FROM outbox WHERE id <= ?1", [up_to_id as i64]).map_err(be)?;
            Ok(())
        })
        .await
    }
}

#[async_trait]
impl Notifier for SqliteStore {
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
    use super::SqliteStore;
    use flowr_storage::engine::*;

    #[tokio::test]
    async fn conformance_in_memory() {
        flowr_storage::conformance::run_all(|| async { SqliteStore::in_memory().unwrap() }).await;
    }

    #[tokio::test]
    async fn conformance_on_a_file() {
        let dir = std::env::temp_dir().join(format!("flowr-sqlite-{}", uuid::Uuid::now_v7()));
        let n = std::sync::atomic::AtomicUsize::new(0);
        flowr_storage::conformance::run_all(|| {
            let path = dir.join(format!("s{}.db", n.fetch_add(1, std::sync::atomic::Ordering::SeqCst)));
            async move { SqliteStore::open(path).unwrap() }
        })
        .await;
        let _ = std::fs::remove_dir_all(dir);
    }

    /// What a crash keeps: everything committed before it.
    #[tokio::test]
    async fn committed_state_survives_reopen() {
        let dir = std::env::temp_dir().join(format!("flowr-sqlite-{}", uuid::Uuid::now_v7()));
        let path = dir.join("flowr.db");
        let run_id;
        {
            let s = SqliteStore::open(&path).unwrap();
            let run = NewRun {
                run_id: uuid::Uuid::now_v7(),
                namespace: "p".into(),
                flow: "f".into(),
                version: "1".into(),
                def_hash: "h".into(),
                parent_run_id: None,
                durability: Durability::Durable,
                debug: false,
                breakpoints: None,
                principal: serde_json::json!({ "id": "u" }),
                trigger: "api".into(),
                input: serde_json::json!({ "a": 1 }),
                deadline_at: None,
                dedupe_key: None,
            };
            run_id = run.run_id;
            let task = NewTask::start(&run);
            s.start(run, task).await.unwrap();
            let (_, _, epoch) = s.claim_runs(DEFAULT_POOL, "w", 1, std::time::Duration::from_millis(50)).await.unwrap().remove(0);
            s.append(run_id, epoch, &[Record::RunStarted { def_hash: "h".into(), trigger: "api".into(), durability: Durability::Durable, version: String::new() }])
                .await
                .unwrap();
        }
        tokio::time::sleep(std::time::Duration::from_millis(80)).await;
        let s = SqliteStore::open(&path).unwrap();
        let row = s.get(run_id).await.unwrap().expect("the run survives");
        assert_eq!(row.input, serde_json::json!({ "a": 1 }));
        assert_eq!(s.read_from(run_id, 0).await.unwrap().len(), 1, "the journal survives");
        assert_eq!(s.orphaned().await.unwrap().len(), 1, "its owner is gone, so it is orphaned");
        let reclaimed = s.claim_runs(DEFAULT_POOL, "w2", 1, std::time::Duration::from_secs(30)).await.unwrap();
        assert_eq!(reclaimed.len(), 1, "its task is claimable again");
        assert_eq!(reclaimed[0].2, 2, "under a new epoch");
        let _ = std::fs::remove_dir_all(dir);
    }

    /// Stage 2 benchmark (run with `--ignored --nocapture`): per-step commit
    /// latency on a file with synchronous=FULL, and run start→claim→release.
    #[tokio::test(flavor = "multi_thread")]
    #[ignore]
    async fn bench() {
        let dir = std::env::temp_dir().join(format!("flowr-bench-{}", uuid::Uuid::now_v7()));
        let s = std::sync::Arc::new(SqliteStore::open(dir.join("b.db")).unwrap());
        let mk = || NewRun {
            run_id: uuid::Uuid::now_v7(), namespace: "p".into(), flow: "f".into(), version: "1".into(), def_hash: "h".into(),
            parent_run_id: None, durability: Durability::Durable, debug: false, breakpoints: None,
            principal: serde_json::json!({ "id": "u" }), trigger: "api".into(), input: serde_json::json!({ "a": 1 }),
            deadline_at: None, dedupe_key: None,
        };
        // Runs: start + claim + release, one at a time.
        let n = 300;
        let t = std::time::Instant::now();
        for _ in 0..n {
            let run = mk();
            let task = NewTask::start(&run);
            s.start(run, task).await.unwrap();
            let (lease, task, epoch) = s.claim_runs(DEFAULT_POOL, "w", 1, std::time::Duration::from_secs(30)).await.unwrap().remove(0);
            s.release(task.run_id, epoch, RunPhase::Completed, RunOutcome::default(), &lease).await.unwrap();
        }
        let per_run = t.elapsed() / n;
        // Steps: commit_step on one run, measuring each.
        let run = mk();
        let id = run.run_id;
        let task = NewTask::start(&run);
        s.start(run, task).await.unwrap();
        let (_, _, epoch) = s.claim_runs(DEFAULT_POOL, "w", 1, std::time::Duration::from_secs(30)).await.unwrap().remove(0);
        let mut lat = Vec::new();
        for i in 0..1000 {
            let t = std::time::Instant::now();
            s.commit_step(id, epoch, StepCommit {
                records: vec![Record::StepCompleted { step_key: format!("/do/{i}/s"), attempt: 0, output: serde_json::json!({ "i": i }),
                    context: serde_json::json!({ "k": "v" }), class: StepClass::Effect, duration_us: 1, directive: String::new(), input: serde_json::Value::Null, at: None }],
                checkpoint: None, outbox: vec![],
            }).await.unwrap();
            lat.push(t.elapsed());
        }
        lat.sort();
        // 8 concurrent runs committing steps.
        let t = std::time::Instant::now();
        let mut hs = Vec::new();
        for _ in 0..8 {
            let s = s.clone();
            let run = mk();
            let id = run.run_id;
            let task = NewTask::start(&run);
            s.start(run, task).await.unwrap();
            hs.push(tokio::spawn(async move {
                let epoch = loop {
                    let c = s.claim_runs(DEFAULT_POOL, "w", 8, std::time::Duration::from_secs(30)).await.unwrap();
                    if let Some(x) = c.into_iter().find(|(_, t, _)| t.run_id == id) { break x.2; }
                    if s.get(id).await.unwrap().unwrap().owner.is_some() { break s.get(id).await.unwrap().unwrap().epoch; }
                };
                for i in 0..200 {
                    s.commit_step(id, epoch, StepCommit { records: vec![Record::StepCompleted { step_key: format!("/do/{i}/s"), attempt: 0,
                        output: serde_json::json!(i), context: serde_json::json!({}), class: StepClass::Pure, duration_us: 1, directive: String::new(), input: serde_json::Value::Null, at: None }],
                        checkpoint: None, outbox: vec![] }).await.unwrap();
                }
            }));
        }
        for h in hs { h.await.unwrap(); }
        let conc = 1600.0 / t.elapsed().as_secs_f64();
        println!("BENCH run start+claim+release: {:?}/run ({:.0} runs/s)", per_run, 1.0 / per_run.as_secs_f64());
        println!("BENCH step commit p50 {:?} p99 {:?} ({:.0} steps/s serial)", lat[500], lat[990], 1000.0 / lat.iter().sum::<std::time::Duration>().as_secs_f64());
        println!("BENCH 8 concurrent runs: {:.0} steps/s", conc);
        let _ = std::fs::remove_dir_all(dir);
    }
}
