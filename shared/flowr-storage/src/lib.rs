//! FlowR Storage - Backend-agnostic storage trait abstractions
//!
//! This crate defines traits for storage operations that can be implemented
//! by different backends:
//! - LanceDB for ClawdFlow (single-node)
//! - Iggy + Redis for FlowR (distributed)
//!
//! # Traits
//!
//! - [`WorkflowStore`] - Workflow definition storage
//! - [`ResourceStore`] - Resource configuration storage
//! - [`StateStore`] - Key-value state storage
//! - [`CheckpointStore`] - Checkpoint persistence
//! - [`EventBus`] - Lifecycle event pub/sub
//! - [`ExecutionQueue`] - Execution request queue
//! - [`TriggerStore`] - Trigger CRUD (Value-based, backend-agnostic)

pub mod engine;
pub mod engine_memory;
pub mod filter;
#[cfg(any(test, feature = "conformance"))]
pub mod conformance;

use std::pin::Pin;
use std::time::Duration;

use async_trait::async_trait;
use futures::Stream;
use serde_json::Value;
use uuid::Uuid;

use flowr_types::{
    Checkpoint, ExecutionRequest, ExecutionState,
    FlowResult, Resource, ResourceType,
    WorkflowRecord, WorkflowSummary,
};

/// Stream of lifecycle events
pub type EventStream = Pin<Box<dyn Stream<Item = LifecycleEvent> + Send>>;

/// Lifecycle event for workflow/task state changes
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LifecycleEvent {
    /// Event ID
    pub id: Uuid,
    /// Event type (e.g., "workflow.started", "task.completed")
    pub event_type: String,
    /// Instance ID this event belongs to
    pub instance_id: Uuid,
    /// Event data
    pub data: Value,
    /// Timestamp
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

impl LifecycleEvent {
    /// Create a new lifecycle event
    pub fn new(event_type: impl Into<String>, instance_id: Uuid, data: Value) -> Self {
        Self {
            id: Uuid::now_v7(),
            event_type: event_type.into(),
            instance_id,
            data,
            timestamp: chrono::Utc::now(),
        }
    }
}

/// Workflow definition storage
#[async_trait]
pub trait WorkflowStore: Send + Sync {
    /// Register a new workflow definition
    async fn register(&self, record: WorkflowRecord) -> FlowResult<()>;

    /// Get a workflow by namespace and name (active version)
    async fn get(&self, namespace: &str, name: &str) -> FlowResult<WorkflowRecord>;

    /// Get a specific version of a workflow
    async fn get_version(
        &self,
        namespace: &str,
        name: &str,
        version: &str,
    ) -> FlowResult<WorkflowRecord>;

    /// List all workflows in a namespace
    async fn list(&self, namespace: &str) -> FlowResult<Vec<WorkflowSummary>>;

    /// Delete a workflow
    async fn delete(&self, namespace: &str, name: &str) -> FlowResult<()>;

    /// Enable or disable a workflow
    async fn set_enabled(&self, namespace: &str, name: &str, enabled: bool) -> FlowResult<()>;

    /// Set the active version for a workflow
    async fn set_active_version(
        &self,
        namespace: &str,
        name: &str,
        version: &str,
    ) -> FlowResult<()>;

    /// Check if a workflow exists
    async fn exists(&self, namespace: &str, name: &str) -> FlowResult<bool>;

    /// A flow's versions and which one is active (`(versions, active)`); none if
    /// the store can't tell.
    async fn versions(&self, _namespace: &str, _name: &str) -> FlowResult<(Vec<String>, String)> {
        Ok((Vec::new(), String::new()))
    }

    /// Every namespace that holds a workflow. Stores that can't enumerate
    /// namespaces return none, and callers fall back to the ones they know.
    async fn namespaces(&self) -> FlowResult<Vec<String>> {
        Ok(Vec::new())
    }
}

/// Resource configuration storage
#[async_trait]
pub trait ResourceStore: Send + Sync {
    /// Register a resource type
    async fn register_type(&self, resource_type: ResourceType) -> FlowResult<()>;

    /// Get a resource type by name
    async fn get_type(&self, name: &str) -> FlowResult<ResourceType>;

    /// List all resource types
    async fn list_types(&self) -> FlowResult<Vec<ResourceType>>;

    /// Create a new resource
    async fn create(&self, resource: Resource) -> FlowResult<Resource>;

    /// Get a resource by namespace and name
    async fn get(&self, namespace: &str, name: &str) -> FlowResult<Resource>;

    /// List all resources in a namespace
    async fn list(&self, namespace: &str) -> FlowResult<Vec<Resource>>;

    /// Every namespace that holds a resource; none if the store can't tell.
    async fn namespaces(&self) -> FlowResult<Vec<String>> {
        Ok(Vec::new())
    }

    /// Update a resource
    async fn update(
        &self,
        namespace: &str,
        name: &str,
        config: Value,
    ) -> FlowResult<Resource>;

    /// Delete a resource
    async fn delete(&self, namespace: &str, name: &str) -> FlowResult<()>;

    /// Check if a resource exists
    async fn exists(&self, namespace: &str, name: &str) -> FlowResult<bool>;
}

/// Key-value state storage
#[async_trait]
pub trait StateStore: Send + Sync {
    /// Get a value by key
    async fn get(&self, key: &str) -> FlowResult<Option<Value>>;

    /// Set a value with optional TTL
    async fn set(&self, key: &str, value: Value, ttl: Option<Duration>) -> FlowResult<()>;

    /// Delete a key
    async fn delete(&self, key: &str) -> FlowResult<bool>;

    /// List keys matching a pattern
    async fn keys(&self, pattern: &str) -> FlowResult<Vec<String>>;

    /// Check if a key exists
    async fn exists(&self, key: &str) -> FlowResult<bool>;

    /// Increment a numeric value
    async fn incr(&self, key: &str, delta: i64) -> FlowResult<i64>;
}

/// Checkpoint persistence
#[async_trait]
pub trait CheckpointStore: Send + Sync {
    /// Save a checkpoint
    async fn save(&self, checkpoint: Checkpoint) -> FlowResult<()>;

    /// Load the latest checkpoint for an instance
    async fn load(&self, instance_id: Uuid) -> FlowResult<Option<Checkpoint>>;

    /// Delete checkpoints for an instance
    async fn delete(&self, instance_id: Uuid) -> FlowResult<()>;

    /// List all checkpoints for an instance
    async fn list(&self, instance_id: Uuid) -> FlowResult<Vec<Checkpoint>>;
}

/// Lifecycle event pub/sub
#[async_trait]
pub trait EventBus: Send + Sync {
    /// Emit a lifecycle event
    async fn emit(&self, event: LifecycleEvent) -> FlowResult<()>;

    /// Subscribe to lifecycle events
    async fn subscribe(&self) -> FlowResult<EventStream>;

    /// Subscribe to events for a specific instance
    async fn subscribe_instance(&self, instance_id: Uuid) -> FlowResult<EventStream>;
}

/// Execution request queue
#[async_trait]
pub trait ExecutionQueue: Send + Sync {
    /// Enqueue an execution request
    async fn enqueue(&self, request: ExecutionRequest) -> FlowResult<()>;

    /// Dequeue the next execution request (blocking with timeout)
    async fn dequeue(&self, timeout: Duration) -> FlowResult<Option<ExecutionRequest>>;

    /// Acknowledge successful processing
    async fn ack(&self, instance_id: Uuid) -> FlowResult<()>;

    /// Return a request to the queue (for retry)
    async fn nack(&self, request: ExecutionRequest) -> FlowResult<()>;

    /// Get queue length
    async fn len(&self) -> FlowResult<usize>;
}

/// Execution state storage
#[async_trait]
pub trait ExecutionStore: Send + Sync {
    /// Save execution state
    async fn save(&self, state: ExecutionState) -> FlowResult<()>;

    /// Get execution state by instance ID
    async fn get(&self, instance_id: Uuid) -> FlowResult<Option<ExecutionState>>;

    /// List recent executions
    async fn list_recent(&self, limit: usize) -> FlowResult<Vec<ExecutionState>>;

    /// List executions for a workflow
    async fn list_by_workflow(
        &self,
        namespace: &str,
        name: &str,
        limit: usize,
    ) -> FlowResult<Vec<ExecutionState>>;

    /// Executions not yet finished (queued, running or suspended). At startup
    /// these are runs a previous process lost; none if the store can't tell.
    async fn list_open(&self) -> FlowResult<Vec<ExecutionState>> {
        Ok(Vec::new())
    }
}

/// Trigger CRUD storage (Value-based for backend flexibility)
///
/// Triggers are first-class resources that route webhooks, cron events, and
/// internal events to workflows or agents. The Value-based interface keeps
/// the shared crate free of CE/EE-specific trigger structs.
#[async_trait]
pub trait TriggerStore: Send + Sync {
    /// List all triggers, optionally filtered by namespace
    async fn list(&self, namespace: Option<&str>) -> FlowResult<Vec<Value>>;

    /// Get a trigger by ID
    async fn get(&self, id: &str) -> FlowResult<Option<Value>>;

    /// Create a trigger from a JSON value. Returns the trigger ID.
    async fn create(&self, trigger: Value) -> FlowResult<String>;

    /// Delete a trigger by ID
    async fn delete(&self, id: &str) -> FlowResult<()>;

    /// Enable a trigger
    async fn enable(&self, id: &str) -> FlowResult<()>;

    /// Pause (disable) a trigger
    async fn pause(&self, id: &str) -> FlowResult<()>;

    /// Update a trigger (replace by ID)
    async fn update(&self, id: &str, trigger: Value) -> FlowResult<()>;

    /// Attach a target (workflow/agent/webhook) to a trigger
    async fn attach(&self, trigger_id: &str, target: Value) -> FlowResult<()>;

    /// Detach a target from a trigger by matching name/id
    async fn detach(&self, trigger_id: &str, target_id: &str) -> FlowResult<()>;
}

/// Key-value platform settings store (auth/security/config knobs).
#[async_trait]
pub trait SettingsStore: Send + Sync {
    async fn get(&self, key: &str) -> FlowResult<Option<Value>>;
    async fn get_string(&self, key: &str) -> FlowResult<Option<String>>;
    async fn set(&self, key: &str, value: &Value, source: &str) -> FlowResult<()>;
    async fn get_all(&self) -> FlowResult<Vec<(String, Value)>>;
    async fn seed_defaults(&self, defaults: &[(&str, Value, &str)]) -> FlowResult<usize>;
}

/// Combined storage backend
///
/// Implementations provide all storage capabilities.
#[async_trait]
pub trait StorageBackend: Send + Sync {
    /// Get settings store
    fn settings(&self) -> &dyn SettingsStore;

    /// Get workflow store
    fn workflows(&self) -> &dyn WorkflowStore;

    /// Get resource store
    fn resources(&self) -> &dyn ResourceStore;

    /// Get state store
    fn state(&self) -> &dyn StateStore;

    /// Get checkpoint store
    fn checkpoints(&self) -> &dyn CheckpointStore;

    /// Get event bus
    fn events(&self) -> &dyn EventBus;

    /// Get execution queue
    fn queue(&self) -> &dyn ExecutionQueue;

    /// Get execution store
    fn executions(&self) -> &dyn ExecutionStore;

    /// Get trigger store
    fn triggers(&self) -> &dyn TriggerStore;

    /// The durable engine store (queue, runs, journal), when this backend has
    /// one. With it, `queue()` and `executions()` are views over it and the
    /// engine worker runs flows; without it, today's in-memory queue does.
    fn engine(&self) -> Option<std::sync::Arc<dyn engine::EngineStore>> {
        None
    }

    /// Initialize the storage backend
    async fn init(&self) -> FlowResult<()>;

    /// Close the storage backend
    async fn close(&self) -> FlowResult<()>;

    /// Housekeeping that must not run concurrently with execution writes —
    /// compaction for an append-only store. The worker calls it only while no
    /// execution is in flight. Returns a short summary for the log; backends
    /// with nothing to do keep the default.
    async fn maintain(&self) -> FlowResult<String> {
        Ok(String::new())
    }
}
