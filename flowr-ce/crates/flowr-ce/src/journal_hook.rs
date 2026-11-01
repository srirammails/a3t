//! The run journal as a step hook (`.ai/FLOWR_QUEUE_CHECKPOINT_SPEC.md` §5.4–5.7,
//! plan stage 4).
//!
//! Every task of a durable run is recorded; a run that resumes after its worker
//! died replays from the journal, so completed steps are not executed again.
//!
//! - **Pure** steps (`set`, `switch`, containers, waits) are recorded when they
//!   complete, batched into the next commit — no extra disk sync.
//! - **Other** steps (calls, `run`, `emit`, `listen`) write `StepStarted`
//!   durably *before* they run, then their completion. A step that started but
//!   never completed is in doubt: read steps run again; an idempotent step whose
//!   `metadata.a3t.inDoubt` is `retry` runs again under the same idempotency key;
//!   anything else faults with `in-doubt side effect` (the default), which the
//!   flow's `try`/`catch` can handle.
//! - Context is recorded only when it changed (null otherwise); containers
//!   always record it, since replaying one skips its inner steps.
//! - Every write carries the run's epoch: a worker that lost the run is fenced
//!   and the run stops here.
//!
//! Values are journaled as they are (replay needs them exactly) and masked when
//! read.

use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use serde_json::{json, Value};

use flowr_storage::engine::*;
use swf_parser::{CallTarget, NamedTask, Task};
use swf_runtime::error::{Result, RuntimeError};
use swf_runtime::{StepHook, StepOutcome, StepReplay};

/// Pure records are flushed at the latest after this many.
const MAX_PENDING: usize = 64;

enum Prior {
    Completed { output: Value, context: Option<Value>, directive: String },
    Failed { error: String },
    Started { attempt: u32 },
}

pub(crate) struct JournalHook {
    engine: Arc<dyn EngineStore>,
    run: RunId,
    epoch: Epoch,
    /// `namespace/name`: task paths are reported under it.
    prefix: String,
    prior: Mutex<HashMap<String, Prior>>,
    attempts: Mutex<HashMap<String, u32>>,
    pending: Mutex<Vec<Record>>,
    /// The context after the last recorded step.
    last_context: Mutex<Value>,
    /// The definition version recorded when the run first started.
    pinned: Option<String>,
    /// Crash-test kill points for this run (`FLOWR_KILL_AT`), first attempt only.
    kill_at: Vec<(String, String)>,
    killed: std::sync::atomic::AtomicBool,
}

/// What kind of step a task is, for in-doubt handling.
pub(crate) fn class_of(task: &NamedTask) -> StepClass {
    if let Some(c) = task.metadata.as_ref().and_then(|m| m.pointer("/a3t/stepClass")).and_then(|v| v.as_str()) {
        match c {
            "pure" => return StepClass::Pure,
            "read" => return StepClass::Read,
            "idempotent" => return StepClass::Idempotent,
            "effect" => return StepClass::Effect,
            _ => {}
        }
    }
    match &task.task {
        Task::Set(_) | Task::Switch(_) | Task::Do(_) | Task::For(_) | Task::Fork(_) | Task::Try(_) | Task::Raise(_) | Task::Wait(_) => {
            StepClass::Pure
        }
        Task::Listen(_) => StepClass::Read,
        Task::Call(c) => match &c.target {
            CallTarget::Http(h) if h.method.as_deref().map(|m| m.eq_ignore_ascii_case("get")).unwrap_or(false) => StepClass::Read,
            _ => StepClass::Effect,
        },
        _ => StepClass::Effect,
    }
}

fn is_container(task: &NamedTask) -> bool {
    matches!(task.task, Task::Do(_) | Task::For(_) | Task::Fork(_) | Task::Try(_))
}

fn hash_of(v: &impl Hash) -> String {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    v.hash(&mut h);
    format!("{:016x}", h.finish())
}

fn without_principal(context: &Value) -> Value {
    let mut c = context.clone();
    if let Some(o) = c.as_object_mut() {
        o.remove("_principal");
    }
    c
}

/// `FLOWR_KILL_AT=ns/flow:point:key,…` — where a crash test stops the worker
/// dead (spec §12). Points: `before_step_started`, `after_step_started`,
/// `after_step` (ran, result not recorded), `after_completed`.
fn kill_points(namespace: &str, name: &str) -> Vec<(String, String)> {
    let flow = format!("{namespace}/{name}:");
    std::env::var("FLOWR_KILL_AT")
        .unwrap_or_default()
        .split(',')
        .filter_map(|k| k.trim().strip_prefix(flow.as_str()))
        .filter_map(|rest| rest.split_once(':'))
        .map(|(p, k)| (p.to_string(), k.to_string()))
        .collect()
}

fn fenced(e: StoreError) -> RuntimeError {
    RuntimeError::TaskFailed { task: "journal".into(), message: format!("run journal: {e}") }
}

impl JournalHook {
    /// Load the run's journal: what already happened decides what replays.
    pub(crate) async fn load(engine: Arc<dyn EngineStore>, run: RunId, epoch: Epoch, namespace: &str, name: &str)
        -> std::result::Result<Self, StoreError> {
        let mut prior = HashMap::new();
        let mut attempts = HashMap::new();
        let mut last_context = json!({});
        let mut pinned = None;
        for e in engine.read_from(run, 0).await? {
            match e.record {
                Record::RunStarted { version, .. } if !version.is_empty() => pinned = Some(version),
                Record::StepStarted { step_key, attempt, .. } => {
                    attempts.insert(step_key.clone(), attempt);
                    prior.entry(step_key).or_insert(Prior::Started { attempt });
                }
                Record::StepCompleted { step_key, output, context, directive, .. } => {
                    let context = (!context.is_null()).then_some(context);
                    if let Some(c) = &context {
                        last_context = c.clone();
                    }
                    prior.insert(step_key, Prior::Completed { output, context, directive });
                }
                Record::StepFailed { step_key, error, .. } => {
                    let error = error.as_str().map(str::to_string).unwrap_or_else(|| error.to_string());
                    prior.insert(step_key, Prior::Failed { error });
                }
                _ => {}
            }
        }
        Ok(Self {
            engine,
            run,
            epoch,
            prefix: format!("{namespace}/{name}"),
            prior: Mutex::new(prior),
            attempts: Mutex::new(attempts),
            pending: Mutex::new(Vec::new()),
            last_context: Mutex::new(last_context),
            pinned,
            kill_at: if epoch == 1 { kill_points(namespace, name) } else { Vec::new() },
            killed: std::sync::atomic::AtomicBool::new(false),
        })
    }

    /// Whether the run's start is not yet recorded.
    pub(crate) fn is_fresh(&self) -> bool {
        self.pinned.is_none() && self.prior.lock().unwrap().is_empty()
    }

    /// Whether a crash-test kill point stopped this run: the worker must act
    /// as a dead process — no further writes, no release.
    pub(crate) fn killed(&self) -> bool {
        self.killed.load(std::sync::atomic::Ordering::SeqCst)
    }

    fn kill(&self, point: &str, key: &str) -> Result<()> {
        if self.kill_at.iter().any(|(p, k)| p == point && k == key) {
            self.killed.store(true, std::sync::atomic::Ordering::SeqCst);
            return Err(RuntimeError::TaskFailed { task: key.to_string(), message: format!("killed at {point} (crash test)") });
        }
        Ok(())
    }

    /// The version the run started on; a resume runs exactly that definition.
    pub(crate) fn pinned_version(&self) -> Option<String> {
        self.pinned.clone()
    }

    fn key<'a>(&self, path: &'a str) -> &'a str {
        path.strip_prefix(self.prefix.as_str()).filter(|k| !k.is_empty()).unwrap_or(path)
    }

    fn take_pending(&self) -> Vec<Record> {
        std::mem::take(&mut *self.pending.lock().unwrap())
    }

    /// Commit buffered pure records and `records` (with optional outbox rows).
    async fn commit(&self, records: Vec<Record>, outbox: Vec<(String, Value)>) -> Result<()> {
        let mut all = self.take_pending();
        all.extend(records);
        if all.is_empty() && outbox.is_empty() {
            return Ok(());
        }
        self.engine
            .commit_step(self.run, self.epoch, StepCommit { records: all, checkpoint: None, outbox })
            .await
            .map(|_| ())
            .map_err(fenced)
    }

    /// Record the start of the run (its pinned version), and its end.
    pub(crate) async fn record(&self, rec: Record) -> std::result::Result<(), StoreError> {
        let mut all = self.take_pending();
        all.push(rec);
        self.engine.append(self.run, self.epoch, &all).await.map(|_| ())
    }
}

#[async_trait]
impl StepHook for JournalHook {
    async fn before_step(&self, path: &str, task: &NamedTask, input: &Value) -> Result<StepReplay> {
        let key = self.key(path).to_string();
        let class = class_of(task);
        let prior = self.prior.lock().unwrap().remove(&key);
        match prior {
            Some(Prior::Completed { output, context, directive }) => {
                if let Some(c) = &context {
                    *self.last_context.lock().unwrap() = c.clone();
                }
                return Ok(StepReplay::Completed { output, context, directive });
            }
            Some(Prior::Failed { error }) => return Ok(StepReplay::Failed { error }),
            Some(Prior::Started { attempt }) => {
                let policy = task.metadata.as_ref().and_then(|m| m.pointer("/a3t/inDoubt")).and_then(|v| v.as_str()).unwrap_or("fault");
                let rerun = matches!(class, StepClass::Pure | StepClass::Read)
                    || (class == StepClass::Idempotent && policy == "retry");
                if !rerun {
                    let error = format!(
                        "in-doubt side effect: step {key} started but did not record a result before the run was interrupted; \
                         it may or may not have taken effect"
                    );
                    self.commit(
                        vec![Record::StepFailed { step_key: key, attempt, error: Value::String(error.clone()), retryable: false, duration_us: 0 }],
                        vec![],
                    )
                    .await?;
                    return Ok(StepReplay::Failed { error });
                }
                self.attempts.lock().unwrap().insert(key.clone(), attempt + 1);
            }
            None => {}
        }
        if class != StepClass::Pure {
            self.kill("before_step_started", &key)?;
            let attempt = *self.attempts.lock().unwrap().get(&key).unwrap_or(&0);
            let started = Record::StepStarted {
                idem_key: format!("{}{}", self.run, key),
                input_hash: hash_of(&input.to_string()),
                step_key: key,
                attempt,
            };
            // Durable before the step runs.
            let k = key_of(&started);
            self.commit(vec![started], vec![]).await?;
            self.kill("after_step_started", &k)?;
        }
        Ok(StepReplay::Run)
    }

    async fn after_step(&self, path: &str, task: &NamedTask, _input: &Value, outcome: StepOutcome<'_>, duration_us: u64) -> Result<()> {
        let key = self.key(path).to_string();
        self.kill("after_step", &key)?;
        let class = class_of(task);
        let attempt = *self.attempts.lock().unwrap().get(&key).unwrap_or(&0);
        let rec = match outcome {
            StepOutcome::Completed { output, context, directive } => {
                let now = without_principal(context);
                let context = {
                    let mut last = self.last_context.lock().unwrap();
                    if *last == now && !is_container(task) {
                        Value::Null
                    } else {
                        *last = now.clone();
                        now
                    }
                };
                Record::StepCompleted { step_key: key, attempt, output: output.clone(), context, class, duration_us, directive: directive.to_string() }
            }
            StepOutcome::Failed { error } => {
                Record::StepFailed { step_key: key, attempt, error: Value::String(error.to_string()), retryable: false, duration_us }
            }
        };
        if class == StepClass::Pure && matches!(rec, Record::StepCompleted { .. }) {
            let flush = {
                let mut p = self.pending.lock().unwrap();
                p.push(rec);
                p.len() >= MAX_PENDING
            };
            if flush {
                self.commit(vec![], vec![]).await?;
            }
            return Ok(());
        }
        let k = key_of(&rec);
        self.commit(vec![rec], vec![]).await?;
        self.kill("after_completed", &k)
    }
}

fn key_of(r: &Record) -> String {
    r.step_key().unwrap_or_default().to_string()
}
