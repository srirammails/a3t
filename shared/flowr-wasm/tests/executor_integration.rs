//! End-to-end: a WASM module registered as a `TaskHandler` runs as a real flow
//! step through the `WorkflowExecutor` — dispatched via the E0.2 policy choke
//! point (AllowAll in CE), reporting `source() == "wasm"`.

use std::sync::Arc;

use serde_json::{json, Value};
use swf_parser::parse_yaml;
use swf_runtime::traits::TaskHandler;
use swf_runtime::{ExecutionContext, RuntimeInfo, WorkflowExecutor};

use flowr_wasm::WasmTaskHandler;

/// Guest that echoes its JSON input back through linear memory (the ABI).
const ECHO_WAT: &str = r#"
(module
  (import "aesop_host" "log" (func $log (param i32 i32)))
  (memory (export "memory") 1)
  (global $bump (mut i32) (i32.const 1024))
  (func (export "alloc") (param $size i32) (result i32)
    (local $p i32)
    (local.set $p (global.get $bump))
    (global.set $bump (i32.add (global.get $bump) (local.get $size)))
    (local.get $p))
  (func (export "run") (param $ptr i32) (param $len i32) (result i64)
    (call $log (local.get $ptr) (local.get $len))
    (i64.or
      (i64.shl (i64.extend_i32_u (local.get $ptr)) (i64.const 32))
      (i64.extend_i32_u (local.get $len)))))
"#;

fn make_ctx() -> ExecutionContext {
    ExecutionContext {
        instance_id: "wasm-run-1".to_string(),
        namespace: "test".to_string(),
        name: "wasm".to_string(),
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

#[tokio::test]
async fn wasm_task_runs_as_a_flow_step() {
    // A WASM module registered for the `call:echo` task type.
    let handler = WasmTaskHandler::from_bytes("call:echo", ECHO_WAT).unwrap();
    assert_eq!(handler.source(), "wasm");

    let yaml = r#"
document:
  dsl: '1.0.3'
  namespace: test
  name: wasm
  version: '1.0.0'
do:
  - echo:
      call: echo
      with:
        msg: hello
        n: 42
"#;
    let wf = parse_yaml(yaml).unwrap();
    let mut executor = WorkflowExecutor::new();
    executor.register_handler(Arc::new(handler));

    let mut ctx = make_ctx();
    // Runs through the executor → choke point (AllowAll) → WasmTaskHandler.
    let result = executor.execute(&wf, json!({}), &mut ctx).await.unwrap();

    // The WASM guest echoed its input (the resolved `with`) back out.
    assert_eq!(result["msg"], "hello");
    assert_eq!(result["n"], 42);
}
