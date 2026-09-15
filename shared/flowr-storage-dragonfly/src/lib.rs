//! FlowR Storage - Redis/Dragonfly backend implementation
//!
//! Provides distributed storage using Redis (or Dragonfly) for FlowR's
//! multi-node workflow engine. EventBus and ExecutionQueue use in-memory
//! channels (same pattern as LanceStorage) — Iggy backends can be added later.
//!
//! # Key Layout
//!
//! - **Workflows**: `flow:{ns}:{name}:def:{ver}`, `flow:{ns}:{name}:version`, etc.
//! - **Resources**: `resources:{ns}:{name}`, `hub:types:{name}`, etc.
//! - **State**: arbitrary keys via GET/SET/DEL
//! - **Executions**: `exec:{instance_id}`, `exec:recent` (sorted set)
//! - **Checkpoints**: `checkpoint:{instance_id}:latest`, `checkpoint:{instance_id}:all`
//! - **Triggers**: `triggers:{id}`, `triggers:_index`
//!
//! # Usage
//!
//! ```rust,no_run
//! use flowr_storage_dragonfly::DragonflyStorage;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let storage = DragonflyStorage::connect("redis://127.0.0.1:6379").await?;
//! storage.init().await?;
//! # Ok(())
//! # }
//! ```

pub mod checkpoints;
pub mod events;
pub mod executions;
pub mod queue;
pub mod resources;
pub mod state;
pub mod triggers;
pub mod workflows;

use async_trait::async_trait;
use tokio::sync::broadcast;

use flowr_storage::{
    CheckpointStore, EventBus, ExecutionQueue, ExecutionStore, ResourceStore, StateStore,
    StorageBackend, TriggerStore, WorkflowStore,
};
use flowr_types::FlowResult;

use crate::checkpoints::RedisCheckpointStore;
use crate::events::InMemoryEventBus;
use crate::executions::RedisExecutionStore;
use crate::queue::InMemoryExecutionQueue;
use crate::resources::RedisResourceStore;
use crate::state::RedisStateStore;
use crate::triggers::RedisTriggerStore;
use crate::workflows::RedisWorkflowStore;

// Re-export storage traits for convenience
pub use flowr_storage::{
    EventStream, LifecycleEvent,
};

/// Redis/Dragonfly storage backend for FlowR
///
/// Provides distributed storage using Redis for persistence with in-memory
/// channels for event pub/sub and execution queue (per-process).
pub struct DragonflyStorage {
    workflows: RedisWorkflowStore,
    resources: RedisResourceStore,
    state: RedisStateStore,
    checkpoints: RedisCheckpointStore,
    queue: InMemoryExecutionQueue,
    executions: RedisExecutionStore,
    events: InMemoryEventBus,
    triggers: RedisTriggerStore,
}

impl DragonflyStorage {
    /// Connect to a Redis/Dragonfly instance and create the storage backend
    pub async fn connect(redis_url: &str) -> Result<Self, flowr_types::FlowError> {
        let client = redis::Client::open(redis_url)
            .map_err(|e| flowr_types::FlowError::Storage(format!("Redis connection error: {e}")))?;

        let conn = client
            .get_multiplexed_async_connection()
            .await
            .map_err(|e| flowr_types::FlowError::Storage(format!("Redis connect error: {e}")))?;

        let (event_tx, _) = broadcast::channel(1024);

        Ok(Self {
            workflows: RedisWorkflowStore::new(conn.clone()),
            resources: RedisResourceStore::new(conn.clone()),
            state: RedisStateStore::new(conn.clone()),
            checkpoints: RedisCheckpointStore::new(conn.clone()),
            queue: InMemoryExecutionQueue::new(),
            executions: RedisExecutionStore::new(conn.clone()),
            events: InMemoryEventBus::new(event_tx),
            triggers: RedisTriggerStore::new(conn),
        })
    }
}

#[async_trait]
impl StorageBackend for DragonflyStorage {
    fn workflows(&self) -> &dyn WorkflowStore {
        &self.workflows
    }

    fn resources(&self) -> &dyn ResourceStore {
        &self.resources
    }

    fn state(&self) -> &dyn StateStore {
        &self.state
    }

    fn checkpoints(&self) -> &dyn CheckpointStore {
        &self.checkpoints
    }

    fn events(&self) -> &dyn EventBus {
        &self.events
    }

    fn queue(&self) -> &dyn ExecutionQueue {
        &self.queue
    }

    fn executions(&self) -> &dyn ExecutionStore {
        &self.executions
    }

    fn triggers(&self) -> &dyn TriggerStore {
        &self.triggers
    }

    async fn init(&self) -> FlowResult<()> {
        // Redis doesn't require table initialization — keys are created on demand.
        // We could ping the server here to verify connectivity.
        Ok(())
    }

    async fn close(&self) -> FlowResult<()> {
        // MultiplexedConnection handles cleanup automatically
        Ok(())
    }
}
