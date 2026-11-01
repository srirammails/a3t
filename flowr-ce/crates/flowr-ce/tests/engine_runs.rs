//! Runs through the durable engine store (plan stage 3): the engine worker
//! claims queued runs, records outcomes, honours cancel requests, keeps queued
//! runs across a restart and never silently re-runs an interrupted one.

use std::sync::Arc;
use std::time::Duration;

use flowr_storage::engine::{RunPhase, RunRequest, DEFAULT_POOL};
use flowr_storage::StorageBackend;
use flowr_types::{ExecutionRequest, ExecutionState, ExecutionStatus, WorkflowRecord};
use serde_json::json;

const QUICK: &str = "document:\n  dsl: 1.0.3\n  namespace: t\n  name: quick\n  version: 1.0.0\ndo:\n- greet:\n    set:\n      hello: ${ .name }\n";
const SLOW: &str = "document:\n  dsl: 1.0.3\n  namespace: t\n  name: slow\n  version: 1.0.0\ndo:\n- nap:\n    wait:\n      seconds: 30\n- after:\n    set:\n      woke: true\n";

async fn storage(dir: &std::path::Path) -> Arc<dyn StorageBackend> {
    let s = flowr_ce::init_storage_with(dir.to_str().unwrap(), true).await.unwrap();
    for y in [QUICK, SLOW] {
        s.workflows().register(WorkflowRecord::from_yaml(y).unwrap()).await.unwrap();
    }
    s
}

async fn wait_for(s: &dyn StorageBackend, id: uuid::Uuid, done: impl Fn(&ExecutionState) -> bool) -> ExecutionState {
    let deadline = std::time::Instant::now() + Duration::from_secs(20);
    loop {
        if let Some(st) = s.executions().get(id).await.unwrap() {
            if done(&st) {
                return st;
            }
        }
        assert!(std::time::Instant::now() < deadline, "timed out waiting on run {id}");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

fn finished(s: &ExecutionState) -> bool {
    !matches!(s.status, ExecutionStatus::Queued | ExecutionStatus::Running | ExecutionStatus::Suspended)
}

#[tokio::test(flavor = "multi_thread")]
async fn runs_complete_through_the_engine() {
    let dir = tempfile::tempdir().unwrap();
    let s = storage(dir.path()).await;
    assert!(s.engine().is_some());
    flowr_ce::spawn_worker(s.clone());

    let req = ExecutionRequest::new("t", "quick").with_input(json!({ "name": "ada" }));
    let id = req.instance_id;
    s.queue().enqueue(req).await.unwrap();
    let st = wait_for(s.as_ref(), id, finished).await;
    assert_eq!(st.status, ExecutionStatus::Completed, "{:?}", st.error);
    assert_eq!(st.output.unwrap()["hello"], "ada");
    assert!(st.duration_us.is_some());

    let row = s.engine().unwrap().get(id).await.unwrap().unwrap();
    assert_eq!(row.phase, RunPhase::Completed);
    assert!(row.owner.is_none(), "released");
    let recent = s.executions().list_by_workflow("t", "quick", 5).await.unwrap();
    assert_eq!(recent[0].instance_id, id, "listed newest first");
    assert_eq!(s.engine().unwrap().pending(DEFAULT_POOL).await.unwrap(), 0, "its task is acked");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_cancel_request_stops_a_running_run() {
    let dir = tempfile::tempdir().unwrap();
    let s = storage(dir.path()).await;
    flowr_ce::spawn_worker(s.clone());
    let req = ExecutionRequest::new("t", "slow");
    let id = req.instance_id;
    s.queue().enqueue(req).await.unwrap();
    wait_for(s.as_ref(), id, |st| st.status == ExecutionStatus::Running).await;
    tokio::time::sleep(Duration::from_millis(200)).await;

    let t = std::time::Instant::now();
    s.engine().unwrap().request(id, RunRequest::Cancel { by: "test".into(), reason: "not needed".into() }).await.unwrap();
    let st = wait_for(s.as_ref(), id, finished).await;
    assert_eq!(st.status, ExecutionStatus::Cancelled);
    assert!(t.elapsed() < Duration::from_secs(5), "stopped promptly, not after the 30 s wait");
}

#[tokio::test(flavor = "multi_thread")]
async fn queued_runs_survive_a_restart() {
    let dir = tempfile::tempdir().unwrap();
    let id = {
        let s = storage(dir.path()).await;
        let req = ExecutionRequest::new("t", "quick").with_input(json!({ "name": "bo" }));
        let id = req.instance_id;
        s.queue().enqueue(req).await.unwrap();
        id
        // No worker: the process "stops" with the run queued.
    };
    let s = storage(dir.path()).await;
    assert_eq!(s.executions().get(id).await.unwrap().unwrap().status, ExecutionStatus::Queued, "still queued after the restart");
    flowr_ce::spawn_worker(s.clone());
    let st = wait_for(s.as_ref(), id, finished).await;
    assert_eq!(st.status, ExecutionStatus::Completed, "{:?}", st.error);
    assert_eq!(st.output.unwrap()["hello"], "bo");
}

#[tokio::test(flavor = "multi_thread")]
async fn an_interrupted_run_is_faulted_not_rerun() {
    let dir = tempfile::tempdir().unwrap();
    let s = storage(dir.path()).await;
    let req = ExecutionRequest::new("t", "quick").with_input(json!({ "name": "cy" }));
    let id = req.instance_id;
    s.queue().enqueue(req).await.unwrap();
    // A worker claims it and dies: its lease runs out.
    let engine = s.engine().unwrap();
    let claimed = engine.claim_runs(DEFAULT_POOL, "dead-worker", 1, Duration::from_millis(100)).await.unwrap();
    assert_eq!(claimed.len(), 1);

    flowr_ce::spawn_worker(s.clone());
    let st = wait_for(s.as_ref(), id, finished).await;
    assert_eq!(st.status, ExecutionStatus::Failed);
    assert!(st.error.as_deref().unwrap_or("").contains("interrupted"), "{:?}", st.error);
    assert!(st.output.is_none(), "not re-run");
}
