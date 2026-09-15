//! Core runtime traits — RuntimeProvider and TaskHandler
//!
//! These traits define the pluggable execution interface.
//! FlowR implements RuntimeProvider for CNCF SWF 1.0.3.
//! Each task type (call:http, set, for, etc.) implements TaskHandler.

use async_trait::async_trait;
use serde_json::Value;

use swf_parser::{WorkflowDefinition, NamedTask, Task};

use crate::context::ExecutionContext;
use crate::error::RuntimeError;

/// Result of executing a task
#[derive(Debug, Clone)]
pub struct TaskResult {
    /// Task output
    pub output: Value,

    /// Flow directive (continue, end, exit, goto)
    pub directive: FlowDirective,

    /// Optional exports to merge into $context
    pub exports: Option<Value>,
}

/// Flow control directives
#[derive(Debug, Clone, PartialEq)]
pub enum FlowDirective {
    /// Continue to next task (default)
    Continue,
    /// End workflow execution gracefully
    End,
    /// Exit current scope (do, for block)
    Exit,
    /// Go to a named task
    Goto(String),
    /// Suspend execution (wait for event/signal)
    Suspend,
}

impl Default for FlowDirective {
    fn default() -> Self {
        Self::Continue
    }
}

impl TaskResult {
    pub fn new(output: Value) -> Self {
        Self {
            output,
            directive: FlowDirective::Continue,
            exports: None,
        }
    }

    pub fn with_directive(mut self, directive: FlowDirective) -> Self {
        self.directive = directive;
        self
    }

    pub fn with_exports(mut self, exports: Value) -> Self {
        self.exports = Some(exports);
        self
    }
}

/// A runtime that can execute CNCF SWF workflows for a specific DSL version.
///
/// Implementations handle the full execution pipeline:
/// - Task dispatch (which handler for which task type)
/// - Data flow (input/output/export transforms + validation)
/// - Extension dispatch (before/after hooks)
/// - Flow control (continue, end, exit, goto)
/// - Expression evaluation (jq)
#[async_trait]
pub trait RuntimeProvider: Send + Sync {
    /// DSL versions this runtime supports (e.g. ["1.0.3"])
    fn supported_versions(&self) -> &[&str];

    /// Execute a parsed workflow with the given input
    async fn execute(
        &self,
        workflow: &WorkflowDefinition,
        input: Value,
        ctx: &mut ExecutionContext,
    ) -> Result<Value, RuntimeError>;
}

/// Handler for a specific task type.
///
/// One handler per task variant (call:http, set, for, etc.).
/// Handlers are stateless — all state flows through TaskContext.
#[async_trait]
pub trait TaskHandler: Send + Sync {
    /// The task type name this handler supports (e.g. "call:http", "set", "for")
    fn task_type(&self) -> &str;

    /// Where this handler's implementation comes from: `"builtin"` for
    /// compiled-in first-party handlers (the default), `"wasm"` for a sandboxed
    /// WASM-plugin handler. Surfaced on the `tool_call` span as
    /// `aesop.task.source` and available to the policy hook, so EE's Cedar can
    /// gate trusted builtins differently from capability-scoped plugins.
    fn source(&self) -> &str {
        "builtin"
    }

    /// The policy `Resource` this step targets, evaluated at the executor choke
    /// point. Default: the task type itself (`Tool{task_type}`), preserving prior
    /// behavior. Handlers that dispatch to a *named target* — an MCP tool, a
    /// subflow, an LLM — override this so policy gates on the real target
    /// (`Tool{list_agents}`, `Flow{ns/name}`, `Llm{model}`) rather than the
    /// wrapping task type. The task type is always attached as the `task_type`
    /// attribute, so a policy can still gate the mechanism (e.g. forbid raw
    /// `call:mcp` outside sanctioned flows).
    fn policy_resource(&self, _task: &NamedTask) -> flowr_policy::Resource {
        flowr_policy::Resource::tool(self.task_type()).with_attr("task_type", self.task_type())
    }

    /// The policy `Action` for this step, paired with [`policy_resource`]. Default
    /// `tool:invoke` (on a `Tool`). Handlers whose resource is a different type
    /// override this so the action matches — `call:llm`→`llm:call` (on `Llm`),
    /// `run:workflow`→`flow:start` (on `Flow`) — since the Cedar schema binds each
    /// action to its resource type.
    fn policy_action(&self) -> flowr_policy::Action {
        flowr_policy::actions::tool_invoke()
    }

    /// Execute the task and return the result
    async fn execute(
        &self,
        task: &NamedTask,
        input: Value,
        ctx: &mut ExecutionContext,
    ) -> Result<TaskResult, RuntimeError>;
}
