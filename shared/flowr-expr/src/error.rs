//! Expression evaluation errors

use thiserror::Error;

/// Result type for expression operations
pub type ExprResult<T> = Result<T, ExprError>;

/// Expression evaluation errors
#[derive(Error, Debug)]
pub enum ExprError {
    /// Failed to parse expression
    #[error("parse error: {0}")]
    Parse(String),

    /// Failed to compile filter
    #[error("compile error: {0}")]
    Compile(String),

    /// Failed during evaluation
    #[error("eval error at '{path}': {message}")]
    Eval { path: String, message: String },

    /// Expression produced no output
    #[error("expression '{expr}' produced no output")]
    NoOutput { expr: String },

    /// Expression produced multiple outputs (expected single)
    #[error("expression '{expr}' produced multiple outputs, expected single value")]
    MultipleOutputs { expr: String },

    /// Invalid expression format
    #[error("invalid expression format: {0}")]
    InvalidFormat(String),

    /// JSON serialization error
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
}
