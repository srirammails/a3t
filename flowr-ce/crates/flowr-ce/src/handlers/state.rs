//! state:get/set/delete handlers — key-value state store operations
//!
//! In CNCF syntax these are function calls:
//!   call: state:get   with: { key: "..." }
//!   call: state:set   with: { key: "...", value: ..., ttl: ... }
//!   call: state:delete with: { key: "..." }

use async_trait::async_trait;
use serde_json::{json, Value};
use tracing::debug;

use swf_parser::{NamedTask, Task};
use swf_runtime::context::ExecutionContext;
use swf_runtime::error::{RuntimeError, Result};
use swf_runtime::traits::{TaskHandler, TaskResult};

use flowr_storage::StorageBackend;
use std::sync::Arc;

fn get_storage(ctx: &ExecutionContext) -> Result<Arc<dyn StorageBackend>> {
    ctx.storage.as_ref()
        .and_then(|s| s.downcast_ref::<Arc<dyn StorageBackend>>().cloned())
        .ok_or_else(|| RuntimeError::Internal("Storage not available in context".to_string()))
}

fn get_with(task: &NamedTask) -> Value {
    match &task.task {
        Task::Call(c) => c.with.clone().unwrap_or(json!({})),
        _ => json!({}),
    }
}

// -- state:get --

pub struct StateGetHandler;

#[async_trait]
impl TaskHandler for StateGetHandler {
    fn task_type(&self) -> &str {
        "call:state:get"
    }

    async fn execute(
        &self,
        task: &NamedTask,
        _input: Value,
        ctx: &mut ExecutionContext,
    ) -> Result<TaskResult> {
        let with = get_with(task);
        let key = with.get("key").and_then(|v| v.as_str())
            .ok_or_else(|| RuntimeError::task_failed(&task.name, "state:get requires 'key'"))?;

        debug!(task = %task.name, key = %key, "state:get");

        let storage = get_storage(ctx)?;
        match storage.state().get(key).await {
            Ok(Some(value)) => Ok(TaskResult::new(value)),
            Ok(None) => Ok(TaskResult::new(Value::Null)),
            Err(e) => Err(RuntimeError::task_failed(&task.name, format!("state:get failed: {}", e))),
        }
    }
}

// -- state:set --

pub struct StateSetHandler;

#[async_trait]
impl TaskHandler for StateSetHandler {
    fn task_type(&self) -> &str {
        "call:state:set"
    }

    async fn execute(
        &self,
        task: &NamedTask,
        _input: Value,
        ctx: &mut ExecutionContext,
    ) -> Result<TaskResult> {
        let with = get_with(task);
        let key = with.get("key").and_then(|v| v.as_str())
            .ok_or_else(|| RuntimeError::task_failed(&task.name, "state:set requires 'key'"))?;
        let value = with.get("value").cloned().unwrap_or(Value::Null);
        let ttl = with.get("ttl").and_then(|v| v.as_u64())
            .map(std::time::Duration::from_secs);

        debug!(task = %task.name, key = %key, "state:set");

        let storage = get_storage(ctx)?;
        storage.state().set(key, value.clone(), ttl).await
            .map_err(|e| RuntimeError::task_failed(&task.name, format!("state:set failed: {}", e)))?;

        Ok(TaskResult::new(json!({"key": key, "stored": true})))
    }
}

// -- state:delete --

pub struct StateDeleteHandler;

#[async_trait]
impl TaskHandler for StateDeleteHandler {
    fn task_type(&self) -> &str {
        "call:state:delete"
    }

    async fn execute(
        &self,
        task: &NamedTask,
        _input: Value,
        ctx: &mut ExecutionContext,
    ) -> Result<TaskResult> {
        let with = get_with(task);
        let key = with.get("key").and_then(|v| v.as_str())
            .ok_or_else(|| RuntimeError::task_failed(&task.name, "state:delete requires 'key'"))?;

        debug!(task = %task.name, key = %key, "state:delete");

        let storage = get_storage(ctx)?;
        storage.state().delete(key).await
            .map_err(|e| RuntimeError::task_failed(&task.name, format!("state:delete failed: {}", e)))?;

        Ok(TaskResult::new(json!({"key": key, "deleted": true})))
    }
}
