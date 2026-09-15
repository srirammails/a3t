//! Extension dispatch — CNCF use.extensions before/after hooks
//!
//! Extensions are before/after interceptors that fire around tasks based on
//! the `extend` field matching the task type. They support conditional firing
//! via `when` expressions and can contain arbitrary tasks as hooks.

use serde_json::Value;
use swf_parser::Extension;
use tracing::{debug, warn};

use crate::context::ExecutionContext;
use crate::error::{Result, RuntimeError};
use crate::traits::{FlowDirective, TaskResult};

/// Resolved extension ready for dispatch. Wraps the parsed Extension
/// with no additional state — matching and dispatch use the parsed fields directly.
#[derive(Debug, Clone)]
pub struct ResolvedExtension {
    pub inner: Extension,
}

impl ResolvedExtension {
    /// Create from a parsed Extension
    pub fn from_parsed(ext: Extension) -> Self {
        Self { inner: ext }
    }

    /// Resolve extensions from UseBlock's Vec<Value> into ResolvedExtensions
    pub fn resolve_all(raw_extensions: &[Value]) -> Vec<Self> {
        raw_extensions
            .iter()
            .filter_map(|v| {
                // Each extension in use.extensions is a single-key map: {name: {extend, when, before, after}}
                let obj = v.as_object()?;
                let (name, body) = obj.iter().next()?;
                let mut ext: Extension = serde_json::from_value(body.clone()).ok()?;
                ext.name = name.clone();
                Some(ResolvedExtension::from_parsed(ext))
            })
            .collect()
    }

    /// Check if this extension matches a given task type
    pub fn matches(&self, task_type: &str) -> bool {
        let extend = &self.inner.extend;
        if extend == "all" {
            return true;
        }
        // Exact match or prefix match (e.g., "call" matches "call:llm", "call:http")
        task_type == extend || task_type.starts_with(&format!("{}:", extend))
    }

    /// Check if this extension matches a task by name (for `when` expressions referencing $task.name)
    pub fn matches_task_name(&self, task_name: &str) -> bool {
        // Some extensions use extend field as task name match (e.g., "dispatch_tools")
        // This is checked via the `when` condition, not here
        false
    }
}

/// Get the task type string for a parsed Task enum
pub fn task_type_name(task: &swf_parser::Task) -> &'static str {
    match task {
        swf_parser::Task::Set(_) => "set",
        swf_parser::Task::Do(_) => "do",
        swf_parser::Task::For(_) => "for",
        swf_parser::Task::Switch(_) => "switch",
        swf_parser::Task::Try(_) => "try",
        swf_parser::Task::Fork(_) => "fork",
        swf_parser::Task::Raise(_) => "raise",
        swf_parser::Task::Wait(_) => "wait",
        swf_parser::Task::Call(_) => "call",
        swf_parser::Task::Run(_) => "run",
        swf_parser::Task::Emit(_) => "emit",
        swf_parser::Task::Listen(_) => "listen",
    }
}

/// Get the specific task type string including subtypes (e.g., "call:llm", "run:workflow")
pub fn task_type_specific(task: &swf_parser::Task) -> String {
    match task {
        swf_parser::Task::Call(ct) => {
            match &ct.target {
                swf_parser::CallTarget::Http(_) => "call:http".to_string(),
                swf_parser::CallTarget::Grpc(_) => "call:grpc".to_string(),
                swf_parser::CallTarget::OpenApi(_) => "call:openapi".to_string(),
                swf_parser::CallTarget::AsyncApi(_) => "call:asyncapi".to_string(),
                swf_parser::CallTarget::A2A(_) => "call:a2a".to_string(),
                swf_parser::CallTarget::Mcp(_) => "call:mcp".to_string(),
                swf_parser::CallTarget::Function(name) => format!("call:{}", name),
            }
        }
        swf_parser::Task::Run(rt) => {
            match rt {
                swf_parser::RunTask::Workflow(_) => "run:workflow".to_string(),
                swf_parser::RunTask::Shell(_) => "run:shell".to_string(),
                swf_parser::RunTask::Script(_) => "run:script".to_string(),
                swf_parser::RunTask::Container(_) => "run:container".to_string(),
            }
        }
        other => task_type_name(other).to_string(),
    }
}
