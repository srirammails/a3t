//! Hub Error Types

use thiserror::Error;

/// Hub operation result type
pub type HubResult<T> = Result<T, HubError>;

/// Errors that can occur in hub operations
#[derive(Debug, Error)]
pub enum HubError {
    /// Resource type not found
    #[error("Resource type not found: {0}")]
    TypeNotFound(String),

    /// Resource not found
    #[error("Resource not found: {namespace}/{name}")]
    ResourceNotFound { namespace: String, name: String },

    /// Resource already exists
    #[error("Resource already exists: {namespace}/{name}")]
    ResourceExists { namespace: String, name: String },

    /// Cannot delete builtin type
    #[error("Cannot delete builtin resource type: {0}")]
    CannotDeleteBuiltin(String),

    /// Type is in use by resources
    #[error("Resource type '{type_name}' is in use by {resource_count} resources")]
    TypeInUse {
        type_name: String,
        resource_count: usize,
    },

    /// Config validation failed
    #[error("Config validation failed: {0}")]
    ValidationFailed(String),

    /// Storage error
    #[error("Storage error: {0}")]
    Storage(String),

    /// Serialization error
    #[error("Serialization error: {0}")]
    Serialization(String),

    /// Secret resolution error
    #[error("Failed to resolve secret: {0}")]
    SecretResolution(String),
}
