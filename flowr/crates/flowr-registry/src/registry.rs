//! Flow registry with Dragonfly backend

use redis::AsyncCommands;
use tracing::{debug, info, instrument};

use crate::definition::{WorkflowDefinition, WorkflowSummary};
use crate::error::{RegistryError, RegistryResult};
use flowr_types::WorkflowRecord;

/// Key prefixes for Dragonfly storage
const KEY_PREFIX: &str = "flow";
const ENABLED_SUFFIX: &str = "enabled";
const VERSION_SUFFIX: &str = "version";
const DEF_SUFFIX: &str = "def";
const VERSIONS_SUFFIX: &str = "versions";
const INDEX_KEY: &str = "flow:index";

/// Flow registry backed by Dragonfly
#[derive(Clone)]
pub struct FlowRegistry {
    /// Redis/Dragonfly connection
    redis: redis::aio::MultiplexedConnection,
}

impl FlowRegistry {
    /// Create a new registry with a Redis connection
    pub fn new(redis: redis::aio::MultiplexedConnection) -> Self {
        Self { redis }
    }

    /// Build a key for a workflow
    fn key(&self, namespace: &str, name: &str, suffix: &str) -> String {
        format!("{}:{}:{}:{}", KEY_PREFIX, namespace, name, suffix)
    }

    /// Build a versioned definition key
    fn def_key(&self, namespace: &str, name: &str, version: &str) -> String {
        format!("{}:{}:{}:{}:{}", KEY_PREFIX, namespace, name, DEF_SUFFIX, version)
    }

    /// Register a new workflow definition
    #[instrument(skip(self, definition), fields(namespace = %definition.metadata.namespace, name = %definition.metadata.name, version = %definition.metadata.version))]
    pub async fn register(&self, definition: WorkflowDefinition) -> RegistryResult<()> {
        let namespace = &definition.metadata.namespace;
        let name = &definition.metadata.name;
        let version = &definition.metadata.version;

        info!("registering workflow");

        // Serialize definition
        let def_json = serde_json::to_string(&definition)?;

        // Store definition
        let def_key = self.def_key(namespace, name, version);
        let mut redis = self.redis.clone();
        redis.set::<_, _, ()>(&def_key, &def_json).await?;

        // Add version to versions set
        let versions_key = self.key(namespace, name, VERSIONS_SUFFIX);
        redis.sadd::<_, _, ()>(&versions_key, version).await?;

        // Set as active version if first or explicitly set
        let version_key = self.key(namespace, name, VERSION_SUFFIX);
        let current_version: Option<String> = redis.get(&version_key).await?;
        if current_version.is_none() {
            redis.set::<_, _, ()>(&version_key, version).await?;
        }

        // Set enabled status
        let enabled_key = self.key(namespace, name, ENABLED_SUFFIX);
        let enabled_str = if definition.metadata.enabled { "true" } else { "false" };
        redis.set::<_, _, ()>(&enabled_key, enabled_str).await?;

        // Add to index
        let workflow_key = format!("{}/{}", namespace, name);
        redis.sadd::<_, _, ()>(INDEX_KEY, &workflow_key).await?;

        debug!("workflow registered successfully");
        Ok(())
    }

    /// Get a workflow definition by namespace and name (active version)
    #[instrument(skip(self))]
    pub async fn get(&self, namespace: &str, name: &str) -> RegistryResult<WorkflowDefinition> {
        let mut redis = self.redis.clone();

        // Get active version
        let version_key = self.key(namespace, name, VERSION_SUFFIX);
        let version: Option<String> = redis.get(&version_key).await?;

        let version = version.ok_or_else(|| RegistryError::NotFound {
            namespace: namespace.to_string(),
            name: name.to_string(),
        })?;

        self.get_version(namespace, name, &version).await
    }

    /// Get a specific version of a workflow
    #[instrument(skip(self))]
    pub async fn get_version(
        &self,
        namespace: &str,
        name: &str,
        version: &str,
    ) -> RegistryResult<WorkflowDefinition> {
        let def_key = self.def_key(namespace, name, version);
        let mut redis = self.redis.clone();

        let def_json: Option<String> = redis.get(&def_key).await?;

        let def_json = def_json.ok_or_else(|| RegistryError::VersionNotFound {
            namespace: namespace.to_string(),
            name: name.to_string(),
            version: version.to_string(),
        })?;

        let definition: WorkflowDefinition = serde_json::from_str(&def_json)?;
        Ok(definition)
    }

    /// List all workflows
    #[instrument(skip(self))]
    pub async fn list(&self) -> RegistryResult<Vec<WorkflowSummary>> {
        let mut redis = self.redis.clone();

        // Get all workflow keys from index
        let keys: Vec<String> = redis.smembers(INDEX_KEY).await?;

        let mut summaries = Vec::with_capacity(keys.len());

        for key in keys {
            let parts: Vec<&str> = key.split('/').collect();
            if parts.len() != 2 {
                continue;
            }

            let namespace = parts[0];
            let name = parts[1];

            match self.get_summary(namespace, name).await {
                Ok(summary) => summaries.push(summary),
                Err(e) => {
                    debug!(namespace, name, error = %e, "failed to get workflow summary");
                }
            }
        }

        Ok(summaries)
    }

    /// Get workflow summary
    pub async fn get_summary(&self, namespace: &str, name: &str) -> RegistryResult<WorkflowSummary> {
        let mut redis = self.redis.clone();

        // Get active version
        let version_key = self.key(namespace, name, VERSION_SUFFIX);
        let version: Option<String> = redis.get(&version_key).await?;
        let version = version.ok_or_else(|| RegistryError::NotFound {
            namespace: namespace.to_string(),
            name: name.to_string(),
        })?;

        // Get enabled status
        let enabled_key = self.key(namespace, name, ENABLED_SUFFIX);
        let enabled_str: Option<String> = redis.get(&enabled_key).await?;
        let enabled = enabled_str.map(|s| s == "true").unwrap_or(true);

        // Get all versions
        let versions_key = self.key(namespace, name, VERSIONS_SUFFIX);
        let versions: Vec<String> = redis.smembers(&versions_key).await?;

        // Get definition for step count and title
        let def = self.get_version(namespace, name, &version).await?;

        Ok(WorkflowSummary {
            namespace: namespace.to_string(),
            name: name.to_string(),
            version,
            title: def.metadata.title,
            enabled,
            step_count: def.steps.len(),
            versions,
            updated_at: def.metadata.updated_at,
        })
    }

    /// Set the active version
    #[instrument(skip(self))]
    pub async fn set_active_version(
        &self,
        namespace: &str,
        name: &str,
        version: &str,
    ) -> RegistryResult<()> {
        // Verify version exists
        let def_key = self.def_key(namespace, name, version);
        let mut redis = self.redis.clone();

        let exists: bool = redis.exists(&def_key).await?;
        if !exists {
            return Err(RegistryError::VersionNotFound {
                namespace: namespace.to_string(),
                name: name.to_string(),
                version: version.to_string(),
            });
        }

        // Set active version
        let version_key = self.key(namespace, name, VERSION_SUFFIX);
        redis.set::<_, _, ()>(&version_key, version).await?;

        info!(namespace, name, version, "active version updated");
        Ok(())
    }

    /// Enable or disable a workflow
    #[instrument(skip(self))]
    pub async fn set_enabled(&self, namespace: &str, name: &str, enabled: bool) -> RegistryResult<()> {
        let enabled_key = self.key(namespace, name, ENABLED_SUFFIX);
        let mut redis = self.redis.clone();

        // Verify workflow exists
        let version_key = self.key(namespace, name, VERSION_SUFFIX);
        let exists: bool = redis.exists(&version_key).await?;
        if !exists {
            return Err(RegistryError::NotFound {
                namespace: namespace.to_string(),
                name: name.to_string(),
            });
        }

        let enabled_str = if enabled { "true" } else { "false" };
        redis.set::<_, _, ()>(&enabled_key, enabled_str).await?;

        info!(namespace, name, enabled, "workflow enabled status updated");
        Ok(())
    }

    /// Check if a workflow is enabled
    pub async fn is_enabled(&self, namespace: &str, name: &str) -> RegistryResult<bool> {
        let enabled_key = self.key(namespace, name, ENABLED_SUFFIX);
        let mut redis = self.redis.clone();

        let enabled_str: Option<String> = redis.get(&enabled_key).await?;
        Ok(enabled_str.map(|s| s == "true").unwrap_or(true))
    }

    /// Delete a specific version
    #[instrument(skip(self))]
    pub async fn delete_version(
        &self,
        namespace: &str,
        name: &str,
        version: &str,
    ) -> RegistryResult<()> {
        let mut redis = self.redis.clone();

        // Delete definition
        let def_key = self.def_key(namespace, name, version);
        redis.del::<_, ()>(&def_key).await?;

        // Remove from versions set
        let versions_key = self.key(namespace, name, VERSIONS_SUFFIX);
        redis.srem::<_, _, ()>(&versions_key, version).await?;

        // Check if this was the active version
        let version_key = self.key(namespace, name, VERSION_SUFFIX);
        let active_version: Option<String> = redis.get(&version_key).await?;

        if active_version.as_deref() == Some(version) {
            // Find another version to activate, or remove entirely
            let remaining: Vec<String> = redis.smembers(&versions_key).await?;
            if let Some(new_active) = remaining.first() {
                redis.set::<_, _, ()>(&version_key, new_active).await?;
            } else {
                // No more versions, clean up entirely
                self.delete(namespace, name).await?;
            }
        }

        info!(namespace, name, version, "version deleted");
        Ok(())
    }

    /// Delete a workflow entirely
    #[instrument(skip(self))]
    pub async fn delete(&self, namespace: &str, name: &str) -> RegistryResult<()> {
        let mut redis = self.redis.clone();

        // Get all versions
        let versions_key = self.key(namespace, name, VERSIONS_SUFFIX);
        let versions: Vec<String> = redis.smembers(&versions_key).await?;

        // Delete all version definitions
        for version in &versions {
            let def_key = self.def_key(namespace, name, version);
            redis.del::<_, ()>(&def_key).await?;
        }

        // Delete metadata keys
        redis.del::<_, ()>(&versions_key).await?;
        redis.del::<_, ()>(&self.key(namespace, name, VERSION_SUFFIX)).await?;
        redis.del::<_, ()>(&self.key(namespace, name, ENABLED_SUFFIX)).await?;

        // Remove from index
        let workflow_key = format!("{}/{}", namespace, name);
        redis.srem::<_, _, ()>(INDEX_KEY, &workflow_key).await?;

        info!(namespace, name, "workflow deleted");
        Ok(())
    }

    /// Check if a workflow exists
    pub async fn exists(&self, namespace: &str, name: &str) -> RegistryResult<bool> {
        let version_key = self.key(namespace, name, VERSION_SUFFIX);
        let mut redis = self.redis.clone();
        let exists: bool = redis.exists(&version_key).await?;
        Ok(exists)
    }

    // ─── CNCF WorkflowRecord methods ────────────────────────────

    /// Register a CNCF workflow from a WorkflowRecord (raw YAML as source of truth)
    #[instrument(skip(self, record), fields(namespace = %record.namespace(), name = %record.name(), version = %record.version()))]
    pub async fn register_record(&self, record: WorkflowRecord) -> RegistryResult<()> {
        let namespace = record.namespace().to_string();
        let name = record.name().to_string();
        let version = record.version().to_string();

        info!("registering CNCF workflow");

        let def_json = serde_json::to_string(&record)?;

        let def_key = self.def_key(&namespace, &name, &version);
        let mut redis = self.redis.clone();
        redis.set::<_, _, ()>(&def_key, &def_json).await?;

        let versions_key = self.key(&namespace, &name, VERSIONS_SUFFIX);
        redis.sadd::<_, _, ()>(&versions_key, &version).await?;

        let version_key = self.key(&namespace, &name, VERSION_SUFFIX);
        let current_version: Option<String> = redis.get(&version_key).await?;
        if current_version.is_none() {
            redis.set::<_, _, ()>(&version_key, &version).await?;
        }

        let enabled_key = self.key(&namespace, &name, ENABLED_SUFFIX);
        let enabled_str = if record.metadata.enabled { "true" } else { "false" };
        redis.set::<_, _, ()>(&enabled_key, enabled_str).await?;

        let workflow_key = format!("{}/{}", namespace, name);
        redis.sadd::<_, _, ()>(INDEX_KEY, &workflow_key).await?;

        debug!("CNCF workflow registered successfully");
        Ok(())
    }

    /// Get a CNCF WorkflowRecord (tries WorkflowRecord first, falls back to legacy)
    pub async fn get_record(&self, namespace: &str, name: &str) -> RegistryResult<WorkflowRecord> {
        let mut redis = self.redis.clone();

        let version_key = self.key(namespace, name, VERSION_SUFFIX);
        let version: Option<String> = redis.get(&version_key).await?;
        let version = version.ok_or_else(|| RegistryError::NotFound {
            namespace: namespace.to_string(),
            name: name.to_string(),
        })?;

        self.get_record_version(namespace, name, &version).await
    }

    /// Get a specific version as WorkflowRecord
    pub async fn get_record_version(
        &self,
        namespace: &str,
        name: &str,
        version: &str,
    ) -> RegistryResult<WorkflowRecord> {
        let def_key = self.def_key(namespace, name, version);
        let mut redis = self.redis.clone();

        let def_json: Option<String> = redis.get(&def_key).await?;
        let def_json = def_json.ok_or_else(|| RegistryError::VersionNotFound {
            namespace: namespace.to_string(),
            name: name.to_string(),
            version: version.to_string(),
        })?;

        // Try as WorkflowRecord first (CNCF format)
        if let Ok(record) = serde_json::from_str::<WorkflowRecord>(&def_json) {
            return Ok(record);
        }

        // Fall back: legacy WorkflowDefinition — wrap as WorkflowRecord
        let legacy: WorkflowDefinition = serde_json::from_str(&def_json)?;
        let raw_yaml = legacy.raw.clone().unwrap_or_else(|| {
            // Synthesize minimal CNCF YAML from legacy definition
            format!(
                "document:\n  dsl: '1.0.3'\n  namespace: {}\n  name: {}\n  version: {}\ndo:\n  - placeholder:\n      set:\n        _migrated: true\n",
                legacy.metadata.namespace, legacy.metadata.name, legacy.metadata.version
            )
        });

        Ok(WorkflowRecord {
            metadata: flowr_types::WorkflowMetadata {
                dsl: Some("1.0.3".to_string()),
                namespace: legacy.metadata.namespace,
                name: legacy.metadata.name,
                version: legacy.metadata.version,
                title: legacy.metadata.title,
                description: legacy.metadata.description,
                tags: legacy.metadata.tags,
                metadata: legacy.metadata.metadata,
                enabled: legacy.metadata.enabled,
                created_at: legacy.metadata.created_at,
                updated_at: legacy.metadata.updated_at,
            },
            raw_yaml,
            durability: flowr_types::FlowDurability::default(),
        })
    }
}
