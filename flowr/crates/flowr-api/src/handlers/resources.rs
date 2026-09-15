//! Resource handlers - Configured resource instance management
//!
//! API endpoints for managing resource instances.

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use flowr_hub::{CreateResourceRequest, Resource, UpdateResourceRequest, HubError};
use serde::{Deserialize, Serialize};
use tracing::error;

use crate::state::AppState;

/// Query parameters for listing resources
#[derive(Debug, Deserialize)]
pub struct ListResourcesQuery {
    /// Filter by namespace (default: "default")
    #[serde(default = "default_namespace")]
    pub namespace: String,
    /// Filter by resource type
    pub type_ref: Option<String>,
}

fn default_namespace() -> String {
    "default".to_string()
}

/// Response for listing resources
#[derive(Debug, Serialize)]
pub struct ListResourcesResponse {
    pub resources: Vec<Resource>,
    pub count: usize,
    pub namespace: String,
}

/// Response for listing namespaces
#[derive(Debug, Serialize)]
pub struct ListNamespacesResponse {
    pub namespaces: Vec<String>,
    pub count: usize,
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
            HubError::ResourceNotFound { .. } => (StatusCode::NOT_FOUND, "RESOURCE_NOT_FOUND"),
            HubError::ResourceExists { .. } => (StatusCode::CONFLICT, "RESOURCE_EXISTS"),
            HubError::TypeNotFound(_) => (StatusCode::BAD_REQUEST, "TYPE_NOT_FOUND"),
            HubError::ValidationFailed(_) => (StatusCode::BAD_REQUEST, "VALIDATION_FAILED"),
            _ => (StatusCode::INTERNAL_SERVER_ERROR, "INTERNAL_ERROR"),
        };

        (status, ErrorResponse {
            error: e.to_string(),
            code: code.to_string(),
        })
    }
}

/// GET /resources - List resources
///
/// Query params:
/// - namespace: Filter by namespace (default: "default")
/// - type_ref: Filter by resource type
pub async fn list_resources(
    State(state): State<AppState>,
    Query(query): Query<ListResourcesQuery>,
) -> Result<Json<ListResourcesResponse>, (StatusCode, Json<ErrorResponse>)> {
    let resources = if let Some(type_ref) = query.type_ref {
        // Filter by type across all namespaces
        state.hub.list_resources_by_type(&type_ref).await
    } else {
        // List by namespace
        state.hub.list_resources(&query.namespace).await
    }
    .map_err(|e| {
        error!(error = %e, "Failed to list resources");
        let (status, err) = ErrorResponse::from_hub_error(&e);
        (status, Json(err))
    })?;

    Ok(Json(ListResourcesResponse {
        count: resources.len(),
        namespace: query.namespace,
        resources,
    }))
}

/// GET /resources/namespaces - List all namespaces with resources
pub async fn list_namespaces(
    State(state): State<AppState>,
) -> Result<Json<ListNamespacesResponse>, (StatusCode, Json<ErrorResponse>)> {
    let namespaces = state.hub.list_namespaces().await.map_err(|e| {
        error!(error = %e, "Failed to list namespaces");
        let (status, err) = ErrorResponse::from_hub_error(&e);
        (status, Json(err))
    })?;

    Ok(Json(ListNamespacesResponse {
        count: namespaces.len(),
        namespaces,
    }))
}

/// GET /resources/:namespace/:name - Get a specific resource
pub async fn get_resource(
    State(state): State<AppState>,
    Path((namespace, name)): Path<(String, String)>,
) -> Result<Json<Resource>, (StatusCode, Json<ErrorResponse>)> {
    let resource = state.hub.get_resource(&namespace, &name).await.map_err(|e| {
        let (status, err) = ErrorResponse::from_hub_error(&e);
        (status, Json(err))
    })?;

    Ok(Json(resource))
}

/// POST /resources - Create a new resource
pub async fn create_resource(
    State(state): State<AppState>,
    Json(request): Json<CreateResourceRequest>,
) -> Result<(StatusCode, Json<Resource>), (StatusCode, Json<ErrorResponse>)> {
    let resource = state.hub.create_resource(request).await.map_err(|e| {
        error!(error = %e, "Failed to create resource");
        let (status, err) = ErrorResponse::from_hub_error(&e);
        (status, Json(err))
    })?;

    Ok((StatusCode::CREATED, Json(resource)))
}

/// PATCH /resources/:namespace/:name - Update a resource
pub async fn update_resource(
    State(state): State<AppState>,
    Path((namespace, name)): Path<(String, String)>,
    Json(request): Json<UpdateResourceRequest>,
) -> Result<Json<Resource>, (StatusCode, Json<ErrorResponse>)> {
    let resource = state
        .hub
        .update_resource(&namespace, &name, request)
        .await
        .map_err(|e| {
            error!(error = %e, "Failed to update resource");
            let (status, err) = ErrorResponse::from_hub_error(&e);
            (status, Json(err))
        })?;

    Ok(Json(resource))
}

/// DELETE /resources/:namespace/:name - Delete a resource
pub async fn delete_resource(
    State(state): State<AppState>,
    Path((namespace, name)): Path<(String, String)>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    state.hub.delete_resource(&namespace, &name).await.map_err(|e| {
        let (status, err) = ErrorResponse::from_hub_error(&e);
        (status, Json(err))
    })?;

    Ok(StatusCode::NO_CONTENT)
}
