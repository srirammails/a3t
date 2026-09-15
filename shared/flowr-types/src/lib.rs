//! FlowR Types - Core type definitions for workflow engine
//!
//! Re-exports CNCF SWF types from `swf-parser` plus FlowR-specific types.

pub mod canvas;
pub mod error;
pub mod execution;
pub mod resource;
pub mod workflow;

// Re-exports
pub use error::{FlowError, FlowResult};
pub use execution::{Checkpoint, ExecutionRequest, ExecutionStatus, ExecutionState};
pub use resource::{Resource, ResourceType, ResourceCategory, ConnectionKind};

// Workflow types — CNCF SWF via swf-parser + FlowR extensions
pub use workflow::{
    WorkflowRecord, WorkflowMetadata, WorkflowSummary, FlowDurability,
    // CNCF types re-exported from swf-parser
    SwfWorkflowDefinition, SwfDocument, NamedTask, Task,
    CallTask, CallTarget, RunTask, Extension, SwfUseBlock,
    parse_yaml, parse_json,
};
