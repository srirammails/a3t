//! The run tools the console uses (`.ai/FLOWR_CONSOLE_RUN_DEBUG_UI_SPEC.md`
//! §11), through MCP dispatch: start a debug run, read it while paused, edit a
//! step's input, continue, read the journal (masked), re-run from a point, list.

use std::sync::{Arc, Once};
use std::time::Duration;

use flowr_mcp_core::protocol::ToolContent;
use flowr_storage::StorageBackend;
use flowr_types::WorkflowRecord;
use serde_json::{json, Value};

static INIT: Once = Once::new();

const FLOW: &str = r#"
document: { dsl: '1.0.3', namespace: rt, name: pay, version: '1.0.0' }
do:
  - prepare:
      set: { amount: 10, api_key: "sk-live-secret-123" }
  - charge:
      set: { charged: "${ .amount }" }
  - receipt:
      set: { ok: true, charged: "${ .charged }" }
"#;

const HEAR: &str = r#"
document: { dsl: '1.0.3', namespace: rt, name: hear, version: '1.0.0' }
do:
  - ping:
      listen:
        to:
          one:
            with: { type: rt.ping }
  - got:
      set: { said: "${ .[0].say }" }
"#;

async fn world() -> (Arc<dyn StorageBackend>, tempfile::TempDir) {
    INIT.call_once(|| std::env::set_var("FLOWR_RUN_LEASE_MS", "600"));
    let dir = tempfile::tempdir().unwrap();
    let s = flowr_ce::init_storage_with(dir.path().to_str().unwrap(), true).await.unwrap();
    s.workflows().register(WorkflowRecord::from_yaml(FLOW).unwrap()).await.unwrap();
    s.workflows().register(WorkflowRecord::from_yaml(HEAR).unwrap()).await.unwrap();
    flowr_ce::spawn_worker(s.clone());
    (s, dir)
}

async fn call(s: &dyn StorageBackend, name: &str, args: Value) -> Value {
    let me = flowr_policy::Principal::human("usr-dev", vec!["Admin".into()]);
    let r = flowr_mcp_core::tools::handle_tool_call(s, &me, name, Some(args)).await;
    let text = match &r.content[0] {
        ToolContent::Text { text } => text.clone(),
        #[allow(unreachable_patterns)]
        _ => String::new(),
    };
    assert!(r.is_error != Some(true), "{name}: {text}");
    serde_json::from_str(&text).unwrap_or(Value::String(text))
}

async fn until(s: &dyn StorageBackend, id: &str, phase: &str) -> Value {
    let deadline = std::time::Instant::now() + Duration::from_secs(20);
    loop {
        let r = call(s, "get_run", json!({ "run_id": id })).await;
        if r["phase"] == phase {
            return r;
        }
        assert!(std::time::Instant::now() < deadline, "waiting for {phase}: {r}");
        tokio::time::sleep(Duration::from_millis(40)).await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn debug_a_run_from_the_tools() {
    let (s, _d) = world().await;
    let started = call(s.as_ref(), "start_run", json!({
        "namespace": "rt", "name": "pay", "debug": true,
        "breakpoints": [{ "step_key": "charge", "when": "before" }]
    })).await;
    let id = started["run_id"].as_str().unwrap().to_string();

    let paused = until(s.as_ref(), &id, "suspended").await;
    assert_eq!(paused["paused"]["step_key"], "/charge");
    assert_eq!(paused["paused"]["when"], "before");
    assert_eq!(paused["steps"][0]["step_key"], "/prepare");
    assert_eq!(paused["steps"][0]["status"], "completed");
    assert_eq!(paused["can_steer"], true);

    // The journal shows prepare's output — with the key masked.
    let j = call(s.as_ref(), "get_run_journal", json!({ "run_id": id })).await;
    let prep = j["entries"].as_array().unwrap().iter().find(|e| e["record"]["step_key"] == "/prepare").unwrap();
    assert_eq!(prep["record"]["output"]["api_key"], "****");
    assert_eq!(prep["record"]["output"]["amount"], 10);

    call(s.as_ref(), "edit_step_input", json!({ "run_id": id, "step_key": "/charge", "input": { "amount": 25 } })).await;
    call(s.as_ref(), "debug_run", json!({ "run_id": id, "command": "continue" })).await;
    let done = until(s.as_ref(), &id, "completed").await;
    assert_eq!(done["output"]["charged"], 25, "charge ran with the edited input");
    let charge = done["steps"].as_array().unwrap().iter().find(|x| x["step_key"] == "/charge").unwrap();
    assert_eq!(charge["edited"], true);

    // Re-run from after `prepare`: prepare replays, the rest runs again.
    let prep_seq = done["steps"][0]["seq"].as_u64().unwrap();
    let again = call(s.as_ref(), "rerun_run", json!({ "run_id": id, "from_seq": prep_seq, "debug": false })).await;
    assert!(again["replayed_records"].as_u64().unwrap() >= 1);
    let id2 = again["run_id"].as_str().unwrap().to_string();
    let done2 = until(s.as_ref(), &id2, "completed").await;
    assert_eq!(done2["output"]["charged"], 10, "the original input this time");
    assert!(done2["trigger"].as_str().unwrap().starts_with("rerun:"));

    let list = call(s.as_ref(), "list_runs", json!({ "namespace": "rt", "limit": 10 })).await;
    let ids: Vec<&str> = list["runs"].as_array().unwrap().iter().map(|r| r["run_id"].as_str().unwrap()).collect();
    assert_eq!(ids, [id2.as_str(), id.as_str()], "newest first");
}

#[tokio::test(flavor = "multi_thread")]
async fn edits_only_where_paused_and_cancel() {
    let (s, _d) = world().await;
    let id = call(s.as_ref(), "start_run", json!({ "namespace": "rt", "name": "pay", "debug": true, "pause_on_start": true })).await["run_id"]
        .as_str().unwrap().to_string();
    let p = until(s.as_ref(), &id, "suspended").await;
    assert_eq!(p["paused"]["step_key"], "/prepare");
    let me = flowr_policy::Principal::human("usr-dev", vec!["Admin".into()]);
    let wrong = flowr_mcp_core::tools::handle_tool_call(s.as_ref(), &me, "edit_step_input",
        Some(json!({ "run_id": id, "step_key": "/receipt", "input": {} }))).await;
    assert_eq!(wrong.is_error, Some(true), "not paused before /receipt");
    call(s.as_ref(), "cancel_run", json!({ "run_id": id, "reason": "enough" })).await;
    let c = until(s.as_ref(), &id, "cancelled").await;
    assert_eq!(c["error"], "enough");
}

#[tokio::test(flavor = "multi_thread")]
async fn send_event_wakes_a_listening_run() {
    let (s, _d) = world().await;
    let id = call(s.as_ref(), "start_run", json!({ "namespace": "rt", "name": "hear" })).await["run_id"].as_str().unwrap().to_string();
    let waiting = until(s.as_ref(), &id, "waiting").await;
    let step = waiting["steps"].as_array().unwrap().iter().find(|x| x["step_key"] == "/ping").unwrap().clone();
    assert_eq!(step["status"], "waiting");
    assert_eq!(step["listening"]["filters"][0]["type"], "rt.ping");

    let sent = call(s.as_ref(), "send_event", json!({ "namespace": "rt", "event": { "type": "rt.other" } })).await;
    assert_eq!(sent["woke"], json!([]));
    let sent = call(s.as_ref(), "send_event", json!({ "namespace": "rt", "event": { "type": "rt.ping", "data": { "say": "hi" } } })).await;
    assert_eq!(sent["woke"], json!([id]));
    let done = until(s.as_ref(), &id, "completed").await;
    assert_eq!(done["output"]["said"], "hi");
}
