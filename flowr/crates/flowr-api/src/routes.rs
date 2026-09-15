//! Route configuration

use axum::{
    routing::{get, post},
    Router,
};
use tower_http::{
    cors::{Any, CorsLayer},
    trace::TraceLayer,
};

#[cfg(feature = "dragonfly")]
use crate::handlers::{events, flows, health, hub, resources};
#[cfg(feature = "dragonfly")]
use crate::state::AppState;

#[cfg(feature = "adb")]
use crate::handlers::adb;
#[cfg(feature = "adb")]
use crate::state::AppStateAdb;

/// Create the application router (Dragonfly mode)
#[cfg(feature = "dragonfly")]
pub fn create_router(state: AppState) -> Router {
    // Flow management routes
    let flow_routes = Router::new()
        .route("/", post(flows::create_workflow).get(flows::list_workflows))
        .route(
            "/:namespace/:name",
            get(flows::get_workflow)
                .patch(flows::update_workflow)
                .delete(flows::delete_workflow),
        )
        .route(
            "/:namespace/:name/versions/:version",
            get(flows::get_workflow_version).delete(flows::delete_workflow_version),
        )
        .route("/:namespace/:name/execute", post(flows::execute_workflow))
        // Event query routes
        .route("/:namespace/:name/events", get(events::get_workflow_events))
        .route("/:namespace/:name/events/stats", get(events::get_event_stats))
        .route(
            "/:namespace/:name/instances/:instance_id/events",
            get(events::get_instance_events),
        );

    // Hub routes - resource type management
    let hub_routes = Router::new()
        .route("/types", get(hub::list_types).post(hub::create_type))
        .route(
            "/types/:name",
            get(hub::get_type).delete(hub::delete_type),
        );

    // Resource routes - configured resource instances
    let resource_routes = Router::new()
        .route("/", get(resources::list_resources).post(resources::create_resource))
        .route("/namespaces", get(resources::list_namespaces))
        .route(
            "/:namespace/:name",
            get(resources::get_resource)
                .patch(resources::update_resource)
                .delete(resources::delete_resource),
        );

    // Health check routes
    let health_routes = Router::new()
        .route("/live", get(health::liveness))
        .route("/ready", get(health::readiness));

    // Combine all routes
    Router::new()
        .nest("/flows", flow_routes)
        .nest("/hub", hub_routes)
        .nest("/resources", resource_routes)
        .nest("/health", health_routes)
        .layer(
            CorsLayer::new()
                .allow_origin(Any)
                .allow_methods(Any)
                .allow_headers(Any),
        )
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

/// Create the application router (ADB mode - ClawDB/LanceDB)
#[cfg(feature = "adb")]
pub fn create_router_adb(state: AppStateAdb) -> Router {
    // Flow management routes
    let flow_routes = Router::new()
        .route("/", post(adb::create_workflow).get(adb::list_workflows))
        .route(
            "/:namespace/:name",
            get(adb::get_workflow)
                .patch(adb::update_workflow)
                .delete(adb::delete_workflow),
        )
        .route(
            "/:namespace/:name/versions/:version",
            get(adb::get_workflow_version),
        )
        .route("/:namespace/:name/execute", post(adb::execute_workflow));

    // Hub routes - resource type management
    let hub_routes = Router::new()
        .route("/types", get(adb::list_types).post(adb::create_type))
        .route(
            "/types/:name",
            get(adb::get_type).delete(adb::delete_type),
        );

    // Resource routes - configured resource instances
    let resource_routes = Router::new()
        .route("/", get(adb::list_resources).post(adb::create_resource))
        .route("/namespaces", get(adb::list_namespaces))
        .route(
            "/:namespace/:name",
            get(adb::get_resource)
                .patch(adb::update_resource)
                .delete(adb::delete_resource),
        );

    // Health check routes
    let health_routes = Router::new()
        .route("/live", get(adb::liveness))
        .route("/ready", get(adb::readiness));

    // Combine all routes
    Router::new()
        .nest("/flows", flow_routes)
        .nest("/hub", hub_routes)
        .nest("/resources", resource_routes)
        .nest("/health", health_routes)
        .layer(
            CorsLayer::new()
                .allow_origin(Any)
                .allow_methods(Any)
                .allow_headers(Any),
        )
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}
