//! FlowR Storage - Backend-agnostic storage trait abstractions
//!
//! This crate defines traits for storage operations that can be implemented
//! by different backends:
//! - LanceDB for ClawdFlow (single-node)
//! - Iggy + Redis for FlowR (distributed)
//!
//! # Traits
//!
//! - [`WorkflowStore`] - Workflow definition storage
//! - [`ResourceStore`] - Resource configuration storage
//! - [`StateStore`] - Key-value state storage
//! - [`CheckpointStore`] - Checkpoint persistence
//! - [`EventBus`] - Lifecycle event pub/sub
//! - [`ExecutionQueue`] - Execution request queue
//! - [`TriggerStore`] - Trigger CRUD (Value-based, backend-agnostic)

pub mod engine;
pub mod engine_core;
pub mod engine_memory;
pub mod engine_wire;
pub mod filter;
#[cfg(any(test, feature = "conformance"))]
pub mod conformance;

#[cfg(feature = "backend")]
mod backend;
#[cfg(feature = "backend")]
pub use backend::*;

