//! CNCF Serverless Workflow DSL 1.0.3 Parser
//!
//! Pure parsing crate — no execution logic, no runtime dependencies.
//! Converts YAML/JSON into a typed AST conforming to the CNCF SWF specification.
//!
//! # Example
//!
//! ```
//! let yaml = r#"
//! document:
//!   dsl: '1.0.3'
//!   namespace: default
//!   name: hello
//!   version: '1.0.0'
//! do:
//!   - greet:
//!       set:
//!         message: hello
//! "#;
//!
//! let workflow = swf_parser::parse_yaml(yaml).unwrap();
//! assert_eq!(workflow.document.name, "hello");
//! assert_eq!(workflow.tasks.len(), 1);
//! ```

pub mod document;
pub mod task;
pub mod call;
pub mod run;
pub mod flow_control;
pub mod event;
pub mod extension;
pub mod auth;
pub mod catalog;
pub mod error;
pub mod retry;
pub mod timeout;
pub mod schedule;
pub mod schema;
pub mod directive;
pub mod resource;
pub mod lifecycle;
pub mod use_block;
mod parser;

// Re-exports
pub use document::{Document, WorkflowDefinition};
pub use task::{NamedTask, Task};
pub use call::{CallTask, CallTarget, HttpCall, GrpcCall, OpenApiCall, AsyncApiCall, A2ACall, McpCall};
pub use run::{RunTask, ShellProcess, ScriptProcess, WorkflowProcess, ContainerProcess};
pub use flow_control::{DoTask, ForTask, ForkTask, SwitchTask, SwitchCase, TryTask, CatchBlock};
pub use event::{EmitTask, ListenTask};
pub use extension::Extension;
pub use auth::Authentication;
pub use catalog::Catalog;
pub use error::SwfError;
pub use schema::{InputDefinition, OutputDefinition, ExportDefinition};
pub use use_block::UseBlock;
pub use directive::FlowDirective;
pub use parser::{parse_yaml, parse_json};
