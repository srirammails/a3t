//! FlowR API
//!
//! HTTP Admin API for workflow management.
//!
//! ## Storage Modes
//!
//! - **Dragonfly mode**: DragonflyDB + Iggy (distributed)
//! - **ADB mode**: ClawDB/LanceDB Agent Database (single-node)
//!
//! ## Endpoints
//!
//! - `POST /flows` - Register a new workflow
//! - `GET /flows` - List all workflows
//! - `GET /flows/:namespace/:name` - Get workflow by namespace/name
//! - `GET /flows/:namespace/:name/versions/:version` - Get specific version
//! - `PATCH /flows/:namespace/:name` - Update workflow (enable/disable, set version)
//! - `DELETE /flows/:namespace/:name` - Delete workflow
//! - `DELETE /flows/:namespace/:name/versions/:version` - Delete specific version
//! - `POST /flows/:namespace/:name/execute` - Execute workflow
//! - `GET /health/ready` - Readiness probe
//! - `GET /health/live` - Liveness probe

pub mod error;
pub mod handlers;
pub mod routes;
pub mod state;

pub use error::ApiError;

#[cfg(feature = "dragonfly")]
pub use routes::create_router;
#[cfg(feature = "dragonfly")]
pub use state::AppState;

#[cfg(feature = "adb")]
pub use routes::create_router_adb;
#[cfg(feature = "adb")]
pub use state::AppStateAdb;
