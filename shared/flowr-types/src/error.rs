//! Error types for FlowR

use thiserror::Error;

/// FlowR error type
#[derive(Debug, Error)]
pub enum FlowError {
    /// Generic not found error
    #[error("not found: {0}")]
    NotFound(String),

    /// Workflow not found
    #[error("workflow not found: {namespace}/{name}")]
    WorkflowNotFound { namespace: String, name: String },

    /// Resource not found
    #[error("resource not found: {namespace}/{name}")]
    ResourceNotFound { namespace: String, name: String },

    /// Task execution failed
    #[error("task '{task}' failed: {message}")]
    TaskFailed { task: String, message: String },

    /// Expression evaluation failed
    #[error("expression error: {0}")]
    Expression(String),

    /// Serialization error
    #[error("serialization error: {0}")]
    Serialization(String),

    /// Storage error
    #[error("storage error: {0}")]
    Storage(String),

    /// Validation error
    #[error("validation error: {0}")]
    Validation(String),

    /// Internal error
    #[error("internal error: {0}")]
    Internal(String),

    /// Timeout
    #[error("timeout: {0}")]
    Timeout(String),

    /// Cancelled
    #[error("cancelled: {0}")]
    Cancelled(String),

    /// HTTP request error
    #[error("http error: {0}")]
    Http(String),

    /// User-raised error (via raise task)
    #[error("raised error [{error_type}]: {message}")]
    Raised { error_type: String, message: String },

    /// Script execution error
    #[error("script error: {0}")]
    Script(String),

    /// IO error
    #[error("io error: {0}")]
    Io(String),
}

impl FlowError {
    /// Create a task failed error
    pub fn task_failed(task: impl Into<String>, message: impl Into<String>) -> Self {
        Self::TaskFailed {
            task: task.into(),
            message: message.into(),
        }
    }

    /// Create a workflow not found error
    pub fn workflow_not_found(namespace: impl Into<String>, name: impl Into<String>) -> Self {
        Self::WorkflowNotFound {
            namespace: namespace.into(),
            name: name.into(),
        }
    }

    /// Create a validation error with location context
    pub fn validation(location: impl Into<String>, message: impl Into<String>) -> Self {
        Self::Validation(format!("{}: {}", location.into(), message.into()))
    }

    /// Create a resource not found error
    pub fn resource_not_found(namespace: impl Into<String>, name: impl Into<String>) -> Self {
        Self::ResourceNotFound {
            namespace: namespace.into(),
            name: name.into(),
        }
    }
}

impl FlowError {
    /// Create a raised error
    pub fn raised(error_type: impl Into<String>, message: impl Into<String>) -> Self {
        Self::Raised {
            error_type: error_type.into(),
            message: message.into(),
        }
    }
}

impl From<flowr_expr::ExprError> for FlowError {
    fn from(e: flowr_expr::ExprError) -> Self {
        FlowError::Expression(e.to_string())
    }
}

impl From<std::io::Error> for FlowError {
    fn from(e: std::io::Error) -> Self {
        FlowError::Io(e.to_string())
    }
}

impl From<serde_json::Error> for FlowError {
    fn from(e: serde_json::Error) -> Self {
        FlowError::Serialization(e.to_string())
    }
}

/// Result type alias for FlowR operations
pub type FlowResult<T> = Result<T, FlowError>;
