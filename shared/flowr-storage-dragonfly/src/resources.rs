//! Redis-backed ResourceStore implementation
//!
//! Key layout (matches flowr-hub):
//! - `hub:types:{name}`            — ResourceType JSON
//! - `hub:types:_index`            — set of all type names
//! - `resources:{ns}:{name}`       — Resource JSON
//! - `resources:{ns}:_index`       — set of resource names in namespace

use async_trait::async_trait;
use redis::AsyncCommands;
use serde_json::Value;
use tracing::debug;

use flowr_storage::ResourceStore;
use flowr_types::{FlowError, FlowResult, Resource, ResourceType};

const TYPE_PREFIX: &str = "hub:types";
const TYPE_INDEX: &str = "hub:types:_index";
const RESOURCE_PREFIX: &str = "resources";
const INDEX_SUFFIX: &str = "_index";

/// Redis-backed resource store
pub struct RedisResourceStore {
    redis: redis::aio::MultiplexedConnection,
}

impl RedisResourceStore {
    pub fn new(redis: redis::aio::MultiplexedConnection) -> Self {
        Self { redis }
    }
}

#[async_trait]
impl ResourceStore for RedisResourceStore {
    async fn register_type(&self, resource_type: ResourceType) -> FlowResult<()> {
        let mut conn = self.redis.clone();

        let key = format!("{TYPE_PREFIX}:{}", resource_type.name);
        let json = serde_json::to_string(&resource_type)
            .map_err(|e| FlowError::Serialization(e.to_string()))?;

        conn.set::<_, _, ()>(&key, &json)
            .await
            .map_err(|e| FlowError::Storage(e.to_string()))?;

        conn.sadd::<_, _, ()>(TYPE_INDEX, &resource_type.name)
            .await
            .map_err(|e| FlowError::Storage(e.to_string()))?;

        Ok(())
    }

    async fn get_type(&self, name: &str) -> FlowResult<ResourceType> {
        let mut conn = self.redis.clone();

        let key = format!("{TYPE_PREFIX}:{name}");
        let json: Option<String> = conn
            .get(&key)
            .await
            .map_err(|e| FlowError::Storage(e.to_string()))?;

        let json = json.ok_or_else(|| FlowError::NotFound(format!("resource type '{name}' not found")))?;

        serde_json::from_str(&json).map_err(|e| FlowError::Serialization(e.to_string()))
    }

    async fn list_types(&self) -> FlowResult<Vec<ResourceType>> {
        let mut conn = self.redis.clone();

        let names: Vec<String> = conn
            .smembers(TYPE_INDEX)
            .await
            .map_err(|e| FlowError::Storage(e.to_string()))?;

        let mut types = Vec::with_capacity(names.len());
        for name in names {
            match self.get_type(&name).await {
                Ok(rt) => types.push(rt),
                Err(e) => debug!(name = %name, error = %e, "failed to load resource type"),
            }
        }

        types.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(types)
    }

    async fn create(&self, resource: Resource) -> FlowResult<Resource> {
        // Check if already exists
        if self.exists(&resource.namespace, &resource.name).await? {
            return Err(FlowError::Validation(format!(
                "resource {}/{} already exists",
                resource.namespace, resource.name
            )));
        }

        self.store_resource(&resource).await?;
        Ok(resource)
    }

    async fn get(&self, namespace: &str, name: &str) -> FlowResult<Resource> {
        let mut conn = self.redis.clone();

        let key = format!("{RESOURCE_PREFIX}:{namespace}:{name}");
        let json: Option<String> = conn
            .get(&key)
            .await
            .map_err(|e| FlowError::Storage(e.to_string()))?;

        let json = json.ok_or_else(|| FlowError::ResourceNotFound {
            namespace: namespace.to_string(),
            name: name.to_string(),
        })?;

        serde_json::from_str(&json).map_err(|e| FlowError::Serialization(e.to_string()))
    }

    async fn list(&self, namespace: &str) -> FlowResult<Vec<Resource>> {
        let mut conn = self.redis.clone();

        let index_key = format!("{RESOURCE_PREFIX}:{namespace}:{INDEX_SUFFIX}");
        let names: Vec<String> = conn
            .smembers(&index_key)
            .await
            .map_err(|e| FlowError::Storage(e.to_string()))?;

        let mut resources = Vec::with_capacity(names.len());
        for name in names {
            match self.get(namespace, &name).await {
                Ok(r) => resources.push(r),
                Err(e) => debug!(namespace = %namespace, name = %name, error = %e, "failed to load resource"),
            }
        }

        resources.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(resources)
    }

    async fn update(
        &self,
        namespace: &str,
        name: &str,
        config: Value,
    ) -> FlowResult<Resource> {
        let mut resource = self.get(namespace, name).await?;
        resource.config = config;
        resource.updated_at = chrono::Utc::now();

        self.store_resource(&resource).await?;
        Ok(resource)
    }

    async fn delete(&self, namespace: &str, name: &str) -> FlowResult<()> {
        let mut conn = self.redis.clone();

        let key = format!("{RESOURCE_PREFIX}:{namespace}:{name}");
        let index_key = format!("{RESOURCE_PREFIX}:{namespace}:{INDEX_SUFFIX}");

        conn.del::<_, ()>(&key)
            .await
            .map_err(|e| FlowError::Storage(e.to_string()))?;

        conn.srem::<_, _, ()>(&index_key, name)
            .await
            .map_err(|e| FlowError::Storage(e.to_string()))?;

        Ok(())
    }

    async fn exists(&self, namespace: &str, name: &str) -> FlowResult<bool> {
        let mut conn = self.redis.clone();

        let key = format!("{RESOURCE_PREFIX}:{namespace}:{name}");
        let exists: bool = conn
            .exists(&key)
            .await
            .map_err(|e| FlowError::Storage(e.to_string()))?;

        Ok(exists)
    }
}

impl RedisResourceStore {
    async fn store_resource(&self, resource: &Resource) -> FlowResult<()> {
        let mut conn = self.redis.clone();

        let key = format!("{RESOURCE_PREFIX}:{}:{}", resource.namespace, resource.name);
        let index_key = format!("{RESOURCE_PREFIX}:{}:{INDEX_SUFFIX}", resource.namespace);

        let json = serde_json::to_string(resource)
            .map_err(|e| FlowError::Serialization(e.to_string()))?;

        conn.set::<_, _, ()>(&key, &json)
            .await
            .map_err(|e| FlowError::Storage(e.to_string()))?;

        conn.sadd::<_, _, ()>(&index_key, &resource.name)
            .await
            .map_err(|e| FlowError::Storage(e.to_string()))?;

        Ok(())
    }
}
