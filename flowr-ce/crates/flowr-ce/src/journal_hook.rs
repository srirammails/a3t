//! The run journal as a step hook (`.ai/FLOWR_QUEUE_CHECKPOINT_SPEC.md` §5.4–5.9,
//! §5.17; plan stages 4–5).
//!
//! Every task of a durable run is recorded; a run that resumes after its worker
//! died replays from the journal, so completed steps are not executed again.
//!
//! - **Pure** steps (`set`, `switch`, containers, short waits) are recorded when
//!   they complete, batched into the next commit — no extra disk sync.
//! - **Other** steps (calls, `run`, `emit`, `listen`) write `StepStarted`
//!   durably *before* they run, then their completion. A step that started but
//!   never completed is in doubt: read steps run again; an idempotent step whose
//!   `metadata.a3t.inDoubt` is `retry` runs again under the same idempotency key;
//!   anything else faults with `in-doubt side effect` (the default), which the
//!   flow's `try`/`catch` can handle.
//! - **Waits** longer than `FLOWR_INLINE_WAIT_MAX_MS` (5 s) are durable timers:
//!   the run lets go and is woken when the timer fires, on any worker.
//! - **`listen`** opens a wait in the store and the run lets go; a matching
//!   event (from `emit` or `send_event`) wakes it on any worker, and the step
//!   outputs the events received. A `timeout` on the step closes the wait and
//!   fails the step (catchable) if nothing came in time.
//! - **Agents** (`call:a3t:agent`, fine mode, spec §5.12) record each model
//!   call and tool call as an effect under `<step>/fx/…`. A resumed agent
//!   replays recorded ones and continues from the first unrecorded call; an
//!   agent step that was interrupted runs again (it is never in doubt as a
//!   whole), and a tool call that started but never finished is reported to
//!   the model as in doubt instead of being repeated.
//! - **Approvals** an agent asks for are durable waits: the question is
//!   recorded, the run lets go, and the answer (or its expiry) wakes it.
//! - **Debug runs** pause at breakpoints (before / after / on error, with an
//!   optional condition on the step input and a hit count) and on stepping
//!   commands; a pause is durable — the run lets go until a command arrives.
//!   An input edited while paused before a step is used for that step.
//! - Context is recorded only when it changed (null otherwise); containers
//!   always record it, since replaying one skips its inner steps.
//! - Every write carries the run's epoch: a worker that lost the run is fenced.
//!
//! Stopping — a pause, a timer, a lost run, a crash-test kill — raises
//! `RuntimeError::Halted`, which `try`/`catch` never catches, and nothing is
//! written after it. Values are journaled as they are (replay needs them
//! exactly) and masked when read.

use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde_json::{json, Value};

use flowr_storage::engine::*;
use swf_parser::{CallTarget, NamedTask, Task};
use swf_runtime::error::{Result, RuntimeError};
use swf_runtime::effects::{Answer, EffectKind, EffectReplay};
use swf_runtime::{StepHook, StepOutcome, StepReplay};

/// Pure records are flushed at the latest after this many, or this long — so
/// a live view of the run keeps up without a disk sync per pure step.
const MAX_PENDING: usize = 64;
const MAX_PENDING_AGE: std::time::Duration = std::time::Duration::from_millis(250);

enum Prior {
    Completed { output: Value, context: Option<Value>, directive: String },
    Failed { error: String },
    Started { attempt: u32 },
}

/// Why the run stopped at a step without finishing.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Halt {
    /// A debug pause: wait for a command.
    Paused,
    /// A durable timer: wake at this time.
    Timer(DateTime<Utc>),
    /// Waiting for events or for a person's answer; wake at the wait's
    /// timeout, if it has one.
    Wait(Option<DateTime<Utc>>),
    /// The run belongs to another worker now, or the journal could not be written.
    Lost,
    /// Crash test: act as a dead process.
    Killed,
}

/// How a debug run continues from its last pause.
#[derive(Debug, Clone, PartialEq)]
enum Mode {
    /// Run to the next breakpoint.
    Continue,
    /// Pause before the very next step.
    StepInto,
    /// Pause before the next step outside this one.
    StepOver(String),
    /// Pause before the next step outside this one's parent.
    StepOut(String),
    /// Pause before this step.
    RunTo(String),
}

#[derive(Debug, Clone)]
struct Breakpoint {
    step_key: String,
    when: String,
    condition: Option<String>,
    hit: Option<u32>,
}

pub(crate) struct JournalHook {
    engine: Arc<dyn EngineStore>,
    run: RunId,
    epoch: Epoch,
    /// `namespace/name`: task paths are reported under it.
    prefix: String,
    prior: Mutex<HashMap<String, Prior>>,
    attempts: Mutex<HashMap<String, u32>>,
    /// Durable timers already set, by step key.
    timers: Mutex<HashMap<String, DateTime<Utc>>>,
    /// Event waits opened (when), and the events delivered to them, by wait id.
    waits: Mutex<HashMap<String, DateTime<Utc>>>,
    events: Mutex<HashMap<String, Value>>,
    /// The journal sequence of each step's last `StepStarted`.
    started_seq: Mutex<HashMap<String, u64>>,
    /// Questions put to a person, by key: (request id, expiry, journal seq).
    asked: Mutex<HashMap<String, (String, DateTime<Utc>, u64)>>,
    answers: Mutex<HashMap<String, swf_runtime::effects::Answer>>,
    pending: Mutex<Vec<Record>>,
    pending_since: Mutex<Option<std::time::Instant>>,
    /// The context after the last recorded step.
    last_context: Mutex<Value>,
    /// The definition version recorded when the run first started.
    pinned: Option<String>,
    // Debug runs
    debug: bool,
    breakpoints: Vec<Breakpoint>,
    hits: Mutex<HashMap<usize, u32>>,
    /// The pause this attempt resumes from — not paused at again.
    resumed_from: Mutex<Option<(String, String)>>,
    mode: Mutex<Mode>,
    edits: Mutex<HashMap<String, Value>>,
    // Stopping
    halt: Mutex<Option<Halt>>,
    stopped: AtomicBool,
    /// Crash-test kill points for this run (`FLOWR_KILL_AT`), first attempt only.
    kill_at: Vec<(String, String)>,
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
        // Its event id is the same on every attempt and the store drops
        // repeats, so sending it again is safe.
        Task::Emit(_) => StepClass::Idempotent,
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

fn wait_of(task: &NamedTask) -> Option<std::time::Duration> {
    match &task.task {
        Task::Wait(w) => Some(std::time::Duration::from_secs(
            w.seconds.unwrap_or(0) + w.minutes.unwrap_or(0) * 60 + w.hours.unwrap_or(0) * 3600 + w.days.unwrap_or(0) * 86400,
        )),
        _ => None,
    }
}

fn inline_wait_max() -> std::time::Duration {
    std::time::Duration::from_millis(std::env::var("FLOWR_INLINE_WAIT_MAX_MS").ok().and_then(|v| v.parse().ok()).unwrap_or(5_000))
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

/// A step key as a breakpoint names it: loop and branch indices, revisit
/// counts and `try`/`catch`/`for`/`fork` scopes left out (`/loop/for[3]/send#2`
/// → `/loop/send`), so a breakpoint on a node in the editor matches every
/// iteration.
pub(crate) fn node_key(key: &str) -> String {
    let mut out = String::new();
    for seg in key.split('/').filter(|s| !s.is_empty()) {
        let seg = seg.split('#').next().unwrap_or(seg);
        let base = seg.split('[').next().unwrap_or(seg);
        if seg.contains('[') && matches!(base, "for" | "fork") {
            continue;
        }
        if matches!(seg, "try" | "catch") {
            continue;
        }
        out.push('/');
        out.push_str(seg);
    }
    out
}

fn bp_matches(bp_key: &str, key: &str) -> bool {
    if bp_key == "*" || bp_key == key {
        return true;
    }
    let node = node_key(key);
    if bp_key.starts_with('/') {
        node_key(bp_key) == node
    } else {
        node.rsplit('/').next() == Some(bp_key)
    }
}

fn is_under(key: &str, scope: &str) -> bool {
    key.len() > scope.len() && key.starts_with(scope) && key.as_bytes()[scope.len()] == b'/'
}

fn parent_of(key: &str) -> String {
    key.rsplit_once('/').map(|(p, _)| p.to_string()).unwrap_or_default()
}

fn parse_breakpoints(v: Option<&Value>) -> Vec<Breakpoint> {
    v.and_then(|v| v.as_array())
        .map(|a| {
            a.iter()
                .filter(|b| b.get("enabled").and_then(|e| e.as_bool()).unwrap_or(true))
                .filter_map(|b| {
                    Some(Breakpoint {
                        step_key: b.get("step_key")?.as_str()?.to_string(),
                        when: b.get("when").and_then(|w| w.as_str()).unwrap_or("before").to_string(),
                        condition: b.get("condition").and_then(|c| c.as_str()).filter(|c| !c.is_empty()).map(String::from),
                        hit: b.get("hit").and_then(|h| h.as_u64()).map(|h| h as u32),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

fn mode_of(command: &Value, paused_at: &str) -> Mode {
    let name = command.get("command").and_then(|c| c.as_str()).or_else(|| command.as_str()).unwrap_or("continue");
    match name {
        "step_into" => Mode::StepInto,
        "step_over" => Mode::StepOver(paused_at.to_string()),
        "step_out" => Mode::StepOut(paused_at.to_string()),
        "run_to" => command.get("step_key").and_then(|k| k.as_str()).map(|k| Mode::RunTo(k.to_string())).unwrap_or(Mode::Continue),
        _ => Mode::Continue,
    }
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

/// An agent step (`call: a3t:agent`): its model and tool calls are journaled
/// one by one.
fn is_agent(task: &NamedTask) -> bool {
    matches!(&task.task, Task::Call(c) if matches!(&c.target, CallTarget::Function(f) if f == "a3t:agent"))
}

/// What a `listen` step waits for, as the store matches it: `{mode, filters}`,
/// each filter the attributes an event must carry. Runtime expressions in
/// filter values (`${ .orderId }`) are evaluated against the step input.
fn correlation_of(listen: &swf_parser::ListenTask, input: &Value, context: &Value) -> std::result::Result<Value, String> {
    let to = &listen.to;
    let (mode, filters) = if let Some(one) = &to.one {
        ("one", vec![one.clone()])
    } else if let Some(all) = &to.all {
        ("all", all.clone())
    } else if let Some(any) = &to.any {
        ("any", any.clone())
    } else {
        return Err("listen needs `to.one`, `to.all` or `to.any`".into());
    };
    let engine = flowr_core::ExpressionEngine::new();
    let scope = flowr_core::Scope::builder().context(context.clone()).build();
    let filters = filters
        .into_iter()
        .map(|f| {
            let with = serde_json::to_value(&f.with).map_err(|e| e.to_string())?;
            resolved(&engine, &scope, &with, input)
        })
        .collect::<std::result::Result<Vec<_>, String>>()?;
    let mut correlation = json!({ "mode": mode, "filters": filters });
    // L38: events sent since the run started, before it reached this step,
    // count too (unless FLOWR_LISTEN_SINCE=open: only after the wait opens).
    if std::env::var("FLOWR_LISTEN_SINCE").map_or(true, |v| v != "open") {
        correlation["since"] = json!("run");
    }
    Ok(correlation)
}

fn resolved(engine: &flowr_core::ExpressionEngine, scope: &flowr_core::Scope, v: &Value, input: &Value) -> std::result::Result<Value, String> {
    Ok(match v {
        Value::String(s) if s.trim_start().starts_with("${") && s.trim_end().ends_with('}') => {
            engine.eval(input, s, scope).map_err(|e| format!("listen filter `{s}`: {e}"))?
        }
        Value::Object(m) => Value::Object(
            m.iter().filter(|(_, x)| !x.is_null()).map(|(k, x)| Ok((k.clone(), resolved(engine, scope, x, input)?))).collect::<std::result::Result<_, String>>()?,
        ),
        Value::Array(a) => Value::Array(a.iter().map(|x| resolved(engine, scope, x, input)).collect::<std::result::Result<_, _>>()?),
        other => other.clone(),
    })
}

/// A step input as journaled for people: values over 64 KiB are replaced by
/// their size (replay never reads inputs).
fn shown(input: &Value) -> Value {
    let size = input.to_string().len();
    if size > 64 * 1024 {
        json!({ "_truncated": true, "bytes": size })
    } else {
        input.clone()
    }
}

fn halted(why: &str) -> RuntimeError {
    RuntimeError::Halted(why.to_string())
}

impl JournalHook {
    /// Load the run's journal: what already happened decides what replays.
    pub(crate) async fn load(engine: Arc<dyn EngineStore>, row: &RunRow, epoch: Epoch) -> std::result::Result<Self, StoreError> {
        let run = row.run_id;
        let mut prior = HashMap::new();
        let mut attempts = HashMap::new();
        let mut timers = HashMap::new();
        let mut waits = HashMap::new();
        let mut events = HashMap::new();
        let mut started_seq = HashMap::new();
        let mut asked: HashMap<String, (String, DateTime<Utc>, u64)> = HashMap::new();
        let mut answers = HashMap::new();
        let mut last_context = json!({});
        let mut pinned = None;
        let mut paused: Option<(String, String)> = None;
        let mut mode = Mode::Continue;
        let mut resumed_from = None;
        let mut edits = HashMap::new();
        for e in engine.read_from(run, 0).await? {
            match e.record {
                Record::RunStarted { version, .. } if !version.is_empty() => pinned = Some(version),
                Record::StepStarted { step_key, attempt, .. } => {
                    started_seq.insert(step_key.clone(), e.seq);
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
                Record::TimerSet { wait_id, fire_at } => {
                    timers.insert(wait_id, fire_at);
                }
                Record::WaitForEvents { wait_id, .. } => {
                    waits.insert(wait_id, e.at);
                }
                Record::EventReceived { wait_id, events: got } => {
                    events.insert(wait_id, got);
                }
                Record::ApprovalRequested { wait_id, request } => {
                    if let Some(k) = request.get("step_key").and_then(|k| k.as_str()) {
                        let expires = request
                            .get("expires_at")
                            .and_then(|t| t.as_str())
                            .and_then(|t| DateTime::parse_from_rfc3339(t).ok())
                            .map(|t| t.with_timezone(&Utc))
                            .unwrap_or(DateTime::<Utc>::MAX_UTC);
                        asked.insert(k.to_string(), (wait_id, expires, e.seq));
                    }
                }
                Record::ApprovalDecided { wait_id, decision, by } => {
                    if let Some(k) = asked.iter().find(|(_, (id, _, _))| *id == wait_id).map(|(k, _)| k.clone()) {
                        let answer = if decision.get("expired").and_then(|v| v.as_bool()) == Some(true) {
                            swf_runtime::effects::Answer::Expired { request_id: wait_id }
                        } else {
                            swf_runtime::effects::Answer::Given { request_id: wait_id, decision, by }
                        };
                        answers.insert(k, answer);
                    }
                }
                Record::Paused { step_key, when, .. } => {
                    paused = Some((step_key, when));
                    edits.clear();
                    resumed_from = None;
                }
                Record::DebugEdit { step_key, input, .. } => {
                    edits.insert(step_key, input);
                }
                Record::DebugCommand { command, .. } => {
                    if let Some((k, w)) = paused.take() {
                        mode = mode_of(&command, &k);
                        resumed_from = Some((k, w));
                    }
                }
                _ => {}
            }
        }
        Ok(Self {
            engine,
            run,
            epoch,
            prefix: format!("{}/{}", row.namespace, row.flow),
            prior: Mutex::new(prior),
            attempts: Mutex::new(attempts),
            timers: Mutex::new(timers),
            waits: Mutex::new(waits),
            events: Mutex::new(events),
            started_seq: Mutex::new(started_seq),
            asked: Mutex::new(asked),
            answers: Mutex::new(answers),
            pending: Mutex::new(Vec::new()),
            pending_since: Mutex::new(None),
            last_context: Mutex::new(last_context),
            pinned,
            debug: row.debug,
            breakpoints: if row.debug { parse_breakpoints(row.breakpoints.as_ref()) } else { Vec::new() },
            hits: Mutex::new(HashMap::new()),
            resumed_from: Mutex::new(resumed_from),
            mode: Mutex::new(mode),
            edits: Mutex::new(edits),
            halt: Mutex::new(None),
            stopped: AtomicBool::new(false),
            kill_at: if epoch == 1 { kill_points(&row.namespace, &row.flow) } else { Vec::new() },
        })
    }

    /// Whether the run's start is not yet recorded.
    pub(crate) fn is_fresh(&self) -> bool {
        self.pinned.is_none() && self.prior.lock().unwrap().is_empty()
    }

    /// The version the run started on; a resume runs exactly that definition.
    pub(crate) fn pinned_version(&self) -> Option<String> {
        self.pinned.clone()
    }

    /// Why the run stopped without finishing, if it did.
    pub(crate) fn halt(&self) -> Option<Halt> {
        self.halt.lock().unwrap().clone()
    }

    fn stop(&self, why: Halt) -> RuntimeError {
        let text = format!("{why:?}");
        self.halt.lock().unwrap().get_or_insert(why);
        self.stopped.store(true, Ordering::SeqCst);
        halted(&text)
    }

    fn check_stopped(&self) -> Result<()> {
        if self.stopped.load(Ordering::SeqCst) {
            return Err(halted("stopped"));
        }
        Ok(())
    }

    fn kill(&self, point: &str, key: &str) -> Result<()> {
        if self.kill_at.iter().any(|(p, k)| p == point && k == key) {
            return Err(self.stop(Halt::Killed));
        }
        Ok(())
    }

    fn key<'a>(&self, path: &'a str) -> &'a str {
        path.strip_prefix(self.prefix.as_str()).filter(|k| !k.is_empty()).unwrap_or(path)
    }

    fn take_pending(&self) -> Vec<Record> {
        *self.pending_since.lock().unwrap() = None;
        std::mem::take(&mut *self.pending.lock().unwrap())
    }

    /// Commit buffered pure records and `records`.
    async fn commit(&self, records: Vec<Record>) -> Result<()> {
        self.check_stopped()?;
        let mut all = self.take_pending();
        all.extend(records);
        if all.is_empty() {
            return Ok(());
        }
        match self.engine.commit_step(self.run, self.epoch, StepCommit { records: all, checkpoint: None, outbox: vec![] }).await {
            Ok(_) => Ok(()),
            Err(e) => {
                tracing::warn!(run = %self.run, error = %e, "run journal write refused; stopping the run here");
                Err(self.stop(Halt::Lost))
            }
        }
    }

    /// Record the start of the run (its pinned version), and its end.
    pub(crate) async fn record(&self, rec: Record) -> std::result::Result<(), StoreError> {
        let mut all = self.take_pending();
        all.push(rec);
        self.engine.append(self.run, self.epoch, &all).await.map(|_| ())
    }

    /// Count hits for a step that is not checked for a pause this attempt
    /// (replayed, or the pause being resumed from), so hit counts carry across
    /// resumes — every attempt replays the same steps in the same order.
    fn count_hits(&self, key: &str, when: &str, input: &Value) {
        if self.debug {
            let _ = self.breakpoint(key, when, input);
        }
    }

    /// Breakpoints matching `key` at `when`, counting hits; whether to pause.
    fn breakpoint(&self, key: &str, when: &str, input: &Value) -> Option<String> {
        let mut fired = None;
        for (i, bp) in self.breakpoints.iter().enumerate() {
            if bp.when != when || !bp_matches(&bp.step_key, key) {
                continue;
            }
            if let Some(cond) = &bp.condition {
                let engine = flowr_core::ExpressionEngine::new();
                let scope = flowr_core::Scope::builder().build();
                if engine.eval(input, cond, &scope).ok() != Some(Value::Bool(true)) {
                    continue;
                }
            }
            let n = {
                let mut hits = self.hits.lock().unwrap();
                let h = hits.entry(i).or_insert(0);
                *h += 1;
                *h
            };
            // Every matching breakpoint counts this hit; the first that fires wins.
            if bp.hit.is_none_or(|want| want == n) && fired.is_none() {
                fired = Some(format!("breakpoint {when} {}", bp.step_key));
            }
        }
        fired
    }

    /// The pause this attempt resumed from is not paused at again.
    fn resuming_from(&self, key: &str, when: &str) -> bool {
        let mut r = self.resumed_from.lock().unwrap();
        if r.as_ref().is_some_and(|(k, w)| k == key && w == when) {
            *r = None;
            return true;
        }
        false
    }

    async fn pause(&self, key: &str, when: &str, reason: String, input: &Value) -> RuntimeError {
        let rec = Record::Paused { step_key: key.to_string(), when: when.to_string(), reason, input: shown(input) };
        match self.commit(vec![rec]).await {
            Ok(()) => self.stop(Halt::Paused),
            Err(e) => e,
        }
    }

    /// A `listen` step: open its wait and let go, or finish with the events
    /// delivered — or with a timeout once its `timeout` has passed.
    async fn listen(&self, key: &str, task: &NamedTask, listen: &swf_parser::ListenTask, input: &Value) -> Result<StepReplay> {
        let wait_id = key.to_string();
        let now = Utc::now();
        let opened = self.waits.lock().unwrap().get(&wait_id).copied();
        let waited = |since: Option<DateTime<Utc>>| since.map_or(0, |t| (now - t).num_microseconds().unwrap_or(0).max(0) as u64);
        let delivered = self.events.lock().unwrap().remove(&wait_id);
        if let Some(events) = delivered {
            let envelope = listen.read.as_deref() == Some("envelope");
            let output = match events {
                Value::Array(list) if !envelope => Value::Array(list.into_iter().map(|e| e.get("data").cloned().unwrap_or(Value::Null)).collect()),
                other => other,
            };
            let rec = Record::StepCompleted {
                step_key: key.to_string(),
                attempt: 0,
                output: output.clone(),
                context: Value::Null,
                class: StepClass::Read,
                duration_us: waited(opened),
                directive: "continue".into(),
                input: shown(input),
                at: Some(now),
            };
            self.commit(vec![rec]).await?;
            return Ok(StepReplay::Completed { output, context: None, directive: "continue".into() });
        }
        let limit = task.timeout.as_ref().and_then(|t| t.duration()).map(|d| chrono::Duration::from_std(d).unwrap_or(chrono::Duration::MAX));
        match opened {
            Some(at) => {
                let deadline = limit.map(|l| at + l);
                if deadline.is_some_and(|d| d <= now) {
                    self.commit(vec![]).await?;
                    if self.engine.close_wait(self.run, self.epoch, &wait_id).await.is_err() {
                        return Err(self.stop(Halt::Lost));
                    }
                    let error = format!("timeout: task '{}' got no matching event within {}s", task.name, limit.unwrap_or_default().num_seconds());
                    let rec = Record::StepFailed { step_key: key.to_string(), attempt: 0, error: Value::String(error.clone()), retryable: false, duration_us: waited(opened), input: shown(input), at: Some(now) };
                    self.commit(vec![rec]).await?;
                    return Ok(StepReplay::Failed { error });
                }
                Err(self.stop(Halt::Wait(deadline)))
            }
            None => {
                let context = self.last_context.lock().unwrap().clone();
                let correlation = correlation_of(listen, input, &context)
                    .map_err(|e| RuntimeError::task_failed(&task.name, e))?;
                self.commit(vec![]).await?;
                self.kill("before_step_started", key)?;
                if let Err(e) = self.engine.open_wait(self.run, self.epoch, &wait_id, correlation).await {
                    tracing::warn!(run = %self.run, error = %e, "could not open the event wait; stopping the run here");
                    return Err(self.stop(Halt::Lost));
                }
                self.waits.lock().unwrap().insert(wait_id, now);
                Err(self.stop(Halt::Wait(limit.map(|l| now + l))))
            }
        }
    }

    /// A debug run: pause before `key`?
    fn pause_before(&self, key: &str, input: &Value) -> Option<String> {
        if self.resuming_from(key, "before") {
            self.count_hits(key, "before", input);
            return None;
        }
        let stepped = match &*self.mode.lock().unwrap() {
            Mode::Continue => false,
            Mode::StepInto => true,
            Mode::StepOver(at) => !is_under(key, at) && key != at,
            Mode::StepOut(at) => !is_under(key, &parent_of(at)),
            Mode::RunTo(target) => bp_matches(target, key),
        };
        if stepped {
            return Some("step".into());
        }
        self.breakpoint(key, "before", input)
    }
}

#[async_trait]
impl StepHook for JournalHook {
    async fn before_step(&self, path: &str, task: &NamedTask, input: &Value) -> Result<StepReplay> {
        self.check_stopped()?;
        let key = self.key(path).to_string();
        let class = class_of(task);
        let prior = self.prior.lock().unwrap().remove(&key);
        match prior {
            Some(Prior::Completed { output, context, directive }) => {
                if let Some(c) = &context {
                    *self.last_context.lock().unwrap() = c.clone();
                }
                self.count_hits(&key, "before", input);
                self.count_hits(&key, "after", input);
                return Ok(StepReplay::Completed { output, context, directive });
            }
            Some(Prior::Failed { error }) => {
                self.count_hits(&key, "before", input);
                self.count_hits(&key, "on_error", input);
                return Ok(StepReplay::Failed { error });
            }
            Some(Prior::Started { attempt }) => {
                let policy = task.metadata.as_ref().and_then(|m| m.pointer("/a3t/inDoubt")).and_then(|v| v.as_str()).unwrap_or("fault");
                let rerun = matches!(class, StepClass::Pure | StepClass::Read)
                    || (class == StepClass::Idempotent && policy == "retry")
                    // An agent records its own calls (fine mode): running it
                    // again replays them and repeats none.
                    || is_agent(task)
                    // A subflow journals its own steps (L2): running it again
                    // replays them and continues.
                    || matches!(task.task, Task::Run(swf_parser::RunTask::Workflow(_)))
                    || matches!(task.task, Task::Emit(_));
                if !rerun {
                    let error = format!(
                        "in-doubt side effect: step {key} started but did not record a result before the run was interrupted; \
                         it may or may not have taken effect"
                    );
                    self.commit(vec![Record::StepFailed { step_key: key, attempt, error: Value::String(error.clone()), retryable: false, duration_us: 0, input: shown(input), at: Some(chrono::Utc::now()) }])
                        .await?;
                    return Ok(StepReplay::Failed { error });
                }
                self.attempts.lock().unwrap().insert(key.clone(), attempt + 1);
            }
            None => {}
        }

        // Debug: pause here?
        let edited = if self.debug {
            // The stepping mode holds until it pauses (a pause ends this attempt).
            if let Some(reason) = self.pause_before(&key, input) {
                return Err(self.pause(&key, "before", reason, input).await);
            }
            self.edits.lock().unwrap().remove(&key)
        } else {
            None
        };

        // A durable timer: set it, or it has fired. A task timeout shorter
        // than the wait fires first, as a (catchable) timeout.
        if let Some(d) = wait_of(task).filter(|d| *d > inline_wait_max()) {
            let limit = task.timeout.as_ref().and_then(|t| t.duration()).filter(|l| *l < d);
            let d = limit.unwrap_or(d);
            let set = self.timers.lock().unwrap().get(&key).copied();
            let now = Utc::now();
            match set {
                Some(fire_at) if fire_at <= now && limit.is_some() => {
                    let error = format!("timeout: task '{}' took longer than {}s", task.name, d.as_secs());
                    let attempt = *self.attempts.lock().unwrap().get(&key).unwrap_or(&0);
                    self.commit(vec![Record::StepFailed { step_key: key, attempt, error: Value::String(error.clone()), retryable: false, duration_us: d.as_micros() as u64, input: shown(input), at: Some(chrono::Utc::now()) }]).await?;
                    return Ok(StepReplay::Failed { error });
                }
                Some(fire_at) if fire_at <= now => {
                    let waited = now - (fire_at - chrono::Duration::from_std(d).unwrap_or_default());
                    let rec = Record::StepCompleted {
                        step_key: key,
                        attempt: 0,
                        output: input.clone(),
                        context: Value::Null,
                        class: StepClass::Pure,
                        duration_us: waited.num_microseconds().unwrap_or(0).max(0) as u64,
                        directive: "continue".into(),
                        input: shown(input),
                        at: Some(now),
                    };
                    self.commit(vec![rec]).await?;
                    return Ok(StepReplay::Completed { output: input.clone(), context: None, directive: "continue".into() });
                }
                Some(fire_at) => return Err(self.stop(Halt::Timer(fire_at))),
                None => {
                    let fire_at = now + chrono::Duration::from_std(d).unwrap_or(chrono::Duration::MAX);
                    self.commit(vec![Record::TimerSet { wait_id: key.clone(), fire_at }]).await?;
                    self.timers.lock().unwrap().insert(key, fire_at);
                    return Err(self.stop(Halt::Timer(fire_at)));
                }
            }
        }

        if let Task::Listen(listen) = &task.task {
            return self.listen(&key, task, listen, input).await;
        }

        if class != StepClass::Pure {
            self.kill("before_step_started", &key)?;
            let attempt = *self.attempts.lock().unwrap().get(&key).unwrap_or(&0);
            let input_used = edited.as_ref().unwrap_or(input);
            let started = Record::StepStarted {
                idem_key: format!("{}{}", self.run, key),
                input_hash: hash_of(&input_used.to_string()),
                step_key: key.clone(),
                attempt,
            };
            // Durable before the step runs.
            self.commit(vec![started]).await?;
            self.kill("after_step_started", &key)?;
        }
        Ok(match edited {
            Some(input) => StepReplay::RunWith { input },
            None => StepReplay::Run,
        })
    }

    async fn after_step(&self, path: &str, task: &NamedTask, input: &Value, outcome: StepOutcome<'_>, duration_us: u64) -> Result<()> {
        // A halt raised inside this step (a pause or timer in a container,
        // a kill, a lost run) is not this step's result.
        if let StepOutcome::Failed { error: RuntimeError::Halted(_) } = outcome {
            return Ok(());
        }
        self.check_stopped()?;
        let key = self.key(path).to_string();
        self.kill("after_step", &key)?;
        let class = class_of(task);
        let attempt = *self.attempts.lock().unwrap().get(&key).unwrap_or(&0);
        let (rec, when) = match outcome {
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
                (Record::StepCompleted { step_key: key.clone(), attempt, output: output.clone(), context, class, duration_us, directive: directive.to_string(), input: shown(input), at: Some(chrono::Utc::now()) }, "after")
            }
            StepOutcome::Failed { error } => {
                (Record::StepFailed { step_key: key.clone(), attempt, error: Value::String(error.to_string()), retryable: false, duration_us, input: shown(input), at: Some(chrono::Utc::now()) }, "on_error")
            }
        };
        let pause = if self.debug && !self.resuming_from(&key, when) { self.breakpoint(&key, when, input) } else { None };
        if class == StepClass::Pure && matches!(rec, Record::StepCompleted { .. }) && pause.is_none() {
            let flush = {
                let mut p = self.pending.lock().unwrap();
                p.push(rec);
                let since = *self.pending_since.lock().unwrap().get_or_insert_with(std::time::Instant::now);
                p.len() >= MAX_PENDING || since.elapsed() >= MAX_PENDING_AGE
            };
            if flush {
                self.commit(vec![]).await?;
            }
            return Ok(());
        }
        self.commit(vec![rec]).await?;
        self.kill("after_completed", &key)?;
        if let Some(reason) = pause {
            return Err(self.pause(&key, when, reason, input).await);
        }
        Ok(())
    }

    fn idempotency_key(&self, path: &str) -> Option<String> {
        Some(format!("{}{}", self.run, self.key(path)))
    }

    async fn effect_replay(&self, path: &str, name: &str) -> Result<EffectReplay> {
        self.check_stopped()?;
        let fx = format!("{}/fx/{name}", self.key(path));
        let prior = self.prior.lock().unwrap().remove(&fx);
        Ok(match prior {
            Some(Prior::Completed { output, .. }) => EffectReplay::Done(match output.get("err") {
                Some(e) => Err(e.as_str().map(str::to_string).unwrap_or_else(|| e.to_string())),
                None => Ok(output.get("ok").cloned().unwrap_or(Value::Null)),
            }),
            Some(Prior::Failed { error }) => EffectReplay::Done(Err(error)),
            Some(Prior::Started { attempt }) => {
                self.attempts.lock().unwrap().insert(fx.clone(), attempt + 1);
                // Stopped at a question inside it (before the call itself)?
                // Then it never ran: run it now.
                let started = self.started_seq.lock().unwrap().get(&fx).copied().unwrap_or(0);
                let inside = format!("{fx}/");
                let asked_after = self.asked.lock().unwrap().iter().any(|(k, (_, _, seq))| k.starts_with(&inside) && *seq > started);
                if asked_after { EffectReplay::Run } else { EffectReplay::InDoubt }
            }
            None => EffectReplay::Run,
        })
    }

    async fn effect_started(&self, path: &str, name: &str) -> Result<()> {
        let fx = format!("{}/fx/{name}", self.key(path));
        let attempt = *self.attempts.lock().unwrap().get(&fx).unwrap_or(&0);
        let rec = Record::StepStarted { idem_key: format!("{}{fx}", self.run), input_hash: String::new(), step_key: fx.clone(), attempt };
        self.commit(vec![rec]).await?;
        self.kill("after_step_started", &fx)
    }

    async fn effect_done(&self, path: &str, name: &str, kind: EffectKind, result: &std::result::Result<Value, String>, duration_us: u64) -> Result<()> {
        // A failed model call is not kept: running the step again calls it again.
        if kind == EffectKind::Read && result.is_err() {
            return Ok(());
        }
        let fx = format!("{}/fx/{name}", self.key(path));
        let attempt = *self.attempts.lock().unwrap().get(&fx).unwrap_or(&0);
        let output = match result {
            Ok(v) => json!({ "ok": v }),
            Err(e) => json!({ "err": e }),
        };
        let class = if kind == EffectKind::Read { StepClass::Read } else { StepClass::Effect };
        let rec = Record::StepCompleted { step_key: fx.clone(), attempt, output, context: Value::Null, class, duration_us, directive: String::new(), input: Value::Null, at: Some(Utc::now()) };
        self.commit(vec![rec]).await?;
        self.kill("after_completed", &fx)
    }

    fn asked(&self, path: &str, name: &str) -> bool {
        let k = format!("{}/fx/{name}", self.key(path));
        self.asked.lock().unwrap().contains_key(&k) || self.answers.lock().unwrap().contains_key(&k)
    }

    async fn ask(&self, path: &str, name: &str, request: &Value, expires_at: DateTime<Utc>) -> Result<Answer> {
        self.check_stopped()?;
        let k = format!("{}/fx/{name}", self.key(path));
        let answered = self.answers.lock().unwrap().remove(&k);
        if let Some(a) = answered {
            return Ok(a);
        }
        let open = self.asked.lock().unwrap().get(&k).cloned();
        if let Some((request_id, expires, _)) = open {
            if expires > Utc::now() {
                return Err(self.stop(Halt::Wait(Some(expires))));
            }
            // Expired unanswered: close it, unless an answer just came in.
            self.commit(vec![]).await?;
            match self.engine.close_wait(self.run, self.epoch, &request_id).await {
                Ok(true) => {
                    let rec = Record::ApprovalDecided { wait_id: request_id.clone(), decision: json!({ "expired": true }), by: "timeout".into() };
                    self.commit(vec![rec]).await?;
                    return Ok(Answer::Expired { request_id });
                }
                // Answered meanwhile, or the run moved on: run again from the journal.
                _ => return Err(self.stop(Halt::Lost)),
            }
        }
        let Some(request_id) = request.get("request_id").and_then(|v| v.as_str()).map(str::to_string) else {
            return Err(RuntimeError::Internal("a question needs a request_id".into()));
        };
        let mut req = request.clone();
        req["step_key"] = json!(k);
        req["expires_at"] = json!(expires_at.to_rfc3339());
        self.commit(vec![]).await?;
        if let Err(e) = self.engine.open_approval(self.run, self.epoch, &request_id, req).await {
            tracing::warn!(run = %self.run, error = %e, "could not record the approval request; stopping the run here");
            return Err(self.stop(Halt::Lost));
        }
        self.asked.lock().unwrap().insert(k, (request_id, expires_at, u64::MAX));
        Err(self.stop(Halt::Wait(Some(expires_at))))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn breakpoints_name_editor_nodes() {
        assert_eq!(node_key("/loop/for[3]/send#2"), "/loop/send");
        assert_eq!(node_key("/guard/try/charge"), "/guard/charge");
        assert_eq!(node_key("/split/fork[1]/b"), "/split/b");
        assert!(bp_matches("/loop/send", "/loop/for[0]/send"));
        assert!(bp_matches("send", "/loop/for[0]/send"));
        assert!(bp_matches("*", "/a"));
        assert!(!bp_matches("/loop", "/loop/for[0]/send"));
        assert!(is_under("/loop/for[0]/send", "/loop"));
        assert!(!is_under("/loopy", "/loop"));
        assert_eq!(parent_of("/a/b"), "/a");
        assert_eq!(parent_of("/a"), "");
    }
}
