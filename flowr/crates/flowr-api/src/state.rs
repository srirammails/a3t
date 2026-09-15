//! Application state

use std::sync::Arc;

#[cfg(feature = "dragonfly")]
use flowr_hub::HubRegistry;
#[cfg(feature = "dragonfly")]
use flowr_registry::FlowRegistry;
#[cfg(feature = "dragonfly")]
use iggy::clients::client::IggyClient;

#[cfg(feature = "adb")]
use flowr_storage::StorageBackend;

/// Shared application state for Dragonfly mode
#[cfg(feature = "dragonfly")]
#[derive(Clone)]
pub struct AppState {
    /// Flow registry for workflow storage
    pub registry: Arc<FlowRegistry>,

    /// Hub registry for resource types and resources
    pub hub: Arc<HubRegistry>,

    /// Redis connection for direct access if needed
    pub redis: redis::aio::MultiplexedConnection,

    /// Iggy client for event queries
    pub iggy: Arc<IggyClient>,
}

#[cfg(feature = "dragonfly")]
impl AppState {
    /// Create new application state
    pub fn new(
        registry: FlowRegistry,
        hub: HubRegistry,
        redis: redis::aio::MultiplexedConnection,
        iggy: Arc<IggyClient>,
    ) -> Self {
        Self {
            registry: Arc::new(registry),
            hub: Arc::new(hub),
            redis,
            iggy,
        }
    }
}

/// Shared application state for ADB (ClawDB/LanceDB) mode
#[cfg(feature = "adb")]
#[derive(Clone)]
pub struct AppStateAdb {
    /// Storage backend implementing all storage traits
    pub storage: Arc<dyn StorageBackend>,
}

#[cfg(feature = "adb")]
impl AppStateAdb {
    /// Create new application state with ADB storage
    pub fn new<S: StorageBackend + 'static>(storage: Arc<S>) -> Self {
        Self { storage }
    }
}
