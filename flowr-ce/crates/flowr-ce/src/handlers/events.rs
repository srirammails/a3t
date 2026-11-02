//! `emit` and `listen` — CloudEvents between runs (`.ai/FLOWR_QUEUE_CHECKPOINT_SPEC.md` §5.9).
//!
//! `emit` delivers an event in the flow's namespace: every run whose `listen`
//! step it completes is woken. `listen` itself runs in the journal hook (the
//! run lets go while it waits); this handler only answers runs without a
//! journal, which cannot wait.

use async_trait::async_trait;
use serde_json::Value;

use swf_parser::{NamedTask, Task};
use swf_runtime::context::ExecutionContext;
use swf_runtime::error::{Result, RuntimeError};
use swf_runtime::traits::{TaskHandler, TaskResult};

use flowr_storage::StorageBackend;
use std::sync::Arc;

pub struct EmitHandler;

#[async_trait]
impl TaskHandler for EmitHandler {
    fn task_type(&self) -> &str {
        "emit"
    }

    async fn execute(&self, task: &NamedTask, input: Value, ctx: &mut ExecutionContext) -> Result<TaskResult> {
        let Task::Emit(emit) = &task.task else {
            return Err(RuntimeError::task_failed(&task.name, "not an emit task"));
        };
        let mut with = serde_json::to_value(&emit.event.with).map_err(|e| RuntimeError::Internal(e.to_string()))?;
        // The same id on every attempt of this step, so a repeat (after a
        // crash) is dropped by the store; an id given in the flow wins.
        if let (Some(key), Some(w)) = (swf_runtime::effects::step_key(), with.as_object_mut()) {
            w.entry("id").or_insert_with(|| Value::String(format!("{}:{key}", ctx.instance_id)));
        }
        let event = flowr_storage::engine::cloud_event(&with, &format!("/flowr/{}/{}", ctx.namespace, ctx.name));
        let storage = ctx
            .storage
            .as_ref()
            .and_then(|s| s.downcast_ref::<Arc<dyn StorageBackend>>().cloned())
            .ok_or_else(|| RuntimeError::Internal("storage not available".into()))?;
        let out = crate::events::publish(storage.as_ref(), &ctx.namespace, event.clone(), Some(ctx.principal.clone()))
            .await
            .map_err(|e| RuntimeError::task_failed(&task.name, format!("emit: {e}")))?;
        tracing::debug!(task = %task.name, event_type = %event["type"], result = %out, "event emitted");
        // The step's output is its input, as the spec has it for `emit`.
        Ok(TaskResult::new(input))
    }
}

pub struct ListenHandler;

#[async_trait]
impl TaskHandler for ListenHandler {
    fn task_type(&self) -> &str {
        "listen"
    }

    async fn execute(&self, task: &NamedTask, _input: Value, _ctx: &mut ExecutionContext) -> Result<TaskResult> {
        Err(RuntimeError::task_failed(
            &task.name,
            "listen needs a durable run: this run has no journal (ephemeral, or FLOWR_STORE=lance), so it cannot wait for events",
        ))
    }
}
