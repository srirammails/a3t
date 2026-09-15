//! Redis-backed CheckpointStore implementation
//!
//! Key layout:
//! - `checkpoint:{instance_id}:latest` — latest Checkpoint JSON
//! - `checkpoint:{instance_id}:all`    — list of all Checkpoint JSONs (lpush)

use async_trait::async_trait;
use redis::AsyncCommands;
use uuid::Uuid;

use flowr_storage::CheckpointStore;
use flowr_types::{Checkpoint, FlowError, FlowResult};

/// Redis-backed checkpoint store
pub struct RedisCheckpointStore {
    redis: redis::aio::MultiplexedConnection,
}

impl RedisCheckpointStore {
    pub fn new(redis: redis::aio::MultiplexedConnection) -> Self {
        Self { redis }
    }

    fn latest_key(instance_id: Uuid) -> String {
        format!("checkpoint:{instance_id}:latest")
    }

    fn all_key(instance_id: Uuid) -> String {
        format!("checkpoint:{instance_id}:all")
    }
}

#[async_trait]
impl CheckpointStore for RedisCheckpointStore {
    async fn save(&self, checkpoint: Checkpoint) -> FlowResult<()> {
        let mut conn = self.redis.clone();

        let json = serde_json::to_string(&checkpoint)
            .map_err(|e| FlowError::Serialization(e.to_string()))?;

        let latest_key = Self::latest_key(checkpoint.instance_id);
        let all_key = Self::all_key(checkpoint.instance_id);

        // Set latest checkpoint
        conn.set::<_, _, ()>(&latest_key, &json)
            .await
            .map_err(|e| FlowError::Storage(e.to_string()))?;

        // Prepend to history list
        conn.lpush::<_, _, ()>(&all_key, &json)
            .await
            .map_err(|e| FlowError::Storage(e.to_string()))?;

        Ok(())
    }

    async fn load(&self, instance_id: Uuid) -> FlowResult<Option<Checkpoint>> {
        let mut conn = self.redis.clone();

        let key = Self::latest_key(instance_id);
        let json: Option<String> = conn
            .get(&key)
            .await
            .map_err(|e| FlowError::Storage(e.to_string()))?;

        match json {
            Some(data) => {
                let cp: Checkpoint = serde_json::from_str(&data)
                    .map_err(|e| FlowError::Serialization(e.to_string()))?;
                Ok(Some(cp))
            }
            None => Ok(None),
        }
    }

    async fn delete(&self, instance_id: Uuid) -> FlowResult<()> {
        let mut conn = self.redis.clone();

        let latest_key = Self::latest_key(instance_id);
        let all_key = Self::all_key(instance_id);

        conn.del::<_, ()>(&latest_key)
            .await
            .map_err(|e| FlowError::Storage(e.to_string()))?;

        conn.del::<_, ()>(&all_key)
            .await
            .map_err(|e| FlowError::Storage(e.to_string()))?;

        Ok(())
    }

    async fn list(&self, instance_id: Uuid) -> FlowResult<Vec<Checkpoint>> {
        let mut conn = self.redis.clone();

        let all_key = Self::all_key(instance_id);
        let entries: Vec<String> = conn
            .lrange(&all_key, 0, -1)
            .await
            .map_err(|e| FlowError::Storage(e.to_string()))?;

        let mut checkpoints = Vec::with_capacity(entries.len());
        for entry in entries {
            let cp: Checkpoint = serde_json::from_str(&entry)
                .map_err(|e| FlowError::Serialization(e.to_string()))?;
            checkpoints.push(cp);
        }

        Ok(checkpoints)
    }
}
