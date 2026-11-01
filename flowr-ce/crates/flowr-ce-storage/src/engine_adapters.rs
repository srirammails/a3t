//! Today's `ExecutionQueue` and `ExecutionStore` over the engine store, so
//! every caller (MCP `execute_flow`, chat, agent messages, triggers, the
//! execution views) moves onto the durable queue without changing.
//!
//! - Enqueueing starts a run (`RunStore::start`): it is durable from that moment.
//! - Reads come from the engine's `runs`; runs from before the engine are still
//!   read from LanceDB, merged newest-first, until retention removes them.
//! - `dequeue` is never called in engine mode: the engine worker claims runs.

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use uuid::Uuid;

use flowr_storage::engine::{Durability, EngineStore, NewRun, NewTask, RunPhase, RunQuery, RunRow, DEFAULT_POOL};
use flowr_storage::{ExecutionQueue, ExecutionStore};
use flowr_types::{ExecutionRequest, ExecutionState, ExecutionStatus, FlowError, FlowResult};

use crate::executions::LanceExecutionStore;

fn store_err(e: flowr_storage::engine::StoreError) -> FlowError {
    FlowError::Storage(e.to_string())
}

/// The run a queued request becomes.
pub fn new_run(request: &ExecutionRequest) -> NewRun {
    NewRun {
        run_id: request.instance_id,
        namespace: request.namespace.clone(),
        flow: request.name.clone(),
        version: request.version.clone().unwrap_or_default(),
        def_hash: String::new(),
        parent_run_id: None,
        durability: match request.durability.as_deref() {
            Some("ephemeral") => Durability::Ephemeral,
            _ => Durability::Durable,
        },
        debug: false,
        breakpoints: None,
        principal: request.principal.as_ref().and_then(|p| serde_json::to_value(p).ok()).unwrap_or(serde_json::Value::Null),
        trigger: request.caller.clone().unwrap_or_else(|| "api".to_string()),
        input: request.input.clone(),
        deadline_at: None,
        dedupe_key: None,
    }
}

/// A run as today's execution views know it.
pub fn execution_state(r: &RunRow) -> ExecutionState {
    let status = match r.phase {
        RunPhase::Pending => ExecutionStatus::Queued,
        RunPhase::Running => ExecutionStatus::Running,
        RunPhase::Waiting | RunPhase::Suspended => ExecutionStatus::Suspended,
        RunPhase::Cancelled => ExecutionStatus::Cancelled,
        RunPhase::Faulted => ExecutionStatus::Failed,
        RunPhase::Completed => ExecutionStatus::Completed,
    };
    let started = r.started_at.unwrap_or(r.created_at);
    ExecutionState {
        instance_id: r.run_id,
        namespace: r.namespace.clone(),
        name: r.flow.clone(),
        version: r.version.clone(),
        status,
        input: r.input.clone(),
        output: r.output.clone(),
        error: r.error.clone(),
        current_step: r.current_step.clone(),
        started_at: started,
        completed_at: r.completed_at,
        duration_us: r.completed_at.and_then(|c| (c - started).num_microseconds()).map(|us| us.max(0) as u64),
        principal: serde_json::from_value(r.principal.clone()).ok(),
    }
}

pub struct EngineQueue {
    engine: Arc<dyn EngineStore>,
}

impl EngineQueue {
    pub fn new(engine: Arc<dyn EngineStore>) -> Self {
        Self { engine }
    }
}

#[async_trait]
impl ExecutionQueue for EngineQueue {
    async fn enqueue(&self, request: ExecutionRequest) -> FlowResult<()> {
        let run = new_run(&request);
        let task = NewTask::start(&run);
        self.engine.start(run, task).await.map_err(store_err)?;
        Ok(())
    }

    async fn dequeue(&self, timeout: Duration) -> FlowResult<Option<ExecutionRequest>> {
        // The engine worker claims runs; nothing dequeues here.
        tokio::time::sleep(timeout).await;
        Ok(None)
    }

    async fn ack(&self, _instance_id: Uuid) -> FlowResult<()> {
        Ok(())
    }

    async fn nack(&self, _request: ExecutionRequest) -> FlowResult<()> {
        Ok(())
    }

    async fn len(&self) -> FlowResult<usize> {
        self.engine.pending(DEFAULT_POOL).await.map_err(store_err)
    }
}

pub struct EngineExecutions {
    engine: Arc<dyn EngineStore>,
    /// Runs recorded before the engine.
    legacy: LanceExecutionStore,
}

impl EngineExecutions {
    pub fn new(engine: Arc<dyn EngineStore>, legacy: LanceExecutionStore) -> Self {
        Self { engine, legacy }
    }

    async fn merged(&self, mut q: RunQuery, legacy: FlowResult<Vec<ExecutionState>>, limit: usize) -> FlowResult<Vec<ExecutionState>> {
        q.limit = limit;
        let mut out: Vec<ExecutionState> = self.engine.list(q).await.map_err(store_err)?.items.iter().map(execution_state).collect();
        let seen: std::collections::HashSet<Uuid> = out.iter().map(|s| s.instance_id).collect();
        out.extend(legacy.unwrap_or_default().into_iter().filter(|s| !seen.contains(&s.instance_id)));
        out.sort_by(|a, b| b.started_at.cmp(&a.started_at));
        out.truncate(limit);
        Ok(out)
    }
}

#[async_trait]
impl ExecutionStore for EngineExecutions {
    async fn save(&self, state: ExecutionState) -> FlowResult<()> {
        // Runs the engine knows are written by their owner; anything else (a
        // refusal recorded before a run exists) goes to the legacy table.
        if self.engine.get(state.instance_id).await.map_err(store_err)?.is_some() {
            return Ok(());
        }
        self.legacy.save(state).await
    }

    async fn get(&self, instance_id: Uuid) -> FlowResult<Option<ExecutionState>> {
        if let Some(r) = self.engine.get(instance_id).await.map_err(store_err)? {
            return Ok(Some(execution_state(&r)));
        }
        self.legacy.get(instance_id).await
    }

    async fn list_recent(&self, limit: usize) -> FlowResult<Vec<ExecutionState>> {
        let legacy = self.legacy.list_recent(limit).await;
        self.merged(RunQuery::default(), legacy, limit).await
    }

    async fn list_by_workflow(&self, namespace: &str, name: &str, limit: usize) -> FlowResult<Vec<ExecutionState>> {
        let legacy = self.legacy.list_by_workflow(namespace, name, limit).await;
        let q = RunQuery { namespaces: Some(vec![namespace.to_string()]), flow: Some(name.to_string()), ..Default::default() };
        self.merged(q, legacy, limit).await
    }

    /// Unfinished runs in the legacy table, for the one-time startup sweep;
    /// the engine recovers its own runs.
    async fn list_open(&self) -> FlowResult<Vec<ExecutionState>> {
        self.legacy.list_open().await
    }
}
