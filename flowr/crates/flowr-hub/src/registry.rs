//! Hub Registry - Storage for resource types and resources
//!
//! Uses Dragonfly/Redis for persistence with the following key structure:
//! - `hub:types:{name}` - Resource type definitions
//! - `hub:types:_index` - Set of all type names
//! - `resources:{namespace}:{name}` - Resource instances
//! - `resources:{namespace}:_index` - Set of resource names in namespace

use redis::AsyncCommands;
use tracing::{debug, info};

use crate::error::{HubError, HubResult};
use crate::types::{CreateResourceRequest, Resource, ResourceType, UpdateResourceRequest};
use crate::validation::validate_resource_config;

/// Key prefix for resource types
const TYPE_PREFIX: &str = "hub:types";
/// Key prefix for resources
const RESOURCE_PREFIX: &str = "resources";
/// Index key suffix
const INDEX_SUFFIX: &str = "_index";

/// Hub Registry - manages resource types and resources
#[derive(Clone)]
pub struct HubRegistry {
    redis: redis::aio::MultiplexedConnection,
}

impl HubRegistry {
    /// Create a new hub registry
    pub fn new(redis: redis::aio::MultiplexedConnection) -> Self {
        Self { redis }
    }

    // ========================================
    // Resource Types
    // ========================================

    /// Register a new resource type
    pub async fn register_type(&self, resource_type: ResourceType) -> HubResult<()> {
        let mut conn = self.redis.clone();

        let key = format!("{}:{}", TYPE_PREFIX, resource_type.name);
        let index_key = format!("{}:{}", TYPE_PREFIX, INDEX_SUFFIX);

        let json = serde_json::to_string(&resource_type)
            .map_err(|e| HubError::Serialization(e.to_string()))?;

        // Store type and add to index
        redis::pipe()
            .set(&key, &json)
            .sadd(&index_key, &resource_type.name)
            .query_async::<_, ()>(&mut conn)
            .await
            .map_err(|e| HubError::Storage(e.to_string()))?;

        info!(name = %resource_type.name, "Registered resource type");
        Ok(())
    }

    /// Get a resource type by name
    pub async fn get_type(&self, name: &str) -> HubResult<ResourceType> {
        let mut conn = self.redis.clone();

        let key = format!("{}:{}", TYPE_PREFIX, name);
        let json: Option<String> = conn
            .get(&key)
            .await
            .map_err(|e| HubError::Storage(e.to_string()))?;

        match json {
            Some(data) => {
                let rt: ResourceType = serde_json::from_str(&data)
                    .map_err(|e| HubError::Serialization(e.to_string()))?;
                Ok(rt)
            }
            None => Err(HubError::TypeNotFound(name.to_string())),
        }
    }

    /// List all resource types
    pub async fn list_types(&self) -> HubResult<Vec<ResourceType>> {
        let mut conn = self.redis.clone();

        let index_key = format!("{}:{}", TYPE_PREFIX, INDEX_SUFFIX);
        let names: Vec<String> = conn
            .smembers(&index_key)
            .await
            .map_err(|e| HubError::Storage(e.to_string()))?;

        let mut types = Vec::with_capacity(names.len());
        for name in names {
            match self.get_type(&name).await {
                Ok(rt) => types.push(rt),
                Err(e) => debug!(name = %name, error = %e, "Failed to load resource type"),
            }
        }

        // Sort by name for consistent ordering
        types.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(types)
    }

    /// Delete a resource type (fails if builtin or has resources)
    pub async fn delete_type(&self, name: &str) -> HubResult<()> {
        // Check if type exists and is not builtin
        let rt = self.get_type(name).await?;
        if rt.builtin {
            return Err(HubError::CannotDeleteBuiltin(name.to_string()));
        }

        // Check if any resources use this type
        let resources = self.list_resources_by_type(name).await?;
        if !resources.is_empty() {
            return Err(HubError::TypeInUse {
                type_name: name.to_string(),
                resource_count: resources.len(),
            });
        }

        let mut conn = self.redis.clone();

        let key = format!("{}:{}", TYPE_PREFIX, name);
        let index_key = format!("{}:{}", TYPE_PREFIX, INDEX_SUFFIX);

        redis::pipe()
            .del(&key)
            .srem(&index_key, name)
            .query_async::<_, ()>(&mut conn)
            .await
            .map_err(|e| HubError::Storage(e.to_string()))?;

        info!(name = %name, "Deleted resource type");
        Ok(())
    }

    /// Check if a type exists
    pub async fn type_exists(&self, name: &str) -> HubResult<bool> {
        let mut conn = self.redis.clone();

        let key = format!("{}:{}", TYPE_PREFIX, name);
        let exists: bool = conn
            .exists(&key)
            .await
            .map_err(|e| HubError::Storage(e.to_string()))?;

        Ok(exists)
    }

    // ========================================
    // Resources
    // ========================================

    /// Create a new resource
    pub async fn create_resource(&self, request: CreateResourceRequest) -> HubResult<Resource> {
        // Validate type exists
        let resource_type = self.get_type(&request.type_ref).await?;

        // Validate config against schema
        validate_resource_config(&request.config, &resource_type.config_schema)?;

        // Check if resource already exists
        if self.resource_exists(&request.namespace, &request.name).await? {
            return Err(HubError::ResourceExists {
                namespace: request.namespace,
                name: request.name,
            });
        }

        let resource = Resource::new(request.name, request.type_ref, request.config)
            .with_namespace(request.namespace)
            .with_tags(request.tags);

        let resource = if let Some(desc) = request.description {
            resource.with_description(desc)
        } else {
            resource
        };

        self.store_resource(&resource).await?;

        info!(
            namespace = %resource.namespace,
            name = %resource.name,
            type_ref = %resource.type_ref,
            "Created resource"
        );

        Ok(resource)
    }

    /// Store a resource (internal)
    async fn store_resource(&self, resource: &Resource) -> HubResult<()> {
        let mut conn = self.redis.clone();

        let key = format!("{}:{}:{}", RESOURCE_PREFIX, resource.namespace, resource.name);
        let index_key = format!("{}:{}:{}", RESOURCE_PREFIX, resource.namespace, INDEX_SUFFIX);
        let type_index_key = format!("{}:by_type:{}", RESOURCE_PREFIX, resource.type_ref);

        let json = serde_json::to_string(resource)
            .map_err(|e| HubError::Serialization(e.to_string()))?;

        redis::pipe()
            .set(&key, &json)
            .sadd(&index_key, &resource.name)
            .sadd(&type_index_key, resource.path())
            .query_async::<_, ()>(&mut conn)
            .await
            .map_err(|e| HubError::Storage(e.to_string()))?;

        Ok(())
    }

    /// Get a resource by namespace and name
    pub async fn get_resource(&self, namespace: &str, name: &str) -> HubResult<Resource> {
        let mut conn = self.redis.clone();

        let key = format!("{}:{}:{}", RESOURCE_PREFIX, namespace, name);
        let json: Option<String> = conn
            .get(&key)
            .await
            .map_err(|e| HubError::Storage(e.to_string()))?;

        match json {
            Some(data) => {
                let resource: Resource = serde_json::from_str(&data)
                    .map_err(|e| HubError::Serialization(e.to_string()))?;
                Ok(resource)
            }
            None => Err(HubError::ResourceNotFound {
                namespace: namespace.to_string(),
                name: name.to_string(),
            }),
        }
    }

    /// List all resources in a namespace
    pub async fn list_resources(&self, namespace: &str) -> HubResult<Vec<Resource>> {
        let mut conn = self.redis.clone();

        let index_key = format!("{}:{}:{}", RESOURCE_PREFIX, namespace, INDEX_SUFFIX);
        let names: Vec<String> = conn
            .smembers(&index_key)
            .await
            .map_err(|e| HubError::Storage(e.to_string()))?;

        let mut resources = Vec::with_capacity(names.len());
        for name in names {
            match self.get_resource(namespace, &name).await {
                Ok(r) => resources.push(r),
                Err(e) => debug!(namespace = %namespace, name = %name, error = %e, "Failed to load resource"),
            }
        }

        // Sort by name
        resources.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(resources)
    }

    /// List all resources of a specific type (across all namespaces)
    pub async fn list_resources_by_type(&self, type_ref: &str) -> HubResult<Vec<Resource>> {
        let mut conn = self.redis.clone();

        let type_index_key = format!("{}:by_type:{}", RESOURCE_PREFIX, type_ref);
        let paths: Vec<String> = conn
            .smembers(&type_index_key)
            .await
            .map_err(|e| HubError::Storage(e.to_string()))?;

        let mut resources = Vec::with_capacity(paths.len());
        for path in paths {
            // Parse namespace/name from path
            if let Some((namespace, name)) = path.split_once('/') {
                match self.get_resource(namespace, name).await {
                    Ok(r) => resources.push(r),
                    Err(e) => debug!(path = %path, error = %e, "Failed to load resource"),
                }
            }
        }

        resources.sort_by(|a, b| a.path().cmp(&b.path()));
        Ok(resources)
    }

    /// Update a resource
    pub async fn update_resource(
        &self,
        namespace: &str,
        name: &str,
        update: UpdateResourceRequest,
    ) -> HubResult<Resource> {
        let mut resource = self.get_resource(namespace, name).await?;

        // If config is being updated, validate it
        if let Some(ref new_config) = update.config {
            let resource_type = self.get_type(&resource.type_ref).await?;
            validate_resource_config(new_config, &resource_type.config_schema)?;
            resource.config = new_config.clone();
        }

        if let Some(desc) = update.description {
            resource.description = Some(desc);
        }

        if let Some(tags) = update.tags {
            resource.tags = tags;
        }

        resource.updated_at = chrono::Utc::now();

        self.store_resource(&resource).await?;

        info!(
            namespace = %namespace,
            name = %name,
            "Updated resource"
        );

        Ok(resource)
    }

    /// Delete a resource
    pub async fn delete_resource(&self, namespace: &str, name: &str) -> HubResult<()> {
        let resource = self.get_resource(namespace, name).await?;

        let mut conn = self.redis.clone();

        let key = format!("{}:{}:{}", RESOURCE_PREFIX, namespace, name);
        let index_key = format!("{}:{}:{}", RESOURCE_PREFIX, namespace, INDEX_SUFFIX);
        let type_index_key = format!("{}:by_type:{}", RESOURCE_PREFIX, resource.type_ref);

        redis::pipe()
            .del(&key)
            .srem(&index_key, name)
            .srem(&type_index_key, resource.path())
            .query_async::<_, ()>(&mut conn)
            .await
            .map_err(|e| HubError::Storage(e.to_string()))?;

        info!(namespace = %namespace, name = %name, "Deleted resource");
        Ok(())
    }

    /// Check if a resource exists
    pub async fn resource_exists(&self, namespace: &str, name: &str) -> HubResult<bool> {
        let mut conn = self.redis.clone();

        let key = format!("{}:{}:{}", RESOURCE_PREFIX, namespace, name);
        let exists: bool = conn
            .exists(&key)
            .await
            .map_err(|e| HubError::Storage(e.to_string()))?;

        Ok(exists)
    }

    /// List all namespaces that have resources
    pub async fn list_namespaces(&self) -> HubResult<Vec<String>> {
        let mut conn = self.redis.clone();

        // Scan for namespace index keys
        let pattern = format!("{}:*:{}", RESOURCE_PREFIX, INDEX_SUFFIX);
        let keys: Vec<String> = redis::cmd("KEYS")
            .arg(&pattern)
            .query_async(&mut conn)
            .await
            .map_err(|e| HubError::Storage(e.to_string()))?;

        let mut namespaces: Vec<String> = keys
            .into_iter()
            .filter_map(|key| {
                // Extract namespace from "resources:{namespace}:_index"
                let parts: Vec<&str> = key.split(':').collect();
                if parts.len() >= 2 {
                    Some(parts[1].to_string())
                } else {
                    None
                }
            })
            .collect();

        namespaces.sort();
        namespaces.dedup();
        Ok(namespaces)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // Note: These tests require a running Redis/Dragonfly instance
    // Run with: cargo test --features integration-tests

    #[test]
    fn test_key_format() {
        assert_eq!(
            format!("{}:{}", TYPE_PREFIX, "mysql"),
            "hub:types:mysql"
        );
        assert_eq!(
            format!("{}:{}:{}", RESOURCE_PREFIX, "default", "prod-db"),
            "resources:default:prod-db"
        );
    }
}
