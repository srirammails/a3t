//! FlowR Registry
//!
//! Workflow definition storage and management using Dragonfly.
//!
//! # Dragonfly Schema
//!
//! ```text
//! flow:{ns}:{name}:enabled        → "true" | "false"
//! flow:{ns}:{name}:version        → "1.0.0" (active version)
//! flow:{ns}:{name}:def:{ver}      → JSON (WorkflowDefinition)
//! flow:{ns}:{name}:versions       → Set of available versions
//! flow:{ns}:{name}:step:{id}:enabled → "true" | "false"
//! ```

mod definition;
mod error;
mod registry;
pub mod converter;

pub use definition::{StepDefinition, WorkflowDefinition, WorkflowMetadata, WorkflowSummary};
pub use error::{RegistryError, RegistryResult};
pub use registry::FlowRegistry;

// Re-export CNCF types for convenience
pub use flowr_types::WorkflowRecord;
pub use converter::{ResourceResolver, RegistryWorkflowResolver};
