//! API error types

use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::Serialize;

/// API error response body
#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    pub error: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<String>,
}

/// API errors
#[derive(Debug)]
pub enum ApiError {
    /// Resource not found
    NotFound(String),

    /// Bad request
    BadRequest(String),

    /// Conflict (e.g., already exists)
    Conflict(String),

    /// Internal server error
    Internal(String),

    /// Service unavailable
    ServiceUnavailable(String),
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, error, details) = match self {
            ApiError::NotFound(msg) => (StatusCode::NOT_FOUND, "Not Found", Some(msg)),
            ApiError::BadRequest(msg) => (StatusCode::BAD_REQUEST, "Bad Request", Some(msg)),
            ApiError::Conflict(msg) => (StatusCode::CONFLICT, "Conflict", Some(msg)),
            ApiError::Internal(msg) => {
                tracing::error!("Internal error: {}", msg);
                (StatusCode::INTERNAL_SERVER_ERROR, "Internal Server Error", None)
            }
            ApiError::ServiceUnavailable(msg) => {
                (StatusCode::SERVICE_UNAVAILABLE, "Service Unavailable", Some(msg))
            }
        };

        let body = ErrorResponse {
            error: error.to_string(),
            details,
        };

        (status, Json(body)).into_response()
    }
}

impl From<flowr_registry::RegistryError> for ApiError {
    fn from(err: flowr_registry::RegistryError) -> Self {
        match err {
            flowr_registry::RegistryError::NotFound { namespace, name } => {
                ApiError::NotFound(format!("Workflow not found: {}/{}", namespace, name))
            }
            flowr_registry::RegistryError::VersionNotFound {
                namespace,
                name,
                version,
            } => ApiError::NotFound(format!(
                "Version not found: {}/{}:{}",
                namespace, name, version
            )),
            flowr_registry::RegistryError::AlreadyExists {
                namespace,
                name,
                version,
            } => ApiError::Conflict(format!(
                "Workflow already exists: {}/{}:{}",
                namespace, name, version
            )),
            flowr_registry::RegistryError::Disabled { namespace, name } => {
                ApiError::BadRequest(format!("Workflow is disabled: {}/{}", namespace, name))
            }
            flowr_registry::RegistryError::InvalidDefinition(msg) => {
                ApiError::BadRequest(format!("Invalid workflow definition: {}", msg))
            }
            flowr_registry::RegistryError::Serialization(e) => {
                ApiError::BadRequest(format!("Serialization error: {}", e))
            }
            flowr_registry::RegistryError::YamlParse(e) => {
                ApiError::BadRequest(format!("YAML parse error: {}", e))
            }
            flowr_registry::RegistryError::Storage(msg) => {
                ApiError::Internal(format!("Storage error: {}", msg))
            }
            flowr_registry::RegistryError::Internal(msg) => {
                ApiError::Internal(format!("Internal error: {}", msg))
            }
        }
    }
}

impl From<serde_json::Error> for ApiError {
    fn from(err: serde_json::Error) -> Self {
        ApiError::BadRequest(format!("JSON error: {}", err))
    }
}

impl From<serde_yaml::Error> for ApiError {
    fn from(err: serde_yaml::Error) -> Self {
        ApiError::BadRequest(format!("YAML error: {}", err))
    }
}
