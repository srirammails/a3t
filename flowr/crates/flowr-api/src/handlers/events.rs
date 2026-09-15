//! Event query handlers
//!
//! Endpoints for querying lifecycle events from Iggy.

use std::str::FromStr;

use axum::{
    extract::{Path, Query, State},
    response::Json,
};
use chrono::{DateTime, Utc};
use iggy::client::MessageClient;
use iggy::consumer::Consumer;
use iggy::identifier::Identifier;
use iggy::messages::poll_messages::PollingStrategy;
use serde::{Deserialize, Serialize};
use tracing::{debug, info};
use uuid::Uuid;

use flowr_core::{LifecycleEvent, LIFECYCLE_EVENTS_TOPIC};

use crate::error::ApiError;
use crate::state::AppState;

/// Query parameters for events endpoint
#[derive(Debug, Deserialize)]
pub struct EventsQuery {
    /// Filter by event type prefix (e.g., "io.serverlessworkflow.workflow")
    #[serde(rename = "type")]
    pub event_type: Option<String>,

    /// Maximum number of events to return
    #[serde(default = "default_limit")]
    pub limit: u32,

    /// Start from this offset
    #[serde(default)]
    pub offset: u64,

    /// Filter events after this timestamp
    pub after: Option<DateTime<Utc>>,

    /// Filter events before this timestamp
    pub before: Option<DateTime<Utc>>,
}

fn default_limit() -> u32 {
    100
}

/// Response for events list
#[derive(Debug, Serialize)]
pub struct EventsResponse {
    /// List of events
    pub events: Vec<LifecycleEvent>,

    /// Total count (if available)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total: Option<u64>,

    /// Next offset for pagination
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_offset: Option<u64>,
}

/// Get events for a specific workflow instance
///
/// GET /flows/{namespace}/{name}/instances/{instance_id}/events
pub async fn get_instance_events(
    State(state): State<AppState>,
    Path((namespace, name, instance_id)): Path<(String, String, String)>,
    Query(query): Query<EventsQuery>,
) -> Result<Json<EventsResponse>, ApiError> {
    info!(
        namespace = %namespace,
        name = %name,
        instance_id = %instance_id,
        "querying instance events"
    );

    let instance_uuid = Uuid::parse_str(&instance_id)
        .map_err(|e| ApiError::BadRequest(format!("invalid instance ID: {}", e)))?;

    // Calculate partition from instance ID (same as in executor)
    let partition_id = {
        const PARTITION_COUNT: u32 = 8;
        let bytes = instance_uuid.as_bytes();
        let hash = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
        (hash % PARTITION_COUNT) + 1
    };

    let events = fetch_events_from_partition(
        &state,
        partition_id,
        Some(&instance_id),
        query.event_type.as_deref(),
        query.limit,
        query.offset,
        query.after,
        query.before,
    )
    .await?;

    let has_more = events.len() == query.limit as usize;
    let next_offset = if has_more {
        Some(query.offset + events.len() as u64)
    } else {
        None
    };

    Ok(Json(EventsResponse {
        events,
        total: None,
        next_offset,
    }))
}

/// Get all events for a workflow (across all instances)
///
/// GET /flows/{namespace}/{name}/events
pub async fn get_workflow_events(
    State(state): State<AppState>,
    Path((namespace, name)): Path<(String, String)>,
    Query(query): Query<EventsQuery>,
) -> Result<Json<EventsResponse>, ApiError> {
    info!(
        namespace = %namespace,
        name = %name,
        "querying workflow events"
    );

    // Filter by source (flowr://namespace/name)
    let source_filter = format!("flowr://{}/{}", namespace, name);

    // Fetch from all partitions and merge
    let mut all_events = Vec::new();

    for partition_id in 1..=8 {
        let partition_events = fetch_events_from_partition(
            &state,
            partition_id,
            None,
            query.event_type.as_deref(),
            query.limit,
            0,
            query.after,
            query.before,
        )
        .await
        .unwrap_or_default();

        // Filter by source
        for event in partition_events {
            if event.source.contains(&source_filter) {
                all_events.push(event);
            }
        }
    }

    // Sort by timestamp (newest first)
    all_events.sort_by(|a, b| b.time.cmp(&a.time));

    // Apply limit
    all_events.truncate(query.limit as usize);

    Ok(Json(EventsResponse {
        events: all_events,
        total: None,
        next_offset: None,
    }))
}

/// Fetch events from a specific partition
async fn fetch_events_from_partition(
    state: &AppState,
    partition_id: u32,
    instance_filter: Option<&str>,
    event_type_filter: Option<&str>,
    limit: u32,
    _offset: u64,
    after: Option<DateTime<Utc>>,
    before: Option<DateTime<Utc>>,
) -> Result<Vec<LifecycleEvent>, ApiError> {
    let stream_id = Identifier::from_str("flowr")
        .map_err(|e| ApiError::Internal(format!("invalid stream id: {}", e)))?;
    let topic_id = Identifier::from_str(LIFECYCLE_EVENTS_TOPIC)
        .map_err(|e| ApiError::Internal(format!("invalid topic id: {}", e)))?;

    // Create a unique consumer for this query
    let consumer_id = Uuid::now_v7().as_u128() as u32;
    let consumer = Consumer::new(
        Identifier::numeric(consumer_id)
            .map_err(|e| ApiError::Internal(format!("invalid consumer id: {}", e)))?,
    );

    // Poll from the end to get recent events
    let polled = state
        .iggy
        .poll_messages(
            &stream_id,
            &topic_id,
            Some(partition_id),
            &consumer,
            &PollingStrategy::last(),
            limit * 2, // Fetch extra to account for filtering
            false,
        )
        .await
        .map_err(|e| ApiError::Internal(format!("failed to poll events: {}", e)))?;

    let mut events = Vec::new();

    for msg in polled.messages.iter().rev() {
        match serde_json::from_slice::<LifecycleEvent>(&msg.payload) {
            Ok(event) => {
                // Apply filters
                if let Some(instance) = instance_filter {
                    if event.subject != instance {
                        continue;
                    }
                }

                if let Some(type_prefix) = event_type_filter {
                    if !event.event_type.starts_with(type_prefix) {
                        continue;
                    }
                }

                if let Some(after) = after {
                    if event.time < after {
                        continue;
                    }
                }

                if let Some(before) = before {
                    if event.time > before {
                        continue;
                    }
                }

                events.push(event);

                if events.len() >= limit as usize {
                    break;
                }
            }
            Err(e) => {
                debug!(error = %e, "failed to deserialize event");
                continue;
            }
        }
    }

    Ok(events)
}

/// Get event statistics for a workflow
///
/// GET /flows/{namespace}/{name}/events/stats
#[derive(Debug, Serialize)]
pub struct EventStats {
    /// Total workflow.started events
    pub workflows_started: u64,
    /// Total workflow.completed events
    pub workflows_completed: u64,
    /// Total workflow.faulted events
    pub workflows_faulted: u64,
    /// Total workflow.suspended events
    pub workflows_suspended: u64,
    /// Total task.started events
    pub tasks_started: u64,
    /// Total task.completed events
    pub tasks_completed: u64,
    /// Total task.faulted events
    pub tasks_faulted: u64,
    /// Total task.retried events
    pub tasks_retried: u64,
}

pub async fn get_event_stats(
    State(state): State<AppState>,
    Path((namespace, name)): Path<(String, String)>,
) -> Result<Json<EventStats>, ApiError> {
    info!(
        namespace = %namespace,
        name = %name,
        "querying event statistics"
    );

    let source_filter = format!("flowr://{}/{}", namespace, name);
    let mut stats = EventStats {
        workflows_started: 0,
        workflows_completed: 0,
        workflows_faulted: 0,
        workflows_suspended: 0,
        tasks_started: 0,
        tasks_completed: 0,
        tasks_faulted: 0,
        tasks_retried: 0,
    };

    // Fetch from all partitions
    for partition_id in 1..=8 {
        let events = fetch_events_from_partition(
            &state,
            partition_id,
            None,
            None,
            1000, // Max for stats
            0,
            None,
            None,
        )
        .await
        .unwrap_or_default();

        for event in events {
            if !event.source.contains(&source_filter) {
                continue;
            }

            match event.event_type.as_str() {
                "io.serverlessworkflow.workflow.started.v1" => stats.workflows_started += 1,
                "io.serverlessworkflow.workflow.completed.v1" => stats.workflows_completed += 1,
                "io.serverlessworkflow.workflow.faulted.v1" => stats.workflows_faulted += 1,
                "io.serverlessworkflow.workflow.suspended.v1" => stats.workflows_suspended += 1,
                "io.serverlessworkflow.task.started.v1" => stats.tasks_started += 1,
                "io.serverlessworkflow.task.completed.v1" => stats.tasks_completed += 1,
                "io.serverlessworkflow.task.faulted.v1" => stats.tasks_faulted += 1,
                "io.serverlessworkflow.task.retried.v1" => stats.tasks_retried += 1,
                _ => {}
            }
        }
    }

    Ok(Json(stats))
}
