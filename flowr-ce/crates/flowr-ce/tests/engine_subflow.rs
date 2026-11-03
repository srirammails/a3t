//! Subflows journaled under their calling step (L2): a worker that dies inside
//! a `run: workflow` child leaves the run to resume, and the child continues
//! from its journal — its side effects are not repeated and the parent does
//! not fault in doubt; a durable wait inside the child parks the whole run.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, Once};
use std::time::Duration;

use flowr_storage::engine::{Record, RunPhase};
use flowr_storage::StorageBackend;
use flowr_types::{ExecutionRequest, ExecutionState, ExecutionStatus, WorkflowRecord};
use serde_json::json;

static INIT: Once = Once::new();

fn init() {
    INIT.call_once(|| {
        std::env::set_var("FLOWR_RUN_LEASE_MS", "600");
        std::env::set_var("FLOWR_INLINE_WAIT_MAX_MS", "100");
        std::env::set_var("FLOWR_KILL_AT", "s/parent:after_completed:/call/sub/one");
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

fn flows(base: &str) -> [String; 2] {
    let parent = r#"
document: { dsl: '1.0.3', namespace: s, name: parent, version: '1.0.0' }
do:
  - call:
      run:
        workflow: { namespace: s, name: child, version: '1.0.0' }
  - done:
      set: { child: '${ . }' }
"#
    .to_string();
    let child = format!(
        r#"
document: {{ dsl: '1.0.3', namespace: s, name: child, version: '1.0.0' }}
do:
  - one:
      call: http
      with: {{ method: POST, endpoint: {base}/hit/one, body: {{}} }}
  - nap:
      wait: {{ seconds: 1 }}
  - two:
      call: http
      with: {{ method: POST, endpoint: {base}/hit/two, body: {{}} }}
  - out:
      set: {{ ok: true }}
"#
    );
    [parent, child]
}

async fn finished(s: &dyn StorageBackend, id: uuid::Uuid) -> ExecutionState {
    let deadline = std::time::Instant::now() + Duration::from_secs(30);
    loop {
        if let Some(st) = s.executions().get(id).await.unwrap() {
            if !matches!(st.status, ExecutionStatus::Queued | ExecutionStatus::Running | ExecutionStatus::Suspended) {
                return st;
            }
        }
        assert!(std::time::Instant::now() < deadline, "run {id} did not finish");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_subflow_resumes_from_its_own_steps() {
    init();
    let dir = tempfile::tempdir().unwrap();
    let s = flowr_ce::init_storage_with(dir.path().to_str().unwrap(), true).await.unwrap();
    let (base, hits) = effect_server().await;
    for y in flows(&base) {
        s.workflows().register(WorkflowRecord::from_yaml(&y).unwrap()).await.unwrap();
    }
    flowr_ce::spawn_worker(s.clone());
    let req = ExecutionRequest::new("s", "parent");
    let id = req.instance_id;
    s.queue().enqueue(req).await.unwrap();

    // The child's durable wait parks the whole run.
    let e = s.engine().unwrap();
    let deadline = std::time::Instant::now() + Duration::from_secs(20);
    while !e.get(id).await.unwrap().is_some_and(|r| r.phase == RunPhase::Waiting) {
        assert!(std::time::Instant::now() < deadline, "never parked on the child's timer");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }

    let st = finished(s.as_ref(), id).await;
    assert_eq!(st.status, ExecutionStatus::Completed, "{:?}", st.error);
    assert_eq!(st.output.unwrap()["child"]["ok"], true);
    let h = hits.lock().unwrap().clone();
    assert_eq!((h.get("one").copied(), h.get("two").copied()), (Some(1), Some(1)), "no child effect repeated");

    let entries = e.read_from(id, 0).await.unwrap();
    assert!(entries.iter().map(|x| x.epoch).max().unwrap() >= 2, "the worker was killed inside the child and the run resumed");
    let done: Vec<String> = entries
        .iter()
        .filter_map(|x| match &x.record {
            Record::StepCompleted { step_key, .. } => Some(step_key.clone()),
            _ => None,
        })
        .collect();
    for k in ["/call/sub/one", "/call/sub/nap", "/call/sub/two", "/call/sub/out", "/call", "/done"] {
        assert_eq!(done.iter().filter(|d| *d == k).count(), 1, "{k} completed once: {done:?}");
    }
}

