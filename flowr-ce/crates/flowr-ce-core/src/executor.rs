//! Workflow executor for ClawdFlow
//!
//! Simple, single-process workflow execution with optional checkpointing.

use std::collections::HashMap;
use std::time::Instant;

use serde_json::{json, Value};
use tracing::{debug, error, info, instrument};
use uuid::Uuid;

use flowr_expr::Scope;

use crate::context::{FlowContext, TaskContext};
use crate::directive::FlowDirective;
use crate::task::TaskDefinition;
use flowr_types::{Checkpoint, FlowDurability, FlowError, FlowResult};

/// Workflow executor
///
/// Orchestrates execution of workflow tasks with optional checkpointing.
pub struct Executor {
    /// Shared services
    ctx: FlowContext,
}

/// Workflow metadata
#[derive(Debug, Clone)]
pub struct WorkflowMetadata {
    /// Workflow namespace
    pub namespace: String,
    /// Workflow name
    pub name: String,
    /// Workflow version
    pub version: String,
}

impl WorkflowMetadata {
    /// Create new workflow metadata
    pub fn new(
        namespace: impl Into<String>,
        name: impl Into<String>,
        version: impl Into<String>,
    ) -> Self {
        Self {
            namespace: namespace.into(),
            name: name.into(),
            version: version.into(),
        }
    }
}

impl Default for WorkflowMetadata {
    fn default() -> Self {
        Self {
            namespace: "default".to_string(),
            name: "unknown".to_string(),
            version: "0.0.0".to_string(),
        }
    }
}

impl Executor {
    /// Create a new executor with the given flow context
    pub fn new(ctx: FlowContext) -> Self {
        Self { ctx }
    }

    /// Execute a workflow
    ///
    /// # Arguments
    /// * `workflow_id` - Identifier for the workflow definition
    /// * `tasks` - Ordered list of task definitions to execute
    /// * `input` - Initial workflow input
    /// * `durability` - Checkpoint behavior
    ///
    /// # Returns
    /// Final workflow output or error
    #[instrument(skip(self, tasks, input), fields(workflow_id = %workflow_id))]
    pub async fn execute(
        &self,
        workflow_id: &str,
        tasks: &[TaskDefinition],
        input: Value,
        durability: FlowDurability,
    ) -> FlowResult<Value> {
        let metadata = WorkflowMetadata::default();
        self.execute_with_metadata(workflow_id, tasks, input, durability, &metadata)
            .await
    }

    /// Execute a workflow with full metadata
    #[instrument(skip(self, tasks, input, metadata), fields(workflow_id = %workflow_id))]
    pub async fn execute_with_metadata(
        &self,
        workflow_id: &str,
        tasks: &[TaskDefinition],
        input: Value,
        durability: FlowDurability,
        metadata: &WorkflowMetadata,
    ) -> FlowResult<Value> {
        let instance_id = Uuid::now_v7();
        let workflow_start = Instant::now();

        info!(instance_id = %instance_id, "starting workflow execution");

        // Build task index for goto lookups
        let task_index: HashMap<&str, usize> = tasks
            .iter()
            .enumerate()
            .map(|(i, task)| (task.name.as_str(), i))
            .collect();

        // Initialize scope
        let scope = self.build_initial_scope(workflow_id, &instance_id, &input, metadata);

        // Current execution state
        let mut current_output = input.clone();
        let mut context = Value::Object(serde_json::Map::new());
        let mut task_idx = 0;
        let mut sequence = 0u64;

        // Execute tasks
        let result = loop {
            if task_idx >= tasks.len() {
                break Ok(current_output.clone());
            }

            let task_def = &tasks[task_idx];
            let task_start = Instant::now();

            debug!(task = %task_def.name, idx = task_idx, "executing task");

            // Apply input transformation if specified
            let task_input = if let Some(ref input_expr) = task_def.input_from {
                self.ctx.expr.eval(&current_output, input_expr, &scope)?
            } else {
                current_output.clone()
            };

            // Build task context
            let task_type = task_def.executor.task_type();
            let position = format!("do/{}", task_def.name);

            let task_scope = self.build_task_scope(
                &scope,
                &task_def.name,
                &task_input,
                &context,
                task_def.resource.as_ref(),
            );
            let mut task_ctx = TaskContext::new(
                task_input,
                task_scope,
                self.ctx.clone(),
                instance_id,
                &task_def.name,
                position.clone(),
            );

            // Execute the task
            let task_result = task_def.executor.execute(&mut task_ctx).await;

            let task_duration_us = task_start.elapsed().as_micros() as u64;
            debug!(task = %task_def.name, duration_us = task_duration_us, "task completed");

            // Handle task execution result
            let mut result = match task_result {
                Ok(r) => r,
                Err(e) => {
                    error!(task = %task_def.name, error = %e, "task failed");
                    return Err(e);
                }
            };

            // Apply output transformation if specified
            if let Some(ref output_expr) = task_def.output_as {
                result.output = self.ctx.expr.eval(&result.output, output_expr, &task_ctx.scope)?;
            }

            // Handle checkpoint if needed
            if durability == FlowDurability::Checkpoint && task_def.is_checkpoint() {
                sequence += 1;
                let checkpoint = Checkpoint::new(
                    instance_id,
                    workflow_id,
                    &task_ctx.position,
                    context.clone(),
                    result.output.clone(),
                    task_idx,
                    sequence,
                );

                if let Err(e) = self.ctx.storage.checkpoints().save(checkpoint).await {
                    error!(error = %e, "failed to save checkpoint");
                }
            }

            // Merge exports into context
            if let Some(exports) = result.exports {
                if let Value::Object(map) = exports {
                    if let Value::Object(ref mut ctx_map) = context {
                        for (k, v) in map {
                            ctx_map.insert(k, v);
                        }
                    }
                }
            }

            // Update current output
            current_output = result.output;

            // Handle directive
            match result.directive {
                FlowDirective::Continue => {
                    task_idx += 1;
                }
                FlowDirective::Goto(target) => {
                    if let Some(&idx) = task_index.get(target.as_str()) {
                        debug!(target = %target, idx = idx, "goto");
                        task_idx = idx;
                    } else {
                        return Err(FlowError::task_failed(
                            &task_def.name,
                            format!("goto target not found: {}", target),
                        ));
                    }
                }
                FlowDirective::Exit | FlowDirective::End => {
                    debug!("workflow ended by directive");
                    break Ok(current_output.clone());
                }
                FlowDirective::Suspend => {
                    debug!("workflow suspended");
                    // In single-process mode, we just return the current state
                    // The caller can handle suspension as needed
                    break Ok(current_output.clone());
                }
            }
        };

        let workflow_duration = workflow_start.elapsed();
        info!(
            instance_id = %instance_id,
            duration_ms = workflow_duration.as_millis() as u64,
            "workflow execution completed"
        );

        result
    }

    /// Build initial scope with runtime and workflow variables
    fn build_initial_scope(
        &self,
        workflow_id: &str,
        instance_id: &Uuid,
        input: &Value,
        metadata: &WorkflowMetadata,
    ) -> Scope {
        Scope::builder()
            .runtime(json!({
                "timestamp": chrono::Utc::now().to_rfc3339(),
            }))
            .workflow(json!({
                "id": workflow_id,
                "instance_id": instance_id.to_string(),
                "namespace": metadata.namespace,
                "name": metadata.name,
                "version": metadata.version,
            }))
            .task(json!({}))
            .context(json!({}))
            .build()
    }

    /// Build task-specific scope
    fn build_task_scope(
        &self,
        base: &Scope,
        task_name: &str,
        task_input: &Value,
        context: &Value,
        resource: Option<&Value>,
    ) -> Scope {
        let mut task_data = json!({
            "name": task_name,
            "input": task_input,
        });

        if let Some(res) = resource {
            task_data["resource"] = res.clone();
        }

        let mut builder = Scope::builder()
            .runtime(base.runtime.clone())
            .workflow(base.workflow.clone())
            .task(task_data)
            .context(context.clone());

        if let Some(res) = resource {
            builder = builder.resource(res.clone());
        }

        builder.build()
    }

    /// Resume a workflow from a checkpoint
    #[instrument(skip(self, tasks), fields(instance_id = %instance_id))]
    pub async fn resume(
        &self,
        instance_id: Uuid,
        workflow_id: &str,
        tasks: &[TaskDefinition],
        durability: FlowDurability,
    ) -> FlowResult<Value> {
        // Load checkpoint
        let checkpoint = self
            .ctx
            .storage
            .checkpoints()
            .load(instance_id)
            .await?
            .ok_or_else(|| FlowError::NotFound(format!("checkpoint not found: {}", instance_id)))?;

        info!(
            instance_id = %instance_id,
            position = %checkpoint.position,
            sequence = checkpoint.sequence,
            "resuming workflow from checkpoint"
        );

        // Rebuild metadata from checkpoint
        let metadata = WorkflowMetadata::default();

        // Continue execution from checkpoint position
        let task_index: HashMap<&str, usize> = tasks
            .iter()
            .enumerate()
            .map(|(i, task)| (task.name.as_str(), i))
            .collect();

        // Find the task index to resume from
        let resume_idx = checkpoint.task_index + 1;

        if resume_idx >= tasks.len() {
            return Ok(checkpoint.output);
        }

        // Resume execution
        let scope = self.build_initial_scope(workflow_id, &instance_id, &checkpoint.output, &metadata);
        let mut current_output = checkpoint.output;
        let mut context = checkpoint.context;
        let mut task_idx = resume_idx;
        let mut sequence = checkpoint.sequence;

        let result = loop {
            if task_idx >= tasks.len() {
                break Ok(current_output.clone());
            }

            let task_def = &tasks[task_idx];
            let task_start = Instant::now();

            debug!(task = %task_def.name, idx = task_idx, "executing task (resumed)");

            let task_input = if let Some(ref input_expr) = task_def.input_from {
                self.ctx.expr.eval(&current_output, input_expr, &scope)?
            } else {
                current_output.clone()
            };

            let position = format!("do/{}", task_def.name);
            let task_scope = self.build_task_scope(
                &scope,
                &task_def.name,
                &task_input,
                &context,
                task_def.resource.as_ref(),
            );
            let mut task_ctx = TaskContext::new(
                task_input,
                task_scope,
                self.ctx.clone(),
                instance_id,
                &task_def.name,
                position.clone(),
            );

            let task_result = task_def.executor.execute(&mut task_ctx).await;
            let task_duration_us = task_start.elapsed().as_micros() as u64;
            debug!(task = %task_def.name, duration_us = task_duration_us, "task completed");

            let mut result = task_result?;

            if let Some(ref output_expr) = task_def.output_as {
                result.output = self.ctx.expr.eval(&result.output, output_expr, &task_ctx.scope)?;
            }

            if durability == FlowDurability::Checkpoint && task_def.is_checkpoint() {
                sequence += 1;
                let checkpoint = Checkpoint::new(
                    instance_id,
                    workflow_id,
                    &task_ctx.position,
                    context.clone(),
                    result.output.clone(),
                    task_idx,
                    sequence,
                );

                if let Err(e) = self.ctx.storage.checkpoints().save(checkpoint).await {
                    error!(error = %e, "failed to save checkpoint");
                }
            }

            if let Some(exports) = result.exports {
                if let Value::Object(map) = exports {
                    if let Value::Object(ref mut ctx_map) = context {
                        for (k, v) in map {
                            ctx_map.insert(k, v);
                        }
                    }
                }
            }

            current_output = result.output;

            match result.directive {
                FlowDirective::Continue => {
                    task_idx += 1;
                }
                FlowDirective::Goto(target) => {
                    if let Some(&idx) = task_index.get(target.as_str()) {
                        task_idx = idx;
                    } else {
                        return Err(FlowError::task_failed(
                            &task_def.name,
                            format!("goto target not found: {}", target),
                        ));
                    }
                }
                FlowDirective::Exit | FlowDirective::End => {
                    break Ok(current_output.clone());
                }
                FlowDirective::Suspend => {
                    break Ok(current_output.clone());
                }
            }
        };

        result
    }
}
