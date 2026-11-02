//! `listen` and `emit` on the durable engine (spec §5.9, limitation L29): a
//! listening run lets go and waits in the store; a matching event — emitted by
//! another run, or sent from outside — wakes it with the event's data; a
//! timeout on the step fails it (catchably); the wait lives in the store file.

use std::sync::Arc;
use std::time::Duration;

use flowr_storage::engine::RunPhase;
use flowr_storage::StorageBackend;
use flowr_types::{ExecutionRequest, ExecutionState, ExecutionStatus, WorkflowRecord};
use serde_json::json;

const ORDER: &str = r#"
document: { dsl: '1.0.3', namespace: e, name: order, version: '1.0.0' }
do:
  - paid:
      listen:
        to:
          one:
            with:
              type: shop.order.paid
              data: { order: '${ .order }' }
  - done:
      set:
        amount: ${ .[0].amount }
"#;

const PAY: &str = r#"
document: { dsl: '1.0.3', namespace: e, name: pay, version: '1.0.0' }
do:
  - tell:
      emit:
        event:
          with:
            type: shop.order.paid
            data: { order: '${ .order }', amount: '${ .amount }' }
"#;

const BOTH: &str = r#"
document: { dsl: '1.0.3', namespace: e, name: both, version: '1.0.0' }
do:
  - ready:
      listen:
        read: envelope
        to:
          all:
            - with: { type: a }
            - with: { type: b }
  - done:
      set:
        types: ${ [.[].type] }
"#;

const PATIENT: &str = r#"
document: { dsl: '1.0.3', namespace: e, name: patient, version: '1.0.0' }
do:
  - guard:
      try:
        do:
          - hear:
              listen:
                to:
                  any:
                    - with: { type: never }
              timeout: { after: { seconds: 1 } }
        catch:
          do:
            - fallback:
                set: { timed_out: true }
"#;

async fn storage(dir: &std::path::Path, worker: bool) -> Arc<dyn StorageBackend> {
    let s = flowr_ce::init_storage_with(dir.to_str().unwrap(), true).await.unwrap();
    for y in [ORDER, PAY, BOTH, PATIENT] {
        s.workflows().register(WorkflowRecord::from_yaml(y).unwrap()).await.unwrap();
    }
    if worker {
        flowr_ce::spawn_worker(s.clone());
    }
    s
}

async fn start(s: &dyn StorageBackend, name: &str, input: serde_json::Value) -> uuid::Uuid {
    let req = ExecutionRequest::new("e", name).with_input(input);
    let id = req.instance_id;
    s.queue().enqueue(req).await.unwrap();
    id
}

async fn phase_is(s: &dyn StorageBackend, id: uuid::Uuid, want: RunPhase) {
    let deadline = std::time::Instant::now() + Duration::from_secs(20);
    loop {
        if s.engine().unwrap().get(id).await.unwrap().is_some_and(|r| r.phase == want) {
            return;
        }
        assert!(std::time::Instant::now() < deadline, "run {id} never reached {want:?}");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

async fn finished(s: &dyn StorageBackend, id: uuid::Uuid) -> ExecutionState {
    let deadline = std::time::Instant::now() + Duration::from_secs(20);
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
async fn an_emitted_event_wakes_the_matching_listener() {
    let dir = tempfile::tempdir().unwrap();
    let s = storage(dir.path(), true).await;
    let listener = start(s.as_ref(), "order", json!({ "order": 7 })).await;
    phase_is(s.as_ref(), listener, RunPhase::Waiting).await;

    // Another order's payment leaves it waiting.
    let other = start(s.as_ref(), "pay", json!({ "order": 8, "amount": 3 })).await;
    assert_eq!(finished(s.as_ref(), other).await.status, ExecutionStatus::Completed);
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(s.engine().unwrap().get(listener).await.unwrap().unwrap().phase, RunPhase::Waiting);

    // Its own wakes it, with the event's data as the step output.
    let pay = start(s.as_ref(), "pay", json!({ "order": 7, "amount": 5 })).await;
    assert_eq!(finished(s.as_ref(), pay).await.status, ExecutionStatus::Completed);
    let st = finished(s.as_ref(), listener).await;
    assert_eq!(st.status, ExecutionStatus::Completed, "{:?}", st.error);
    assert_eq!(st.output.unwrap()["amount"], 5);
}

#[tokio::test(flavor = "multi_thread")]
async fn all_waits_for_every_event_and_reads_envelopes() {
    let dir = tempfile::tempdir().unwrap();
    let s = storage(dir.path(), true).await;
    let id = start(s.as_ref(), "both", json!({})).await;
    phase_is(s.as_ref(), id, RunPhase::Waiting).await;
    let e = s.engine().unwrap();
    assert!(e.deliver_event("e", json!({ "type": "b", "id": "1" })).await.unwrap().is_empty());
    assert_eq!(e.deliver_event("e", json!({ "type": "a", "id": "2" })).await.unwrap(), vec![id]);
    let st = finished(s.as_ref(), id).await;
    assert_eq!(st.status, ExecutionStatus::Completed, "{:?}", st.error);
    assert_eq!(st.output.unwrap()["types"], json!(["a", "b"]), "in filter order, whole events");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_listen_timeout_is_catchable() {
    let dir = tempfile::tempdir().unwrap();
    let s = storage(dir.path(), true).await;
    let id = start(s.as_ref(), "patient", json!({})).await;
    let st = finished(s.as_ref(), id).await;
    assert_eq!(st.status, ExecutionStatus::Completed, "{:?}", st.error);
    assert_eq!(st.output.unwrap()["timed_out"], true);
    // The wait is closed: a late event wakes nothing.
    assert!(s.engine().unwrap().deliver_event("e", json!({ "type": "never" })).await.unwrap().is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn a_wait_survives_a_restart() {
    let dir = tempfile::tempdir().unwrap();
    let id = {
        let s = storage(dir.path(), true).await;
        let id = start(s.as_ref(), "order", json!({ "order": 1 })).await;
        phase_is(s.as_ref(), id, RunPhase::Waiting).await;
        id
    };
    // A fresh process on the same store: the wait is there, and its event
    // wakes the run on this process's worker.
    let s = storage(dir.path(), true).await;
    let woke = s.engine().unwrap().deliver_event("e", json!({ "type": "shop.order.paid", "data": { "order": 1, "amount": 9 } })).await.unwrap();
    assert_eq!(woke, vec![id]);
    let st = finished(s.as_ref(), id).await;
    assert_eq!(st.status, ExecutionStatus::Completed, "{:?}", st.error);
    assert_eq!(st.output.unwrap()["amount"], 9);
}
