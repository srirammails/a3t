//! FlowR Expression Engine
//!
//! jq-based expression evaluation for CNCF Serverless Workflow.
//! Handles `${ ... }` expressions as defined in the DSL spec.

mod engine;
mod error;
mod scope;

pub use engine::ExpressionEngine;
pub use error::{ExprError, ExprResult};
pub use scope::Scope;

#[cfg(test)]
mod tests;
