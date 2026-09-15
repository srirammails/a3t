//! Workflow definition types
//!
//! Re-exports CNCF SWF types from `swf-parser` and adds FlowR-specific metadata.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

// ═══════════════════════════════════════════════════
// Re-export CNCF SWF types as canonical workflow types
// ═══════════════════════════════════════════════════

pub use swf_parser::WorkflowDefinition as SwfWorkflowDefinition;
pub use swf_parser::Document as SwfDocument;
pub use swf_parser::{NamedTask, Task};
pub use swf_parser::call::{CallTask, CallTarget};
pub use swf_parser::run::RunTask;
pub use swf_parser::extension::Extension;
pub use swf_parser::use_block::UseBlock as SwfUseBlock;
pub use swf_parser::{parse_yaml, parse_json};

// ═══════════════════════════════════════════════════
// FlowR-specific types (extend CNCF with runtime metadata)
// ═══════════════════════════════════════════════════

/// Workflow durability mode (FlowR extension)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum FlowDurability {
    #[default]
    Completion,
    Checkpoint,
    Full,
}

/// Stored workflow — CNCF definition + FlowR metadata
///
/// This is what gets stored in LanceDB and returned by WorkflowStore.
/// The `raw_yaml` is the source of truth — parsed into `SwfWorkflowDefinition` on demand.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowRecord {
    /// Workflow metadata (FlowR-specific: enabled, timestamps, tags)
    pub metadata: WorkflowMetadata,

    /// Raw CNCF YAML — the source of truth
    pub raw_yaml: String,

    /// Durability mode (FlowR extension)
    #[serde(default)]
    pub durability: FlowDurability,
}

impl WorkflowRecord {
    /// Create from raw CNCF YAML, recording who is creating it.
    ///
    /// Separate from [`from_yaml`] because parsing happens in read-only paths
    /// too (`propose_flow` validates a draft without saving), and those have no
    /// owner to record.
    pub fn from_yaml_owned(yaml: &str, owner: Option<String>) -> Result<Self, swf_parser::SwfError> {
        let mut rec = Self::from_yaml(yaml)?;
        rec.metadata.owner = owner;
        Ok(rec)
    }

    /// Create from raw CNCF YAML
    pub fn from_yaml(yaml: &str) -> Result<Self, swf_parser::SwfError> {
        let swf = swf_parser::parse_yaml(yaml)?;
        Ok(Self {
            metadata: WorkflowMetadata::from_document(&swf.document),
            raw_yaml: yaml.to_string(),
            durability: FlowDurability::default(),
        })
    }

    /// Parse the raw YAML into a CNCF WorkflowDefinition
    pub fn parse(&self) -> Result<SwfWorkflowDefinition, swf_parser::SwfError> {
        swf_parser::parse_yaml(&self.raw_yaml)
    }

    /// Convenience: get namespace
    pub fn namespace(&self) -> &str {
        &self.metadata.namespace
    }

    /// Convenience: get name
    pub fn name(&self) -> &str {
        &self.metadata.name
    }

    /// Convenience: get version
    pub fn version(&self) -> &str {
        &self.metadata.version
    }
}

/// Workflow metadata (FlowR-specific fields beyond CNCF spec)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowMetadata {
    /// DSL version
    #[serde(default)]
    pub dsl: Option<String>,

    /// Workflow namespace
    pub namespace: String,

    /// Workflow name
    pub name: String,

    /// Workflow version
    pub version: String,

    /// Human-readable title
    #[serde(default)]
    pub title: Option<String>,

    /// Description
    #[serde(default)]
    pub description: Option<String>,

    /// Tags
    #[serde(default)]
    pub tags: Vec<String>,

    /// Arbitrary metadata
    #[serde(default)]
    pub metadata: Option<Value>,

    /// Whether enabled
    #[serde(default = "default_true")]
    pub enabled: bool,

    /// Creation timestamp
    #[serde(default = "Utc::now")]
    pub created_at: DateTime<Utc>,

    /// Last update timestamp
    #[serde(default = "Utc::now")]
    pub updated_at: DateTime<Utc>,

    /// The principal that created this flow.
    ///
    /// `None` means genuinely unowned — a flow created before ownership was
    /// recorded, or one registered by internal machinery. It is deliberately NOT
    /// defaulted to any real identity: an invented owner is worse than an absent
    /// one, because the owner is the default authority over what the flow's agent
    /// may be granted (see .ai/policy-implementation.md §2.3).
    #[serde(default)]
    pub owner: Option<String>,
}

fn default_true() -> bool {
    true
}

impl WorkflowMetadata {
    pub fn new(
        namespace: impl Into<String>,
        name: impl Into<String>,
        version: impl Into<String>,
    ) -> Self {
        let now = Utc::now();
        Self {
            dsl: None,
            namespace: namespace.into(),
            name: name.into(),
            version: version.into(),
            title: None,
            description: None,
            tags: Vec::new(),
            metadata: None,
            enabled: true,
            created_at: now,
            updated_at: now,
            owner: None,
        }
    }

    pub fn from_document(doc: &swf_parser::Document) -> Self {
        Self {
            dsl: Some(doc.dsl.clone()),
            ..Self::new(&doc.namespace, &doc.name, &doc.version)
        }
    }

    /// Record who created this. Called at the write path, which is the only
    /// place that knows the authenticated caller.
    pub fn with_owner(mut self, owner: Option<String>) -> Self {
        self.owner = owner;
        self
    }

    pub fn key(&self) -> String {
        format!("{}/{}", self.namespace, self.name)
    }

    pub fn versioned_key(&self) -> String {
        format!("{}/{}:{}", self.namespace, self.name, self.version)
    }
}

/// Workflow listing summary
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowSummary {
    pub namespace: String,
    pub name: String,
    pub version: String,
    pub enabled: bool,
    pub step_count: usize,
}
