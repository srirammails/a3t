//! Execution contexts for ClawdFlow
//!
//! Simplified context without Iggy/Redis dependencies.

use std::sync::Arc;

use serde_json::Value;
use uuid::Uuid;

use flowr_expr::{ExpressionEngine, Scope};
use flowr_storage::StorageBackend;

/// Shared services for workflow execution
///
/// Contains storage backend, HTTP client, and expression engine.
#[derive(Clone)]
pub struct FlowContext {
    /// Storage backend for state, checkpoints, events
    pub storage: Arc<dyn StorageBackend>,

    /// HTTP client for Call:http tasks
    pub http: reqwest::Client,

    /// Expression engine for jq evaluation
    pub expr: Arc<ExpressionEngine>,

    /// Whether lifecycle events are enabled
    pub lifecycle_enabled: bool,
}

impl FlowContext {
    /// Create a new flow context
    pub fn new(storage: Arc<dyn StorageBackend>, http: reqwest::Client) -> Self {
        Self {
            storage,
            http,
            expr: Arc::new(ExpressionEngine::new()),
            lifecycle_enabled: true,
        }
    }

    /// Create a flow context with a custom expression engine
    pub fn with_expr(mut self, expr: Arc<ExpressionEngine>) -> Self {
        self.expr = expr;
        self
    }

    /// Create a flow context with lifecycle events disabled
    pub fn without_lifecycle_events(mut self) -> Self {
        self.lifecycle_enabled = false;
        self
    }

    /// Initialize storage backend
    pub async fn init(&self) -> Result<(), String> {
        self.storage.init().await.map_err(|e| e.to_string())
    }
}

/// Context for a single task execution
///
/// Contains the task's input, scope variables, and access to shared services.
pub struct TaskContext {
    /// Task input (after input transformation)
    pub input: Value,

    /// Expression scope ($runtime, $workflow, $task, $context)
    pub scope: Scope,

    /// Shared flow context (services)
    pub flow_ctx: FlowContext,

    /// Current workflow instance ID
    pub instance_id: Uuid,

    /// Current task name
    pub task_name: String,

    /// Position in workflow (e.g., "do/step1")
    pub position: String,
}

impl TaskContext {
    /// Create a new task context
    pub fn new(
        input: Value,
        scope: Scope,
        flow_ctx: FlowContext,
        instance_id: Uuid,
        task_name: impl Into<String>,
        position: impl Into<String>,
    ) -> Self {
        Self {
            input,
            scope,
            flow_ctx,
            instance_id,
            task_name: task_name.into(),
            position: position.into(),
        }
    }

    /// Evaluate an expression against the task input
    pub fn eval(&self, expr: &str) -> Result<Value, flowr_expr::ExprError> {
        self.flow_ctx.expr.eval(&self.input, expr, &self.scope)
    }

    /// Evaluate a value recursively, processing any expressions
    pub fn eval_recursive(&self, value: &Value) -> Result<Value, flowr_expr::ExprError> {
        self.flow_ctx
            .expr
            .eval_recursive(value, &self.input, &self.scope)
    }

    /// Update the scope context (for Set tasks)
    pub fn update_context(&mut self, updates: Value) {
        if let (Value::Object(ctx), Value::Object(upd)) = (&mut self.scope.context, updates) {
            for (k, v) in upd {
                ctx.insert(k, v);
            }
        }
    }

    /// Get the current workflow context
    pub fn context(&self) -> &Value {
        &self.scope.context
    }

    /// Get a mutable reference to the workflow context
    pub fn context_mut(&mut self) -> &mut Value {
        &mut self.scope.context
    }
}
