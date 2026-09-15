//! run:script handler — executes inline scripts (Lua, JavaScript, Python)
//!
//! Stub for now — full implementation will use mlua, rquickjs, rustpython
//! imported from the ClawdFlow layer.

use async_trait::async_trait;
use serde_json::{json, Value};

use swf_parser::{NamedTask, Task, RunTask};
use crate::context::ExecutionContext;
use crate::error::{RuntimeError, Result};
use crate::traits::{TaskHandler, TaskResult};

pub struct ScriptHandler;

#[async_trait]
impl TaskHandler for ScriptHandler {
    fn task_type(&self) -> &str {
        "run:script"
    }

    async fn execute(
        &self,
        task: &NamedTask,
        _input: Value,
        _ctx: &mut ExecutionContext,
    ) -> Result<TaskResult> {
        let (language, code) = match &task.task {
            Task::Run(RunTask::Script(script)) => {
                (&script.language, script.code.as_deref().unwrap_or(""))
            }
            _ => return Err(RuntimeError::task_failed(&task.name, "expected run:script task")),
        };

        // TODO: Integrate actual script runtimes from clawdflow-tasks
        Err(RuntimeError::task_failed(
            &task.name,
            format!("run:script handler not yet implemented for language '{}'. Code: {}...", language, &code[..code.len().min(50)]),
        ))
    }
}
