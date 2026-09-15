//! Parser error types

use thiserror::Error;

#[derive(Debug, Error)]
pub enum SwfError {
    #[error("parse error: {0}")]
    Parse(String),

    #[error("missing required field: {field} in {context}")]
    MissingField { field: String, context: String },

    #[error("invalid task type: {0}")]
    InvalidTaskType(String),

    #[error("invalid value for {field}: {message}")]
    InvalidValue { field: String, message: String },

    #[error("YAML error: {0}")]
    Yaml(#[from] serde_yaml::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
}

pub type Result<T> = std::result::Result<T, SwfError>;
