//! E0.3 — Deny semantics through the *real* executor choke point (site B).
//!
//! This is an isolated test binary on purpose: it installs a process-wide
//! `DenyAll` policy hook via `flowr_policy::set_policy_hook`. Because the hook
//! lives in a `OnceLock`, every test in this file must expect denial — so the
//! whole binary is dedicated to the Deny path. The Allow path is covered by the
//! default (`AllowAll`) in `executor_test.rs`, which runs as a separate binary
//! and is therefore unaffected by the global set here.
//!
//! What we prove:
//!   1. A denied flow step returns `RuntimeError::Denied` (clean fail, not a
//!      panic, not `TaskFailed`, not `Ok`).
//!   2. The handler's `execute()` is **never called** — the side-effecting
//!      action does not run at all, so there is nothing to retry.
//!   3. The error surfaces out of `executor.execute()` carrying the hook's
//!      `reason` + `policy_ids`, and maps to the CNCF authorization error type.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use serde_json::{json, Value};
use swf_parser::parse_yaml;
use swf_runtime::error::RuntimeError;
use swf_runtime::traits::{TaskHandler, TaskResult};
use swf_runtime::{ExecutionContext, RuntimeInfo, WorkflowExecutor};

/// Install `DenyAll` once for this test binary. Idempotent: ignores the
/// "already set" error so concurrent `#[tokio::test]`s can all call it.
fn install_deny_all() {
    let _ = flowr_policy::set_policy_hook(Arc::new(flowr_policy::DenyAll));
}

fn make_ctx() -> ExecutionContext {
    ExecutionContext {
        instance_id: "deny-001".to_string(),
        namespace: "test".to_string(),
        name: "deny".to_string(),
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

/// A handler that records whether `execute()` ever ran. A denied step must
/// leave this at zero — the destructive action never fires.
struct SpyHandler {
    task_type: String,
    calls: Arc<AtomicUsize>,
}

#[async_trait::async_trait]
impl TaskHandler for SpyHandler {
    fn task_type(&self) -> &str {
        &self.task_type
    }

    async fn execute(
        &self,
        _task: &swf_parser::NamedTask,
        _input: Value,
        _ctx: &mut ExecutionContext,
    ) -> swf_runtime::error::Result<TaskResult> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok(TaskResult::new(json!({ "ran": true })))
    }
}

const WORKFLOW: &str = r#"
document:
  dsl: '1.0.3'
  namespace: test
  name: deny
  version: '1.0.0'
do:
  - danger:
      call: delete_everything
"#;

/// 1 + 2: denied step returns `Denied` and the handler never runs.
#[tokio::test]
async fn denied_step_fails_clean_and_handler_never_runs() {
    install_deny_all();

    let calls = Arc::new(AtomicUsize::new(0));
    let wf = parse_yaml(WORKFLOW).unwrap();
    let mut executor = WorkflowExecutor::new();
    executor.register_handler(Arc::new(SpyHandler {
        task_type: "call:delete_everything".to_string(),
        calls: calls.clone(),
    }));

    let mut ctx = make_ctx();
    let err = executor
        .execute(&wf, json!({}), &mut ctx)
        .await
        .expect_err("deny-all must fail the workflow");

    match err {
        RuntimeError::Denied { task, reason, policy_ids } => {
            assert_eq!(task, "danger");
            assert!(reason.contains("deny-all"), "reason carried through: {reason}");
            assert_eq!(policy_ids, vec!["deny-all".to_string()]);
        }
        other => panic!("expected RuntimeError::Denied, got {other:?}"),
    }

    // The side-effecting action must not have executed even once — a denied
    // destructive step has nothing to retry because it never started.
    assert_eq!(
        calls.load(Ordering::SeqCst),
        0,
        "handler.execute() must never be called for a denied step"
    );
}

/// 3: the Denied error maps to the CNCF authorization error type (403 class),
/// distinct from the generic runtime error type used by ordinary failures.
#[tokio::test]
async fn denied_maps_to_authorization_error_type() {
    install_deny_all();

    let denied = RuntimeError::Denied {
        task: "danger".into(),
        reason: "deny-all (test hook)".into(),
        policy_ids: vec!["deny-all".into()],
    };
    assert_eq!(
        denied.error_type_uri(),
        "https://serverlessworkflow.io/spec/1.0.0/errors/authorization",
        "Denied must advertise the authorization error type, not generic runtime"
    );

    // Display is human-readable and names the offending policies.
    let shown = denied.to_string();
    assert!(shown.contains("denied"), "display: {shown}");
    assert!(shown.contains("deny-all"), "display names policy: {shown}");
}
