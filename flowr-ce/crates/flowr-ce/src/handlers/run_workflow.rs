//! run:workflow handler — executes nested workflows via the CNCF runtime
//!
//! Resolves the workflow from storage, parses with swf-parser,
//! and executes through WorkflowExecutor (full pipeline).

use async_trait::async_trait;
use serde_json::Value;
use std::sync::Arc;
use tracing::info;

use swf_parser::{NamedTask, Task, RunTask};
use swf_runtime::context::ExecutionContext;
use swf_runtime::error::{RuntimeError, Result};
use swf_runtime::traits::{TaskHandler, TaskResult};

use flowr_storage::StorageBackend;

pub struct RunWorkflowHandler;

#[async_trait]
impl TaskHandler for RunWorkflowHandler {
    fn task_type(&self) -> &str {
        "run:workflow"
    }

    /// Gate on the *target flow* (`Flow{ns/name}`), so policy can allow a flow to
    /// call some subflows but not others. The subflow's own steps are still gated
    /// individually as they run (under the inherited principal).
    fn policy_resource(&self, task: &NamedTask) -> flowr_policy::Resource {
        let id = match &task.task {
            Task::Run(RunTask::Workflow(wf)) => format!("{}/{}", wf.namespace, wf.name),
            _ => "run:workflow".to_string(),
        };
        flowr_policy::Resource::new("Flow", id).with_attr("task_type", "run:workflow")
    }

    fn policy_action(&self) -> flowr_policy::Action {
        flowr_policy::actions::flow_start()
    }

    async fn execute(
        &self,
        task: &NamedTask,
        input: Value,
        ctx: &mut ExecutionContext,
    ) -> Result<TaskResult> {
        let (namespace, name, version, wf_input) = match &task.task {
            Task::Run(RunTask::Workflow(wf)) => {
                let inp = wf.input.clone().unwrap_or(input.clone());
                (wf.namespace.clone(), wf.name.clone(), wf.version.clone(), inp)
            }
            _ => return Err(RuntimeError::task_failed(&task.name, "expected run:workflow task")),
        };

        info!(task = %task.name, namespace = %namespace, name = %name, "executing nested workflow");

        // A subflow runs as its caller, so the caller must be allowed to run
        // flows in that namespace: its own projects, the platform, or a project
        // exposed to it. Without this a flow could call another org's private flow.
        let guard = flowr_policy::namespace::namespace_guard();
        if guard.enforcing() && guard.access(&ctx.principal, &namespace).await < flowr_policy::namespace::NsAccess::Run {
            return Err(RuntimeError::task_failed(&task.name,
                format!("no flow {namespace}/{name} that this run may use")));
        }

        // Get storage from context
        let storage = ctx.storage.as_ref()
            .and_then(|s| s.downcast_ref::<Arc<dyn StorageBackend>>().cloned())
            .ok_or_else(|| RuntimeError::Internal("Storage not available for run:workflow".to_string()))?;

        // Load workflow record
        let record = match version {
            Some(ref v) => storage.workflows().get_version(&namespace, &name, v).await,
            None => storage.workflows().get(&namespace, &name).await,
        }.map_err(|e| RuntimeError::WorkflowNotFound {
            namespace: namespace.clone(),
            name: format!("{}: {}", name, e),
        })?;

        // Parse CNCF YAML
        let swf_workflow = record.parse()
            .map_err(|e| RuntimeError::task_failed(&task.name, format!("Failed to parse nested workflow: {}", e)))?;

        // Create child executor with same handlers
        let mut child_executor = swf_runtime::WorkflowExecutor::new();
        swf_runtime::handlers::register_builtin_handlers(&mut child_executor);
        crate::handlers::register_clawdflow_handlers(&mut child_executor);

        // Wire expression engine
        let expr_engine = flowr_core::ExpressionEngine::new();
        child_executor.set_eval_fn(move |input, expr, context| {
            use flowr_core::Scope;
            let scope = Scope::builder()
                .context(context.clone())
                .build();
            expr_engine.eval(input, expr, &scope)
                .map_err(|e| e.to_string())
        });

        // Create child context
        let mut child_ctx = ExecutionContext {
            instance_id: format!("{}/nested", ctx.instance_id),
            namespace: namespace.clone(),
            name: name.clone(),
            version: record.version().to_string(),
            context: Value::Null,
            secrets: ctx.secrets.clone(),
            runtime: ctx.runtime.clone(),
            storage: ctx.storage.clone(),
            http_client: ctx.http_client.clone(),
            extensions: if let Some(ref use_block) = swf_workflow.use_block {
                swf_runtime::ResolvedExtension::resolve_all(&use_block.extensions)
            } else {
                Vec::new()
            },
            eval_fn: ctx.eval_fn.clone(),
            // E0.4: a nested workflow runs as its parent's principal (inherited).
            principal: ctx.principal.clone(),
            // E2 #4: inherit the parent's capability template so the subflow's
            // executor composes attenuate-only (it can narrow, never widen).
            template_meta: ctx.template_meta.clone(),
        };

        // Execute
        let output = child_executor.execute(&swf_workflow, wf_input, &mut child_ctx).await
            .map_err(|e| RuntimeError::task_failed(&task.name, format!("Nested workflow failed: {}", e)))?;

        Ok(TaskResult::new(output))
    }
}
