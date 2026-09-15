//! FlowR Hub - Resource Type Registry and Resource Management
//!
//! # Overview
//!
//! The Hub provides a registry of resource types (templates) and configured
//! resource instances that workflows can use to connect to external services.
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────┐
//! │                            HUB                               │
//! │  (Resource Type Templates)                                   │
//! ├─────────────────┬─────────────────┬─────────────────────────┤
//! │ mysql           │ rest_api        │ qdrant                  │
//! │ postgres        │ graphql         │ redis                   │
//! │ kafka           │ grpc            │ s3                      │
//! └─────────────────┴─────────────────┴─────────────────────────┘
//!                              │
//!                              ▼
//! ┌─────────────────────────────────────────────────────────────┐
//! │                         RESOURCES                            │
//! │  (Configured Instances)                                      │
//! ├─────────────────┬─────────────────┬─────────────────────────┤
//! │ prod-mysql      │ stripe-api      │ embeddings-db           │
//! │ staging-db      │ payment-gw      │ product-vectors         │
//! └─────────────────┴─────────────────┴─────────────────────────┘
//! ```
//!
//! # Usage
//!
//! ```rust,ignore
//! use flowr_hub::{HubRegistry, builtin_types};
//!
//! // Create registry with Redis/Dragonfly connection
//! let registry = HubRegistry::new(redis_conn);
//!
//! // Register built-in types
//! for rt in builtin_types() {
//!     registry.register_type(rt).await?;
//! }
//!
//! // Create a resource
//! let request = CreateResourceRequest {
//!     name: "prod-mysql".to_string(),
//!     namespace: "production".to_string(),
//!     type_ref: "mysql".to_string(),
//!     config: json!({
//!         "host": "db.production.internal",
//!         "port": 3306,
//!         "username": "app",
//!         "password": "${secrets.mysql_password}",
//!         "database": "myapp"
//!     }),
//!     ..Default::default()
//! };
//!
//! let resource = registry.create_resource(request).await?;
//! ```

mod builtins;
mod error;
mod registry;
mod types;
mod validation;

// Public exports
pub use builtins::builtin_types;
pub use error::{HubError, HubResult};
pub use registry::HubRegistry;
pub use types::{
    ConnectionKind, CreateResourceRequest, Resource, ResourceCategory, ResourceType,
    UpdateResourceRequest,
};
pub use validation::validate_resource_config;

/// Initialize the hub with built-in types
///
/// Call this at startup to ensure all built-in resource types are registered.
pub async fn init_builtins(registry: &HubRegistry) -> HubResult<()> {
    for rt in builtin_types() {
        // Only register if not already exists
        if !registry.type_exists(&rt.name).await? {
            registry.register_type(rt).await?;
        }
    }
    Ok(())
}
