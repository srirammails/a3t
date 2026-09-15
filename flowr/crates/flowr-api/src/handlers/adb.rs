//! ADB (Agent Database) handlers
//!
//! Handlers for ClawDB/LanceDB storage backend mode.
//! Uses the StorageBackend trait for all operations.

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use chrono::Utc;
use flowr_types::{
    ExecutionRequest, Resource, ResourceType, WorkflowDefinition, WorkflowSummary,
};
use serde::{Deserialize, Serialize};
use tracing::{info, instrument, error};
use uuid::Uuid;

use crate::error::ApiError;
use crate::state::AppStateAdb;

// ============================================================================
// Common Types
// ============================================================================

/// Error response for ADB handlers
#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    pub error: String,
    pub code: String,
}

// ============================================================================
// Workflow Handlers
// ============================================================================

/// Query parameters for listing workflows
#[derive(Debug, Deserialize)]
pub struct ListWorkflowParams {
    /// Filter by namespace (default: "default")
    #[serde(default = "default_namespace")]
    pub namespace: String,
    /// Filter by enabled status
    pub enabled: Option<bool>,
}

fn default_namespace() -> String {
    "default".to_string()
}

/// Request body for creating a workflow (JSON only for ADB mode)
pub type CreateWorkflowRequest = WorkflowDefinition;

/// Request body for updating a workflow
#[derive(Debug, Deserialize)]
pub struct UpdateWorkflowRequest {
    /// Enable or disable the workflow
    pub enabled: Option<bool>,
    /// Set active version
    pub version: Option<String>,
}

/// POST /flows - Register a new workflow
#[instrument(skip(state, request))]
pub async fn create_workflow(
    State(state): State<AppStateAdb>,
    Json(request): Json<CreateWorkflowRequest>,
) -> Result<(StatusCode, Json<WorkflowDefinition>), ApiError> {
    let definition = request;

    info!(
        namespace = %definition.metadata.namespace,
        name = %definition.metadata.name,
        version = %definition.metadata.version,
        "registering workflow (ADB)"
    );

    state.storage.workflows().register(definition.clone()).await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    Ok((StatusCode::CREATED, Json(definition)))
}

/// GET /flows - List all workflows
#[instrument(skip(state))]
pub async fn list_workflows(
    State(state): State<AppStateAdb>,
    Query(params): Query<ListWorkflowParams>,
) -> Result<Json<Vec<WorkflowSummary>>, ApiError> {
    let mut summaries = state.storage.workflows().list(&params.namespace).await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    // Apply enabled filter if provided
    if let Some(enabled) = params.enabled {
        summaries.retain(|s| s.enabled == enabled);
    }

    Ok(Json(summaries))
}

/// GET /flows/:namespace/:name - Get workflow by namespace and name
#[instrument(skip(state))]
pub async fn get_workflow(
    State(state): State<AppStateAdb>,
    Path((namespace, name)): Path<(String, String)>,
) -> Result<Json<WorkflowDefinition>, ApiError> {
    let definition = state.storage.workflows().get(&namespace, &name).await
        .map_err(|e| ApiError::NotFound(e.to_string()))?;
    Ok(Json(definition))
}

/// GET /flows/:namespace/:name/versions/:version - Get specific version
#[instrument(skip(state))]
pub async fn get_workflow_version(
    State(state): State<AppStateAdb>,
    Path((namespace, name, version)): Path<(String, String, String)>,
) -> Result<Json<WorkflowDefinition>, ApiError> {
    let definition = state.storage.workflows().get_version(&namespace, &name, &version).await
        .map_err(|e| ApiError::NotFound(e.to_string()))?;
    Ok(Json(definition))
}

/// PATCH /flows/:namespace/:name - Update workflow
#[instrument(skip(state))]
pub async fn update_workflow(
    State(state): State<AppStateAdb>,
    Path((namespace, name)): Path<(String, String)>,
    Json(request): Json<UpdateWorkflowRequest>,
) -> Result<StatusCode, ApiError> {
    // Update enabled status if provided
    if let Some(enabled) = request.enabled {
        state.storage.workflows().set_enabled(&namespace, &name, enabled).await
            .map_err(|e| ApiError::Internal(e.to_string()))?;
        info!(namespace, name, enabled, "workflow enabled status updated (ADB)");
    }

    // Update active version if provided
    if let Some(version) = request.version {
        state.storage.workflows().set_active_version(&namespace, &name, &version).await
            .map_err(|e| ApiError::Internal(e.to_string()))?;
        info!(namespace, name, version, "active version updated (ADB)");
    }

    Ok(StatusCode::NO_CONTENT)
}

/// DELETE /flows/:namespace/:name - Delete workflow
#[instrument(skip(state))]
pub async fn delete_workflow(
    State(state): State<AppStateAdb>,
    Path((namespace, name)): Path<(String, String)>,
) -> Result<StatusCode, ApiError> {
    state.storage.workflows().delete(&namespace, &name).await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    info!(namespace, name, "workflow deleted (ADB)");
    Ok(StatusCode::NO_CONTENT)
}

// ============================================================================
// Execution Handlers
// ============================================================================

/// Request body for executing a workflow
#[derive(Debug, Deserialize)]
pub struct ExecuteWorkflowRequest {
    /// Input data for the workflow
    #[serde(default)]
    pub input: serde_json::Value,
    /// Optional specific version to execute
    pub version: Option<String>,
    /// Durability mode (completion or checkpoint)
    pub durability: Option<String>,
}

/// Response for workflow execution
#[derive(Debug, Serialize)]
pub struct ExecuteWorkflowResponse {
    /// Instance ID for the execution
    pub instance_id: Uuid,
    /// Status of the execution
    pub status: String,
    /// Message
    pub message: String,
}

/// POST /flows/:namespace/:name/execute - Execute workflow
#[instrument(skip(state))]
pub async fn execute_workflow(
    State(state): State<AppStateAdb>,
    Path((namespace, name)): Path<(String, String)>,
    Json(request): Json<ExecuteWorkflowRequest>,
) -> Result<(StatusCode, Json<ExecuteWorkflowResponse>), ApiError> {
    // Get the workflow definition to validate it exists
    let definition = if let Some(version) = &request.version {
        state.storage.workflows().get_version(&namespace, &name, version).await
    } else {
        state.storage.workflows().get(&namespace, &name).await
    }.map_err(|e| ApiError::NotFound(e.to_string()))?;

    // Generate instance ID
    let instance_id = Uuid::new_v4();

    info!(
        namespace,
        name,
        version = %definition.metadata.version,
        %instance_id,
        "workflow execution requested (ADB)"
    );

    // Create execution request
    let exec_request = ExecutionRequest {
        instance_id,
        namespace: namespace.clone(),
        name: name.clone(),
        version: Some(definition.metadata.version.clone()),
        input: request.input,
        durability: request.durability,
        requested_at: Utc::now(),
        caller: None, // API caller
        // TODO: thread the JWT principal here once this handler runs behind auth.
        principal: None,
    };

    // Enqueue execution request using storage backend
    state.storage.queue().enqueue(exec_request).await
        .map_err(|e| ApiError::Internal(format!("failed to enqueue execution: {}", e)))?;

    info!(
        namespace,
        name,
        version = %definition.metadata.version,
        %instance_id,
        "execution request enqueued (ADB)"
    );

    let response = ExecuteWorkflowResponse {
        instance_id,
        status: "queued".to_string(),
        message: format!(
            "Workflow {}/{}:{} queued for execution",
            namespace, name, definition.metadata.version
        ),
    };

    Ok((StatusCode::ACCEPTED, Json(response)))
}

// ============================================================================
// Resource Type Handlers (Hub)
// ============================================================================

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

/// GET /hub/types - List all resource types
pub async fn list_types(
    State(state): State<AppStateAdb>,
) -> Result<Json<ListTypesResponse>, (StatusCode, Json<ErrorResponse>)> {
    let types = state.storage.resources().list_types().await.map_err(|e| {
        error!(error = %e, "Failed to list resource types (ADB)");
        (StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorResponse {
            error: e.to_string(),
            code: "INTERNAL_ERROR".to_string(),
        }))
    })?;

    Ok(Json(ListTypesResponse {
        count: types.len(),
        types,
    }))
}

/// GET /hub/types/:name - Get a specific resource type
pub async fn get_type(
    State(state): State<AppStateAdb>,
    Path(name): Path<String>,
) -> Result<Json<ResourceType>, (StatusCode, Json<ErrorResponse>)> {
    let resource_type = state.storage.resources().get_type(&name).await.map_err(|e| {
        (StatusCode::NOT_FOUND, Json(ErrorResponse {
            error: e.to_string(),
            code: "TYPE_NOT_FOUND".to_string(),
        }))
    })?;

    Ok(Json(resource_type))
}

/// POST /hub/types - Create a new resource type
pub async fn create_type(
    State(state): State<AppStateAdb>,
    Json(request): Json<CreateTypeRequest>,
) -> Result<(StatusCode, Json<ResourceType>), (StatusCode, Json<ErrorResponse>)> {
    state.storage.resources().register_type(request.resource_type.clone()).await.map_err(|e| {
        error!(error = %e, "Failed to create resource type (ADB)");
        (StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorResponse {
            error: e.to_string(),
            code: "INTERNAL_ERROR".to_string(),
        }))
    })?;

    Ok((StatusCode::CREATED, Json(request.resource_type)))
}

/// DELETE /hub/types/:name - Delete a resource type
pub async fn delete_type(
    State(state): State<AppStateAdb>,
    Path(name): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    // Note: ResourceStore trait doesn't have delete_type - would need to add
    // For now, return not implemented
    Err((StatusCode::NOT_IMPLEMENTED, Json(ErrorResponse {
        error: "delete_type not implemented for ADB mode".to_string(),
        code: "NOT_IMPLEMENTED".to_string(),
    })))
}

// ============================================================================
// Resource Instance Handlers
// ============================================================================

/// Query parameters for listing resources
#[derive(Debug, Deserialize)]
pub struct ListResourcesQuery {
    /// Filter by namespace (default: "default")
    #[serde(default = "default_namespace")]
    pub namespace: String,
    /// Filter by resource type
    pub type_ref: Option<String>,
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

/// Request to create a resource
#[derive(Debug, Deserialize)]
pub struct CreateResourceRequest {
    /// Namespace (default: "default")
    #[serde(default = "default_namespace")]
    pub namespace: String,
    /// Resource name
    pub name: String,
    /// Resource type reference
    pub type_ref: String,
    /// Configuration values
    #[serde(default)]
    pub config: serde_json::Value,
}

/// Request to update a resource
#[derive(Debug, Deserialize)]
pub struct UpdateResourceRequest {
    /// Updated configuration
    pub config: serde_json::Value,
}

/// GET /resources - List resources
pub async fn list_resources(
    State(state): State<AppStateAdb>,
    Query(query): Query<ListResourcesQuery>,
) -> Result<Json<ListResourcesResponse>, (StatusCode, Json<ErrorResponse>)> {
    let resources = state.storage.resources().list(&query.namespace).await
        .map_err(|e| {
            error!(error = %e, "Failed to list resources (ADB)");
            (StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorResponse {
                error: e.to_string(),
                code: "INTERNAL_ERROR".to_string(),
            }))
        })?;

    // Filter by type if provided
    let resources: Vec<_> = if let Some(type_ref) = query.type_ref {
        resources.into_iter().filter(|r| r.type_ref == type_ref).collect()
    } else {
        resources
    };

    Ok(Json(ListResourcesResponse {
        count: resources.len(),
        namespace: query.namespace,
        resources,
    }))
}

/// GET /resources/namespaces - List all namespaces with resources
pub async fn list_namespaces(
    State(_state): State<AppStateAdb>,
) -> Result<Json<ListNamespacesResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Note: ResourceStore trait doesn't have list_namespaces
    // Return default namespace for now
    Ok(Json(ListNamespacesResponse {
        count: 1,
        namespaces: vec!["default".to_string()],
    }))
}

/// GET /resources/:namespace/:name - Get a specific resource
pub async fn get_resource(
    State(state): State<AppStateAdb>,
    Path((namespace, name)): Path<(String, String)>,
) -> Result<Json<Resource>, (StatusCode, Json<ErrorResponse>)> {
    let resource = state.storage.resources().get(&namespace, &name).await.map_err(|e| {
        (StatusCode::NOT_FOUND, Json(ErrorResponse {
            error: e.to_string(),
            code: "RESOURCE_NOT_FOUND".to_string(),
        }))
    })?;

    Ok(Json(resource))
}

/// POST /resources - Create a new resource
pub async fn create_resource(
    State(state): State<AppStateAdb>,
    Json(request): Json<CreateResourceRequest>,
) -> Result<(StatusCode, Json<Resource>), (StatusCode, Json<ErrorResponse>)> {
    let resource = Resource::new(&request.name, &request.type_ref, request.config)
        .with_namespace(&request.namespace);

    let created = state.storage.resources().create(resource).await.map_err(|e| {
        error!(error = %e, "Failed to create resource (ADB)");
        (StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorResponse {
            error: e.to_string(),
            code: "INTERNAL_ERROR".to_string(),
        }))
    })?;

    Ok((StatusCode::CREATED, Json(created)))
}

/// PATCH /resources/:namespace/:name - Update a resource
pub async fn update_resource(
    State(state): State<AppStateAdb>,
    Path((namespace, name)): Path<(String, String)>,
    Json(request): Json<UpdateResourceRequest>,
) -> Result<Json<Resource>, (StatusCode, Json<ErrorResponse>)> {
    let resource = state.storage.resources()
        .update(&namespace, &name, request.config)
        .await
        .map_err(|e| {
            error!(error = %e, "Failed to update resource (ADB)");
            (StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorResponse {
                error: e.to_string(),
                code: "INTERNAL_ERROR".to_string(),
            }))
        })?;

    Ok(Json(resource))
}

/// DELETE /resources/:namespace/:name - Delete a resource
pub async fn delete_resource(
    State(state): State<AppStateAdb>,
    Path((namespace, name)): Path<(String, String)>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    state.storage.resources().delete(&namespace, &name).await.map_err(|e| {
        (StatusCode::NOT_FOUND, Json(ErrorResponse {
            error: e.to_string(),
            code: "RESOURCE_NOT_FOUND".to_string(),
        }))
    })?;

    Ok(StatusCode::NO_CONTENT)
}

// ============================================================================
// Health Handlers
// ============================================================================

/// Health check response
#[derive(Debug, Serialize)]
pub struct HealthResponse {
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<HealthDetails>,
}

/// Health check details
#[derive(Debug, Serialize)]
pub struct HealthDetails {
    pub storage: ComponentHealth,
}

/// Component health status
#[derive(Debug, Serialize)]
pub struct ComponentHealth {
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latency_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// GET /health/live - Liveness probe
pub async fn liveness() -> (StatusCode, Json<HealthResponse>) {
    (
        StatusCode::OK,
        Json(HealthResponse {
            status: "ok".to_string(),
            details: None,
        }),
    )
}

/// GET /health/ready - Readiness probe
pub async fn readiness(State(state): State<AppStateAdb>) -> (StatusCode, Json<HealthResponse>) {
    let start = std::time::Instant::now();

    // Check storage connectivity by attempting to list workflows
    let storage_health = match state.storage.workflows().list("default").await {
        Ok(_) => ComponentHealth {
            status: "ok".to_string(),
            latency_ms: Some(start.elapsed().as_millis() as u64),
            error: None,
        },
        Err(e) => ComponentHealth {
            status: "error".to_string(),
            latency_ms: None,
            error: Some(e.to_string()),
        },
    };

    let all_healthy = storage_health.status == "ok";

    let response = HealthResponse {
        status: if all_healthy { "ok" } else { "degraded" }.to_string(),
        details: Some(HealthDetails {
            storage: storage_health,
        }),
    };

    let status_code = if all_healthy {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };

    (status_code, Json(response))
}
