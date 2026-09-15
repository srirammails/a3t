//! Flow management handlers

use std::str::FromStr;

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use chrono::Utc;
use flowr_registry::WorkflowDefinition;
use iggy::client::{MessageClient, StreamClient, TopicClient};
use iggy::compression::compression_algorithm::CompressionAlgorithm;
use iggy::identifier::Identifier;
use iggy::messages::send_messages::{Message, Partitioning};
use iggy::utils::expiry::IggyExpiry;
use iggy::utils::topic_size::MaxTopicSize;
use serde::{Deserialize, Serialize};
use tracing::{info, instrument, warn};
use uuid::Uuid;

use crate::error::ApiError;
use crate::state::AppState;

/// Iggy stream for FlowR
const FLOWR_STREAM: &str = "flowr";

/// Topic for execution requests
const EXECUTION_REQUESTS_TOPIC: &str = "execution-requests";

/// Query parameters for listing workflows
#[derive(Debug, Deserialize)]
pub struct ListParams {
    /// Filter by namespace
    pub namespace: Option<String>,
    /// Filter by enabled status
    pub enabled: Option<bool>,
}

/// Request body for creating a workflow
#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum CreateWorkflowRequest {
    /// JSON definition
    Json(WorkflowDefinition),
    /// YAML definition as string
    Yaml { yaml: String },
}

/// Request body for updating a workflow
#[derive(Debug, Deserialize)]
pub struct UpdateWorkflowRequest {
    /// Enable or disable the workflow
    pub enabled: Option<bool>,
    /// Set active version
    pub version: Option<String>,
}

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

/// Execution request message published to Iggy
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionRequest {
    /// Unique instance ID for this execution
    pub instance_id: Uuid,
    /// Workflow namespace
    pub namespace: String,
    /// Workflow name
    pub name: String,
    /// Workflow version (optional)
    pub version: Option<String>,
    /// Input data for the workflow
    pub input: serde_json::Value,
    /// Requested durability mode
    pub durability: Option<String>,
    /// Request timestamp
    pub requested_at: chrono::DateTime<Utc>,
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

/// POST /flows - Register a new workflow
#[instrument(skip(state, request))]
pub async fn create_workflow(
    State(state): State<AppState>,
    Json(request): Json<CreateWorkflowRequest>,
) -> Result<(StatusCode, Json<WorkflowDefinition>), ApiError> {
    let definition = match request {
        CreateWorkflowRequest::Json(def) => def,
        CreateWorkflowRequest::Yaml { yaml } => {
            WorkflowDefinition::from_yaml(&yaml).map_err(|e| ApiError::BadRequest(e.to_string()))?
        }
    };

    info!(
        namespace = %definition.metadata.namespace,
        name = %definition.metadata.name,
        version = %definition.metadata.version,
        "registering workflow"
    );

    state.registry.register(definition.clone()).await?;

    Ok((StatusCode::CREATED, Json(definition)))
}

/// GET /flows - List all workflows
#[instrument(skip(state))]
pub async fn list_workflows(
    State(state): State<AppState>,
    Query(params): Query<ListParams>,
) -> Result<Json<Vec<flowr_registry::WorkflowSummary>>, ApiError> {
    let mut summaries = state.registry.list().await?;

    // Apply filters
    if let Some(ns) = params.namespace {
        summaries.retain(|s| s.namespace == ns);
    }
    if let Some(enabled) = params.enabled {
        summaries.retain(|s| s.enabled == enabled);
    }

    Ok(Json(summaries))
}

/// GET /flows/:namespace/:name - Get workflow by namespace and name
#[instrument(skip(state))]
pub async fn get_workflow(
    State(state): State<AppState>,
    Path((namespace, name)): Path<(String, String)>,
) -> Result<Json<WorkflowDefinition>, ApiError> {
    let definition = state.registry.get(&namespace, &name).await?;
    Ok(Json(definition))
}

/// GET /flows/:namespace/:name/versions/:version - Get specific version
#[instrument(skip(state))]
pub async fn get_workflow_version(
    State(state): State<AppState>,
    Path((namespace, name, version)): Path<(String, String, String)>,
) -> Result<Json<WorkflowDefinition>, ApiError> {
    let definition = state.registry.get_version(&namespace, &name, &version).await?;
    Ok(Json(definition))
}

/// PATCH /flows/:namespace/:name - Update workflow
#[instrument(skip(state))]
pub async fn update_workflow(
    State(state): State<AppState>,
    Path((namespace, name)): Path<(String, String)>,
    Json(request): Json<UpdateWorkflowRequest>,
) -> Result<StatusCode, ApiError> {
    // Update enabled status if provided
    if let Some(enabled) = request.enabled {
        state.registry.set_enabled(&namespace, &name, enabled).await?;
        info!(namespace, name, enabled, "workflow enabled status updated");
    }

    // Update active version if provided
    if let Some(version) = request.version {
        state
            .registry
            .set_active_version(&namespace, &name, &version)
            .await?;
        info!(namespace, name, version, "active version updated");
    }

    Ok(StatusCode::NO_CONTENT)
}

/// DELETE /flows/:namespace/:name - Delete workflow
#[instrument(skip(state))]
pub async fn delete_workflow(
    State(state): State<AppState>,
    Path((namespace, name)): Path<(String, String)>,
) -> Result<StatusCode, ApiError> {
    state.registry.delete(&namespace, &name).await?;
    info!(namespace, name, "workflow deleted");
    Ok(StatusCode::NO_CONTENT)
}

/// DELETE /flows/:namespace/:name/versions/:version - Delete specific version
#[instrument(skip(state))]
pub async fn delete_workflow_version(
    State(state): State<AppState>,
    Path((namespace, name, version)): Path<(String, String, String)>,
) -> Result<StatusCode, ApiError> {
    state
        .registry
        .delete_version(&namespace, &name, &version)
        .await?;
    info!(namespace, name, version, "version deleted");
    Ok(StatusCode::NO_CONTENT)
}

/// POST /flows/:namespace/:name/execute - Execute workflow
#[instrument(skip(state))]
pub async fn execute_workflow(
    State(state): State<AppState>,
    Path((namespace, name)): Path<(String, String)>,
    Json(request): Json<ExecuteWorkflowRequest>,
) -> Result<(StatusCode, Json<ExecuteWorkflowResponse>), ApiError> {
    // Check if workflow exists and is enabled
    let enabled = state.registry.is_enabled(&namespace, &name).await?;
    if !enabled {
        return Err(ApiError::BadRequest(format!(
            "Workflow {}/{} is disabled",
            namespace, name
        )));
    }

    // Get the workflow definition to validate it exists
    let definition = if let Some(version) = &request.version {
        state.registry.get_version(&namespace, &name, version).await?
    } else {
        state.registry.get(&namespace, &name).await?
    };

    // Generate instance ID
    let instance_id = Uuid::new_v4();

    info!(
        namespace,
        name,
        version = %definition.metadata.version,
        %instance_id,
        "workflow execution requested"
    );

    // Create execution request message
    let exec_request = ExecutionRequest {
        instance_id,
        namespace: namespace.clone(),
        name: name.clone(),
        version: Some(definition.metadata.version.clone()),
        input: request.input,
        durability: request.durability,
        requested_at: Utc::now(),
    };

    // Publish to Iggy execution-requests topic
    let stream_id = Identifier::from_str(FLOWR_STREAM)
        .map_err(|e| ApiError::Internal(format!("invalid stream id: {}", e)))?;
    let topic_id = Identifier::from_str(EXECUTION_REQUESTS_TOPIC)
        .map_err(|e| ApiError::Internal(format!("invalid topic id: {}", e)))?;

    // Partition by instance_id for even distribution
    let partition_id = {
        let bytes = instance_id.as_bytes();
        let hash = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
        (hash % 8) + 1
    };

    let payload = serde_json::to_vec(&exec_request)
        .map_err(|e| ApiError::Internal(format!("failed to serialize request: {}", e)))?;

    // Ensure stream and topic exist (create if not)
    ensure_infrastructure(&state).await?;

    state
        .iggy
        .send_messages(
            &stream_id,
            &topic_id,
            &Partitioning::partition_id(partition_id),
            &mut vec![Message::from_str(&String::from_utf8_lossy(&payload)).map_err(|e| ApiError::Internal(format!("invalid message: {}", e)))?],
        )
        .await
        .map_err(|e| ApiError::Internal(format!("failed to publish to Iggy: {}", e)))?;

    info!(
        namespace,
        name,
        version = %definition.metadata.version,
        %instance_id,
        "execution request published to Iggy"
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

/// Ensure Iggy stream and topics exist
async fn ensure_infrastructure(state: &AppState) -> Result<(), ApiError> {
    let stream_id = Identifier::from_str(FLOWR_STREAM)
        .map_err(|e| ApiError::Internal(format!("invalid stream id: {}", e)))?;

    // Create stream if not exists
    match state.iggy.create_stream(FLOWR_STREAM, None).await {
        Ok(_) => info!("Created stream: {}", FLOWR_STREAM),
        Err(e) => {
            if !e.to_string().contains("already exists") {
                warn!(error = %e, "Failed to create stream");
            }
        }
    }

    // Create execution-requests topic if not exists
    match state
        .iggy
        .create_topic(
            &stream_id,
            EXECUTION_REQUESTS_TOPIC,
            8,    // partitions
            CompressionAlgorithm::None,
            None, // replication factor
            None, // topic_id
            IggyExpiry::NeverExpire,
            MaxTopicSize::Unlimited,
        )
        .await
    {
        Ok(_) => info!("Created topic: {}", EXECUTION_REQUESTS_TOPIC),
        Err(e) => {
            if !e.to_string().contains("already exists") {
                warn!(error = %e, "Failed to create topic");
            }
        }
    }

    Ok(())
}
