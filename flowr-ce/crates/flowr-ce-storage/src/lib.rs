//! ClawdFlow Storage - LanceDB backend implementation
//!
//! Provides persistent storage using LanceDB for ClawdFlow's single-process
//! workflow engine. Event pub/sub and execution queue use in-memory channels.
//!
//! # Features
//!
//! - **LanceDB Tables**: workflows, resources, state, checkpoints, executions
//! - **In-Memory Channels**: EventBus (broadcast), ExecutionQueue (mpsc)
//! - **TTL Support**: State store supports optional TTL for entries
//!
//! # Usage
//!
//! ```rust,no_run
//! use flowr_ce_storage::LanceStorage;
//! use flowr_storage::StorageBackend;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! // Create storage with file path
//! let storage = LanceStorage::new("./data/clawdflow.lance").await?;
//! storage.init().await?;
//!
//! // Or create in-memory storage for testing
//! let test_storage = LanceStorage::in_memory().await?;
//! # Ok(())
//! # }
//! ```

pub mod checkpoints;
pub mod engine_sqlite;
pub mod error;
pub mod events;
pub mod executions;
pub mod lance;
pub mod queue;
pub mod resources;
pub mod settings;
pub mod state;
pub mod triggers;
pub mod workflows;

pub use error::{StorageError, StorageResult};
pub use lance::LanceStorage;

// Re-export storage traits for convenience
pub use flowr_storage::{
    CheckpointStore, EventBus, EventStream, ExecutionQueue, ExecutionStore, LifecycleEvent,
    ResourceStore, StateStore, StorageBackend, TriggerStore, WorkflowStore,
};
