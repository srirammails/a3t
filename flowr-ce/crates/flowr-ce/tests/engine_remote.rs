//! The engine store across processes (plan stage 7): a runtime works against
//! the console's store through the gateway. The remote store passes the same
//! conformance suite as the local ones (fencing included), and a run queued on
//! a console that executes nothing is run by a runtime worker, its journal
//! written to the console.

use std::sync::Arc;
use std::time::Duration;

use flowr_ce::engine_remote::{gateway_router, live_pools, RemoteEngineStore};
use flowr_storage::engine::{EngineStore, Record, RunPhase, RunQuery, TaskQueue};
use flowr_storage::engine_memory::MemoryStore;
use flowr_storage::StorageBackend;
use flowr_types::{ExecutionRequest, WorkflowRecord};
use serde_json::json;

async fn serve(store: Arc<dyn EngineStore>, token: &str) -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let app = gateway_router(store, token.to_string());
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    format!("http://{addr}")
}

#[tokio::test(flavor = "multi_thread")]
async fn the_remote_store_passes_the_conformance_suite() {
    flowr_storage::conformance::run_all(|| async {
        let url = serve(Arc::new(MemoryStore::new()), "tok").await;
        RemoteEngineStore::new(&url, "tok")
    })
    .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_wrong_token_is_refused() {
    let url = serve(Arc::new(MemoryStore::new()), "right").await;
    let r = RemoteEngineStore::new(&url, "wrong").pending("default").await;
    assert!(r.unwrap_err().to_string().contains("refused"));
}

const FLOW: &str = "document:\n  dsl: 1.0.3\n  namespace: r\n  name: hello\n  version: 1.0.0\ndo:\n- greet:\n    set:\n      hello: ${ .name }\n- shout:\n    set:\n      loud: ${ .hello + \"!\" }\n";

#[tokio::test(flavor = "multi_thread")]
async fn a_runtime_runs_what_the_console_queues() {
    // Console: holds the store, executes nothing. Runs of r/hello go to pool rt1.
    let dir = tempfile::tempdir().unwrap();
    let console = flowr_ce::init_storage_with(dir.path().to_str().unwrap(), true).await.unwrap();
    console.workflows().register(WorkflowRecord::from_yaml(FLOW).unwrap()).await.unwrap();
    flowr_storage::engine::set_pool_router(|_, flow| if flow == "hello" { "rt1".into() } else { "default".into() });
    let url = serve(console.engine().unwrap(), "tok").await;

    // Runtime: its own registry copy (in production, the shared ADB registry),
    // the console's engine store over the gateway, working pool rt1.
    let rt_dir = tempfile::tempdir().unwrap();
    let runtime = flowr_ce::flowr_ce_storage::LanceStorage::new(rt_dir.path().join("rt.lance")).await.unwrap()
        .with_engine(Arc::new(RemoteEngineStore::new(&url, "tok")));
    let runtime: Arc<dyn StorageBackend> = Arc::new(runtime);
    runtime.init().await.unwrap();
    runtime.workflows().register(WorkflowRecord::from_yaml(FLOW).unwrap()).await.unwrap();
    flowr_ce::spawn_pool_worker(runtime.clone(), vec!["rt1".into()]);

    let req = ExecutionRequest::new("r", "hello").with_input(json!({ "name": "ada" }));
    let id = req.instance_id;
    console.queue().enqueue(req).await.unwrap();

    let engine = console.engine().unwrap();
    let deadline = std::time::Instant::now() + Duration::from_secs(20);
    let row = loop {
        let r = engine.get(id).await.unwrap().unwrap();
        if r.phase.is_terminal() {
            break r;
        }
        assert!(std::time::Instant::now() < deadline, "still {:?}", r.phase);
        tokio::time::sleep(Duration::from_millis(50)).await;
    };
    assert_eq!(row.phase, RunPhase::Completed, "{:?}", row.error);
    assert_eq!(row.output.unwrap()["loud"], "ada!");
    let steps: Vec<String> = engine.read_from(id, 0).await.unwrap().into_iter()
        .filter_map(|e| match e.record { Record::StepCompleted { step_key, .. } => Some(step_key), _ => None }).collect();
    assert_eq!(steps, ["/greet", "/shout"], "the journal is on the console");
    assert!(live_pools().contains(&"rt1".to_string()), "the gateway saw rt1's worker");
    assert_eq!(engine.list(RunQuery { limit: 5, ..Default::default() }).await.unwrap().items.len(), 1);
}
