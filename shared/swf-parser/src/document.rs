//! CNCF SWF Document and WorkflowDefinition

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::schema::{InputDefinition, OutputDefinition};
use crate::schedule::Schedule;
use crate::task::NamedTask;
use crate::timeout::Timeout;
use crate::use_block::UseBlock;

/// Top-level workflow definition per CNCF SWF DSL 1.0.3
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowDefinition {
    /// Document metadata (dsl version, namespace, name, version)
    pub document: Document,

    /// Reusable components: extensions, functions, secrets, etc.
    #[serde(rename = "use")]
    #[serde(default)]
    pub use_block: Option<UseBlock>,

    /// Workflow scheduling
    #[serde(default)]
    pub schedule: Option<Schedule>,

    /// Input definition (schema + transform)
    #[serde(default)]
    pub input: Option<InputDefinition>,

    /// Output definition (schema + transform)
    #[serde(default)]
    pub output: Option<OutputDefinition>,

    /// Workflow-level timeout
    #[serde(default)]
    pub timeout: Option<Timeout>,

    /// Arbitrary metadata
    #[serde(default)]
    pub metadata: Option<Value>,

    /// The ordered list of tasks to execute
    /// Parsed from the `do` block: [{taskName: {taskDef}}]
    #[serde(skip)]
    pub tasks: Vec<NamedTask>,

    /// Raw `do` block (used during parsing, then converted to `tasks`)
    #[serde(rename = "do")]
    #[serde(default)]
    raw_do: Option<Value>,
}

/// Document metadata block
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Document {
    /// DSL version (e.g. "1.0.3")
    pub dsl: String,

    /// Workflow namespace
    #[serde(default = "default_namespace")]
    pub namespace: String,

    /// Workflow name
    pub name: String,

    /// Workflow version
    #[serde(default = "default_version")]
    pub version: String,
}

fn default_namespace() -> String {
    "default".to_string()
}

fn default_version() -> String {
    "0.1.0".to_string()
}

impl WorkflowDefinition {
    /// Get the raw `do` block for parsing
    pub(crate) fn take_raw_do(&mut self) -> Option<Value> {
        self.raw_do.take()
    }
}
