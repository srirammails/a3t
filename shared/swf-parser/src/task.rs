//! CNCF SWF Task types — the 12 task type enum

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::call::CallTask;
use crate::directive::FlowDirective;
use crate::event::{EmitTask, ListenTask};
use crate::flow_control::{DoTask, ForTask, ForkTask, SwitchTask, TryTask};
use crate::run::RunTask;
use crate::schema::{InputDefinition, OutputDefinition, ExportDefinition};
use crate::timeout::Timeout;

/// A named task in the workflow's `do` block
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NamedTask {
    /// Task name (the key in the YAML map)
    pub name: String,

    /// The task definition
    pub task: Task,

    /// Optional condition — skip task if false
    #[serde(rename = "if")]
    #[serde(default)]
    pub condition: Option<String>,

    /// Input transformation + validation
    #[serde(default)]
    pub input: Option<InputDefinition>,

    /// Output transformation + validation
    #[serde(default)]
    pub output: Option<OutputDefinition>,

    /// Export to context
    #[serde(default)]
    pub export: Option<ExportDefinition>,

    /// Task-level timeout
    #[serde(default)]
    pub timeout: Option<Timeout>,

    /// Flow directive (then: continue|end|exit|<taskName>)
    #[serde(default)]
    pub then: Option<FlowDirective>,

    /// Arbitrary metadata
    #[serde(default)]
    pub metadata: Option<Value>,
}

/// The 12 CNCF SWF task types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Task {
    /// Call a service or function
    Call(CallTask),
    /// Execute subtasks sequentially
    Do(DoTask),
    /// Emit a CloudEvent
    Emit(EmitTask),
    /// Iterate over a collection
    For(ForTask),
    /// Execute branches in parallel
    Fork(ForkTask),
    /// Listen for events
    Listen(ListenTask),
    /// Raise an error
    Raise(RaiseTask),
    /// Run a process (container, shell, script, workflow)
    Run(RunTask),
    /// Set data values
    Set(SetTask),
    /// Conditional branching
    Switch(SwitchTask),
    /// Error handling with retry
    Try(TryTask),
    /// Wait for a duration
    Wait(WaitTask),
}

/// Set task — assigns key-value pairs as output
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetTask {
    /// Key-value map (values can be runtime expressions)
    #[serde(flatten)]
    pub values: serde_json::Map<String, Value>,
}

/// Raise task — raises an error (RFC 7807)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RaiseTask {
    pub error: RaiseError,
}

/// Error definition for raise task
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RaiseError {
    /// Error type URI
    #[serde(rename = "type")]
    #[serde(default)]
    pub error_type: Option<String>,

    /// HTTP status code
    #[serde(default)]
    pub status: Option<u16>,

    /// Short title
    #[serde(default)]
    pub title: Option<String>,

    /// Detailed description
    #[serde(default)]
    pub detail: Option<String>,

    /// JSON Pointer to faulting component
    #[serde(default)]
    pub instance: Option<String>,

    /// Reference to a named error in use.errors
    #[serde(rename = "use")]
    #[serde(default)]
    pub use_ref: Option<String>,
}

/// Wait task — pause for a duration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WaitTask {
    #[serde(default)]
    pub seconds: Option<u64>,
    #[serde(default)]
    pub minutes: Option<u64>,
    #[serde(default)]
    pub hours: Option<u64>,
    #[serde(default)]
    pub days: Option<u64>,
}

impl Task {
    /// Get the task type name
    pub fn type_name(&self) -> &'static str {
        match self {
            Task::Call(_) => "call",
            Task::Do(_) => "do",
            Task::Emit(_) => "emit",
            Task::For(_) => "for",
            Task::Fork(_) => "fork",
            Task::Listen(_) => "listen",
            Task::Raise(_) => "raise",
            Task::Run(_) => "run",
            Task::Set(_) => "set",
            Task::Switch(_) => "switch",
            Task::Try(_) => "try",
            Task::Wait(_) => "wait",
        }
    }
}
