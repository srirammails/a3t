//! Agents in fine mode (spec §5.12): each model call and tool call is
//! journaled, so an agent whose worker died resumes without calling the model
//! or the tool again; a tool call that started but never finished is reported
//! to the model as in doubt, not repeated.

mod common;

use std::sync::Once;
use std::time::Duration;

use flowr_storage::engine::Record;
use flowr_storage::StorageBackend;
use flowr_types::{ExecutionRequest, ExecutionState, ExecutionStatus, WorkflowRecord};

static INIT: Once = Once::new();

fn init() {
    INIT.call_once(|| {
        std::env::set_var("FLOWR_RUN_LEASE_MS", "600");
        std::env::set_var(
            "FLOWR_KILL_AT",
            ["t/after:after_completed:/chat/fx/tool/1.0", "t/during:after_step_started:/chat/fx/tool/1.0"].join(","),
        );
    });
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

async fn run(flow: &str) -> (ExecutionState, std::sync::Arc<common::Counts>, Vec<Record>) {
    init();
    let dir = tempfile::tempdir().unwrap();
    let s = flowr_ce::init_storage_with(dir.path().to_str().unwrap(), true).await.unwrap();
    let (base, counts) = common::fake_server().await;
    common::register_fakes(s.as_ref(), &base).await;
    s.workflows().register(WorkflowRecord::from_yaml(&common::agent_flow(flow)).unwrap()).await.unwrap();
    flowr_ce::spawn_worker(s.clone());
    let req = ExecutionRequest::new("t", flow);
    let id = req.instance_id;
    s.queue().enqueue(req).await.unwrap();
    let st = finished(s.as_ref(), id).await;
    let journal = s.engine().unwrap().read_from(id, 0).await.unwrap().into_iter().map(|e| e.record).collect();
    (st, counts, journal)
}

fn fx_keys(journal: &[Record], kind: &str) -> Vec<String> {
    journal
        .iter()
        .filter_map(|r| match r {
            Record::StepStarted { step_key, .. } if kind == "started" => Some(step_key.clone()),
            Record::StepCompleted { step_key, .. } if kind == "completed" => Some(step_key.clone()),
            _ => None,
        })
        .filter(|k| k.contains("/fx/"))
        .collect()
}

#[tokio::test(flavor = "multi_thread")]
async fn an_agent_records_each_model_and_tool_call() {
    let (st, counts, journal) = run("plain").await;
    assert_eq!(st.status, ExecutionStatus::Completed, "{:?}", st.error);
    assert!(st.output.unwrap()["content"].as_str().unwrap().contains("poked\\\":1"));
    assert_eq!((counts.model(), counts.pokes()), (2, 1));
    assert_eq!(fx_keys(&journal, "completed"), ["/chat/fx/model/0", "/chat/fx/tool/1.0", "/chat/fx/model/1"]);
    assert_eq!(fx_keys(&journal, "started"), ["/chat/fx/tool/1.0"], "only the tool call is written ahead");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_resumed_agent_repeats_no_call() {
    // The worker dies right after the tool call is recorded.
    let (st, counts, _) = run("after").await;
    assert_eq!(st.status, ExecutionStatus::Completed, "{:?}", st.error);
    assert!(st.output.unwrap()["content"].as_str().unwrap().contains("poked\\\":1"));
    assert_eq!(counts.pokes(), 1, "the tool was not called again");
    assert_eq!(counts.model(), 2, "the first model call was replayed, not repeated");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_tool_call_in_doubt_is_reported_not_repeated() {
    // The worker dies after the tool call was written ahead, before it ran.
    let (st, counts, journal) = run("during").await;
    assert_eq!(st.status, ExecutionStatus::Completed, "{:?}", st.error);
    assert_eq!(counts.pokes(), 0, "never repeated (here it never ran at all)");
    let content = st.output.unwrap()["content"].as_str().unwrap().to_string();
    assert!(content.contains("may or may not have taken effect"), "{content}");
    let tool = journal.iter().find_map(|r| match r {
        Record::StepCompleted { step_key, output, .. } if step_key == "/chat/fx/tool/1.0" => Some(output.clone()),
        _ => None,
    });
    assert!(tool.unwrap()["err"].as_str().unwrap().contains("interrupted"));
}
