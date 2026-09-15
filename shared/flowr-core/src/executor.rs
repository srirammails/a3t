//! Workflow executor for ClawdFlow
//!
//! Simple, single-process workflow execution with optional checkpointing.
//! Supports CNCF Serverless Workflow extensions (before/after interceptors),
//! task `if` conditions, and task-level timeouts.

use std::collections::HashMap;
use std::time::Instant;

use serde_json::{json, Value};
use tracing::{debug, error, info, instrument, warn};
use uuid::Uuid;

use flowr_expr::Scope;

use crate::context::{FlowContext, TaskContext};
use crate::directive::FlowDirective;
use crate::task::{ResolvedExtension, TaskDefinition};
use flowr_types::{Checkpoint, FlowDurability, FlowError, FlowResult};

/// Validate a value against a JSON Schema. Returns Ok(()) or a validation error.
fn validate_schema(value: &Value, schema: &Value, location: &str) -> FlowResult<()> {
    let validator = jsonschema::validator_for(schema).map_err(|e| {
        FlowError::Internal(format!("Invalid JSON Schema at {}: {}", location, e))
    })?;

    if !validator.is_valid(value) {
        // Collect error details via iter
        let errors: Vec<String> = validator
            .iter_errors(value)
            .map(|e| format!("{} (at {})", e, e.instance_path))
            .collect();
        return Err(FlowError::validation(
            location,
            format!("Schema validation failed: {}", errors.join("; ")),
        ));
    }
    Ok(())
}

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

    /// Execute a workflow (backward compatible, no extensions)
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

    /// Execute a workflow with full metadata (backward compatible, no extensions)
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
        self.execute_full(workflow_id, tasks, input, durability, metadata, extensions, None, None).await
    }

    /// Execute a workflow with extensions and workflow-level schemas
    #[instrument(skip(self, tasks, input, metadata, extensions, input_schema, output_schema), fields(workflow_id = %workflow_id))]
    pub async fn execute_full(
        &self,
        workflow_id: &str,
        tasks: &[TaskDefinition],
        input: Value,
        durability: FlowDurability,
        metadata: &WorkflowMetadata,
        extensions: &[ResolvedExtension],
        input_schema: Option<&Value>,
        output_schema: Option<&Value>,
    ) -> FlowResult<Value> {
        let instance_id = Uuid::now_v7();
        let workflow_start = Instant::now();

        info!(instance_id = %instance_id, "starting workflow execution");

        // ── Validate workflow input ────────────────────────────────
        if let Some(schema) = input_schema {
            validate_schema(&input, schema, &format!("{}/input", workflow_id))?;
        }

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

            // Check if condition — skip task if evaluates to false
            if let Some(ref if_expr) = task_def.if_condition {
                let task_scope = self.build_task_scope(
                    &scope,
                    &task_def.name,
                    &current_output,
                    &context,
                    task_def.resource.as_ref(),
                );
                let cond = self.ctx.expr.eval(&current_output, if_expr, &task_scope)?;
                if cond != Value::Bool(true) {
                    debug!(task = %task_def.name, "task skipped by if condition");
                    // Passthrough: output = input, no exports, no directive change
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

            // ── Validate task input ────────────────────────────────
            if let Some(ref schema) = task_def.input_schema {
                validate_schema(&task_input, schema, &format!("{}/input", task_def.name))?;
            }

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
                task_input.clone(),
                task_scope,
                self.ctx.clone(),
                instance_id,
                &task_def.name,
                position.clone(),
            );

            // Run matching before-extensions
            if let Some(directive) = self.run_extensions_before(
                extensions,
                task_type,
                &task_input,
                &task_ctx.scope,
                &self.ctx,
                instance_id,
                &position,
            )
            .await
            {
                match directive {
                    FlowDirective::Exit => {
                        debug!(task = %task_def.name, "task skipped by before-extension exit");
                        task_idx += 1;
                        continue;
                    }
                    FlowDirective::End => {
                        debug!(task = %task_def.name, "workflow ended by before-extension");
                        break Ok(current_output.clone());
                    }
                    _ => {}
                }
            }

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
            debug!(task = %task_def.name, duration_us = task_duration_us, "task completed");

            // Handle task execution result
            let mut result = match task_result {
                Ok(r) => r,
                Err(e) => {
                    error!(task = %task_def.name, error = %e, "task failed");
                    return Err(e);
                }
            };

            // Run matching after-extensions
            self.run_extensions_after(
                extensions,
                task_type,
                &result.output,
                &task_ctx.scope,
                &self.ctx,
                instance_id,
                &position,
            )
            .await;

            // Apply output transformation if specified
            if let Some(ref output_expr) = task_def.output_as {
                result.output =
                    self.ctx
                        .expr
                        .eval(&result.output, output_expr, &task_ctx.scope)?;
            }

            // ── Validate task output ───────────────────────────────
            if let Some(ref schema) = task_def.output_schema {
                validate_schema(&result.output, schema, &format!("{}/output", task_def.name))?;
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

            // Apply export transformation if specified, otherwise use result.exports
            let exports = if let Some(ref export_expr) = task_def.export_as {
                Some(self.ctx.expr.eval(
                    &result.output,
                    export_expr,
                    &task_ctx.scope,
                )?)
            } else {
                result.exports
            };

            // ── Validate exported context ──────────────────────────
            if let Some(ref schema) = task_def.export_schema {
                if let Some(ref export_val) = exports {
                    validate_schema(export_val, schema, &format!("{}/export", task_def.name))?;
                }
            }

            // Merge exports into context
            if let Some(exports) = exports {
                merge_json(&mut context, exports);
            }

            // Update current output
            current_output = result.output;

            // Determine flow directive (task definition's `then` overrides result directive)
            let directive = if let Some(ref then) = task_def.then {
                FlowDirective::from(then.as_str())
            } else {
                result.directive
            };

            // Handle directive
            match directive {
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

        // ── Validate workflow output ───────────────────────────────
        if let (Ok(ref output), Some(schema)) = (&result, output_schema) {
            validate_schema(output, schema, &format!("{}/output", workflow_id))?;
        }

        result
    }

    /// Run matching before-extensions for a task.
    /// Returns Some(directive) if a before-task requests exit/end (skipping the main task).
    async fn run_extensions_before(
        &self,
        extensions: &[ResolvedExtension],
        task_type: &str,
        task_input: &Value,
        task_scope: &Scope,
        flow_ctx: &FlowContext,
        instance_id: Uuid,
        position: &str,
    ) -> Option<FlowDirective> {
        for ext in extensions.iter().filter(|e| e.matches(task_type)) {
            // Check when condition
            if let Some(ref when_expr) = ext.when {
                match flow_ctx.expr.eval(task_input, when_expr, task_scope) {
                    Ok(Value::Bool(true)) => {}
                    Ok(_) => continue,
                    Err(e) => {
                        warn!(extension = %ext.name, error = %e, "extension when-condition failed");
                        continue;
                    }
                }
            }

            // Execute before-tasks
            for before_task in &ext.before {
                let mut ext_ctx = TaskContext::new(
                    task_input.clone(),
                    task_scope.clone(),
                    flow_ctx.clone(),
                    instance_id,
                    &format!("ext:{}/{}", ext.name, before_task.name),
                    format!("{}/ext:before/{}", position, before_task.name),
                );
                match before_task.executor.execute(&mut ext_ctx).await {
                    Ok(result) => {
                        // Check the before-task's `then` override, or use the result directive
                        let directive = if let Some(ref then) = before_task.then {
                            FlowDirective::from(then.as_str())
                        } else {
                            result.directive
                        };
                        // If exit → skip the extended task; if end → terminate workflow
                        if directive.is_exit() || directive.is_terminal() {
                            debug!(
                                extension = %ext.name,
                                task = %before_task.name,
                                directive = ?directive,
                                "before-extension requested skip"
                            );
                            return Some(directive);
                        }
                    }
                    Err(e) => {
                        warn!(extension = %ext.name, task = %before_task.name, error = %e, "before-extension task failed");
                    }
                }
            }
        }
        None
    }

    /// Run matching after-extensions for a task
    async fn run_extensions_after(
        &self,
        extensions: &[ResolvedExtension],
        task_type: &str,
        task_output: &Value,
        task_scope: &Scope,
        flow_ctx: &FlowContext,
        instance_id: Uuid,
        position: &str,
    ) {
        for ext in extensions.iter().filter(|e| e.matches(task_type)) {
            // Check when condition
            if let Some(ref when_expr) = ext.when {
                match flow_ctx.expr.eval(task_output, when_expr, task_scope) {
                    Ok(Value::Bool(true)) => {}
                    Ok(_) => continue,
                    Err(e) => {
                        warn!(extension = %ext.name, error = %e, "extension when-condition failed");
                        continue;
                    }
                }
            }

            // Execute after-tasks
            for after_task in &ext.after {
                let mut ext_ctx = TaskContext::new(
                    task_output.clone(),
                    task_scope.clone(),
                    flow_ctx.clone(),
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
            "reference": format!("/do/{}", task_name),
            "input": task_input,
        });

        if let Some(res) = resource {
            task_data["resource"] = res.clone();
        }

        let mut builder = Scope::builder()
            .runtime(base.runtime.clone())
            .workflow(base.workflow.clone())
            .task(task_data)
            .context(context.clone())
            .secrets(base.secrets.clone());

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
        self.resume_with_extensions(instance_id, workflow_id, tasks, durability, &[])
            .await
    }

    /// Resume a workflow from a checkpoint with extensions
    #[instrument(skip(self, tasks, extensions), fields(instance_id = %instance_id))]
    pub async fn resume_with_extensions(
        &self,
        instance_id: Uuid,
        workflow_id: &str,
        tasks: &[TaskDefinition],
        durability: FlowDurability,
        extensions: &[ResolvedExtension],
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

        let metadata = WorkflowMetadata::default();
        let task_index: HashMap<&str, usize> = tasks
            .iter()
            .enumerate()
            .map(|(i, task)| (task.name.as_str(), i))
            .collect();

        let resume_idx = checkpoint.task_index + 1;

        if resume_idx >= tasks.len() {
            return Ok(checkpoint.output);
        }

        let scope =
            self.build_initial_scope(workflow_id, &instance_id, &checkpoint.output, &metadata);
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

            // Check if condition
            if let Some(ref if_expr) = task_def.if_condition {
                let task_scope = self.build_task_scope(
                    &scope,
                    &task_def.name,
                    &current_output,
                    &context,
                    task_def.resource.as_ref(),
                );
                let cond = self.ctx.expr.eval(&current_output, if_expr, &task_scope)?;
                if cond != Value::Bool(true) {
                    debug!(task = %task_def.name, "task skipped by if condition");
                    task_idx += 1;
                    continue;
                }
            }

            let task_input = if let Some(ref input_expr) = task_def.input_from {
                self.ctx.expr.eval(&current_output, input_expr, &scope)?
            } else {
                current_output.clone()
            };

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
                task_input.clone(),
                task_scope,
                self.ctx.clone(),
                instance_id,
                &task_def.name,
                position.clone(),
            );

            // Run before-extensions
            if let Some(directive) = self.run_extensions_before(
                extensions,
                task_type,
                &task_input,
                &task_ctx.scope,
                &self.ctx,
                instance_id,
                &position,
            )
            .await
            {
                match directive {
                    FlowDirective::Exit => {
                        debug!(task = %task_def.name, "task skipped by before-extension exit");
                        task_idx += 1;
                        continue;
                    }
                    FlowDirective::End => {
                        debug!(task = %task_def.name, "workflow ended by before-extension");
                        break Ok(current_output.clone());
                    }
                    _ => {}
                }
            }

            // Execute with optional timeout
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
            debug!(task = %task_def.name, duration_us = task_duration_us, "task completed");

            let mut result = task_result?;

            // Run after-extensions
            self.run_extensions_after(
                extensions,
                task_type,
                &result.output,
                &task_ctx.scope,
                &self.ctx,
                instance_id,
                &position,
            )
            .await;

            if let Some(ref output_expr) = task_def.output_as {
                result.output =
                    self.ctx
                        .expr
                        .eval(&result.output, output_expr, &task_ctx.scope)?;
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

            let exports = if let Some(ref export_expr) = task_def.export_as {
                Some(self.ctx.expr.eval(
                    &result.output,
                    export_expr,
                    &task_ctx.scope,
                )?)
            } else {
                result.exports
            };

            if let Some(exports) = exports {
                merge_json(&mut context, exports);
            }

            current_output = result.output;

            let directive = if let Some(ref then) = task_def.then {
                FlowDirective::from(then.as_str())
            } else {
                result.directive
            };

            match directive {
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

/// Merge source JSON object into target
fn merge_json(target: &mut Value, source: Value) {
    if let (Value::Object(t), Value::Object(s)) = (target, source) {
        for (k, v) in s {
            t.insert(k, v);
        }
    }
}
