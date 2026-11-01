//! Crash tests (`.ai/FLOWR_QUEUE_CHECKPOINT_SPEC.md` §11): a worker stopped dead
//! at a kill point (`FLOWR_KILL_AT`) leaves its lease to run out, exactly like a
//! crashed process; the run is then claimed again and resumes from its journal.
//! Side effects are HTTP calls to a local server that counts them, so "not
//! repeated" is measured, not assumed.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, Once};
use std::time::Duration;

use flowr_storage::engine::Record;
use flowr_storage::StorageBackend;
use flowr_types::{ExecutionRequest, ExecutionState, ExecutionStatus, WorkflowRecord};
use serde_json::{json, Value};

static INIT: Once = Once::new();

fn init() {
    INIT.call_once(|| {
        std::env::set_var("FLOWR_RUN_LEASE_MS", "600");
        std::env::set_var(
            "FLOWR_KILL_AT",
            [
                "c/f2:before_step_started:/charge",
                "c/f3:after_step_started:/charge",
                "c/f4:after_step:/charge",
                "c/f4r:after_step:/charge",
                "c/f4c:after_step:/guard/try/charge",
                "c/f5:after_completed:/charge",
            ]
            .join(","),
        );
    });
}

type Hits = Arc<Mutex<HashMap<String, usize>>>;

async fn effect_server() -> (String, Hits) {
    let hits: Hits = Arc::default();
    let h = hits.clone();
    let app = axum::Router::new().route(
        "/hit/:tag",
        axum::routing::post(move |axum::extract::Path(tag): axum::extract::Path<String>| {
            let h = h.clone();
            async move {
                let mut m = h.lock().unwrap();
                let n = m.entry(tag).or_insert(0);
                *n += 1;
                axum::Json(json!({ "hits": *n }))
            }
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    (format!("http://{addr}"), hits)
}

fn charge_flow(name: &str, base: &str, task_meta: &str) -> String {
    format!(
        r#"
document: {{ dsl: '1.0.3', namespace: c, name: {name}, version: '1.0.0' }}
do:
  - a:
      set: {{ t: "${{ now }}", order: 42 }}
      export: {{ as: "{{t: .t}}" }}
  - charge:
      call: http
      with:
        method: POST
        endpoint: {base}/hit/{name}
        body: {{ order: 42 }}
{task_meta}
  - done:
      set:
        t: "${{ $context.t }}"
        hits: "${{ .body.hits }}"
"#
    )
}

fn guarded_flow(base: &str) -> String {
    format!(
        r#"
document: {{ dsl: '1.0.3', namespace: c, name: f4c, version: '1.0.0' }}
do:
  - guard:
      try:
        do:
          - charge:
              call: http
              with:
                method: POST
                endpoint: {base}/hit/f4c
        catch:
          do:
            - recover:
                set: {{ recovered: true }}
"#
    )
}

const EPHEMERAL: &str = r#"
document:
  dsl: '1.0.3'
  namespace: c
  name: eph
  version: '1.0.0'
metadata:
  a3t: { durability: ephemeral }
do:
  - a:
      set: { ok: true }
"#;

struct World {
    s: Arc<dyn StorageBackend>,
    hits: Hits,
    _dir: tempfile::TempDir,
}

async fn world() -> World {
    world_with(true).await
}

async fn world_with(worker: bool) -> World {
    init();
    let dir = tempfile::tempdir().unwrap();
    let s = flowr_ce::init_storage_with(dir.path().to_str().unwrap(), true).await.unwrap();
    let (base, hits) = effect_server().await;
    let retry = "      metadata: { a3t: { stepClass: idempotent, inDoubt: retry } }";
    let mut flows = vec![guarded_flow(&base), EPHEMERAL.to_string()];
    for (n, meta) in [("f1", ""), ("f2", ""), ("f3", ""), ("f4", ""), ("f4r", retry), ("f5", "")] {
        flows.push(charge_flow(n, &base, meta));
    }
    for y in flows {
        s.workflows().register(WorkflowRecord::from_yaml(&y).unwrap()).await.unwrap();
    }
    if worker {
        flowr_ce::spawn_worker(s.clone());
    }
    World { s, hits, _dir: dir }
}

impl World {
    async fn run(&self, flow: &str) -> ExecutionState {
        let req = ExecutionRequest::new("c", flow);
        let id = req.instance_id;
        self.s.queue().enqueue(req).await.unwrap();
        let deadline = std::time::Instant::now() + Duration::from_secs(20);
        loop {
            let st = self.s.executions().get(id).await.unwrap().unwrap();
            if !matches!(st.status, ExecutionStatus::Queued | ExecutionStatus::Running | ExecutionStatus::Suspended) {
                return st;
            }
            assert!(std::time::Instant::now() < deadline, "{flow}: still {:?}", st.status);
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }

    fn hits(&self, tag: &str) -> usize {
        *self.hits.lock().unwrap().get(tag).unwrap_or(&0)
    }

    async fn journal(&self, id: uuid::Uuid) -> Vec<Record> {
        self.s.engine().unwrap().read_from(id, 0).await.unwrap().into_iter().map(|e| e.record).collect()
    }

    async fn epoch(&self, id: uuid::Uuid) -> u64 {
        self.s.engine().unwrap().get(id).await.unwrap().unwrap().epoch
    }
}

fn out(st: &ExecutionState) -> Value {
    st.output.clone().unwrap_or(Value::Null)
}

#[tokio::test(flavor = "multi_thread")]
async fn a_run_without_a_crash_journals_every_step() {
    let w = world().await;
    let st = w.run("f1").await;
    assert_eq!(st.status, ExecutionStatus::Completed, "{:?}", st.error);
    assert_eq!(w.hits("f1"), 1);
    let j = w.journal(st.instance_id).await;
    let kinds: Vec<String> = j.iter().map(|r| serde_json::to_value(r).unwrap()["kind"].as_str().unwrap().to_string()).collect();
    assert_eq!(kinds, ["run_started", "step_completed", "step_started", "step_completed", "step_completed", "run_completed"]);
    assert_eq!(w.epoch(st.instance_id).await, 1);
}

/// F5 + F16: crash after the effect was recorded — it is not repeated, and the
/// `now` from step a is the recorded one, not re-evaluated.
#[tokio::test(flavor = "multi_thread")]
async fn after_a_recorded_effect_the_run_resumes_without_repeating_it() {
    let w = world().await;
    let st = w.run("f5").await;
    assert_eq!(st.status, ExecutionStatus::Completed, "{:?}", st.error);
    assert_eq!(w.hits("f5"), 1, "the charge is not repeated");
    assert_eq!(w.epoch(st.instance_id).await, 2, "finished by a second owner");
    let a_out = w.journal(st.instance_id).await.into_iter().find_map(|r| match r {
        Record::StepCompleted { step_key, output, .. } if step_key == "/a" => Some(output),
        _ => None,
    });
    assert_eq!(out(&st)["t"], a_out.unwrap()["t"], "replay used the recorded time");
    assert_eq!(out(&st)["hits"], 1);
}

/// F2: crash before the effect was even announced — it simply runs on resume.
#[tokio::test(flavor = "multi_thread")]
async fn before_an_effect_started_the_run_resumes_and_runs_it_once() {
    let w = world().await;
    let st = w.run("f2").await;
    assert_eq!(st.status, ExecutionStatus::Completed, "{:?}", st.error);
    assert_eq!(w.hits("f2"), 1);
}

/// F3: the effect was announced but never sent — in doubt; default is fault.
#[tokio::test(flavor = "multi_thread")]
async fn an_announced_effect_without_a_result_is_in_doubt() {
    let w = world().await;
    let st = w.run("f3").await;
    assert_eq!(st.status, ExecutionStatus::Failed);
    assert!(st.error.as_deref().unwrap_or("").contains("in-doubt side effect"), "{:?}", st.error);
    assert_eq!(w.hits("f3"), 0, "not executed on resume");
}

/// F4: the effect happened but its result was not recorded — in doubt; never
/// silently repeated.
#[tokio::test(flavor = "multi_thread")]
async fn an_unrecorded_effect_is_never_silently_repeated() {
    let w = world().await;
    let st = w.run("f4").await;
    assert_eq!(st.status, ExecutionStatus::Failed);
    assert!(st.error.as_deref().unwrap_or("").contains("in-doubt"), "{:?}", st.error);
    assert_eq!(w.hits("f4"), 1, "happened once, not repeated");
}

/// F4 with `inDoubt: retry` on an idempotent step: re-sent under the same key.
#[tokio::test(flavor = "multi_thread")]
async fn an_idempotent_step_can_be_retried_when_in_doubt() {
    let w = world().await;
    let st = w.run("f4r").await;
    assert_eq!(st.status, ExecutionStatus::Completed, "{:?}", st.error);
    assert_eq!(w.hits("f4r"), 2, "sent again (the receiver dedupes on the idempotency key)");
    let attempts: Vec<u32> = w
        .journal(st.instance_id)
        .await
        .into_iter()
        .filter_map(|r| match r {
            Record::StepStarted { step_key, attempt, .. } if step_key == "/charge" => Some(attempt),
            _ => None,
        })
        .collect();
    assert_eq!(attempts, [0, 1], "the retry is a new attempt of the same step");
}

/// An in-doubt fault is an ordinary error: the flow's try/catch handles it.
#[tokio::test(flavor = "multi_thread")]
async fn try_catch_handles_an_in_doubt_step() {
    let w = world().await;
    let st = w.run("f4c").await;
    assert_eq!(st.status, ExecutionStatus::Completed, "{:?}", st.error);
    assert_eq!(out(&st)["recovered"], true);
    assert_eq!(w.hits("f4c"), 1);
}

/// F14: an ephemeral run keeps no journal; interrupted, it faults.
#[tokio::test(flavor = "multi_thread")]
async fn an_ephemeral_run_keeps_no_journal() {
    let w = world().await;
    let ok = w.run("eph").await;
    assert_eq!(ok.status, ExecutionStatus::Completed);
    assert!(w.journal(ok.instance_id).await.is_empty(), "no journal for an ephemeral run");
}

#[tokio::test(flavor = "multi_thread")]
async fn an_interrupted_ephemeral_run_faults() {
    let w = world_with(false).await;
    let req = ExecutionRequest::new("c", "eph");
    let id = req.instance_id;
    w.s.queue().enqueue(req).await.unwrap();
    // A worker claims it and dies; then the real worker starts.
    let engine = w.s.engine().unwrap();
    let claimed = engine.claim_runs("default", "dead-worker", 1, Duration::from_millis(300)).await.unwrap();
    assert_eq!(claimed.len(), 1);
    flowr_ce::spawn_worker(w.s.clone());
    let deadline = std::time::Instant::now() + Duration::from_secs(20);
    let st = loop {
        let st = w.s.executions().get(id).await.unwrap().unwrap();
        if st.status != ExecutionStatus::Running && st.status != ExecutionStatus::Queued {
            break st;
        }
        assert!(std::time::Instant::now() < deadline);
        tokio::time::sleep(Duration::from_millis(50)).await;
    };
    assert_eq!(st.status, ExecutionStatus::Failed);
    assert!(st.error.unwrap_or_default().contains("interrupted"));
}
