//! Health check handlers

use axum::{extract::State, http::StatusCode, Json};
use redis::cmd;
use serde::Serialize;

use crate::state::AppState;

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
    pub dragonfly: ComponentHealth,
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
///
/// Returns 200 if the service is running.
/// Does not check dependencies.
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
///
/// Returns 200 if the service is ready to accept traffic.
/// Checks connectivity to Dragonfly.
pub async fn readiness(State(state): State<AppState>) -> (StatusCode, Json<HealthResponse>) {
    let start = std::time::Instant::now();
    let mut redis = state.redis.clone();

    // Check Dragonfly connectivity using PING command
    let dragonfly_health = match cmd("PING")
        .query_async::<_, String>(&mut redis)
        .await
    {
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

    let all_healthy = dragonfly_health.status == "ok";

    let response = HealthResponse {
        status: if all_healthy { "ok" } else { "degraded" }.to_string(),
        details: Some(HealthDetails {
            dragonfly: dragonfly_health,
        }),
    };

    let status_code = if all_healthy {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };

    (status_code, Json(response))
}
