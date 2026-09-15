//! Flow execution errors

use thiserror::Error;

/// Result type for flow operations
pub type FlowResult<T> = Result<T, FlowError>;

/// Flow execution errors
#[derive(Error, Debug)]
pub enum FlowError {
    /// Task execution failed
    #[error("task '{task}' failed: {message}")]
    TaskFailed { task: String, message: String },

    /// Expression evaluation failed
    #[error("expression error: {0}")]
    Expression(#[from] flowr_expr::ExprError),

    /// Invalid flow directive
    #[error("invalid directive: goto '{target}' not found")]
    InvalidGoto { target: String },

    /// Checkpoint write failed
    #[error("checkpoint write failed: {0}")]
    CheckpointWrite(String),

    /// Checkpoint read failed
    #[error("checkpoint read failed: {0}")]
    CheckpointRead(String),

    /// Recovery failed
    #[error("recovery failed for instance {instance_id}: {message}")]
    Recovery { instance_id: String, message: String },

    /// Dragonfly connection error
    #[error("dragonfly error: {0}")]
    Dragonfly(String),

    /// Iggy connection error
    #[error("iggy error: {0}")]
    Iggy(String),

    /// HTTP request error
    #[error("http error: {0}")]
    Http(String),

    /// Serialization error
    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    /// Flow was cancelled
    #[error("flow cancelled")]
    Cancelled,

    /// Flow raised an error
    #[error("raised error: {error_type} - {message}")]
    Raised { error_type: String, message: String },

    /// Internal error
    #[error("internal error: {0}")]
    Internal(String),
}

impl FlowError {
    /// Create a task failed error
    pub fn task_failed(task: impl Into<String>, message: impl Into<String>) -> Self {
        Self::TaskFailed {
            task: task.into(),
            message: message.into(),
        }
    }

    /// Create an invalid goto error
    pub fn invalid_goto(target: impl Into<String>) -> Self {
        Self::InvalidGoto {
            target: target.into(),
        }
    }

    /// Create a raised error
    pub fn raised(error_type: impl Into<String>, message: impl Into<String>) -> Self {
        Self::Raised {
            error_type: error_type.into(),
            message: message.into(),
        }
    }
}
