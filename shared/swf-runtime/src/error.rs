//! Runtime error types

use thiserror::Error;

#[derive(Debug, Error)]
pub enum RuntimeError {
    /// Task execution failed
    #[error("task '{task}' failed: {message}")]
    TaskFailed { task: String, message: String },

    /// A task was denied by policy (E0.2/E0.3). Maps to the CNCF
    /// `https://serverlessworkflow.io/spec/1.0.0/errors/authorization` (403)
    /// error type. Denied steps fail cleanly and are never retried when the
    /// action was destructive.
    #[error("denied: task '{task}' — {reason} (policies: {policy_ids:?})")]
    Denied {
        task: String,
        reason: String,
        policy_ids: Vec<String>,
    },

    /// Expression evaluation failed
    #[error("expression error: {0}")]
    Expression(String),

    /// Schema validation failed (CNCF spec error type)
    #[error("validation error at {location}: {message}")]
    Validation { location: String, message: String },

    /// Timeout exceeded
    #[error("timeout: {0}")]
    Timeout(String),

    /// Workflow not found
    #[error("workflow not found: {namespace}/{name}")]
    WorkflowNotFound { namespace: String, name: String },

    /// Resource not found
    #[error("resource not found: {0}")]
    ResourceNotFound(String),

    /// Unsupported DSL version
    #[error("unsupported DSL version: {0}")]
    UnsupportedVersion(String),

    /// Communication error (HTTP, gRPC, etc.)
    #[error("communication error: {0}")]
    Communication(String),

    /// User-raised error (via raise task, RFC 7807)
    #[error("raised error [{error_type}]: {detail}")]
    Raised {
        error_type: String,
        status: u16,
        title: String,
        detail: String,
        instance: Option<String>,
    },

    /// Internal error
    #[error("internal error: {0}")]
    Internal(String),
}

impl RuntimeError {
    pub fn task_failed(task: impl Into<String>, message: impl Into<String>) -> Self {
        Self::TaskFailed { task: task.into(), message: message.into() }
    }

    /// CNCF spec error type URI
    pub fn error_type_uri(&self) -> &str {
        match self {
            Self::Denied { .. } => "https://serverlessworkflow.io/spec/1.0.0/errors/authorization",
            Self::Validation { .. } => "https://serverlessworkflow.io/spec/1.0.0/errors/validation",
            Self::Timeout(_) => "https://serverlessworkflow.io/spec/1.0.0/errors/timeout",
            Self::Communication(_) => "https://serverlessworkflow.io/spec/1.0.0/errors/communication",
            Self::Expression(_) => "https://serverlessworkflow.io/spec/1.0.0/errors/expression",
            Self::Raised { error_type, .. } => error_type.as_str(),
            _ => "https://serverlessworkflow.io/spec/1.0.0/errors/runtime",
        }
    }
}

pub type Result<T> = std::result::Result<T, RuntimeError>;
