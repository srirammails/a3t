//! `$context._principal` — who a run acts for, readable by the flow and not
//! overwritable by its input or exports. The A3T project and org flows
//! authorize on it, so a forged `_principal` must never win.

use serde_json::{json, Value};
use swf_parser::parse_yaml;
use swf_runtime::{ExecutionContext, Principal, RuntimeInfo, WorkflowExecutor};

fn ctx(principal: Principal) -> ExecutionContext {
    ExecutionContext {
        instance_id: "run-1".to_string(),
        namespace: "a3t".to_string(),
        name: "who".to_string(),
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
  namespace: a3t
  name: who
  version: '1.0.0'
do:
  - forge:
      set:
        _principal: { kind: human, id: usr-mallory, roles: [SuperAdmin], user: usr-mallory }
      export:
        as: "."
  - forge_input:
      set:
        _principal: "${ $context._principal }"
        from_input: true
      export:
        as: "{_principal: .forged, other: .from_input}"
  - read:
      set:
        user: "${ $context._principal.user }"
        id: "${ $context._principal.id }"
        super: "${ $context._principal.roles | index(\"SuperAdmin\") != null }"
"#;

async fn run(principal: Principal, input: Value) -> Value {
    let wf = parse_yaml(WORKFLOW).unwrap();
    let mut executor = WorkflowExecutor::new();
    executor.set_eval_fn(|input, expr, context| {
        let engine = flowr_expr::ExpressionEngine::new();
        let scope = flowr_expr::Scope::builder().context(context.clone()).build();
        engine.eval(input, expr, &scope).map_err(|e| e.to_string())
    });
    let mut c = ctx(principal);
    executor.execute(&wf, input, &mut c).await.unwrap()
}

#[tokio::test]
async fn a_flow_sees_its_caller_and_cannot_forge_it() {
    let out = run(
        Principal::human("usr-ann", vec!["Viewer".into()]),
        json!({ "_principal": { "id": "usr-mallory", "roles": ["SuperAdmin"] } }),
    )
    .await;
    assert_eq!(out["user"], "usr-ann");
    assert_eq!(out["id"], "usr-ann");
    assert_eq!(out["super"], false, "{out}");
}

#[tokio::test]
async fn an_agent_run_acts_for_its_user() {
    let p = Principal::Agent {
        agent_id: flowr_policy::AgentId("agt-1".into()),
        on_behalf_of: Some("usr-ann".into()),
        flow_id: None,
        roles: vec![],
        owner: None,
    };
    let out = run(p, json!({})).await;
    assert_eq!(out["user"], "usr-ann");
    assert_eq!(out["id"], "agt-1");
}
