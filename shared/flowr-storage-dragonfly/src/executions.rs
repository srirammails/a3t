//! Redis-backed ExecutionStore implementation
//!
//! Key layout:
//! - `exec:{instance_id}`               — ExecutionState JSON
//! - `exec:recent`                       — sorted set (score = timestamp)
//! - `exec:workflow:{ns}:{name}`         — set of instance IDs

use async_trait::async_trait;
use redis::AsyncCommands;
use uuid::Uuid;

use flowr_storage::ExecutionStore;
use flowr_types::{ExecutionState, FlowError, FlowResult};

/// Redis-backed execution store
pub struct RedisExecutionStore {
    redis: redis::aio::MultiplexedConnection,
}

impl RedisExecutionStore {
    pub fn new(redis: redis::aio::MultiplexedConnection) -> Self {
        Self { redis }
    }

    fn exec_key(instance_id: Uuid) -> String {
        format!("exec:{instance_id}")
    }

    fn workflow_key(namespace: &str, name: &str) -> String {
        format!("exec:workflow:{namespace}:{name}")
    }
}

#[async_trait]
impl ExecutionStore for RedisExecutionStore {
    async fn save(&self, state: ExecutionState) -> FlowResult<()> {
        let mut conn = self.redis.clone();

        let json = serde_json::to_string(&state)
            .map_err(|e| FlowError::Serialization(e.to_string()))?;

        let key = Self::exec_key(state.instance_id);
        let id_str = state.instance_id.to_string();
        let score = state.started_at.timestamp_millis() as f64;

        // Store execution state
        conn.set::<_, _, ()>(&key, &json)
            .await
            .map_err(|e| FlowError::Storage(e.to_string()))?;

        // Add to recent sorted set
        conn.zadd::<_, _, _, ()>("exec:recent", &id_str, score)
            .await
            .map_err(|e| FlowError::Storage(e.to_string()))?;

        // Add to workflow index
        let wf_key = Self::workflow_key(&state.namespace, &state.name);
        conn.sadd::<_, _, ()>(&wf_key, &id_str)
            .await
            .map_err(|e| FlowError::Storage(e.to_string()))?;

        Ok(())
    }

    async fn get(&self, instance_id: Uuid) -> FlowResult<Option<ExecutionState>> {
        let mut conn = self.redis.clone();

        let key = Self::exec_key(instance_id);
        let json: Option<String> = conn
            .get(&key)
            .await
            .map_err(|e| FlowError::Storage(e.to_string()))?;

        match json {
            Some(data) => {
                let state: ExecutionState = serde_json::from_str(&data)
                    .map_err(|e| FlowError::Serialization(e.to_string()))?;
                Ok(Some(state))
            }
            None => Ok(None),
        }
    }

    async fn list_recent(&self, limit: usize) -> FlowResult<Vec<ExecutionState>> {
        let mut conn = self.redis.clone();

        // Get most recent instance IDs (highest scores = most recent)
        let ids: Vec<String> = conn
            .zrevrange("exec:recent", 0, (limit as isize) - 1)
            .await
            .map_err(|e| FlowError::Storage(e.to_string()))?;

        let mut states = Vec::with_capacity(ids.len());
        for id_str in ids {
            let id = Uuid::parse_str(&id_str).map_err(|e| {
                FlowError::Storage(format!("invalid UUID in exec:recent: {e}"))
            })?;
            if let Some(state) = self.get(id).await? {
                states.push(state);
            }
        }

        Ok(states)
    }

    async fn list_by_workflow(
        &self,
        namespace: &str,
        name: &str,
        limit: usize,
    ) -> FlowResult<Vec<ExecutionState>> {
        let mut conn = self.redis.clone();

        let wf_key = Self::workflow_key(namespace, name);
        let ids: Vec<String> = conn
            .smembers(&wf_key)
            .await
            .map_err(|e| FlowError::Storage(e.to_string()))?;

        let mut states = Vec::with_capacity(ids.len().min(limit));
        for id_str in ids {
            if states.len() >= limit {
                break;
            }
            let id = Uuid::parse_str(&id_str).map_err(|e| {
                FlowError::Storage(format!("invalid UUID in workflow exec set: {e}"))
            })?;
            if let Some(state) = self.get(id).await? {
                states.push(state);
            }
        }

        // Sort by started_at descending
        states.sort_by(|a, b| b.started_at.cmp(&a.started_at));
        Ok(states)
    }
}
