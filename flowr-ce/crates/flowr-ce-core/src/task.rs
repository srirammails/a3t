//! Task executor trait and result types

use std::sync::Arc;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::context::TaskContext;
use crate::directive::FlowDirective;
use flowr_types::FlowError;

/// Result of task execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskResult {
    /// Output value from the task
    pub output: Value,

    /// Flow directive (continue, exit, end, goto)
    #[serde(default)]
    pub directive: FlowDirective,

    /// Optional exports to merge into workflow context
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exports: Option<Value>,
}

impl TaskResult {
    /// Create a new task result with output
    pub fn new(output: Value) -> Self {
        Self {
            output,
            directive: FlowDirective::Continue,
            exports: None,
        }
    }

    /// Create a result that ends the workflow
    pub fn end(output: Value) -> Self {
        Self {
            output,
            directive: FlowDirective::End,
            exports: None,
        }
    }

    /// Create a result that exits current scope
    pub fn exit(output: Value) -> Self {
        Self {
            output,
            directive: FlowDirective::Exit,
            exports: None,
        }
    }

    /// Create a result that jumps to another task
    pub fn goto(output: Value, target: impl Into<String>) -> Self {
        Self {
            output,
            directive: FlowDirective::Goto(target.into()),
            exports: None,
        }
    }

    /// Add exports to merge into workflow context
    pub fn with_exports(mut self, exports: Value) -> Self {
        self.exports = Some(exports);
        self
    }

    /// Set the flow directive
    pub fn with_directive(mut self, directive: FlowDirective) -> Self {
        self.directive = directive;
        self
    }
}

impl Default for TaskResult {
    fn default() -> Self {
        Self::new(Value::Null)
    }
}

/// Trait for executing individual tasks
///
/// Each task type (Set, Call, Emit, etc.) implements this trait.
#[async_trait]
pub trait TaskExecutor: Send + Sync {
    /// Execute the task
    ///
    /// # Arguments
    /// * `ctx` - Task context containing input, scope, and services
    ///
    /// # Returns
    /// Result containing output and flow directive
    async fn execute(&self, ctx: &mut TaskContext) -> Result<TaskResult, FlowError>;

    /// Get the task type name (for logging/metrics)
    fn task_type(&self) -> &'static str;

    /// Check if this task is a checkpoint (for durability)
    ///
    /// Override to return true for tasks that should trigger checkpoint writes.
    fn is_checkpoint(&self) -> bool {
        false
    }
}

/// Simple pass-through executor for testing
#[allow(dead_code)]
pub struct NoOpExecutor;

#[async_trait]
impl TaskExecutor for NoOpExecutor {
    async fn execute(&self, ctx: &mut TaskContext) -> Result<TaskResult, FlowError> {
        // Pass input through as output
        Ok(TaskResult::new(ctx.input.clone()))
    }

    fn task_type(&self) -> &'static str {
        "noop"
    }
}

/// Task definition with transformations
///
/// Combines an executor with input/output transformations per CNCF Serverless Workflow spec.
pub struct TaskDefinition {
    /// Task name
    pub name: String,

    /// The task executor
    pub executor: Arc<dyn TaskExecutor>,

    /// Input transformation: jq expression to transform input before execution
    /// Corresponds to `input.from` in the spec
    pub input_from: Option<String>,

    /// Output transformation: jq expression to transform output after execution
    /// Corresponds to `output.as` in the spec
    pub output_as: Option<String>,

    /// Export transformation: jq expression to define what gets exported to context
    /// Corresponds to `export.as` in the spec
    pub export_as: Option<String>,

    /// Flow directive override
    /// Corresponds to `then` in the spec (continue, exit, end, or task name for goto)
    pub then: Option<String>,

    /// Whether this task is a checkpoint (for durability)
    pub checkpoint: bool,

    /// Resolved resource configuration (FlowR extension)
    /// Set when the task references a resource via the `resource` field
    pub resource: Option<serde_json::Value>,
}

impl TaskDefinition {
    /// Create a new task definition
    pub fn new(name: impl Into<String>, executor: Arc<dyn TaskExecutor>) -> Self {
        Self {
            name: name.into(),
            executor,
            input_from: None,
            output_as: None,
            export_as: None,
            then: None,
            checkpoint: false,
            resource: None,
        }
    }

    /// Set the input transformation expression
    pub fn with_input_from(mut self, expr: impl Into<String>) -> Self {
        self.input_from = Some(expr.into());
        self
    }

    /// Set the output transformation expression
    pub fn with_output_as(mut self, expr: impl Into<String>) -> Self {
        self.output_as = Some(expr.into());
        self
    }

    /// Set the export transformation expression
    pub fn with_export_as(mut self, expr: impl Into<String>) -> Self {
        self.export_as = Some(expr.into());
        self
    }

    /// Set the flow directive override
    pub fn with_then(mut self, directive: impl Into<String>) -> Self {
        self.then = Some(directive.into());
        self
    }

    /// Mark this task as a checkpoint
    pub fn as_checkpoint(mut self) -> Self {
        self.checkpoint = true;
        self
    }

    /// Set the resolved resource configuration
    pub fn with_resource(mut self, resource: serde_json::Value) -> Self {
        self.resource = Some(resource);
        self
    }

    /// Check if this task should trigger a checkpoint write
    pub fn is_checkpoint(&self) -> bool {
        self.checkpoint || self.executor.is_checkpoint()
    }
}
