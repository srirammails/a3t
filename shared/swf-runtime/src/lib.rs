//! CNCF Serverless Workflow Runtime
//!
//! Pluggable execution engine for CNCF SWF workflows.
//! Defines traits that runtime implementations (like FlowR) implement.
//!
//! # Architecture
//!
//! ```text
//! swf-parser → WorkflowDefinition
//!                    ↓
//! swf-runtime → RuntimeRegistry → RuntimeProvider → execute()
//!                                       ↓
//!                               TaskHandler (per task type)
//! ```

pub mod traits;
pub mod registry;
pub mod context;
pub mod error;
pub mod executor;
pub mod handlers;

pub use traits::{RuntimeProvider, TaskHandler, TaskResult, FlowDirective};
pub use registry::RuntimeRegistry;
pub use context::{ExecutionContext, TaskContext, RuntimeInfo, default_flow_principal};
// Re-export so flow-start call sites can name the principal type without taking
// a direct dependency on flowr-policy (E0.4).
pub use flowr_policy::Principal;
pub use error::RuntimeError;
pub use executor::WorkflowExecutor;
pub use executor::extensions::ResolvedExtension;
