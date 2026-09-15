//! Registry errors

use thiserror::Error;

/// Result type for registry operations
pub type RegistryResult<T> = Result<T, RegistryError>;

/// Registry errors
#[derive(Error, Debug)]
pub enum RegistryError {
    /// Workflow not found
    #[error("workflow not found: {namespace}/{name}")]
    NotFound { namespace: String, name: String },

    /// Version not found
    #[error("version not found: {namespace}/{name}:{version}")]
    VersionNotFound {
        namespace: String,
        name: String,
        version: String,
    },

    /// Workflow already exists
    #[error("workflow already exists: {namespace}/{name}:{version}")]
    AlreadyExists {
        namespace: String,
        name: String,
        version: String,
    },

    /// Workflow is disabled
    #[error("workflow is disabled: {namespace}/{name}")]
    Disabled { namespace: String, name: String },

    /// Invalid workflow definition
    #[error("invalid workflow definition: {0}")]
    InvalidDefinition(String),

    /// Serialization error
    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    /// YAML parsing error
    #[error("yaml parsing error: {0}")]
    YamlParse(#[from] serde_yaml::Error),

    /// Redis/Dragonfly error
    #[error("storage error: {0}")]
    Storage(String),

    /// Internal error
    #[error("internal error: {0}")]
    Internal(String),
}

impl From<redis::RedisError> for RegistryError {
    fn from(err: redis::RedisError) -> Self {
        RegistryError::Storage(err.to_string())
    }
}
