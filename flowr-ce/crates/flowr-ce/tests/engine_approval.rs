//! Durable approvals (limitation L1): an agent whose tool call needs a person's
//! approval records the question and lets go of the run; the answer — given
//! through the store, as `POST /inputs/:id` does, even from a new process —
//! wakes it, and the agent continues without repeating its model call.

mod common;

use std::sync::{Arc, Once};
use std::time::Duration;

use flowr_policy::{ExecutionRequest as PolicyRequest, GrantScope, PolicyDecision, PolicyHook, Principal};
use flowr_storage::engine::{Record, RunPhase};
use flowr_storage::StorageBackend;
use flowr_types::{ExecutionRequest, ExecutionState, ExecutionStatus, WorkflowRecord};
use serde_json::json;

/// Asks before `poke`; allows everything else.
struct AskBeforePoke;

#[async_trait::async_trait]
impl PolicyHook for AskBeforePoke {
    async fn check(&self, req: &PolicyRequest) -> PolicyDecision {
        if req.action.as_str() == flowr_policy::actions::MCP_CALL && req.resource.id.ends_with("poke") {
            PolicyDecision::Ask { reason: "poking needs your OK".into(), policy_ids: vec!["test".into()], scopes: vec![GrantScope::Once, GrantScope::Session] }
        } else {
            PolicyDecision::Allow
        }
    }
}

static INIT: Once = Once::new();
/// The tests take turns: one sets the approval timeout for the process.
static TURN: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

fn init() {
    INIT.call_once(|| {
        std::env::set_var("FLOWR_RUN_LEASE_MS", "600");
        let _ = flowr_policy::set_policy_hook(Arc::new(AskBeforePoke));
    });
}

async fn storage(dir: &std::path::Path, base: &str, flows: &[&str], fresh: bool) -> Arc<dyn StorageBackend> {
    let s = flowr_ce::init_storage_with(dir.to_str().unwrap(), true).await.unwrap();
    if fresh {
        common::register_fakes(s.as_ref(), base).await;
        for f in flows {
            s.workflows().register(WorkflowRecord::from_yaml(&common::agent_flow(f)).unwrap()).await.unwrap();
        }
    }
    flowr_ce::spawn_worker(s.clone());
    s
}

async fn phase_is(s: &dyn StorageBackend, id: uuid::Uuid, want: RunPhase) {
    let deadline = std::time::Instant::now() + Duration::from_secs(20);
    while !s.engine().unwrap().get(id).await.unwrap().is_some_and(|r| r.phase == want) {
        assert!(std::time::Instant::now() < deadline, "run {id} never reached {want:?}");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
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

fn start(flow: &str) -> ExecutionRequest {
    ExecutionRequest::new("t", flow).with_principal(Principal::human("u1", vec!["Admin".into()]))
}

#[tokio::test(flavor = "multi_thread")]
async fn an_approval_waits_durably_and_its_answer_resumes_the_agent() {
    let _turn = TURN.lock().await;
    init();
    let dir = tempfile::tempdir().unwrap();
    let (base, counts) = common::fake_server().await;
    let (id, request_id) = {
        let s = storage(dir.path(), &base, &["ask", "deny"], true).await;
        let req = start("ask");
        let id = req.instance_id;
        s.queue().enqueue(req).await.unwrap();
        phase_is(s.as_ref(), id, RunPhase::Waiting).await;
        let open = s.engine().unwrap().approvals().await.unwrap();
        assert_eq!(open.len(), 1);
        let (run, request_id, q) = open[0].clone();
        assert_eq!(run, id);
        assert_eq!(q["context"]["approver"], "u1");
        assert_eq!(q["context"]["tool"], "poke");
        assert_eq!((counts.model(), counts.pokes()), (1, 0), "asked before poking; nothing runs while it waits");
        (id, request_id)
    };

    // A new process answers it (the first is "gone": its worker idles).
    let s = storage(dir.path(), &base, &[], false).await;
    let engine = s.engine().unwrap();
    assert!(engine.decide_approval(&request_id, json!({ "allow": true, "scope": "once" }), "u1").await.unwrap());
    let st = finished(s.as_ref(), id).await;
    assert_eq!(st.status, ExecutionStatus::Completed, "{:?}", st.error);
    assert!(st.output.unwrap()["content"].as_str().unwrap().contains("poked"));
    assert_eq!(counts.pokes(), 1, "poked once, after the answer");
    assert_eq!(counts.model(), 2, "the first model call was replayed, not repeated");

    let journal: Vec<Record> = engine.read_from(id, 0).await.unwrap().into_iter().map(|e| e.record).collect();
    assert!(journal.iter().any(|r| matches!(r, Record::ApprovalRequested { wait_id, .. } if *wait_id == request_id)));
    assert!(journal.iter().any(|r| matches!(r, Record::ApprovalDecided { by, .. } if by == "u1")));
    let tool_starts = journal.iter().filter(|r| matches!(r, Record::StepStarted { step_key, .. } if step_key == "/chat/fx/tool/1.0")).count();
    assert_eq!(tool_starts, 2, "written ahead before the question, and again before the call");

    // A refusal: the tool is not called; the model hears it was denied.
    let req = start("deny");
    let id = req.instance_id;
    s.queue().enqueue(req).await.unwrap();
    phase_is(s.as_ref(), id, RunPhase::Waiting).await;
    let (_, rid, _) = engine.approvals().await.unwrap().remove(0);
    assert!(engine.decide_approval(&rid, json!({ "allow": false }), "u1").await.unwrap());
    let st = finished(s.as_ref(), id).await;
    assert_eq!(st.status, ExecutionStatus::Completed, "{:?}", st.error);
    assert!(st.output.unwrap()["content"].as_str().unwrap().contains("denied"));
    assert_eq!(counts.pokes(), 1, "not poked again");
}

#[tokio::test(flavor = "multi_thread")]
async fn an_unanswered_approval_expires_into_a_denial() {
    let _turn = TURN.lock().await;
    init();
    std::env::set_var("FLOWR_APPROVAL_TIMEOUT_SECS", "1");
    let dir = tempfile::tempdir().unwrap();
    let (base, counts) = common::fake_server().await;
    let s = storage(dir.path(), &base, &["late"], true).await;
    let req = start("late");
    let id = req.instance_id;
    s.queue().enqueue(req).await.unwrap();
    let st = finished(s.as_ref(), id).await;
    std::env::remove_var("FLOWR_APPROVAL_TIMEOUT_SECS");
    assert_eq!(st.status, ExecutionStatus::Completed, "{:?}", st.error);
    assert!(st.output.unwrap()["content"].as_str().unwrap().contains("not answered in time"));
    assert_eq!(counts.pokes(), 0);
    assert!(s.engine().unwrap().approvals().await.unwrap().is_empty(), "closed");
}
