//! E0.4 (site B) — the flow's principal, set on `ExecutionContext`, reaches the
//! choke point's `ExecutionRequest`.
//!
//! Isolated test binary: installs a *recording* hook that captures every
//! `ExecutionRequest` and then allows it. (One process, one hook — so this
//! can't share a binary with the DenyAll suite.) We set a real `Agent`
//! principal on the context, run a flow, and assert the executor authorized
//! against that exact principal — not the `Service{"flow-runtime"}` default.

use std::sync::{Arc, Mutex};

use serde_json::{json, Value};
use swf_parser::parse_yaml;
use swf_runtime::traits::{TaskHandler, TaskResult};
use swf_runtime::{ExecutionContext, Principal, RuntimeInfo, WorkflowExecutor};

/// Records the principal of every checked request, then allows.
struct RecordingHook {
    seen: Arc<Mutex<Vec<flowr_policy::Principal>>>,
}

#[async_trait::async_trait]
impl flowr_policy::PolicyHook for RecordingHook {
    async fn check(&self, req: &flowr_policy::ExecutionRequest) -> flowr_policy::PolicyDecision {
        self.seen.lock().unwrap().push(req.principal.clone());
        flowr_policy::PolicyDecision::Allow
    }
}

struct NoopHandler {
    task_type: String,
}

#[async_trait::async_trait]
impl TaskHandler for NoopHandler {
    fn task_type(&self) -> &str {
        &self.task_type
    }
    async fn execute(
        &self,
        _task: &swf_parser::NamedTask,
        _input: Value,
        _ctx: &mut ExecutionContext,
    ) -> swf_runtime::error::Result<TaskResult> {
        Ok(TaskResult::new(json!({ "ok": true })))
    }
}

fn ctx_with(principal: Principal) -> ExecutionContext {
    ExecutionContext {
        instance_id: "run-42".to_string(),
        namespace: "acme".to_string(),
        name: "campaign".to_string(),
        version: "1.0.0".to_string(),
        context: Value::Null,
        secrets: Default::default(),
        runtime: RuntimeInfo::default(),
        storage: None,
        http_client: None,
        extensions: Vec::new(),
        eval_fn: None,
        principal,
        template_meta: Default::default(),
    }
}

const WORKFLOW: &str = r#"
document:
  dsl: '1.0.3'
  namespace: acme
  name: campaign
  version: '1.0.0'
do:
  - act:
      call: do_thing
"#;

#[tokio::test]
async fn ctx_principal_reaches_choke_point_and_context_is_populated() {
    let seen = Arc::new(Mutex::new(Vec::new()));
    flowr_policy::set_policy_hook(Arc::new(RecordingHook { seen: seen.clone() }))
        .expect("first and only hook install in this binary");

    let actor = Principal::Agent {
        agent_id: flowr_policy::AgentId("agent-007".into()),
        on_behalf_of: Some("user::sriram".into()),
        flow_id: Some("acme/campaign".into()),
        roles: vec!["Viewer".to_string()],
        owner: Some("user::sriram".into()),
    };

    let wf = parse_yaml(WORKFLOW).unwrap();
    let mut executor = WorkflowExecutor::new();
    executor.register_handler(Arc::new(NoopHandler {
        task_type: "call:do_thing".to_string(),
    }));

    let mut ctx = ctx_with(actor.clone());
    executor.execute(&wf, json!({}), &mut ctx).await.unwrap();

    let seen = seen.lock().unwrap();
    assert_eq!(seen.len(), 1, "the single step was authorized exactly once");

    // The principal the executor asked about is the one we put on the context,
    // not the runtime-service default.
    match &seen[0] {
        Principal::Agent { agent_id, on_behalf_of, .. } => {
            assert_eq!(agent_id.0, "agent-007");
            assert_eq!(on_behalf_of.as_deref(), Some("user::sriram"));
        }
        other => panic!("expected the threaded Agent principal, got {other:?}"),
    }
}

#[tokio::test]
async fn default_principal_is_the_runtime_service() {
    // A context built with the default helper authorizes as the service — the
    // behavior preserved for callers that have no better identity yet.
    let p = swf_runtime::default_flow_principal();
    match p {
        Principal::Service { service_id } => assert_eq!(service_id, "flow-runtime"),
        other => panic!("default flow principal must be a Service, got {other:?}"),
    }
}
