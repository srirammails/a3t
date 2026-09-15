//! run:shell handler — executes shell commands

use async_trait::async_trait;
use serde_json::{json, Value};
use tracing::info;

use swf_parser::{NamedTask, Task, RunTask};
use crate::context::ExecutionContext;
use crate::error::{RuntimeError, Result};
use crate::traits::{TaskHandler, TaskResult};

pub struct ShellHandler;

#[async_trait]
impl TaskHandler for ShellHandler {
    fn task_type(&self) -> &str {
        "run:shell"
    }

    async fn execute(
        &self,
        task: &NamedTask,
        _input: Value,
        _ctx: &mut ExecutionContext,
    ) -> Result<TaskResult> {
        let command = match &task.task {
            Task::Run(RunTask::Shell(shell)) => &shell.command,
            _ => return Err(RuntimeError::task_failed(&task.name, "expected run:shell task")),
        };

        info!(task = %task.name, command = %command, "executing shell command");

        let output = tokio::process::Command::new("sh")
            .arg("-c")
            .arg(command)
            .output()
            .await
            .map_err(|e| RuntimeError::task_failed(&task.name, format!("shell execution failed: {}", e)))?;

        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let exit_code = output.status.code().unwrap_or(-1);

        if !output.status.success() {
            return Err(RuntimeError::task_failed(
                &task.name,
                format!("command exited with code {}: {}", exit_code, stderr),
            ));
        }

        Ok(TaskResult::new(json!({
            "stdout": stdout,
            "stderr": stderr,
            "exit_code": exit_code,
        })))
    }
}
