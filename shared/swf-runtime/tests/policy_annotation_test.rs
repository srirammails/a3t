//! Inline AESOP policy declared in the SWF YAML (`metadata.a3t.policy`).
//!
//! Isolated test binary: installs a *recording* hook that captures every
//! `ExecutionRequest` and allows it, so we can assert exactly what the choke
//! point authorized against. Proves a step annotated in YAML reaches Cedar as a
//! classified resource with **no handler-side Rust at all** — the handler here
//! is a bare NoopHandler with the default `policy_resource`.

use std::sync::{Arc, Mutex};

use serde_json::{json, Value};
use swf_parser::parse_yaml;
use swf_runtime::traits::{TaskHandler, TaskResult};
use swf_runtime::{ExecutionContext, RuntimeInfo, WorkflowExecutor};

// One process, one hook: both tests share a single recorder installed once, and
// each filters the log for its own step. (Installing a hook per test races —
// the second install wins and the first test's records land in the wrong vec.)
static SEEN: Mutex<Vec<flowr_policy::Resource>> = Mutex::new(Vec::new());
static INSTALL: std::sync::Once = std::sync::Once::new();

struct RecordingHook;

#[async_trait::async_trait]
impl flowr_policy::PolicyHook for RecordingHook {
    async fn check(&self, req: &flowr_policy::ExecutionRequest) -> flowr_policy::PolicyDecision {
        SEEN.lock().unwrap().push(req.resource.clone());
        flowr_policy::PolicyDecision::Allow
    }
}

fn install_recorder() {
    INSTALL.call_once(|| {
        let _ = flowr_policy::set_policy_hook(Arc::new(RecordingHook));
    });
}

/// The recorded resource for a given step's task type.
fn recorded(task_type: &str) -> flowr_policy::Resource {
    SEEN.lock()
        .unwrap()
        .iter()
        .find(|r| r.id == task_type)
        .unwrap_or_else(|| panic!("no policy check recorded for {task_type}"))
        .clone()
}

struct NoopHandler(&'static str);

#[async_trait::async_trait]
impl TaskHandler for NoopHandler {
    fn task_type(&self) -> &str {
        self.0
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

fn ctx() -> ExecutionContext {
    ExecutionContext {
        instance_id: "run-1".to_string(),
        namespace: "a3t".to_string(),
        name: "annotated".to_string(),
        version: "1.0.0".to_string(),
        context: Value::Null,
        secrets: Default::default(),
        runtime: RuntimeInfo::default(),
        storage: None,
        http_client: None,
        extensions: Vec::new(),
        eval_fn: None,
        principal: swf_runtime::default_flow_principal(),
        template_meta: Default::default(),
    }
}

// A step whose policy is declared entirely in YAML — including an `id`
// expression resolved from the step input, exactly as templates_crud.yaml
// writes `${ .input.id }`.
const WORKFLOW: &str = r#"
document:
  dsl: '1.0.3'
  namespace: a3t
  name: annotated
  version: '1.0.0'
do:
  - delete-template:
      call: lancedb
      with:
        operation: delete
        table: template_registry
      metadata:
        a3t:
          policy:
            op: delete
            resource:
              type: Template
              id: ${ .input.id }
              attrs:
                table: template_registry
"#;

#[tokio::test]
async fn yaml_annotation_reaches_the_policy_hook_with_no_handler_code() {
    install_recorder();

    let wf = parse_yaml(WORKFLOW).expect("workflow parses");
    let mut executor = WorkflowExecutor::new();
    executor.register_handler(Arc::new(NoopHandler("call:lancedb")));
    // Real jq evaluation, as flowr-ce wires in production.
    let engine = flowr_expr::ExpressionEngine::new();
    executor.set_eval_fn(move |input, expr, context| {
        let scope = flowr_expr::Scope::builder().context(context.clone()).build();
        engine.eval(input, expr, &scope).map_err(|e| e.to_string())
    });

    let mut c = ctx();
    // `.input.id` matches the real convention: templates_crud.yaml reads
    // `${ .input.id }` and the console wraps its payload as { input: {...} }.
    executor
        .execute(&wf, json!({ "input": { "id": "tpl-42" } }), &mut c)
        .await
        .expect("flow runs");

    let r = recorded("call:lancedb");

    // Classified purely from YAML — NoopHandler overrides nothing.
    assert_eq!(
        r.attributes.get("operation").and_then(|v| v.as_str()),
        Some("delete"),
        "annotation supplies the operation bucket"
    );
    assert_eq!(
        r.attributes.get("object_type").and_then(|v| v.as_str()),
        Some("Template")
    );
    assert_eq!(
        r.attributes.get("table").and_then(|v| v.as_str()),
        Some("template_registry")
    );
    // The entity id stays the handler's trusted value, never the annotation's.
    assert_eq!(r.id, "call:lancedb");
    // The `${ .input.id }` expression was evaluated against the step input.
    assert_eq!(
        r.attributes.get("object_id").and_then(|v| v.as_str()),
        Some("tpl-42"),
        "the id expression is evaluated in the step's context"
    );
}


// `run:` tasks reach the same choke point, so the annotation applies there too —
// this is the other half of the call/run enforced surface.
const RUN_WORKFLOW: &str = r#"
document:
  dsl: '1.0.3'
  namespace: a3t
  name: annotated-run
  version: '1.0.0'
do:
  - purge:
      run:
        script:
          language: python
          code: "print('x')"
      metadata:
        a3t:
          policy:
            op: delete
            resource:
              type: Dataset
              id: ${ .input.dataset }
"#;

#[tokio::test]
async fn annotation_applies_to_run_tasks_too() {
    install_recorder();

    let wf = parse_yaml(RUN_WORKFLOW).expect("workflow parses");
    let mut executor = WorkflowExecutor::new();
    executor.register_handler(Arc::new(NoopHandler("run:script")));
    let engine = flowr_expr::ExpressionEngine::new();
    executor.set_eval_fn(move |input, expr, context| {
        let scope = flowr_expr::Scope::builder().context(context.clone()).build();
        engine.eval(input, expr, &scope).map_err(|e| e.to_string())
    });

    let mut c = ctx();
    executor
        .execute(&wf, json!({ "input": { "dataset": "ds-9" } }), &mut c)
        .await
        .expect("flow runs");

    let r = recorded("run:script");
    assert_eq!(r.attributes.get("operation").and_then(|v| v.as_str()), Some("delete"));
    assert_eq!(r.attributes.get("object_type").and_then(|v| v.as_str()), Some("Dataset"));
    assert_eq!(r.attributes.get("object_id").and_then(|v| v.as_str()), Some("ds-9"));
    assert_eq!(r.id, "run:script");
}
