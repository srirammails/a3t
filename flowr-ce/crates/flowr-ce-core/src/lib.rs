//! ClawdFlow Core
//!
//! Single-node workflow executor for ClawdFlow.
//!
//! This crate re-exports types from flowr-core to ensure type compatibility
//! across the clawdflow crates. This allows clawdflow-tasks executors to
//! work seamlessly with flowr-core's Executor.
//!
//! Key types:
//! - [`Executor`] - Workflow executor
//! - [`TaskExecutor`] - Trait for executing individual tasks
//! - [`FlowContext`] - Shared services (storage, HTTP, expressions)
//! - [`TaskContext`] - Per-task execution context

// Re-export everything from flowr-core for type compatibility
pub use flowr_core::{
    Executor,
    FlowContext,
    FlowDirective,
    ResolvedExtension,
    TaskContext,
    TaskDefinition,
    TaskExecutor,
    TaskResult,
    WorkflowMetadata,
};

// Re-export from shared crates
pub use flowr_expr::{ExpressionEngine, Scope};
pub use flowr_types::{FlowDurability, FlowError, FlowResult};
