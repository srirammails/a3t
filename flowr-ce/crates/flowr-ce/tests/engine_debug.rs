//! Durable timers and debug runs (`.ai/FLOWR_QUEUE_CHECKPOINT_SPEC.md` §5.9,
//! §5.17; plan stage 5): a long wait lets go of the run and wakes on time, even
//! across a restart; a debug run pauses at breakpoints and stepping commands,
//! durably, and continues on command — with an edited input if one was given.

use std::sync::{Arc, Once};
use std::time::Duration;

use flowr_storage::engine::{NewTask, Record, RunId, RunPhase, RunRequest};
use flowr_storage::StorageBackend;
use flowr_types::{ExecutionRequest, WorkflowRecord};
use serde_json::{json, Value};

static INIT: Once = Once::new();

fn init() {
    INIT.call_once(|| {
        std::env::set_var("FLOWR_RUN_LEASE_MS", "600");
        std::env::set_var("FLOWR_INLINE_WAIT_MAX_MS", "500");
    });
}

const STEPS: &str = r#"
document: { dsl: '1.0.3', namespace: d, name: steps, version: '1.0.0' }
do:
  - a:
      set: { order: 7 }
  - b:
      set: { got: "${ .order }" }
  - c:
      set: { done: true, got: "${ .got }" }
"#;

const TIMER: &str = r#"
document: { dsl: '1.0.3', namespace: d, name: timer, version: '1.0.0' }
do:
  - before:
      set: { started: true }
  - nap:
      wait: { seconds: 2 }
  - after:
      set: { woke: true }
"#;

const GUARDED: &str = r#"
document: { dsl: '1.0.3', namespace: d, name: guarded, version: '1.0.0' }
do:
  - guard:
      try:
        do:
          - x:
              set: { inside: true }
        catch:
          do:
            - recover:
                set: { recovered: true }
"#;

async fn storage(dir: &std::path::Path, worker: bool) -> Arc<dyn StorageBackend> {
    init();
    let s = flowr_ce::init_storage_with(dir.to_str().unwrap(), true).await.unwrap();
    for y in [STEPS, TIMER, GUARDED] {
        s.workflows().register(WorkflowRecord::from_yaml(y).unwrap()).await.unwrap();
    }
    if worker {
        flowr_ce::spawn_worker(s.clone());
    }
    s
}

async fn start(s: &dyn StorageBackend, flow: &str, debug: bool, breakpoints: Value) -> RunId {
    let req = ExecutionRequest::new("d", flow);
    let mut run = flowr_ce::flowr_ce_storage::engine_adapters::new_run(&req);
    run.debug = debug;
    run.breakpoints = debug.then_some(breakpoints);
    let task = NewTask::start(&run);
    s.engine().unwrap().start(run, task).await.unwrap().0
}

async fn until(s: &dyn StorageBackend, id: RunId, phase: RunPhase) -> flowr_storage::engine::RunRow {
    let deadline = std::time::Instant::now() + Duration::from_secs(20);
    loop {
        let r = s.engine().unwrap().get(id).await.unwrap().unwrap();
        if r.phase == phase {
            return r;
        }
        assert!(std::time::Instant::now() < deadline, "waiting for {phase:?}, run is {:?} ({:?})", r.phase, r.error);
        tokio::time::sleep(Duration::from_millis(40)).await;
    }
}

async fn paused_at(s: &dyn StorageBackend, id: RunId) -> (String, String) {
    s.engine()
        .unwrap()
        .read_from(id, 0)
        .await
        .unwrap()
        .into_iter()
        .rev()
        .find_map(|e| match e.record {
            Record::Paused { step_key, when, .. } => Some((step_key, when)),
            _ => None,
        })
        .expect("a pause")
}

async fn command(s: &dyn StorageBackend, id: RunId, cmd: Value) {
    s.engine().unwrap().request(id, RunRequest::Debug { command: cmd, by: "dev".into() }).await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn a_long_wait_lets_go_of_the_run_and_wakes_on_time() {
    let dir = tempfile::tempdir().unwrap();
    let s = storage(dir.path(), true).await;
    let t = std::time::Instant::now();
    let id = start(s.as_ref(), "timer", false, Value::Null).await;
    let waiting = until(s.as_ref(), id, RunPhase::Waiting).await;
    assert!(waiting.owner.is_none(), "no worker holds a waiting run");
    let done = until(s.as_ref(), id, RunPhase::Completed).await;
    assert!(t.elapsed() >= Duration::from_secs(2), "woke on time, not early");
    assert_eq!(done.output.unwrap()["woke"], true);
    let kinds: Vec<String> = s.engine().unwrap().read_from(id, 0).await.unwrap().iter()
        .map(|e| serde_json::to_value(&e.record).unwrap()["kind"].as_str().unwrap().to_string()).collect();
    assert!(kinds.contains(&"timer_set".to_string()), "{kinds:?}");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_timer_survives_a_restart() {
    let dir = tempfile::tempdir().unwrap();
    let id = {
        let s = storage(dir.path(), true).await;
        let id = start(s.as_ref(), "timer", false, Value::Null).await;
        until(s.as_ref(), id, RunPhase::Waiting).await;
        id
        // The process stops while the run waits.
    };
    let s = storage(dir.path(), true).await;
    let done = until(s.as_ref(), id, RunPhase::Completed).await;
    assert_eq!(done.output.unwrap()["woke"], true);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_breakpoint_pauses_durably_and_continue_finishes() {
    let dir = tempfile::tempdir().unwrap();
    let s = storage(dir.path(), true).await;
    let id = start(s.as_ref(), "steps", true, json!([{ "step_key": "/b", "when": "before" }])).await;
    let paused = until(s.as_ref(), id, RunPhase::Suspended).await;
    assert!(paused.owner.is_none(), "a paused run holds no worker");
    assert_eq!(paused_at(s.as_ref(), id).await, ("/b".to_string(), "before".to_string()));
    command(s.as_ref(), id, json!({ "command": "continue" })).await;
    let done = until(s.as_ref(), id, RunPhase::Completed).await;
    assert_eq!(done.output.unwrap()["got"], 7);
}

/// Wait for pause number `n` (1-based) and return where it is.
async fn nth_pause(s: &dyn StorageBackend, id: RunId, n: usize) -> (String, String) {
    let deadline = std::time::Instant::now() + Duration::from_secs(20);
    loop {
        let pauses: Vec<(String, String)> = s.engine().unwrap().read_from(id, 0).await.unwrap().into_iter()
            .filter_map(|e| match e.record { Record::Paused { step_key, when, .. } => Some((step_key, when)), _ => None })
            .collect();
        let row = s.engine().unwrap().get(id).await.unwrap().unwrap();
        if pauses.len() >= n && row.phase == RunPhase::Suspended {
            return pauses[n - 1].clone();
        }
        assert!(std::time::Instant::now() < deadline, "no pause {n}: {pauses:?}, {:?}", row.phase);
        tokio::time::sleep(Duration::from_millis(40)).await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn stepping_pauses_before_each_next_step() {
    let dir = tempfile::tempdir().unwrap();
    let s = storage(dir.path(), true).await;
    let id = start(s.as_ref(), "steps", true, json!([{ "step_key": "*", "when": "before", "hit": 1 }])).await;
    let mut seen = Vec::new();
    for n in 1..=3 {
        seen.push(nth_pause(s.as_ref(), id, n).await.0);
        command(s.as_ref(), id, json!({ "command": "step_over" })).await;
    }
    assert_eq!(seen, ["/a", "/b", "/c"]);
    until(s.as_ref(), id, RunPhase::Completed).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn an_input_edited_while_paused_is_used() {
    let dir = tempfile::tempdir().unwrap();
    let s = storage(dir.path(), true).await;
    let id = start(s.as_ref(), "steps", true, json!([{ "step_key": "b", "when": "before" }])).await;
    let paused = until(s.as_ref(), id, RunPhase::Suspended).await;
    s.engine().unwrap()
        .append(id, paused.epoch, &[Record::DebugEdit { step_key: "/b".into(), input: json!({ "order": 99 }), by: "dev".into() }])
        .await
        .unwrap();
    command(s.as_ref(), id, json!({ "command": "continue" })).await;
    let done = until(s.as_ref(), id, RunPhase::Completed).await;
    assert_eq!(done.output.unwrap()["got"], 99, "b ran with the edited input");
}

#[tokio::test(flavor = "multi_thread")]
async fn after_and_run_to() {
    let dir = tempfile::tempdir().unwrap();
    let s = storage(dir.path(), true).await;
    let id = start(s.as_ref(), "steps", true, json!([{ "step_key": "/a", "when": "after" }])).await;
    until(s.as_ref(), id, RunPhase::Suspended).await;
    assert_eq!(paused_at(s.as_ref(), id).await, ("/a".to_string(), "after".to_string()));
    command(s.as_ref(), id, json!({ "command": "run_to", "step_key": "/c" })).await;
    assert_eq!(nth_pause(s.as_ref(), id, 2).await, ("/c".to_string(), "before".to_string()));
    command(s.as_ref(), id, json!("continue")).await;
    until(s.as_ref(), id, RunPhase::Completed).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_pause_inside_try_is_not_caught() {
    let dir = tempfile::tempdir().unwrap();
    let s = storage(dir.path(), true).await;
    let id = start(s.as_ref(), "guarded", true, json!([{ "step_key": "x", "when": "before" }])).await;
    until(s.as_ref(), id, RunPhase::Suspended).await;
    command(s.as_ref(), id, json!("continue")).await;
    let done = until(s.as_ref(), id, RunPhase::Completed).await;
    let out = done.output.unwrap();
    assert_eq!(out["inside"], true);
    assert!(out.get("recovered").is_none(), "the catch did not run: {out}");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_paused_run_can_be_cancelled() {
    let dir = tempfile::tempdir().unwrap();
    let s = storage(dir.path(), true).await;
    let id = start(s.as_ref(), "steps", true, json!([{ "step_key": "/b" }])).await;
    until(s.as_ref(), id, RunPhase::Suspended).await;
    s.engine().unwrap().request(id, RunRequest::Cancel { by: "dev".into(), reason: "done debugging".into() }).await.unwrap();
    let r = until(s.as_ref(), id, RunPhase::Cancelled).await;
    assert_eq!(r.error.as_deref(), Some("done debugging"));
}

/// Pause on start (`*`, hit 1) pauses once: hit counts carry across resumes.
#[tokio::test(flavor = "multi_thread")]
async fn pause_on_start_then_continue_runs_to_the_end() {
    let dir = tempfile::tempdir().unwrap();
    let s = storage(dir.path(), true).await;
    let id = start(s.as_ref(), "steps", true, json!([{ "step_key": "*", "when": "before", "hit": 1 }])).await;
    assert_eq!(nth_pause(s.as_ref(), id, 1).await.0, "/a");
    command(s.as_ref(), id, json!({ "command": "continue" })).await;
    let done = until(s.as_ref(), id, RunPhase::Completed).await;
    assert_eq!(done.output.unwrap()["got"], 7);
}

/// A hit count is counted over the whole run, not per resume.
#[tokio::test(flavor = "multi_thread")]
async fn hit_counts_carry_across_resumes() {
    let dir = tempfile::tempdir().unwrap();
    let s = storage(dir.path(), true).await;
    // Pause before /a (hit 1 of *), then the 3rd step overall (/c, hit 3 of *).
    let id = start(s.as_ref(), "steps", true, json!([
        { "step_key": "*", "when": "before", "hit": 1 },
        { "step_key": "*", "when": "before", "hit": 3 }
    ])).await;
    assert_eq!(nth_pause(s.as_ref(), id, 1).await.0, "/a");
    command(s.as_ref(), id, json!("continue")).await;
    assert_eq!(nth_pause(s.as_ref(), id, 2).await.0, "/c");
    command(s.as_ref(), id, json!("continue")).await;
    until(s.as_ref(), id, RunPhase::Completed).await;
}
