//! FlowR Core
//!
//! Executor and task abstractions for CNCF Serverless Workflow runtime.
//!
//! Key types:
//! - [`TaskExecutor`] - Trait for executing individual tasks
//! - [`Executor`] - Orchestrates workflow execution
//! - [`FlowDurability`] - Controls checkpoint behavior
//! - [`FlowContext`] - Shared services (Dragonfly, Iggy, HTTP, expressions)
//! - [`LifecycleEventEmitter`] - Emits CloudEvents for workflow/task lifecycle

mod context;
mod directive;
mod durability;
mod error;
mod executor;
mod lifecycle;
mod task;

pub use context::{FlowContext, TaskContext};
pub use directive::FlowDirective;
pub use durability::{Checkpoint, CheckpointIndex, FlowDurability};
pub use error::{FlowError, FlowResult};
pub use executor::{Executor, WorkflowMetadata};
pub use lifecycle::{
    event_types, ErrorData, LifecycleEvent, LifecycleEventData, LifecycleEventEmitter,
    SuspensionData, TaskEventData, WorkflowEventData, LIFECYCLE_EVENTS_TOPIC,
};
pub use task::{TaskDefinition, TaskExecutor, TaskResult};

// Re-export expression engine types
pub use flowr_expr::{ExpressionEngine, Scope};
