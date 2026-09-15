//! Redis-backed WorkflowStore implementation
//!
//! Key layout (matches flowr-registry):
//! - `flow:{ns}:{name}:def:{ver}` — WorkflowRecord JSON
//! - `flow:{ns}:{name}:version`   — active version string
//! - `flow:{ns}:{name}:versions`  — set of all version strings
//! - `flow:{ns}:{name}:enabled`   — "true"/"false"
//! - `flow:index`                  — set of "{ns}/{name}" keys

use async_trait::async_trait;
use redis::AsyncCommands;
use tracing::debug;

use flowr_storage::WorkflowStore;
use flowr_types::{FlowError, FlowResult, WorkflowRecord, WorkflowSummary};

const KEY_PREFIX: &str = "flow";
const INDEX_KEY: &str = "flow:index";

/// Redis-backed workflow store
pub struct RedisWorkflowStore {
    redis: redis::aio::MultiplexedConnection,
}

impl RedisWorkflowStore {
    pub fn new(redis: redis::aio::MultiplexedConnection) -> Self {
        Self { redis }
    }

    fn key(&self, namespace: &str, name: &str, suffix: &str) -> String {
        format!("{KEY_PREFIX}:{namespace}:{name}:{suffix}")
    }

    fn def_key(&self, namespace: &str, name: &str, version: &str) -> String {
        format!("{KEY_PREFIX}:{namespace}:{name}:def:{version}")
    }
}

#[async_trait]
impl WorkflowStore for RedisWorkflowStore {
    async fn register(&self, record: WorkflowRecord) -> FlowResult<()> {
        let namespace = record.namespace().to_string();
        let name = record.name().to_string();
        let version = record.version().to_string();

        let def_json = serde_json::to_string(&record)
            .map_err(|e| FlowError::Serialization(e.to_string()))?;

        let mut conn = self.redis.clone();

        // Store definition at versioned key
        let def_key = self.def_key(&namespace, &name, &version);
        conn.set::<_, _, ()>(&def_key, &def_json)
            .await
            .map_err(|e| FlowError::Storage(e.to_string()))?;

        // Add version to versions set
        let versions_key = self.key(&namespace, &name, "versions");
        conn.sadd::<_, _, ()>(&versions_key, &version)
            .await
            .map_err(|e| FlowError::Storage(e.to_string()))?;

        // Set active version if first registration
        let version_key = self.key(&namespace, &name, "version");
        let current: Option<String> = conn
            .get(&version_key)
            .await
            .map_err(|e| FlowError::Storage(e.to_string()))?;
        if current.is_none() {
            conn.set::<_, _, ()>(&version_key, &version)
                .await
                .map_err(|e| FlowError::Storage(e.to_string()))?;
        }

        // Set enabled status
        let enabled_key = self.key(&namespace, &name, "enabled");
        let enabled_str = if record.metadata.enabled { "true" } else { "false" };
        conn.set::<_, _, ()>(&enabled_key, enabled_str)
            .await
            .map_err(|e| FlowError::Storage(e.to_string()))?;

        // Add to global index
        let workflow_key = format!("{namespace}/{name}");
        conn.sadd::<_, _, ()>(INDEX_KEY, &workflow_key)
            .await
            .map_err(|e| FlowError::Storage(e.to_string()))?;

        debug!(namespace, name, version, "workflow registered");
        Ok(())
    }

    async fn get(&self, namespace: &str, name: &str) -> FlowResult<WorkflowRecord> {
        let mut conn = self.redis.clone();

        // Get active version
        let version_key = self.key(namespace, name, "version");
        let version: Option<String> = conn
            .get(&version_key)
            .await
            .map_err(|e| FlowError::Storage(e.to_string()))?;

        let version = version.ok_or_else(|| FlowError::WorkflowNotFound {
            namespace: namespace.to_string(),
            name: name.to_string(),
        })?;

        self.get_version(namespace, name, &version).await
    }

    async fn get_version(
        &self,
        namespace: &str,
        name: &str,
        version: &str,
    ) -> FlowResult<WorkflowRecord> {
        let mut conn = self.redis.clone();

        let def_key = self.def_key(namespace, name, version);
        let json: Option<String> = conn
            .get(&def_key)
            .await
            .map_err(|e| FlowError::Storage(e.to_string()))?;

        let json = json.ok_or_else(|| FlowError::NotFound(format!(
            "workflow version {namespace}/{name}:{version} not found"
        )))?;

        serde_json::from_str(&json).map_err(|e| FlowError::Serialization(e.to_string()))
    }

    async fn list(&self, namespace: &str) -> FlowResult<Vec<WorkflowSummary>> {
        let mut conn = self.redis.clone();

        // Get all workflow keys from global index
        let keys: Vec<String> = conn
            .smembers(INDEX_KEY)
            .await
            .map_err(|e| FlowError::Storage(e.to_string()))?;

        let mut summaries = Vec::new();

        for key in keys {
            let parts: Vec<&str> = key.split('/').collect();
            if parts.len() != 2 {
                continue;
            }

            let ns = parts[0];
            let name = parts[1];

            // Filter by namespace
            if ns != namespace {
                continue;
            }

            match self.build_summary(ns, name).await {
                Ok(summary) => summaries.push(summary),
                Err(e) => {
                    debug!(namespace = ns, name, error = %e, "failed to build workflow summary");
                }
            }
        }

        Ok(summaries)
    }

    async fn delete(&self, namespace: &str, name: &str) -> FlowResult<()> {
        let mut conn = self.redis.clone();

        // Get all versions
        let versions_key = self.key(namespace, name, "versions");
        let versions: Vec<String> = conn
            .smembers(&versions_key)
            .await
            .map_err(|e| FlowError::Storage(e.to_string()))?;

        // Delete all version definitions
        for version in &versions {
            let def_key = self.def_key(namespace, name, version);
            conn.del::<_, ()>(&def_key)
                .await
                .map_err(|e| FlowError::Storage(e.to_string()))?;
        }

        // Delete metadata keys
        conn.del::<_, ()>(&versions_key)
            .await
            .map_err(|e| FlowError::Storage(e.to_string()))?;
        conn.del::<_, ()>(&self.key(namespace, name, "version"))
            .await
            .map_err(|e| FlowError::Storage(e.to_string()))?;
        conn.del::<_, ()>(&self.key(namespace, name, "enabled"))
            .await
            .map_err(|e| FlowError::Storage(e.to_string()))?;

        // Remove from index
        let workflow_key = format!("{namespace}/{name}");
        conn.srem::<_, _, ()>(INDEX_KEY, &workflow_key)
            .await
            .map_err(|e| FlowError::Storage(e.to_string()))?;

        debug!(namespace, name, "workflow deleted");
        Ok(())
    }

    async fn set_enabled(&self, namespace: &str, name: &str, enabled: bool) -> FlowResult<()> {
        let mut conn = self.redis.clone();

        // Verify workflow exists
        let version_key = self.key(namespace, name, "version");
        let exists: bool = conn
            .exists(&version_key)
            .await
            .map_err(|e| FlowError::Storage(e.to_string()))?;
        if !exists {
            return Err(FlowError::WorkflowNotFound {
                namespace: namespace.to_string(),
                name: name.to_string(),
            });
        }

        let enabled_key = self.key(namespace, name, "enabled");
        let enabled_str = if enabled { "true" } else { "false" };
        conn.set::<_, _, ()>(&enabled_key, enabled_str)
            .await
            .map_err(|e| FlowError::Storage(e.to_string()))?;

        Ok(())
    }

    async fn set_active_version(
        &self,
        namespace: &str,
        name: &str,
        version: &str,
    ) -> FlowResult<()> {
        let mut conn = self.redis.clone();

        // Verify the version exists
        let def_key = self.def_key(namespace, name, version);
        let exists: bool = conn
            .exists(&def_key)
            .await
            .map_err(|e| FlowError::Storage(e.to_string()))?;
        if !exists {
            return Err(FlowError::NotFound(format!(
                "workflow version {namespace}/{name}:{version} not found"
            )));
        }

        let version_key = self.key(namespace, name, "version");
        conn.set::<_, _, ()>(&version_key, version)
            .await
            .map_err(|e| FlowError::Storage(e.to_string()))?;

        Ok(())
    }

    async fn exists(&self, namespace: &str, name: &str) -> FlowResult<bool> {
        let mut conn = self.redis.clone();

        let version_key = self.key(namespace, name, "version");
        let exists: bool = conn
            .exists(&version_key)
            .await
            .map_err(|e| FlowError::Storage(e.to_string()))?;

        Ok(exists)
    }
}

impl RedisWorkflowStore {
    async fn build_summary(&self, namespace: &str, name: &str) -> FlowResult<WorkflowSummary> {
        let mut conn = self.redis.clone();

        // Get active version
        let version_key = self.key(namespace, name, "version");
        let version: Option<String> = conn
            .get(&version_key)
            .await
            .map_err(|e| FlowError::Storage(e.to_string()))?;
        let version = version.ok_or_else(|| FlowError::WorkflowNotFound {
            namespace: namespace.to_string(),
            name: name.to_string(),
        })?;

        // Get enabled status
        let enabled_key = self.key(namespace, name, "enabled");
        let enabled_str: Option<String> = conn
            .get(&enabled_key)
            .await
            .map_err(|e| FlowError::Storage(e.to_string()))?;
        let enabled = enabled_str.map(|s| s == "true").unwrap_or(true);

        // Get record for step count
        let record = self.get_version(namespace, name, &version).await?;
        let step_count = record
            .parse()
            .map(|swf| swf.tasks.len())
            .unwrap_or(0);

        Ok(WorkflowSummary {
            namespace: namespace.to_string(),
            name: name.to_string(),
            version,
            enabled,
            step_count,
        })
    }
}
