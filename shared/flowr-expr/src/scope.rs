//! Scope - runtime descriptors for expression evaluation
//!
//! CNCF Serverless Workflow defines these scope variables:
//! - `$runtime` - Runtime information
//! - `$workflow` - Workflow instance data
//! - `$task` - Current task information
//! - `$context` - Workflow context (user data)
//! - `$resource` - Resolved resource configuration (FlowR extension)

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Expression evaluation scope containing runtime descriptors
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Scope {
    /// Runtime information ($runtime)
    /// Contains: name, version, etc.
    #[serde(default)]
    pub runtime: Value,

    /// Workflow instance information ($workflow)
    /// Contains: id, name, namespace, version, input, startedAt
    #[serde(default)]
    pub workflow: Value,

    /// Current task information ($task)
    /// Contains: name, reference, input, startedAt
    #[serde(default)]
    pub task: Value,

    /// Workflow context ($context)
    /// User-defined data that persists across tasks
    #[serde(default)]
    pub context: Value,

    /// Resolved resource configuration ($resource)
    /// Contains the config of the resource referenced by the task
    /// FlowR extension to CNCF spec
    #[serde(default)]
    pub resource: Value,

    /// Workflow secrets ($secrets)
    /// Provides access to secrets declared in the workflow's `use.secrets` block
    #[serde(default)]
    pub secrets: Value,
}

impl Scope {
    /// Create a new empty scope
    pub fn new() -> Self {
        Self::default()
    }

    /// Create scope with builder pattern
    pub fn builder() -> ScopeBuilder {
        ScopeBuilder::default()
    }

    /// Convert scope to a Vec of values for jq evaluation
    /// Order matches the with_global_vars declaration in engine.rs
    /// Declaration: ["$secrets", "$resource", "$context", "$task", "$workflow", "$runtime"]
    /// Values must be in same order as declaration
    pub fn to_vars(&self) -> Vec<(String, Value)> {
        vec![
            ("secrets".to_string(), self.secrets.clone()),
            ("resource".to_string(), self.resource.clone()),
            ("context".to_string(), self.context.clone()),
            ("task".to_string(), self.task.clone()),
            ("workflow".to_string(), self.workflow.clone()),
            ("runtime".to_string(), self.runtime.clone()),
        ]
    }
}

/// Builder for constructing Scope
#[derive(Debug, Default)]
pub struct ScopeBuilder {
    runtime: Value,
    workflow: Value,
    task: Value,
    context: Value,
    resource: Value,
    secrets: Value,
}

impl ScopeBuilder {
    /// Set runtime information
    pub fn runtime(mut self, runtime: Value) -> Self {
        self.runtime = runtime;
        self
    }

    /// Set workflow instance information
    pub fn workflow(mut self, workflow: Value) -> Self {
        self.workflow = workflow;
        self
    }

    /// Set current task information
    pub fn task(mut self, task: Value) -> Self {
        self.task = task;
        self
    }

    /// Set workflow context
    pub fn context(mut self, context: Value) -> Self {
        self.context = context;
        self
    }

    /// Set resolved resource configuration
    pub fn resource(mut self, resource: Value) -> Self {
        self.resource = resource;
        self
    }

    /// Set workflow secrets
    pub fn secrets(mut self, secrets: Value) -> Self {
        self.secrets = secrets;
        self
    }

    /// Build the scope
    pub fn build(self) -> Scope {
        Scope {
            runtime: self.runtime,
            workflow: self.workflow,
            task: self.task,
            context: self.context,
            resource: self.resource,
            secrets: self.secrets,
        }
    }
}

/// Helpers for creating runtime descriptor values
pub mod descriptors {
    use serde_json::{json, Value};

    /// Create a runtime descriptor
    pub fn runtime(name: &str, version: &str) -> Value {
        json!({
            "name": name,
            "version": version
        })
    }

    /// Create a workflow descriptor
    pub fn workflow(
        id: &str,
        name: &str,
        namespace: &str,
        version: &str,
        input: Value,
    ) -> Value {
        json!({
            "id": id,
            "name": name,
            "namespace": namespace,
            "version": version,
            "input": input,
            "startedAt": chrono::Utc::now().to_rfc3339()
        })
    }

    /// Create a task descriptor
    pub fn task(name: &str, reference: &str, input: Value) -> Value {
        json!({
            "name": name,
            "reference": reference,
            "input": input,
            "startedAt": chrono::Utc::now().to_rfc3339()
        })
    }
}
