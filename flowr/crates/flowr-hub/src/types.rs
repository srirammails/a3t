//! Hub Types - Resource type definitions and instances
//!
//! # Overview
//!
//! - `ResourceType`: Template defining what a resource needs (e.g., mysql, qdrant)
//! - `Resource`: Configured instance of a type with actual connection details

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// How the resource manages connections
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionKind {
    /// Connection pool (databases, redis)
    #[default]
    Pool,
    /// On-demand connections (HTTP APIs)
    OnDemand,
    /// Persistent connection (websockets, streams)
    Persistent,
}

/// Category of resource for UI organization
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum ResourceCategory {
    /// SQL databases (mysql, postgres, sqlite)
    Database,
    /// Key-value stores (redis, dragonfly)
    KeyValue,
    /// Vector databases (qdrant, pinecone, milvus)
    Vector,
    /// Message queues (kafka, rabbitmq, iggy)
    Queue,
    /// HTTP/REST APIs
    #[default]
    Api,
    /// Object storage (s3, gcs, minio)
    Storage,
    /// LLM providers (Anthropic, OpenAI, Ollama)
    Ai,
    /// AI Agents (LLM + Memory + Tools + I/O)
    Agent,
    /// MCP (Model Context Protocol) servers
    Mcp,
    /// Agent Database (ADB) memory backends
    Memory,
    /// Input/Output channels (streams, webhooks)
    Channel,
    /// Custom/other
    Custom,
}

/// Resource Type - Template in the Hub
///
/// Defines what configuration a resource of this type requires.
/// Think of it as a "class" that resources instantiate.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceType {
    /// Unique identifier (e.g., "mysql", "qdrant", "rest_api")
    pub name: String,

    /// Human-readable description
    pub description: String,

    /// Category for organization
    #[serde(default)]
    pub category: ResourceCategory,

    /// JSON Schema defining required configuration
    /// Example for mysql: { "type": "object", "properties": { "host": {...}, "port": {...} } }
    pub config_schema: Value,

    /// How connections are managed
    #[serde(default)]
    pub connection_kind: ConnectionKind,

    /// Icon identifier for UI (e.g., "database", "api", "vector")
    #[serde(default)]
    pub icon: Option<String>,

    /// Whether this is a built-in type (cannot be deleted)
    #[serde(default)]
    pub builtin: bool,

    /// Version of the type schema
    #[serde(default = "default_version")]
    pub version: String,
}

fn default_version() -> String {
    "1.0".to_string()
}

impl ResourceType {
    /// Create a new resource type
    pub fn new(name: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
            category: ResourceCategory::default(),
            config_schema: serde_json::json!({
                "type": "object",
                "properties": {},
                "required": []
            }),
            connection_kind: ConnectionKind::default(),
            icon: None,
            builtin: false,
            version: default_version(),
        }
    }

    /// Set the category
    pub fn with_category(mut self, category: ResourceCategory) -> Self {
        self.category = category;
        self
    }

    /// Set the config schema
    pub fn with_schema(mut self, schema: Value) -> Self {
        self.config_schema = schema;
        self
    }

    /// Set connection kind
    pub fn with_connection_kind(mut self, kind: ConnectionKind) -> Self {
        self.connection_kind = kind;
        self
    }

    /// Set icon
    pub fn with_icon(mut self, icon: impl Into<String>) -> Self {
        self.icon = Some(icon.into());
        self
    }

    /// Mark as builtin
    pub fn as_builtin(mut self) -> Self {
        self.builtin = true;
        self
    }
}

/// Resource - Configured instance of a ResourceType
///
/// Contains actual connection details for a specific service.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Resource {
    /// Unique name within namespace (e.g., "prod-mysql", "stripe-api")
    pub name: String,

    /// Namespace for organization (e.g., "default", "production")
    #[serde(default = "default_namespace")]
    pub namespace: String,

    /// Reference to the resource type (e.g., "mysql", "rest_api")
    pub type_ref: String,

    /// Actual configuration values
    /// May contain secret references like "${secrets.db_password}"
    pub config: Value,

    /// Optional description
    #[serde(default)]
    pub description: Option<String>,

    /// Tags for filtering
    #[serde(default)]
    pub tags: Vec<String>,

    /// Creation timestamp
    pub created_at: DateTime<Utc>,

    /// Last update timestamp
    pub updated_at: DateTime<Utc>,

    /// Created by (user or system)
    #[serde(default)]
    pub created_by: Option<String>,
}

fn default_namespace() -> String {
    "default".to_string()
}

impl Resource {
    /// Create a new resource
    pub fn new(
        name: impl Into<String>,
        type_ref: impl Into<String>,
        config: Value,
    ) -> Self {
        let now = Utc::now();
        Self {
            name: name.into(),
            namespace: default_namespace(),
            type_ref: type_ref.into(),
            config,
            description: None,
            tags: Vec::new(),
            created_at: now,
            updated_at: now,
            created_by: None,
        }
    }

    /// Set namespace
    pub fn with_namespace(mut self, namespace: impl Into<String>) -> Self {
        self.namespace = namespace.into();
        self
    }

    /// Set description
    pub fn with_description(mut self, desc: impl Into<String>) -> Self {
        self.description = Some(desc.into());
        self
    }

    /// Add tags
    pub fn with_tags(mut self, tags: Vec<String>) -> Self {
        self.tags = tags;
        self
    }

    /// Set created_by
    pub fn with_created_by(mut self, user: impl Into<String>) -> Self {
        self.created_by = Some(user.into());
        self
    }

    /// Get the full resource path (namespace/name)
    pub fn path(&self) -> String {
        format!("{}/{}", self.namespace, self.name)
    }
}

/// Request to create a new resource
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateResourceRequest {
    pub name: String,
    #[serde(default = "default_namespace")]
    pub namespace: String,
    pub type_ref: String,
    pub config: Value,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
}

/// Request to update a resource
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateResourceRequest {
    #[serde(default)]
    pub config: Option<Value>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub tags: Option<Vec<String>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resource_type_builder() {
        let rt = ResourceType::new("mysql", "MySQL Database")
            .with_category(ResourceCategory::Database)
            .with_connection_kind(ConnectionKind::Pool)
            .with_icon("database")
            .as_builtin();

        assert_eq!(rt.name, "mysql");
        assert_eq!(rt.category, ResourceCategory::Database);
        assert!(rt.builtin);
    }

    #[test]
    fn test_resource_path() {
        let r = Resource::new("prod-db", "mysql", serde_json::json!({}))
            .with_namespace("production");

        assert_eq!(r.path(), "production/prod-db");
    }
}
