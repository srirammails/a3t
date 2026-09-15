//! Redis-backed TriggerStore implementation
//!
//! Key layout:
//! - `triggers:{id}`        — trigger JSON (Value)
//! - `triggers:_index`      — set of all trigger IDs
//! - `triggers:ns:{ns}`     — set of trigger IDs in namespace

use async_trait::async_trait;
use redis::AsyncCommands;
use serde_json::Value;
use tracing::debug;

use flowr_storage::TriggerStore;
use flowr_types::{FlowError, FlowResult};

const TRIGGER_PREFIX: &str = "triggers";
const TRIGGER_INDEX: &str = "triggers:_index";

/// Redis-backed trigger store
pub struct RedisTriggerStore {
    redis: redis::aio::MultiplexedConnection,
}

impl RedisTriggerStore {
    pub fn new(redis: redis::aio::MultiplexedConnection) -> Self {
        Self { redis }
    }

    fn trigger_key(id: &str) -> String {
        format!("{TRIGGER_PREFIX}:{id}")
    }

    fn ns_key(namespace: &str) -> String {
        format!("{TRIGGER_PREFIX}:ns:{namespace}")
    }

    /// Extract the "id" field from a trigger JSON value
    fn extract_id(trigger: &Value) -> Option<String> {
        trigger.get("id").and_then(|v| v.as_str()).map(|s| s.to_string())
    }

    /// Extract the "namespace" field from a trigger JSON value
    fn extract_namespace(trigger: &Value) -> Option<String> {
        trigger.get("namespace").and_then(|v| v.as_str()).map(|s| s.to_string())
    }
}

#[async_trait]
impl TriggerStore for RedisTriggerStore {
    async fn list(&self, namespace: Option<&str>) -> FlowResult<Vec<Value>> {
        let mut conn = self.redis.clone();

        let ids: Vec<String> = if let Some(ns) = namespace {
            // Get IDs for specific namespace
            conn.smembers(&Self::ns_key(ns))
                .await
                .map_err(|e| FlowError::Storage(e.to_string()))?
        } else {
            // Get all IDs
            conn.smembers(TRIGGER_INDEX)
                .await
                .map_err(|e| FlowError::Storage(e.to_string()))?
        };

        let mut triggers = Vec::with_capacity(ids.len());
        for id in ids {
            match self.get(&id).await? {
                Some(t) => triggers.push(t),
                None => debug!(id = %id, "trigger in index but not found"),
            }
        }

        Ok(triggers)
    }

    async fn get(&self, id: &str) -> FlowResult<Option<Value>> {
        let mut conn = self.redis.clone();

        let key = Self::trigger_key(id);
        let json: Option<String> = conn
            .get(&key)
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

    async fn create(&self, trigger: Value) -> FlowResult<String> {
        let id = Self::extract_id(&trigger).unwrap_or_else(|| {
            uuid::Uuid::now_v7().to_string()
        });
        let namespace = Self::extract_namespace(&trigger);

        // Ensure the stored value has an id field
        let mut trigger = trigger;
        if trigger.get("id").is_none() {
            if let Value::Object(ref mut map) = trigger {
                map.insert("id".to_string(), Value::String(id.clone()));
            }
        }

        let mut conn = self.redis.clone();

        let key = Self::trigger_key(&id);
        let json = serde_json::to_string(&trigger)
            .map_err(|e| FlowError::Serialization(e.to_string()))?;

        conn.set::<_, _, ()>(&key, &json)
            .await
            .map_err(|e| FlowError::Storage(e.to_string()))?;

        // Add to global index
        conn.sadd::<_, _, ()>(TRIGGER_INDEX, &id)
            .await
            .map_err(|e| FlowError::Storage(e.to_string()))?;

        // Add to namespace index
        if let Some(ns) = namespace {
            conn.sadd::<_, _, ()>(&Self::ns_key(&ns), &id)
                .await
                .map_err(|e| FlowError::Storage(e.to_string()))?;
        }

        Ok(id)
    }

    async fn delete(&self, id: &str) -> FlowResult<()> {
        let mut conn = self.redis.clone();

        // Get trigger first to find namespace
        let existing = self.get(id).await?;
        let namespace = existing.as_ref().and_then(Self::extract_namespace);

        let key = Self::trigger_key(id);
        conn.del::<_, ()>(&key)
            .await
            .map_err(|e| FlowError::Storage(e.to_string()))?;

        conn.srem::<_, _, ()>(TRIGGER_INDEX, id)
            .await
            .map_err(|e| FlowError::Storage(e.to_string()))?;

        if let Some(ns) = namespace {
            conn.srem::<_, _, ()>(&Self::ns_key(&ns), id)
                .await
                .map_err(|e| FlowError::Storage(e.to_string()))?;
        }

        Ok(())
    }

    async fn enable(&self, id: &str) -> FlowResult<()> {
        let trigger = self.get(id).await?;
        let mut trigger = trigger.ok_or_else(|| FlowError::NotFound(format!("trigger '{id}' not found")))?;

        if let Value::Object(ref mut map) = trigger {
            map.insert("enabled".to_string(), Value::Bool(true));
        }

        let mut conn = self.redis.clone();
        let key = Self::trigger_key(id);
        let json = serde_json::to_string(&trigger)
            .map_err(|e| FlowError::Serialization(e.to_string()))?;
        conn.set::<_, _, ()>(&key, &json)
            .await
            .map_err(|e| FlowError::Storage(e.to_string()))?;

        Ok(())
    }

    async fn pause(&self, id: &str) -> FlowResult<()> {
        let trigger = self.get(id).await?;
        let mut trigger = trigger.ok_or_else(|| FlowError::NotFound(format!("trigger '{id}' not found")))?;

        if let Value::Object(ref mut map) = trigger {
            map.insert("enabled".to_string(), Value::Bool(false));
        }

        let mut conn = self.redis.clone();
        let key = Self::trigger_key(id);
        let json = serde_json::to_string(&trigger)
            .map_err(|e| FlowError::Serialization(e.to_string()))?;
        conn.set::<_, _, ()>(&key, &json)
            .await
            .map_err(|e| FlowError::Storage(e.to_string()))?;

        Ok(())
    }

    async fn update(&self, id: &str, trigger: Value) -> FlowResult<()> {
        // Verify trigger exists
        let existing = self.get(id).await?;
        if existing.is_none() {
            return Err(FlowError::NotFound(format!("trigger '{id}' not found")));
        }

        let mut conn = self.redis.clone();
        let key = Self::trigger_key(id);
        let json = serde_json::to_string(&trigger)
            .map_err(|e| FlowError::Serialization(e.to_string()))?;
        conn.set::<_, _, ()>(&key, &json)
            .await
            .map_err(|e| FlowError::Storage(e.to_string()))?;

        Ok(())
    }

    async fn attach(&self, trigger_id: &str, target: Value) -> FlowResult<()> {
        let trigger = self.get(trigger_id).await?;
        let mut trigger = trigger.ok_or_else(|| {
            FlowError::NotFound(format!("trigger '{trigger_id}' not found"))
        })?;

        if let Value::Object(ref mut map) = trigger {
            let targets = map
                .entry("targets".to_string())
                .or_insert_with(|| Value::Array(Vec::new()));
            if let Value::Array(ref mut arr) = targets {
                arr.push(target);
            }
        }

        let mut conn = self.redis.clone();
        let key = Self::trigger_key(trigger_id);
        let json = serde_json::to_string(&trigger)
            .map_err(|e| FlowError::Serialization(e.to_string()))?;
        conn.set::<_, _, ()>(&key, &json)
            .await
            .map_err(|e| FlowError::Storage(e.to_string()))?;

        Ok(())
    }

    async fn detach(&self, trigger_id: &str, target_id: &str) -> FlowResult<()> {
        let trigger = self.get(trigger_id).await?;
        let mut trigger = trigger.ok_or_else(|| {
            FlowError::NotFound(format!("trigger '{trigger_id}' not found"))
        })?;

        if let Value::Object(ref mut map) = trigger {
            if let Some(Value::Array(ref mut arr)) = map.get_mut("targets") {
                arr.retain(|t| {
                    // Match by id or name field
                    let id_match = t.get("id").and_then(|v| v.as_str()) == Some(target_id);
                    let name_match = t.get("name").and_then(|v| v.as_str()) == Some(target_id);
                    !(id_match || name_match)
                });
            }
        }

        let mut conn = self.redis.clone();
        let key = Self::trigger_key(trigger_id);
        let json = serde_json::to_string(&trigger)
            .map_err(|e| FlowError::Serialization(e.to_string()))?;
        conn.set::<_, _, ()>(&key, &json)
            .await
            .map_err(|e| FlowError::Storage(e.to_string()))?;

        Ok(())
    }
}
