//! `timeout.after` on a task: the task is stopped at the limit with a
//! timeout error, which `try`/`catch` handles like any other.

use serde_json::{json, Value};
use swf_parser::parse_yaml;
use swf_runtime::{ExecutionContext, Principal, RuntimeInfo, WorkflowExecutor};

const FLOW: &str = r#"
document: { dsl: '1.0.3', namespace: t, name: slow, version: '1.0.0' }
do:
  - guard:
      try:
        do:
          - nap:
              wait: { seconds: 3 }
              timeout: { after: { seconds: 1 } }
        catch:
          do:
            - fallback:
                set: { timed_out: true }
"#;

#[tokio::test]
async fn a_task_over_its_timeout_is_stopped_and_catchable() {
    let wf = parse_yaml(FLOW).unwrap();
    let mut ex = WorkflowExecutor::new();
    ex.set_eval_fn(|input, expr, context| {
        let engine = flowr_expr::ExpressionEngine::new();
        let scope = flowr_expr::Scope::builder().context(context.clone()).build();
        engine.eval(input, expr, &scope).map_err(|e| e.to_string())
    });
    let mut ctx = ExecutionContext {
        instance_id: "r".into(), namespace: "t".into(), name: "slow".into(), version: "1.0.0".into(),
        context: Value::Null, secrets: Default::default(), runtime: RuntimeInfo::default(), storage: None,
        http_client: None, extensions: Vec::new(), eval_fn: None,
        principal: Principal::human("u", vec![]), template_meta: Default::default(),
    };
    let t = std::time::Instant::now();
    let out = ex.execute(&wf, json!({}), &mut ctx).await.unwrap();
    assert_eq!(out["timed_out"], true);
    assert!(t.elapsed() < std::time::Duration::from_millis(2500), "stopped at the limit");
}
