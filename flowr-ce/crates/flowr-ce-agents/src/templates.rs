//! Agent Template Registry — versioned CNCF Workflow blueprints
//!
//! Templates are reusable workflow definitions that can be instantiated as agents.
//! Multiple versions can coexist; one version is "active" per template name.

use std::collections::HashMap;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::RwLock;
use tracing::{info, warn};

/// A versioned agent template
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentTemplate {
    /// Template name (e.g., "rtb-bidder")
    pub name: String,
    /// Version string (e.g., "2.1.0")
    pub version: String,
    /// Namespace
    #[serde(default = "default_ns")]
    pub namespace: String,
    /// Human-readable description
    #[serde(default)]
    pub description: Option<String>,
    /// Tags for categorization
    #[serde(default)]
    pub tags: Vec<String>,
    /// Whether this version is the active one
    #[serde(default)]
    pub active: bool,
    /// The workflow definition (CNCF Serverless Workflow)
    pub spec: Value,
    /// Default agent resource config (merged with instance config)
    #[serde(default)]
    pub defaults: Option<Value>,
    /// Created timestamp
    #[serde(default)]
    pub created_at: Option<String>,
}

fn default_ns() -> String {
    "default".to_string()
}

/// Template summary (for list operations)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemplateSummary {
    pub name: String,
    pub namespace: String,
    pub active_version: Option<String>,
    pub versions: Vec<String>,
    pub description: Option<String>,
    pub tags: Vec<String>,
}

/// Template manager — stores and retrieves templates
pub struct TemplateManager {
    /// Templates keyed by "namespace/name:version"
    templates: RwLock<HashMap<String, AgentTemplate>>,
}

impl TemplateManager {
    pub fn new() -> Self {
        Self {
            templates: RwLock::new(HashMap::new()),
        }
    }

    fn key(namespace: &str, name: &str, version: &str) -> String {
        format!("{}/{}:{}", namespace, name, version)
    }

    fn name_prefix(namespace: &str, name: &str) -> String {
        format!("{}/{}:", namespace, name)
    }

    /// Create or update a template version
    pub async fn create(&self, mut template: AgentTemplate) -> anyhow::Result<String> {
        let key = Self::key(&template.namespace, &template.name, &template.version);

        if template.created_at.is_none() {
            template.created_at = Some(chrono::Utc::now().to_rfc3339());
        }

        let mut templates = self.templates.write().await;

        // If this is the first version, make it active
        let prefix = Self::name_prefix(&template.namespace, &template.name);
        let has_existing = templates.keys().any(|k| k.starts_with(&prefix));
        if !has_existing {
            template.active = true;
        }

        let version = template.version.clone();
        info!(
            name = %template.name,
            version = %version,
            active = template.active,
            "Template created"
        );

        templates.insert(key, template);
        Ok(version)
    }

    /// Get a specific template version
    pub async fn get(&self, namespace: &str, name: &str, version: &str) -> Option<AgentTemplate> {
        let key = Self::key(namespace, name, version);
        self.templates.read().await.get(&key).cloned()
    }

    /// Get the active version of a template
    pub async fn get_active(&self, namespace: &str, name: &str) -> Option<AgentTemplate> {
        let prefix = Self::name_prefix(namespace, name);
        let templates = self.templates.read().await;

        templates.values()
            .find(|t| t.namespace == namespace && t.name == name && t.active)
            .cloned()
            .or_else(|| {
                // Fallback: return the latest version
                templates.iter()
                    .filter(|(k, _)| k.starts_with(&prefix))
                    .map(|(_, v)| v)
                    .max_by(|a, b| a.version.cmp(&b.version))
                    .cloned()
            })
    }

    /// Get a template by "name:version" or just "name" (active version)
    pub async fn resolve(&self, namespace: &str, template_ref: &str) -> Option<AgentTemplate> {
        if let Some((name, version)) = template_ref.split_once(':') {
            self.get(namespace, name, version).await
        } else {
            self.get_active(namespace, template_ref).await
        }
    }

    /// List all templates (grouped by name)
    pub async fn list(&self, namespace: Option<&str>) -> Vec<TemplateSummary> {
        let templates = self.templates.read().await;
        let mut groups: HashMap<String, Vec<&AgentTemplate>> = HashMap::new();

        for t in templates.values() {
            if let Some(ns) = namespace {
                if t.namespace != ns {
                    continue;
                }
            }
            let group_key = format!("{}/{}", t.namespace, t.name);
            groups.entry(group_key).or_default().push(t);
        }

        groups.into_iter().map(|(_, versions)| {
            let first = &versions[0];
            let active = versions.iter().find(|v| v.active);
            TemplateSummary {
                name: first.name.clone(),
                namespace: first.namespace.clone(),
                active_version: active.map(|a| a.version.clone()),
                versions: versions.iter().map(|v| v.version.clone()).collect(),
                description: first.description.clone(),
                tags: first.tags.clone(),
            }
        }).collect()
    }

    /// Set the active version for a template
    pub async fn activate(&self, namespace: &str, name: &str, version: &str) -> anyhow::Result<()> {
        let mut templates = self.templates.write().await;
        let prefix = Self::name_prefix(namespace, name);

        // Deactivate all versions
        for (k, t) in templates.iter_mut() {
            if k.starts_with(&prefix) {
                t.active = false;
            }
        }

        // Activate the requested version
        let key = Self::key(namespace, name, version);
        match templates.get_mut(&key) {
            Some(t) => {
                t.active = true;
                info!(name = %name, version = %version, "Template version activated");
                Ok(())
            }
            None => anyhow::bail!("Template version '{}:{}' not found", name, version),
        }
    }

    /// Delete a specific template version
    pub async fn delete_version(&self, namespace: &str, name: &str, version: &str) -> anyhow::Result<()> {
        let key = Self::key(namespace, name, version);
        let mut templates = self.templates.write().await;

        if templates.remove(&key).is_some() {
            info!(name = %name, version = %version, "Template version deleted");
            Ok(())
        } else {
            anyhow::bail!("Template '{}:{}' not found", name, version)
        }
    }

    /// Delete all versions of a template
    pub async fn delete_all(&self, namespace: &str, name: &str) -> anyhow::Result<usize> {
        let prefix = Self::name_prefix(namespace, name);
        let mut templates = self.templates.write().await;
        let keys: Vec<String> = templates.keys()
            .filter(|k| k.starts_with(&prefix))
            .cloned()
            .collect();

        let count = keys.len();
        for key in keys {
            templates.remove(&key);
        }

        if count > 0 {
            info!(name = %name, versions = count, "Template deleted");
            Ok(count)
        } else {
            anyhow::bail!("Template '{}' not found", name)
        }
    }
}

// Global singleton
static TEMPLATE_MANAGER: std::sync::OnceLock<Arc<TemplateManager>> = std::sync::OnceLock::new();

pub fn template_manager() -> &'static Arc<TemplateManager> {
    TEMPLATE_MANAGER.get_or_init(|| Arc::new(TemplateManager::new()))
}
