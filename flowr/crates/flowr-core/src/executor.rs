//! Workflow executor
//!
//! Distributed workflow execution with Iggy checkpoints, Dragonfly state,
//! and full lifecycle event emission. Supports CNCF Serverless Workflow
//! extensions (before/after interceptors), task `if` conditions, and timeouts.

use std::collections::HashMap;
use std::str::FromStr;
use std::time::Instant;

use iggy::client::MessageClient;
use iggy::consumer::Consumer;
use iggy::identifier::Identifier;
use iggy::messages::poll_messages::PollingStrategy;
use iggy::messages::send_messages::{Message, Partitioning};
use redis::AsyncCommands;
use chrono::Utc;
use serde_json::{json, Value};
use tracing::{debug, error, info, instrument, warn};
use uuid::Uuid;

use flowr_expr::Scope;

use crate::context::{FlowContext, TaskContext};
use crate::directive::FlowDirective;
use crate::durability::{Checkpoint, CheckpointIndex, FlowDurability};
use crate::error::{FlowError, FlowResult};
use crate::lifecycle::{ErrorData, SuspensionData};
use crate::task::{ResolvedExtension, TaskDefinition};

/// Iggy stream name for FlowR
const FLOWR_STREAM: &str = "flowr";
/// Iggy topic for checkpoints
const CHECKPOINT_TOPIC: &str = "checkpoints";
/// Iggy topic for completion events
const COMPLETION_TOPIC: &str = "completions";
/// Dragonfly key prefix for checkpoint index
const CHECKPOINT_INDEX_PREFIX: &str = "flowr:checkpoint:";

/// Workflow executor
///
/// Orchestrates execution of workflow tasks with configurable durability.
pub struct Executor {
    /// Shared services
    ctx: FlowContext,
}

/// Workflow metadata for lifecycle events
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

    /// Execute a workflow (backward compatible)
    #[instrument(skip(self, tasks, input), fields(workflow_id = %workflow_id))]
    pub async fn execute(
        &self,
        workflow_id: &str,
        tasks: &[TaskDefinition],
        input: Value,
        durability: FlowDurability,
    ) -> FlowResult<Value> {
        let metadata = WorkflowMetadata::default();
        self.execute_with_extensions(workflow_id, tasks, input, durability, &metadata, &[])
            .await
    }

    /// Execute a workflow with full metadata (backward compatible)
    #[instrument(skip(self, tasks, input, metadata), fields(workflow_id = %workflow_id))]
    pub async fn execute_with_metadata(
        &self,
        workflow_id: &str,
        tasks: &[TaskDefinition],
        input: Value,
        durability: FlowDurability,
        metadata: &WorkflowMetadata,
    ) -> FlowResult<Value> {
        self.execute_with_extensions(workflow_id, tasks, input, durability, metadata, &[])
            .await
    }

    /// Execute a workflow with extensions (CNCF spec before/after interceptors)
    #[instrument(skip(self, tasks, input, metadata, extensions), fields(workflow_id = %workflow_id))]
    pub async fn execute_with_extensions(
        &self,
        workflow_id: &str,
        tasks: &[TaskDefinition],
        input: Value,
        durability: FlowDurability,
        metadata: &WorkflowMetadata,
        extensions: &[ResolvedExtension],
    ) -> FlowResult<Value> {
        let instance_id = Uuid::now_v7();
        let workflow_start = Instant::now();

        info!(instance_id = %instance_id, "starting workflow execution");

        // Emit workflow.started event
        if let Err(e) = self
            .ctx
            .lifecycle
            .workflow_started(
                instance_id,
                &metadata.namespace,
                &metadata.name,
                &metadata.version,
                Some(input.clone()),
            )
            .await
        {
            error!(error = %e, "failed to emit workflow.started event");
        }

        // Build task index for goto lookups
        let task_index: HashMap<&str, usize> = tasks
            .iter()
            .enumerate()
            .map(|(i, task)| (task.name.as_str(), i))
            .collect();

        // Initialize scope
        let scope = self.build_initial_scope(workflow_id, &instance_id, &input);

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
            let task_type = task_def.executor.task_type();
            let position = format!("do/{}", task_def.name);

            debug!(task = %task_def.name, idx = task_idx, "executing task");

            // Emit task.created event
            if let Err(e) = self
                .ctx
                .lifecycle
                .task_created(
                    instance_id,
                    &metadata.namespace,
                    &metadata.name,
                    &task_def.name,
                    task_type,
                    &position,
                )
                .await
            {
                error!(error = %e, "failed to emit task.created event");
            }

            // Check if condition — skip task if evaluates to false
            if let Some(ref if_expr) = task_def.if_condition {
                let cond_scope = self.build_task_scope(
                    &scope,
                    &task_def.name,
                    &current_output,
                    &context,
                    task_def.resource.as_ref(),
                );
                let cond = self.ctx.expr.eval(&current_output, if_expr, &cond_scope)?;
                if cond != Value::Bool(true) {
                    debug!(task = %task_def.name, "task skipped by if condition");
                    // Emit task.skipped event
                    if let Err(e) = self
                        .ctx
                        .lifecycle
                        .task_skipped(
                            instance_id,
                            &metadata.namespace,
                            &metadata.name,
                            &task_def.name,
                            task_type,
                            &position,
                        )
                        .await
                    {
                        error!(error = %e, "failed to emit task.skipped event");
                    }
                    task_idx += 1;
                    continue;
                }
            }

            // Apply input transformation if specified
            let task_input = if let Some(ref input_expr) = task_def.input_from {
                self.ctx.expr.eval(&current_output, input_expr, &scope)?
            } else {
                current_output.clone()
            };

            // Emit task.started event
            if let Err(e) = self
                .ctx
                .lifecycle
                .task_started(
                    instance_id,
                    &metadata.namespace,
                    &metadata.name,
                    &task_def.name,
                    task_type,
                    &position,
                    Some(task_input.clone()),
                )
                .await
            {
                error!(error = %e, "failed to emit task.started event");
            }

            // Build task context with resource if present
            let task_scope = self.build_task_scope(
                &scope,
                &task_def.name,
                &task_input,
                &context,
                task_def.resource.as_ref(),
            );
            let mut task_ctx = TaskContext::new(
                task_input.clone(),
                task_scope,
                self.ctx.clone(),
                instance_id,
                &task_def.name,
                position.clone(),
            );

            // Run matching before-extensions
            self.run_extensions_before(
                extensions,
                task_type,
                &task_input,
                &task_ctx.scope,
                instance_id,
                &position,
            )
            .await;

            // Execute the task (with optional timeout)
            let task_result = if let Some(timeout_dur) = task_def.timeout {
                match tokio::time::timeout(timeout_dur, task_def.executor.execute(&mut task_ctx))
                    .await
                {
                    Ok(result) => result,
                    Err(_) => Err(FlowError::task_failed(
                        &task_def.name,
                        format!("task exceeded timeout of {:?}", timeout_dur),
                    )),
                }
            } else {
                task_def.executor.execute(&mut task_ctx).await
            };

            let task_duration_us = task_start.elapsed().as_micros() as u64;

            // Handle task execution result
            let mut result = match task_result {
                Ok(r) => r,
                Err(e) => {
                    // Emit task.faulted event
                    let error_data = ErrorData::new("task_error", e.to_string());
                    if let Err(emit_err) = self
                        .ctx
                        .lifecycle
                        .task_faulted(
                            instance_id,
                            &metadata.namespace,
                            &metadata.name,
                            &task_def.name,
                            task_type,
                            &position,
                            error_data.clone(),
                        )
                        .await
                    {
                        error!(error = %emit_err, "failed to emit task.faulted event");
                    }

                    // Emit workflow.faulted event
                    if let Err(emit_err) = self
                        .ctx
                        .lifecycle
                        .workflow_faulted(
                            instance_id,
                            &metadata.namespace,
                            &metadata.name,
                            &metadata.version,
                            error_data,
                        )
                        .await
                    {
                        error!(error = %emit_err, "failed to emit workflow.faulted event");
                    }

                    return Err(e);
                }
            };

            // Run matching after-extensions
            self.run_extensions_after(
                extensions,
                task_type,
                &result.output,
                &task_ctx.scope,
                instance_id,
                &position,
            )
            .await;

            // Emit task.completed event
            if let Err(e) = self
                .ctx
                .lifecycle
                .task_completed(
                    instance_id,
                    &metadata.namespace,
                    &metadata.name,
                    &task_def.name,
                    task_type,
                    &position,
                    Some(result.output.clone()),
                    Some(task_duration_us),
                )
                .await
            {
                error!(error = %e, "failed to emit task.completed event");
            }

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
                    sequence,
                ).with_output(result.output.clone());

                self.write_checkpoint(&checkpoint).await?;
            }

            // Apply export transformation if specified, otherwise use result.exports
            let exports = if let Some(ref export_expr) = task_def.export_as {
                Some(self.ctx.expr.eval(&result.output, export_expr, &task_ctx.scope)?)
            } else {
                result.exports
            };

            // Merge exports into context
            if let Some(exports) = exports {
                merge_json(&mut context, exports);
            }

            // Update output
            current_output = result.output;

            // Determine flow directive (task definition's `then` overrides result directive)
            let directive = if let Some(ref then) = task_def.then {
                FlowDirective::from(then.as_str())
            } else {
                result.directive
            };

            // Handle flow directive
            match directive {
                FlowDirective::Continue => {
                    task_idx += 1;
                }
                FlowDirective::Exit => {
                    debug!("exit directive - exiting current scope");
                    break Ok(current_output.clone());
                }
                FlowDirective::End => {
                    info!("end directive - terminating workflow");
                    break Ok(current_output.clone());
                }
                FlowDirective::Goto(ref target) => {
                    if let Some(&idx) = task_index.get(target.as_str()) {
                        debug!(target = target, idx = idx, "goto directive");
                        task_idx = idx;
                    } else {
                        let error_data = ErrorData::new("invalid_goto", format!("target '{}' not found", target));
                        if let Err(e) = self
                            .ctx
                            .lifecycle
                            .workflow_faulted(
                                instance_id,
                                &metadata.namespace,
                                &metadata.name,
                                &metadata.version,
                                error_data,
                            )
                            .await
                        {
                            error!(error = %e, "failed to emit workflow.faulted event");
                        }
                        return Err(FlowError::invalid_goto(target));
                    }
                }
                FlowDirective::Suspend => {
                    info!("suspend directive - workflow suspended");

                    if let Err(e) = self
                        .ctx
                        .lifecycle
                        .workflow_suspended(
                            instance_id,
                            &metadata.namespace,
                            &metadata.name,
                            &metadata.version,
                            SuspensionData::wait(chrono::Utc::now()),
                        )
                        .await
                    {
                        error!(error = %e, "failed to emit workflow.suspended event");
                    }

                    return Ok(current_output);
                }
            }
        };

        let workflow_duration_us = workflow_start.elapsed().as_micros() as u64;

        // Write completion event (for both durability modes)
        self.write_completion(instance_id, workflow_id, &result.as_ref().unwrap_or(&current_output), durability)
            .await?;

        // Emit workflow.completed event
        if let Err(e) = self
            .ctx
            .lifecycle
            .workflow_completed(
                instance_id,
                &metadata.namespace,
                &metadata.name,
                &metadata.version,
                result.as_ref().ok().cloned(),
                Some(workflow_duration_us),
            )
            .await
        {
            error!(error = %e, "failed to emit workflow.completed event");
        }

        info!(instance_id = %instance_id, duration_us = workflow_duration_us, "workflow completed");
        result
    }

    /// Run matching before-extensions for a task
    async fn run_extensions_before(
        &self,
        extensions: &[ResolvedExtension],
        task_type: &str,
        task_input: &Value,
        task_scope: &Scope,
        instance_id: Uuid,
        position: &str,
    ) {
        for ext in extensions.iter().filter(|e| e.matches(task_type)) {
            if let Some(ref when_expr) = ext.when {
                match self.ctx.expr.eval(task_input, when_expr, task_scope) {
                    Ok(Value::Bool(true)) => {}
                    Ok(_) => continue,
                    Err(e) => {
                        warn!(extension = %ext.name, error = %e, "extension when-condition failed");
                        continue;
                    }
                }
            }

            for before_task in &ext.before {
                let mut ext_ctx = TaskContext::new(
                    task_input.clone(),
                    task_scope.clone(),
                    self.ctx.clone(),
                    instance_id,
                    &format!("ext:{}/{}", ext.name, before_task.name),
                    format!("{}/ext:before/{}", position, before_task.name),
                );
                if let Err(e) = before_task.executor.execute(&mut ext_ctx).await {
                    warn!(extension = %ext.name, task = %before_task.name, error = %e, "before-extension task failed");
                }
            }
        }
    }

    /// Run matching after-extensions for a task
    async fn run_extensions_after(
        &self,
        extensions: &[ResolvedExtension],
        task_type: &str,
        task_output: &Value,
        task_scope: &Scope,
        instance_id: Uuid,
        position: &str,
    ) {
        for ext in extensions.iter().filter(|e| e.matches(task_type)) {
            if let Some(ref when_expr) = ext.when {
                match self.ctx.expr.eval(task_output, when_expr, task_scope) {
                    Ok(Value::Bool(true)) => {}
                    Ok(_) => continue,
                    Err(e) => {
                        warn!(extension = %ext.name, error = %e, "extension when-condition failed");
                        continue;
                    }
                }
            }

            for after_task in &ext.after {
                let mut ext_ctx = TaskContext::new(
                    task_output.clone(),
                    task_scope.clone(),
                    self.ctx.clone(),
                    instance_id,
                    &format!("ext:{}/{}", ext.name, after_task.name),
                    format!("{}/ext:after/{}", position, after_task.name),
                );
                if let Err(e) = after_task.executor.execute(&mut ext_ctx).await {
                    warn!(extension = %ext.name, task = %after_task.name, error = %e, "after-extension task failed");
                }
            }
        }
    }

    /// Resume a workflow from a checkpoint
    #[instrument(skip(self, tasks), fields(instance_id = %instance_id))]
    pub async fn resume(
        &self,
        instance_id: Uuid,
        workflow_id: &str,
        tasks: &[TaskDefinition],
    ) -> FlowResult<Value> {
        let metadata = WorkflowMetadata::default();
        self.resume_with_metadata(instance_id, workflow_id, tasks, &metadata).await
    }

    /// Resume a workflow from a checkpoint with full metadata
    #[instrument(skip(self, tasks, metadata), fields(instance_id = %instance_id))]
    pub async fn resume_with_metadata(
        &self,
        instance_id: Uuid,
        workflow_id: &str,
        tasks: &[TaskDefinition],
        metadata: &WorkflowMetadata,
    ) -> FlowResult<Value> {
        info!("resuming workflow from checkpoint");

        // Emit workflow.resumed event
        if let Err(e) = self
            .ctx
            .lifecycle
            .workflow_resumed(
                instance_id,
                &metadata.namespace,
                &metadata.name,
                &metadata.version,
            )
            .await
        {
            error!(error = %e, "failed to emit workflow.resumed event");
        }

        // Read last checkpoint
        let checkpoint = self.read_last_checkpoint(instance_id).await?;

        // Find the task position to resume from
        let resume_position = &checkpoint.step_position;
        let resume_task = resume_position
            .strip_prefix("do/")
            .unwrap_or(resume_position);

        // Find task index
        let start_idx = tasks
            .iter()
            .position(|task| task.name == resume_task)
            .ok_or_else(|| FlowError::Recovery {
                instance_id: instance_id.to_string(),
                message: format!("task '{}' not found", resume_task),
            })?;

        // Resume execution from the next task
        let remaining_tasks = &tasks[start_idx + 1..];
        let input = checkpoint.output.unwrap_or(checkpoint.context.clone());

        // Use the stored context
        let scope = self.build_initial_scope(workflow_id, &instance_id, &input);

        info!(
            resume_from = resume_task,
            remaining = remaining_tasks.len(),
            "resuming execution"
        );

        // Continue with remaining tasks
        self.execute_from_with_metadata(
            instance_id,
            workflow_id,
            remaining_tasks,
            input,
            checkpoint.context,
            scope,
            checkpoint.sequence,
            FlowDurability::Checkpoint,
            metadata,
            &[],
        )
        .await
    }

    /// Internal: Execute from a specific point with existing state and metadata
    #[allow(clippy::too_many_arguments)]
    async fn execute_from_with_metadata(
        &self,
        instance_id: Uuid,
        workflow_id: &str,
        tasks: &[TaskDefinition],
        input: Value,
        mut context: Value,
        base_scope: Scope,
        mut sequence: u64,
        durability: FlowDurability,
        metadata: &WorkflowMetadata,
        extensions: &[ResolvedExtension],
    ) -> FlowResult<Value> {
        let workflow_start = Instant::now();
        let task_index: HashMap<&str, usize> = tasks
            .iter()
            .enumerate()
            .map(|(i, task)| (task.name.as_str(), i))
            .collect();

        let mut current_output = input;
        let mut task_idx = 0;

        while task_idx < tasks.len() {
            let task_def = &tasks[task_idx];
            let task_start = Instant::now();
            let task_type = task_def.executor.task_type();
            let position = format!("do/{}", task_def.name);

            // Emit task.created
            if let Err(e) = self.ctx.lifecycle.task_created(
                instance_id, &metadata.namespace, &metadata.name,
                &task_def.name, task_type, &position,
            ).await {
                error!(error = %e, "failed to emit task.created event");
            }

            // Check if condition
            if let Some(ref if_expr) = task_def.if_condition {
                let cond_scope = self.build_task_scope(
                    &base_scope, &task_def.name, &current_output, &context,
                    task_def.resource.as_ref(),
                );
                let cond = self.ctx.expr.eval(&current_output, if_expr, &cond_scope)?;
                if cond != Value::Bool(true) {
                    debug!(task = %task_def.name, "task skipped by if condition");
                    if let Err(e) = self.ctx.lifecycle.task_skipped(
                        instance_id, &metadata.namespace, &metadata.name,
                        &task_def.name, task_type, &position,
                    ).await {
                        error!(error = %e, "failed to emit task.skipped event");
                    }
                    task_idx += 1;
                    continue;
                }
            }

            // Apply input transformation if specified
            let task_input = if let Some(ref input_expr) = task_def.input_from {
                self.ctx.expr.eval(&current_output, input_expr, &base_scope)?
            } else {
                current_output.clone()
            };

            // Emit task.started event
            if let Err(e) = self
                .ctx
                .lifecycle
                .task_started(
                    instance_id,
                    &metadata.namespace,
                    &metadata.name,
                    &task_def.name,
                    task_type,
                    &position,
                    Some(task_input.clone()),
                )
                .await
            {
                error!(error = %e, "failed to emit task.started event");
            }

            let task_scope = self.build_task_scope(
                &base_scope,
                &task_def.name,
                &task_input,
                &context,
                task_def.resource.as_ref(),
            );
            let mut task_ctx = TaskContext::new(
                task_input.clone(),
                task_scope,
                self.ctx.clone(),
                instance_id,
                &task_def.name,
                position.clone(),
            );

            // Run before-extensions
            self.run_extensions_before(
                extensions, task_type, &task_input, &task_ctx.scope, instance_id, &position,
            ).await;

            // Execute with optional timeout
            let task_result = if let Some(timeout_dur) = task_def.timeout {
                match tokio::time::timeout(timeout_dur, task_def.executor.execute(&mut task_ctx)).await {
                    Ok(result) => result,
                    Err(_) => Err(FlowError::task_failed(
                        &task_def.name,
                        format!("task exceeded timeout of {:?}", timeout_dur),
                    )),
                }
            } else {
                task_def.executor.execute(&mut task_ctx).await
            };

            let task_duration_us = task_start.elapsed().as_micros() as u64;

            let mut result = match task_result {
                Ok(r) => r,
                Err(e) => {
                    let error_data = ErrorData::new("task_error", e.to_string());
                    if let Err(emit_err) = self.ctx.lifecycle.task_faulted(
                        instance_id, &metadata.namespace, &metadata.name,
                        &task_def.name, task_type, &position, error_data.clone(),
                    ).await {
                        error!(error = %emit_err, "failed to emit task.faulted event");
                    }

                    if let Err(emit_err) = self.ctx.lifecycle.workflow_faulted(
                        instance_id, &metadata.namespace, &metadata.name,
                        &metadata.version, error_data,
                    ).await {
                        error!(error = %emit_err, "failed to emit workflow.faulted event");
                    }

                    return Err(e);
                }
            };

            // Run after-extensions
            self.run_extensions_after(
                extensions, task_type, &result.output, &task_ctx.scope, instance_id, &position,
            ).await;

            // Emit task.completed event
            if let Err(e) = self.ctx.lifecycle.task_completed(
                instance_id, &metadata.namespace, &metadata.name,
                &task_def.name, task_type, &position,
                Some(result.output.clone()), Some(task_duration_us),
            ).await {
                error!(error = %e, "failed to emit task.completed event");
            }

            // Apply output transformation if specified
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
                    sequence,
                ).with_output(result.output.clone());

                self.write_checkpoint(&checkpoint).await?;
            }

            // Apply export transformation if specified
            let exports = if let Some(ref export_expr) = task_def.export_as {
                Some(self.ctx.expr.eval(&result.output, export_expr, &task_ctx.scope)?)
            } else {
                result.exports
            };

            if let Some(exports) = exports {
                merge_json(&mut context, exports);
            }

            current_output = result.output;

            // Determine flow directive
            let directive = if let Some(ref then) = task_def.then {
                FlowDirective::from(then.as_str())
            } else {
                result.directive
            };

            match directive {
                FlowDirective::Continue => task_idx += 1,
                FlowDirective::Exit | FlowDirective::End => break,
                FlowDirective::Goto(ref target) => {
                    if let Some(&idx) = task_index.get(target.as_str()) {
                        task_idx = idx;
                    } else {
                        let error_data = ErrorData::new("invalid_goto", format!("target '{}' not found", target));
                        if let Err(e) = self.ctx.lifecycle.workflow_faulted(
                            instance_id, &metadata.namespace, &metadata.name,
                            &metadata.version, error_data,
                        ).await {
                            error!(error = %e, "failed to emit workflow.faulted event");
                        }
                        return Err(FlowError::invalid_goto(target));
                    }
                }
                FlowDirective::Suspend => {
                    if let Err(e) = self.ctx.lifecycle.workflow_suspended(
                        instance_id, &metadata.namespace, &metadata.name,
                        &metadata.version, SuspensionData::wait(Utc::now()),
                    ).await {
                        error!(error = %e, "failed to emit workflow.suspended event");
                    }
                    return Ok(current_output);
                }
            }
        }

        let workflow_duration_us = workflow_start.elapsed().as_micros() as u64;

        self.write_completion(instance_id, workflow_id, &current_output, durability)
            .await?;

        if let Err(e) = self.ctx.lifecycle.workflow_completed(
            instance_id, &metadata.namespace, &metadata.name,
            &metadata.version, Some(current_output.clone()), Some(workflow_duration_us),
        ).await {
            error!(error = %e, "failed to emit workflow.completed event");
        }

        Ok(current_output)
    }

    /// Build initial scope for workflow
    fn build_initial_scope(&self, workflow_id: &str, instance_id: &Uuid, input: &Value) -> Scope {
        Scope::builder()
            .runtime(json!({
                "name": "flowr",
                "version": env!("CARGO_PKG_VERSION")
            }))
            .workflow(json!({
                "id": instance_id.to_string(),
                "definition": workflow_id,
                "input": input
            }))
            .task(Value::Null)
            .context(Value::Object(serde_json::Map::new()))
            .build()
    }

    /// Build scope for a specific task
    fn build_task_scope(
        &self,
        base: &Scope,
        task_name: &str,
        input: &Value,
        context: &Value,
        resource: Option<&Value>,
    ) -> Scope {
        let mut builder = Scope::builder()
            .runtime(base.runtime.clone())
            .workflow(base.workflow.clone())
            .task(json!({
                "name": task_name,
                "input": input
            }))
            .context(context.clone())
            .secrets(base.secrets.clone());

        if let Some(res) = resource {
            builder = builder.resource(res.clone());
        }

        builder.build()
    }

    /// Write checkpoint to Iggy
    async fn write_checkpoint(&self, checkpoint: &Checkpoint) -> FlowResult<()> {
        debug!(
            instance_id = %checkpoint.instance_id,
            position = checkpoint.step_position,
            sequence = checkpoint.sequence,
            "writing checkpoint"
        );

        let payload = checkpoint.to_bytes()?;
        let message = Message::from_str(&String::from_utf8_lossy(&payload))
            .map_err(|e| FlowError::Iggy(format!("failed to create message: {}", e)))?;

        let stream_id = Identifier::from_str(FLOWR_STREAM)
            .map_err(|e| FlowError::Iggy(format!("invalid stream id: {}", e)))?;
        let topic_id = Identifier::from_str(CHECKPOINT_TOPIC)
            .map_err(|e| FlowError::Iggy(format!("invalid topic id: {}", e)))?;

        let partition_id = self.partition_for_instance(&checkpoint.instance_id);

        self.ctx
            .iggy
            .send_messages(
                &stream_id,
                &topic_id,
                &Partitioning::partition_id(partition_id),
                &mut [message],
            )
            .await
            .map_err(|e| FlowError::CheckpointWrite(format!("iggy send failed: {}", e)))?;

        let index = CheckpointIndex {
            instance_id: checkpoint.instance_id,
            last_position: checkpoint.step_position.clone(),
            last_sequence: checkpoint.sequence,
            partition_id,
            last_offset: 0,
        };

        let index_key = format!("{}{}", CHECKPOINT_INDEX_PREFIX, checkpoint.instance_id);
        let index_json = serde_json::to_string(&index)?;

        let mut redis = self.ctx.redis.clone();
        redis
            .set::<_, _, ()>(&index_key, index_json)
            .await
            .map_err(|e| FlowError::Dragonfly(format!("index write failed: {}", e)))?;

        info!(
            instance_id = %checkpoint.instance_id,
            position = checkpoint.step_position,
            "checkpoint written"
        );

        Ok(())
    }

    /// Read last checkpoint from Iggy
    async fn read_last_checkpoint(&self, instance_id: Uuid) -> FlowResult<Checkpoint> {
        debug!(instance_id = %instance_id, "reading last checkpoint");

        let index_key = format!("{}{}", CHECKPOINT_INDEX_PREFIX, instance_id);
        let mut redis = self.ctx.redis.clone();

        let index_json: Option<String> = redis
            .get(&index_key)
            .await
            .map_err(|e| FlowError::Dragonfly(format!("index read failed: {}", e)))?;

        let partition_id = if let Some(json) = index_json {
            let index: CheckpointIndex = serde_json::from_str(&json)?;
            index.partition_id
        } else {
            self.partition_for_instance(&instance_id)
        };

        let stream_id = Identifier::from_str(FLOWR_STREAM)
            .map_err(|e| FlowError::Iggy(format!("invalid stream id: {}", e)))?;
        let topic_id = Identifier::from_str(CHECKPOINT_TOPIC)
            .map_err(|e| FlowError::Iggy(format!("invalid topic id: {}", e)))?;
        let consumer = Consumer::new(Identifier::numeric(instance_id.as_u128() as u32)
            .map_err(|e| FlowError::Iggy(format!("invalid consumer id: {}", e)))?);

        let polled = self
            .ctx
            .iggy
            .poll_messages(
                &stream_id,
                &topic_id,
                Some(partition_id),
                &consumer,
                &PollingStrategy::last(),
                100,
                false,
            )
            .await
            .map_err(|e| FlowError::CheckpointRead(format!("iggy poll failed: {}", e)))?;

        let mut latest_checkpoint: Option<Checkpoint> = None;

        for msg in polled.messages.iter().rev() {
            match Checkpoint::from_bytes(&msg.payload) {
                Ok(cp) if cp.instance_id == instance_id => {
                    if latest_checkpoint
                        .as_ref()
                        .map(|c| cp.sequence > c.sequence)
                        .unwrap_or(true)
                    {
                        latest_checkpoint = Some(cp);
                    }
                }
                Ok(_) => continue,
                Err(e) => {
                    warn!("failed to deserialize checkpoint: {}", e);
                    continue;
                }
            }
        }

        latest_checkpoint.ok_or_else(|| {
            FlowError::CheckpointRead(format!("no checkpoint found for instance {}", instance_id))
        })
    }

    /// Write completion event to Iggy
    async fn write_completion(
        &self,
        instance_id: Uuid,
        workflow_id: &str,
        output: &Value,
        durability: FlowDurability,
    ) -> FlowResult<()> {
        debug!(
            instance_id = %instance_id,
            durability = ?durability,
            "writing completion"
        );

        let completion = json!({
            "type": "workflow.completed",
            "instance_id": instance_id.to_string(),
            "workflow_id": workflow_id,
            "output": output,
            "timestamp": chrono::Utc::now().to_rfc3339(),
            "durability": format!("{:?}", durability),
        });

        let payload = serde_json::to_string(&completion)?;
        let message = Message::from_str(&payload)
            .map_err(|e| FlowError::Iggy(format!("failed to create message: {}", e)))?;

        let stream_id = Identifier::from_str(FLOWR_STREAM)
            .map_err(|e| FlowError::Iggy(format!("invalid stream id: {}", e)))?;
        let topic_id = Identifier::from_str(COMPLETION_TOPIC)
            .map_err(|e| FlowError::Iggy(format!("invalid topic id: {}", e)))?;

        let partition_id = self.partition_for_instance(&instance_id);

        self.ctx
            .iggy
            .send_messages(
                &stream_id,
                &topic_id,
                &Partitioning::partition_id(partition_id),
                &mut [message],
            )
            .await
            .map_err(|e| FlowError::Iggy(format!("completion write failed: {}", e)))?;

        let index_key = format!("{}{}", CHECKPOINT_INDEX_PREFIX, instance_id);
        let mut redis = self.ctx.redis.clone();
        let _: Option<()> = redis.del(&index_key).await.ok();

        info!(
            instance_id = %instance_id,
            workflow_id = workflow_id,
            "workflow completed"
        );

        Ok(())
    }

    /// Calculate partition ID for an instance
    fn partition_for_instance(&self, instance_id: &Uuid) -> u32 {
        const PARTITION_COUNT: u32 = 8;
        let bytes = instance_id.as_bytes();
        let hash = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
        (hash % PARTITION_COUNT) + 1
    }
}

/// Merge source JSON object into target
fn merge_json(target: &mut Value, source: Value) {
    if let (Value::Object(t), Value::Object(s)) = (target, source) {
        for (k, v) in s {
            t.insert(k, v);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_merge_json() {
        let mut target = json!({"a": 1, "b": 2});
        let source = json!({"b": 3, "c": 4});
        merge_json(&mut target, source);
        assert_eq!(target, json!({"a": 1, "b": 3, "c": 4}));
    }

    #[test]
    fn test_flow_directive_from_str() {
        assert_eq!(FlowDirective::from("continue"), FlowDirective::Continue);
        assert_eq!(FlowDirective::from("exit"), FlowDirective::Exit);
        assert_eq!(FlowDirective::from("end"), FlowDirective::End);
        assert_eq!(
            FlowDirective::from("step2"),
            FlowDirective::Goto("step2".to_string())
        );
    }
}
