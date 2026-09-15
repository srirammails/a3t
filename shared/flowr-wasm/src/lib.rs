//! FlowR-CE WASM task runtime.
//!
//! Loads sandboxed WASM modules as [`swf_runtime::TaskHandler`]s. This is CE
//! **infrastructure** (the mechanism to run tasks in a sandbox); the
//! fine-grained *control* — which principal may load/use/execute which module,
//! and with what capabilities — is EE's job, decided by Cedar against the
//! `aesop.task.source = "wasm"` dimension the choke point already records.
//!
//! ## Core-module ABI (guest exports)
//! - `memory`                        — exported linear memory
//! - `alloc(size: i32) -> i32`       — reserve `size` bytes, return the offset
//! - `run(ptr: i32, len: i32) -> i64`— process JSON input at `[ptr, len]`;
//!   return a packed pointer `(out_ptr << 32) | out_len` to JSON output.
//!
//! ## Host imports (capabilities)
//! Module `"aesop_host"`: `log(ptr, len)` today. Additional capabilities
//! (storage, http, …) are added to the [`Linker`] and granted per policy. A
//! module importing a host function the linker does not provide **fails to
//! instantiate** — that is the capability boundary.
//!
//! The component model (WIT interfaces) is the planned upgrade; the core-module
//! import mechanism above already gives per-module capability control.

use async_trait::async_trait;
use serde_json::Value;
use swf_parser::{NamedTask, Task};
use swf_runtime::context::ExecutionContext;
use swf_runtime::error::{Result as RtResult, RuntimeError};
use swf_runtime::traits::{TaskHandler, TaskResult};
use wasmtime::{Caller, Engine, Linker, Module, Store};

/// Host-side state reachable by a WASM module's imported functions — the
/// capability surface. A module can only affect the host through what the
/// [`Linker`] grants.
#[derive(Default)]
pub struct HostState {
    /// Lines the module emitted via the `log` host function.
    pub logs: Vec<String>,
}

/// A task handler backed by a sandboxed WASM module (see the crate-level ABI).
pub struct WasmTaskHandler {
    task_type: String,
    engine: Engine,
    module: Module,
}

impl WasmTaskHandler {
    /// Compile a handler from WASM bytes — or WAT text; wasmtime accepts both.
    pub fn from_bytes(task_type: impl Into<String>, wasm: impl AsRef<[u8]>) -> anyhow::Result<Self> {
        let engine = Engine::default();
        let module = Module::new(&engine, wasm)?;
        Ok(Self { task_type: task_type.into(), engine, module })
    }

    /// Load a handler from a `.wasm` (or `.wat`) file on disk.
    pub fn from_file(
        task_type: impl Into<String>,
        path: impl AsRef<std::path::Path>,
    ) -> anyhow::Result<Self> {
        let bytes = std::fs::read(path)?;
        Self::from_bytes(task_type, bytes)
    }
}

/// Instantiate `module` and run it against `input` (JSON bytes), returning the
/// output JSON bytes plus any host-`log` lines. Synchronous CPU + sandbox work,
/// so callers run it inside `spawn_blocking`.
fn run_module(engine: &Engine, module: &Module, input: &[u8]) -> anyhow::Result<(Vec<u8>, Vec<String>)> {
    let mut store = Store::new(engine, HostState::default());
    let mut linker: Linker<HostState> = Linker::new(engine);

    // Capability: `log`. Reading it wires the module's only channel to the host.
    linker.func_wrap(
        "aesop_host",
        "log",
        |mut caller: Caller<'_, HostState>, ptr: i32, len: i32| {
            let Some(mem) = caller.get_export("memory").and_then(|e| e.into_memory()) else {
                return;
            };
            let data = mem.data(&caller);
            let start = ptr as usize;
            let end = start.saturating_add(len as usize);
            let msg = data
                .get(start..end)
                .map(|b| String::from_utf8_lossy(b).into_owned())
                .unwrap_or_default();
            caller.data_mut().logs.push(msg);
        },
    )?;

    let instance = linker.instantiate(&mut store, module)?;
    let memory = instance
        .get_memory(&mut store, "memory")
        .ok_or_else(|| anyhow::anyhow!("wasm task: no exported `memory`"))?;
    let alloc = instance.get_typed_func::<i32, i32>(&mut store, "alloc")?;
    let run = instance.get_typed_func::<(i32, i32), i64>(&mut store, "run")?;

    // Write input into guest memory, then execute.
    let in_ptr = alloc.call(&mut store, input.len() as i32)?;
    memory.write(&mut store, in_ptr as usize, input)?;
    let packed = run.call(&mut store, (in_ptr, input.len() as i32))?;

    let out_ptr = ((packed as u64) >> 32) as usize;
    let out_len = ((packed as u64) & 0xFFFF_FFFF) as usize;
    let mut out = vec![0u8; out_len];
    memory.read(&store, out_ptr, &mut out)?;

    let logs = std::mem::take(&mut store.data_mut().logs);
    Ok((out, logs))
}

#[async_trait]
impl TaskHandler for WasmTaskHandler {
    fn task_type(&self) -> &str {
        &self.task_type
    }

    /// Reported on the `tool_call` span as `aesop.task.source` — the seam EE's
    /// Cedar policy uses to gate WASM plugins apart from trusted builtins.
    fn source(&self) -> &str {
        "wasm"
    }

    async fn execute(
        &self,
        task: &NamedTask,
        input: Value,
        _ctx: &mut ExecutionContext,
    ) -> RtResult<TaskResult> {
        let task_name = task.name.clone();
        // A Call task's declared arguments live in the (already expression-
        // resolved) `with`; fall back to the flow-data `input` otherwise. This
        // matches how the builtin call:* handlers read their input.
        let module_input = match &task.task {
            Task::Call(c) => c.with.clone().unwrap_or_else(|| input.clone()),
            _ => input.clone(),
        };
        let input_bytes = serde_json::to_vec(&module_input)
            .map_err(|e| RuntimeError::task_failed(&task_name, format!("wasm input encode: {e}")))?;
        let engine = self.engine.clone();
        let module = self.module.clone();

        let (out, logs) =
            tokio::task::spawn_blocking(move || run_module(&engine, &module, &input_bytes))
                .await
                .map_err(|e| RuntimeError::task_failed(&task_name, format!("wasm join: {e}")))?
                .map_err(|e| RuntimeError::task_failed(&task_name, format!("wasm exec: {e}")))?;

        for line in logs {
            tracing::debug!(task = %task_name, "wasm: {line}");
        }

        let value: Value = serde_json::from_slice(&out)
            .map_err(|e| RuntimeError::task_failed(&task_name, format!("wasm output decode: {e}")))?;
        Ok(TaskResult::new(value))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Minimal guest implementing the ABI: it `log`s the input (proving the
    /// granted capability works) and echoes it back through linear memory
    /// (proving the host↔guest JSON data path both directions).
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

    #[test]
    fn wasm_abi_roundtrip_and_capability() {
        let engine = Engine::default();
        let module = Module::new(&engine, ECHO_WAT).unwrap();
        let input = br#"{"hello":"wasm"}"#;
        let (out, logs) = run_module(&engine, &module, input).unwrap();
        assert_eq!(&out, input, "guest echoed input back through linear memory");
        assert_eq!(
            logs,
            vec![r#"{"hello":"wasm"}"#.to_string()],
            "guest reached the host through the granted `log` capability"
        );
    }

    #[test]
    fn ungranted_import_is_rejected_at_the_boundary() {
        // A module importing a host function the linker does NOT provide must
        // fail to instantiate — this is the capability sandbox in action.
        const WANTS_FORBIDDEN: &str = r#"
        (module
          (import "aesop_host" "exfiltrate" (func $x (param i32 i32)))
          (memory (export "memory") 1)
          (func (export "alloc") (param i32) (result i32) (i32.const 1024))
          (func (export "run") (param i32) (param i32) (result i64) (i64.const 0)))
        "#;
        let engine = Engine::default();
        let module = Module::new(&engine, WANTS_FORBIDDEN).unwrap();
        let err = run_module(&engine, &module, b"{}").unwrap_err();
        let msg = err.to_string().to_lowercase();
        assert!(
            msg.contains("exfiltrate") || msg.contains("import") || msg.contains("unknown"),
            "instantiation must reject the ungranted import, got: {msg}"
        );
    }

    #[test]
    fn handler_reports_wasm_source() {
        let h = WasmTaskHandler::from_bytes("call:wasm:echo", ECHO_WAT).unwrap();
        assert_eq!(h.task_type(), "call:wasm:echo");
        assert_eq!(h.source(), "wasm");
    }
}
