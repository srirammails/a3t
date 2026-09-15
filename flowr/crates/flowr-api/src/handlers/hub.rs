//! Hub handlers - Resource type management
//!
//! API endpoints for managing resource types in the Hub.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use flowr_hub::{ResourceType, HubError};
use serde::{Deserialize, Serialize};
use tracing::error;

use crate::state::AppState;

/// Response for listing resource types
#[derive(Debug, Serialize)]
pub struct ListTypesResponse {
    pub types: Vec<ResourceType>,
    pub count: usize,
}

/// Request to create a resource type
#[derive(Debug, Deserialize)]
pub struct CreateTypeRequest {
    #[serde(flatten)]
    pub resource_type: ResourceType,
}

/// Error response
#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    pub error: String,
    pub code: String,
}

impl ErrorResponse {
    fn from_hub_error(e: &HubError) -> (StatusCode, Self) {
        let (status, code) = match e {
            HubError::TypeNotFound(_) => (StatusCode::NOT_FOUND, "TYPE_NOT_FOUND"),
            HubError::CannotDeleteBuiltin(_) => (StatusCode::FORBIDDEN, "CANNOT_DELETE_BUILTIN"),
            HubError::TypeInUse { .. } => (StatusCode::CONFLICT, "TYPE_IN_USE"),
            HubError::ValidationFailed(_) => (StatusCode::BAD_REQUEST, "VALIDATION_FAILED"),
            _ => (StatusCode::INTERNAL_SERVER_ERROR, "INTERNAL_ERROR"),
        };

        (status, ErrorResponse {
            error: e.to_string(),
            code: code.to_string(),
        })
    }
}

/// GET /hub/types - List all resource types
pub async fn list_types(
    State(state): State<AppState>,
) -> Result<Json<ListTypesResponse>, (StatusCode, Json<ErrorResponse>)> {
    let types = state.hub.list_types().await.map_err(|e| {
        error!(error = %e, "Failed to list resource types");
        let (status, err) = ErrorResponse::from_hub_error(&e);
        (status, Json(err))
    })?;

    Ok(Json(ListTypesResponse {
        count: types.len(),
        types,
    }))
}

/// GET /hub/types/:name - Get a specific resource type
pub async fn get_type(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<ResourceType>, (StatusCode, Json<ErrorResponse>)> {
    let resource_type = state.hub.get_type(&name).await.map_err(|e| {
        let (status, err) = ErrorResponse::from_hub_error(&e);
        (status, Json(err))
    })?;

    Ok(Json(resource_type))
}

/// POST /hub/types - Create a new resource type
pub async fn create_type(
    State(state): State<AppState>,
    Json(request): Json<CreateTypeRequest>,
) -> Result<(StatusCode, Json<ResourceType>), (StatusCode, Json<ErrorResponse>)> {
    // Check if type already exists
    if state.hub.type_exists(&request.resource_type.name).await.unwrap_or(false) {
        return Err((
            StatusCode::CONFLICT,
            Json(ErrorResponse {
                error: format!("Resource type '{}' already exists", request.resource_type.name),
                code: "TYPE_EXISTS".to_string(),
            }),
        ));
    }

    state.hub.register_type(request.resource_type.clone()).await.map_err(|e| {
        error!(error = %e, "Failed to create resource type");
        let (status, err) = ErrorResponse::from_hub_error(&e);
        (status, Json(err))
    })?;

    Ok((StatusCode::CREATED, Json(request.resource_type)))
}

/// DELETE /hub/types/:name - Delete a resource type
pub async fn delete_type(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    state.hub.delete_type(&name).await.map_err(|e| {
        let (status, err) = ErrorResponse::from_hub_error(&e);
        (status, Json(err))
    })?;

    Ok(StatusCode::NO_CONTENT)
}
