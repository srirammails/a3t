//! Events start flows (limitation L37) and are delivered once per id (L38): an
//! event trigger in the project starts its flow with the event as input; the
//! same event sent twice — an `emit` with a fixed id, or a repeat after a
//! crash — wakes and starts nothing more; other projects' triggers never fire.

use std::sync::Arc;
use std::time::Duration;

use flowr_storage::engine::{RunPhase, RunQuery};
use flowr_storage::StorageBackend;
use flowr_types::{ExecutionRequest, ExecutionStatus, WorkflowRecord};
use serde_json::{json, Value};

fn flows(ns: &str) -> Vec<String> {
    vec![
        format!(
            r#"
document: {{ dsl: '1.0.3', namespace: {ns}, name: twice, version: '1.0.0' }}
do:
  - first:
      emit:
        event:
          with: {{ id: order-7-paid, type: shop.order.paid, data: {{ order: 7, amount: 5 }} }}
  - again:
      emit:
        event:
          with: {{ id: order-7-paid, type: shop.order.paid, data: {{ order: 7, amount: 5 }} }}
"#
        ),
        format!(
            r#"
document: {{ dsl: '1.0.3', namespace: {ns}, name: receipt, version: '1.0.0' }}
do:
  - note:
      set: {{ order: '${{ .data.order }}', amount: '${{ .data.amount }}', kind: '${{ .type }}' }}
"#
        ),
        format!(
            r#"
document: {{ dsl: '1.0.3', namespace: {ns}, name: wait, version: '1.0.0' }}
do:
  - paid:
      listen:
        to:
          any:
            - with: {{ type: shop.order.paid }}
"#
        ),
    ]
}

async fn trigger(ns: &str, target_ns: &str) {
    let t: flowr_ce_agents::Trigger = serde_json::from_value(json!({
        "name": format!("paid-{ns}-{target_ns}"),
        "namespace": ns,
        "config": { "type": "event", "source": "shop.order.paid" },
        "targets": [{ "type": "workflow", "namespace": target_ns, "name": "receipt" }],
    }))
    .unwrap();
    flowr_ce_agents::trigger_manager().create(t).await.unwrap();
}

async fn world(ns: &str) -> (Arc<dyn StorageBackend>, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let s = flowr_ce::init_storage_with(dir.path().to_str().unwrap(), true).await.unwrap();
    for y in flows(ns).into_iter().chain(flows("ev2")).chain(flows("ev3")).chain(flows("other")) {
        s.workflows().register(WorkflowRecord::from_yaml(&y).unwrap()).await.unwrap();
    }
    let _ = flowr_ce::build_ce_router(s.clone());
    flowr_ce::spawn_worker(s.clone());
    (s, dir)
}

async fn runs_of(s: &dyn StorageBackend, ns: &str, flow: &str) -> Vec<flowr_storage::engine::RunRow> {
    s.engine()
        .unwrap()
        .list(RunQuery { namespaces: Some(vec![ns.into()]), flow: Some(flow.into()), limit: 50, ..Default::default() })
        .await
        .unwrap()
        .items
}

async fn until<F: Fn(&[flowr_storage::engine::RunRow]) -> bool>(s: &dyn StorageBackend, ns: &str, flow: &str, ok: F) -> Vec<flowr_storage::engine::RunRow> {
    let deadline = std::time::Instant::now() + Duration::from_secs(20);
    loop {
        let rows = runs_of(s, ns, flow).await;
        if ok(&rows) {
            return rows;
        }
        assert!(std::time::Instant::now() < deadline, "{ns}/{flow}: {rows:?}");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

// One test: triggers start flows in the process's one store (set once).
#[tokio::test(flavor = "multi_thread")]
async fn events_start_flows_once() {
    let (s, _d) = world("ev1").await;
    emitted_events_start_and_wake_once(&s).await;
    sent_events_start_too(&s).await;
    posted_events_start_too(&s).await;
}

/// `POST /events/:namespace`, binary mode: `ce-*` headers, the body as `data`.
async fn posted_events_start_too(s: &Arc<dyn StorageBackend>) {
    trigger("ev3", "ev3").await;
    let app = flowr_ce::build_ce_router(s.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let post = |id: &'static str| async move {
        reqwest::Client::new()
            .post(format!("http://{addr}/events/ev3"))
            .header("ce-type", "shop.order.paid")
            .header("ce-id", id)
            .header("ce-source", "/shop")
            .json(&json!({ "order": 3, "amount": 4 }))
            .send()
            .await
            .unwrap()
    };
    let r = post("h-1").await;
    assert_eq!(r.status(), 202);
    let body: Value = r.json().await.unwrap();
    assert_eq!(body["id"], "h-1");
    assert_eq!(post("h-1").await.status(), 202, "a repeat is accepted, and does nothing");
    let rows = until(s.as_ref(), "ev3", "receipt", |r| r.first().is_some_and(|r| r.phase == RunPhase::Completed)).await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(runs_of(s.as_ref(), "ev3", "receipt").await.len(), 1);
    assert_eq!(rows[0].output.clone().unwrap()["amount"], 4);
    let bad = reqwest::Client::new().post(format!("http://{addr}/events/ev3")).json(&json!({ "data": 1 })).send().await.unwrap();
    assert_eq!(bad.status(), 400, "a CloudEvent needs a type");
}

async fn emitted_events_start_and_wake_once(s: &Arc<dyn StorageBackend>) {
    trigger("ev1", "ev1").await;
    // Another project's trigger for the same type never fires from ev1.
    trigger("other", "other").await;

    let listener = ExecutionRequest::new("ev1", "wait");
    let lid = listener.instance_id;
    s.queue().enqueue(listener).await.unwrap();
    until(s.as_ref(), "ev1", "wait", |r| r.first().is_some_and(|r| r.phase == RunPhase::Waiting)).await;

    let req = ExecutionRequest::new("ev1", "twice");
    s.queue().enqueue(req).await.unwrap();
    until(s.as_ref(), "ev1", "twice", |r| r.first().is_some_and(|r| r.phase == RunPhase::Completed)).await;

    let receipts = until(s.as_ref(), "ev1", "receipt", |r| r.first().is_some_and(|r| r.phase == RunPhase::Completed)).await;
    tokio::time::sleep(Duration::from_millis(500)).await;
    let receipts = if runs_of(s.as_ref(), "ev1", "receipt").await.len() != receipts.len() { runs_of(s.as_ref(), "ev1", "receipt").await } else { receipts };
    assert_eq!(receipts.len(), 1, "the event was sent twice with one id: one run");
    assert_eq!(receipts[0].trigger, "event:shop.order.paid");
    let out: Value = receipts[0].output.clone().unwrap();
    assert_eq!((out["order"].clone(), out["amount"].clone(), out["kind"].clone()), (json!(7), json!(5), json!("shop.order.paid")));

    let st = s.executions().get(lid).await.unwrap().unwrap();
    assert_eq!(st.status, ExecutionStatus::Completed, "{:?}", st.error);
    assert!(runs_of(s.as_ref(), "other", "receipt").await.is_empty(), "other projects' triggers do not fire");
}

async fn sent_events_start_too(s: &Arc<dyn StorageBackend>) {
    trigger("ev2", "ev2").await;
    let me = flowr_policy::Principal::human("usr-dev", vec!["Admin".into()]);
    let call = |id: &'static str| {
        let (s, me) = (s.clone(), me.clone());
        async move {
            let args = json!({ "namespace": "ev2", "event": { "id": id, "type": "shop.order.paid", "data": { "order": 1, "amount": 2 } } });
            let r = flowr_mcp_core::tools::handle_tool_call(s.as_ref(), &me, "send_event", Some(args)).await;
            assert!(r.is_error != Some(true));
        }
    };
    call("e-1").await;
    call("e-1").await;
    call("e-2").await;
    let rows = until(s.as_ref(), "ev2", "receipt", |r| r.len() == 2 && r.iter().all(|r| r.phase == RunPhase::Completed)).await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(runs_of(s.as_ref(), "ev2", "receipt").await.len(), rows.len(), "e-1 twice and e-2: two runs");
}
