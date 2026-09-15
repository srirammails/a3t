//! Redis-backed StateStore implementation
//!
//! Simple key-value operations mapping directly to Redis GET/SET/DEL/KEYS.

use std::time::Duration;

use async_trait::async_trait;
use redis::AsyncCommands;
use serde_json::Value;

use flowr_storage::StateStore;
use flowr_types::{FlowError, FlowResult};

/// Redis-backed state store
pub struct RedisStateStore {
    redis: redis::aio::MultiplexedConnection,
}

impl RedisStateStore {
    pub fn new(redis: redis::aio::MultiplexedConnection) -> Self {
        Self { redis }
    }
}

#[async_trait]
impl StateStore for RedisStateStore {
    async fn get(&self, key: &str) -> FlowResult<Option<Value>> {
        let mut conn = self.redis.clone();

        let json: Option<String> = conn
            .get(key)
            .await
            .map_err(|e| FlowError::Storage(e.to_string()))?;

        match json {
            Some(data) => {
                let value: Value = serde_json::from_str(&data)
                    .map_err(|e| FlowError::Serialization(e.to_string()))?;
                Ok(Some(value))
            }
            None => Ok(None),
        }
    }

    async fn set(&self, key: &str, value: Value, ttl: Option<Duration>) -> FlowResult<()> {
        let mut conn = self.redis.clone();

        let json = serde_json::to_string(&value)
            .map_err(|e| FlowError::Serialization(e.to_string()))?;

        match ttl {
            Some(duration) => {
                let seconds = duration.as_secs().max(1);
                conn.set_ex::<_, _, ()>(key, &json, seconds)
                    .await
                    .map_err(|e| FlowError::Storage(e.to_string()))?;
            }
            None => {
                conn.set::<_, _, ()>(key, &json)
                    .await
                    .map_err(|e| FlowError::Storage(e.to_string()))?;
            }
        }

        Ok(())
    }

    async fn delete(&self, key: &str) -> FlowResult<bool> {
        let mut conn = self.redis.clone();

        let deleted: i64 = conn
            .del(key)
            .await
            .map_err(|e| FlowError::Storage(e.to_string()))?;

        Ok(deleted > 0)
    }

    async fn keys(&self, pattern: &str) -> FlowResult<Vec<String>> {
        let mut conn = self.redis.clone();

        // Use SCAN for safety on large datasets
        let mut cursor: u64 = 0;
        let mut all_keys = Vec::new();

        loop {
            let (next_cursor, batch): (u64, Vec<String>) = redis::cmd("SCAN")
                .arg(cursor)
                .arg("MATCH")
                .arg(pattern)
                .arg("COUNT")
                .arg(100)
                .query_async(&mut conn)
                .await
                .map_err(|e| FlowError::Storage(e.to_string()))?;

            all_keys.extend(batch);
            cursor = next_cursor;

            if cursor == 0 {
                break;
            }
        }

        Ok(all_keys)
    }

    async fn exists(&self, key: &str) -> FlowResult<bool> {
        let mut conn = self.redis.clone();

        let exists: bool = conn
            .exists(key)
            .await
            .map_err(|e| FlowError::Storage(e.to_string()))?;

        Ok(exists)
    }

    async fn incr(&self, key: &str, delta: i64) -> FlowResult<i64> {
        let mut conn = self.redis.clone();

        let value: i64 = conn
            .incr(key, delta)
            .await
            .map_err(|e| FlowError::Storage(e.to_string()))?;

        Ok(value)
    }
}
