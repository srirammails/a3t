//! FlowR Core
//!
//! Shared workflow executor and task abstractions for FlowR and ClawdFlow.
//!
//! This crate provides backend-agnostic workflow execution using the
//! `StorageBackend` trait for pluggable storage (LanceDB, Redis+Iggy, etc.).
//!
//! Key types:
//! - [`Executor`] - Workflow executor
//! - [`TaskExecutor`] - Trait for executing individual tasks
//! - [`FlowContext`] - Shared services (storage, HTTP, expressions)
//! - [`TaskContext`] - Per-task execution context

mod context;
mod directive;
mod executor;
mod task;

pub use context::{FlowContext, TaskContext};
pub use directive::FlowDirective;
pub use executor::{Executor, WorkflowMetadata};
pub use task::{TaskDefinition, TaskExecutor, TaskResult, ResolvedExtension};

// Re-export from shared crates
pub use flowr_expr::{ExpressionEngine, Scope};
pub use flowr_types::{FlowDurability, FlowError, FlowResult};
