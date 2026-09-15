//! Generic workflow executor
//!
//! Walks a `WorkflowDefinition`'s task tree and dispatches to registered `TaskHandler`s.
//! Handles the full CNCF data flow pipeline, extensions, and flow control.
//!
//! This is NOT a bridge/adapter to the old executor — it operates directly on
//! `swf-parser` types (`NamedTask`, `Task`).

mod data_flow;
pub mod extensions;

use std::collections::HashMap;
use std::sync::Arc;

use async_recursion::async_recursion;
use serde_json::Value;
use tracing::{debug, info, warn, Instrument};

use swf_parser::{NamedTask, Task, WorkflowDefinition};
use swf_parser::flow_control::{DoTask, ForTask, SwitchTask, TryTask, ForkTask};
use swf_parser::directive::FlowDirective as ParsedDirective;

use crate::context::ExecutionContext;
use crate::error::{RuntimeError, Result};
use crate::traits::{FlowDirective, TaskHandler, TaskResult};

/// Generic workflow executor — dispatches tasks to registered handlers
pub struct WorkflowExecutor {
    handlers: HashMap<String, Arc<dyn TaskHandler>>,
    /// Expression evaluator function
    eval_fn: Option<Arc<dyn Fn(&Value, &str, &Value) -> std::result::Result<Value, String> + Send + Sync>>,
}

/// Compose a subflow's `a3t` template over the inherited (parent) one,
/// **attenuate-only** (E2 #4): the capability sets `exposedTools`/`enabledTools`
/// INTERSECT, so a subflow can only narrow them; a set the subflow omits is
/// inherited unchanged (it cannot widen by staying silent). Other fields take the
/// subflow's value, falling back to the parent's. With no inherited template this
/// is just the subflow's own template (root-flow behavior).
fn compose_template(inherited: Option<Value>, current: Value) -> Value {
    let Some(parent) = inherited.as_ref().and_then(|v| v.as_object()) else {
        return current;
    };
    let mut result = current.as_object().cloned().unwrap_or_default();
    for key in ["exposedTools", "enabledTools"] {
        let cur = result.get(key).and_then(|v| v.as_array()).cloned();
        let par = parent.get(key).and_then(|v| v.as_array()).cloned();
        match (cur, par) {
            (Some(cur), Some(par)) => {
                let inter: Vec<Value> = cur.into_iter().filter(|x| par.contains(x)).collect();
                result.insert(key.to_string(), Value::Array(inter));
            }
            (None, Some(par)) => {
                result.insert(key.to_string(), Value::Array(par));
            }
            _ => {}
        }
    }
    // Inherit any parent field the subflow didn't set (e.g. policyRef, tags).
    for (k, v) in parent {
        result.entry(k.clone()).or_insert_with(|| v.clone());
    }
    Value::Object(result)
}

impl WorkflowExecutor {
    pub fn new() -> Self {
        Self {
            handlers: HashMap::new(),
            eval_fn: None,
        }
    }

    /// Register a task handler
    pub fn register_handler(&mut self, handler: Arc<dyn TaskHandler>) {
        let name = handler.task_type().to_string();
        self.handlers.insert(name, handler);
    }

    /// Set the expression evaluator (jq)
    pub fn set_eval_fn<F>(&mut self, f: F)
    where
        F: Fn(&Value, &str, &Value) -> std::result::Result<Value, String> + Send + Sync + 'static,
    {
        self.eval_fn = Some(Arc::new(f));
    }

    /// Execute a workflow
    pub async fn execute(
        &self,
        workflow: &WorkflowDefinition,
        input: Value,
        ctx: &mut ExecutionContext,
    ) -> Result<Value> {
        let workflow_id = format!("{}/{}", workflow.document.namespace, workflow.document.name);
        info!(workflow = %workflow_id, tasks = workflow.tasks.len(), "executing workflow");

        // Validate workflow input schema
        if let Some(ref input_def) = workflow.input {
            if let Some(ref schema) = input_def.schema {
                data_flow::validate_schema(&input, schema, &format!("{}/input", workflow_id))?;
            }
        }

        // Transform workflow input
        let mut current = if let Some(ref input_def) = workflow.input {
            if let Some(ref from_expr) = input_def.from {
                self.eval_expr(&input, from_expr, &ctx.context)?
            } else {
                input.clone()
            }
        } else {
            input.clone()
        };

        // Initialize context
        ctx.context = Value::Object(serde_json::Map::new());

        // E2 #2a/#4: lift the flow's `metadata.a3t` (exposedTools, runtimePolicy, …)
        // into template attributes, exposed to policy as `principal.template.*`.
        // When this flow is a *subflow*, `ctx.template_meta` already carries the
        // parent's template (inherited at the child-context edge); compose
        // attenuate-only so a subflow can narrow the capability surface but never
        // widen it. If the subflow declares no `a3t`, it keeps the inherited one.
        if let Some(a3t) = workflow.metadata.as_ref().and_then(|m| m.get("a3t")) {
            let inherited = ctx.template_meta.get("template").cloned();
            let composed = compose_template(inherited, a3t.clone());
            ctx.template_meta.insert("template".to_string(), composed);
        }

        // Execute task list under the aesop `flow_run` span (E0.5). Every
        // `flow_step` and `tool_call` span nests beneath this root, so the whole
        // run — and every policy decision in it — shares one run_id/flow_id.
        let flow_run_span = tracing::info_span!(
            "flow_run",
            "aesop.flow_id" = %workflow_id,
            "aesop.run_id" = %ctx.instance_id,
            "aesop.principal.kind" = ctx.principal.kind(),
            "aesop.principal.id" = %ctx.principal.id(),
        );
        current = self
            .execute_task_list(&workflow.tasks, current, ctx, &workflow_id)
            .instrument(flow_run_span)
            .await?;

        // Transform workflow output
        if let Some(ref output_def) = workflow.output {
            if let Some(ref as_expr) = output_def.transform {
                current = self.eval_expr(&current, as_expr, &ctx.context)?;
            }
            if let Some(ref schema) = output_def.schema {
                data_flow::validate_schema(&current, schema, &format!("{}/output", workflow_id))?;
            }
        }

        info!(workflow = %workflow_id, "workflow completed");
        Ok(current)
    }

    /// Execute an ordered list of named tasks
    #[async_recursion]
    async fn execute_task_list(
        &self,
        tasks: &[NamedTask],
        input: Value,
        ctx: &mut ExecutionContext,
        scope_path: &str,
    ) -> Result<Value> {
        let mut current = input;
        let mut task_idx = 0;

        // Build name→index map for goto
        let task_index: HashMap<&str, usize> = tasks.iter()
            .enumerate()
            .map(|(i, t)| (t.name.as_str(), i))
            .collect();

        while task_idx < tasks.len() {
            let named_task = &tasks[task_idx];
            let task_path = format!("{}/{}", scope_path, named_task.name);

            debug!(task = %named_task.name, idx = task_idx, "executing task");

            // Debug: log context _messages type
            if let Some(msgs) = ctx.context.get("_messages") {
                debug!(task = %named_task.name, msgs_type = %msgs.is_array(), msgs_is_str = %msgs.is_string(), "context._messages check");
                if msgs.is_string() {
                    warn!(task = %named_task.name, msgs_val = %msgs, "BUG: _messages is string, should be array!");
                }
            }

            // Check `if` condition
            if let Some(ref condition) = named_task.condition {
                let cond_result = self.eval_expr(&current, condition, &ctx.context)?;
                if cond_result != Value::Bool(true) {
                    debug!(task = %named_task.name, "task skipped by if condition");
                    task_idx += 1;
                    continue;
                }
            }

            // Apply input.from transform
            let task_input = if let Some(ref input_def) = named_task.input {
                if let Some(ref from_expr) = input_def.from {
                    self.eval_expr(&current, from_expr, &ctx.context)?
                } else {
                    current.clone()
                }
            } else {
                current.clone()
            };

            // Validate input schema
            if let Some(ref input_def) = named_task.input {
                if let Some(ref schema) = input_def.schema {
                    data_flow::validate_schema(&task_input, schema, &format!("{}/input", task_path))?;
                }
            }

            // Determine task type for extension matching
            let task_type = extensions::task_type_specific(&named_task.task);

            // Run before-extensions
            if let Some(directive) = self.run_extensions_before(ctx, &named_task.name, &task_type, &task_input).await? {
                match directive {
                    FlowDirective::Exit => {
                        debug!(task = %named_task.name, "task skipped by before-extension");
                        task_idx += 1;
                        continue;
                    }
                    FlowDirective::End => {
                        debug!(task = %named_task.name, "workflow ended by before-extension");
                        break;
                    }
                    _ => {}
                }
            }

            // Execute the task under an aesop `flow_step` span (E0.5). The
            // `tool_call` span at the policy choke point nests beneath this.
            let step_span = tracing::info_span!(
                "flow_step",
                "aesop.flow_id" = %format!("{}/{}", ctx.namespace, ctx.name),
                "aesop.run_id" = %ctx.instance_id,
                "aesop.step" = task_idx,
                "aesop.resource.id" = %named_task.name,
            );
            let result = self
                .execute_single_task(named_task, task_input.clone(), ctx, &task_path)
                .instrument(step_span)
                .await?;

            // Run after-extensions
            self.run_extensions_after(ctx, &named_task.name, &task_type, &result.output).await;

            // Debug: trace output before export
            if let Some(msgs) = result.output.get("_messages") {
                debug!(task = %named_task.name, msgs_type = %msgs.is_array(), msgs_str = %msgs.is_string(), "OUTPUT._messages before export");
            }

            // Apply output.as transform
            let mut output = result.output;
            if let Some(ref output_def) = named_task.output {
                if let Some(ref as_expr) = output_def.transform {
                    output = self.eval_expr(&output, as_expr, &ctx.context)?;
                }
                if let Some(ref schema) = output_def.schema {
                    data_flow::validate_schema(&output, schema, &format!("{}/output", task_path))?;
                }
            }

            // Apply export.as → merge into context
            if let Some(ref export_def) = named_task.export {
                if let Some(ref as_expr) = export_def.transform {
                    let exported = self.eval_expr(&output, as_expr, &ctx.context)?;
                    debug!(task = %named_task.name, export_expr = %as_expr,
                        exported_type = %if exported.is_object() { "object" } else if exported.is_string() { "STRING!" } else { "other" },
                        "export.as evaluated");
                    if let Some(ref schema) = export_def.schema {
                        data_flow::validate_schema(&exported, schema, &format!("{}/export", task_path))?;
                    }
                    data_flow::merge_into_context(&mut ctx.context, exported);
                }
            } else if let Some(exports) = result.exports {
                data_flow::merge_into_context(&mut ctx.context, exports);
            }

            // Handle flow directive
            let directive = if let Some(ref then) = named_task.then {
                match then {
                    ParsedDirective::Named(s) => match s.as_str() {
                        "continue" => FlowDirective::Continue,
                        "end" => FlowDirective::End,
                        "exit" => FlowDirective::Exit,
                        name => FlowDirective::Goto(name.to_string()),
                    },
                }
            } else {
                result.directive
            };

            match directive {
                FlowDirective::Continue => {
                    current = output;
                    task_idx += 1;
                }
                FlowDirective::End => {
                    current = output;
                    break;
                }
                FlowDirective::Exit => {
                    current = output;
                    break;
                }
                FlowDirective::Goto(ref target) => {
                    if let Some(&idx) = task_index.get(target.as_str()) {
                        current = output;
                        task_idx = idx;
                    } else {
                        return Err(RuntimeError::TaskFailed {
                            task: named_task.name.clone(),
                            message: format!("goto target '{}' not found", target),
                        });
                    }
                }
                FlowDirective::Suspend => {
                    current = output;
                    break;
                }
            }
        }

        Ok(current)
    }

    /// Execute a single task by dispatching to the appropriate handler
    #[async_recursion]
    async fn execute_single_task(
        &self,
        named_task: &NamedTask,
        input: Value,
        ctx: &mut ExecutionContext,
        task_path: &str,
    ) -> Result<TaskResult> {
        match &named_task.task {
            // ── Built-in flow control (handled by executor, not handlers) ──

            Task::Set(set_task) => {
                let mut result = serde_json::Map::new();
                for (key, val) in &set_task.values {
                    let evaluated = self.eval_value_recursive(val, &input, &ctx.context)?;
                    result.insert(key.clone(), evaluated);
                }
                Ok(TaskResult::new(Value::Object(result)))
            }

            Task::Do(do_task) => {
                let output = self.execute_task_list(&do_task.tasks, input, ctx, task_path).await?;
                Ok(TaskResult::new(output))
            }

            Task::For(for_task) => {
                self.execute_for(for_task, input, ctx, task_path).await
            }

            Task::Switch(switch_task) => {
                self.execute_switch(switch_task, input, ctx, task_path).await
            }

            Task::Try(try_task) => {
                self.execute_try(try_task, input, ctx, task_path).await
            }

            Task::Fork(fork_task) => {
                self.execute_fork(fork_task, input, ctx, task_path).await
            }

            Task::Raise(raise_task) => {
                Err(RuntimeError::Raised {
                    error_type: raise_task.error.error_type.clone().unwrap_or_default(),
                    status: raise_task.error.status.unwrap_or(500),
                    title: raise_task.error.title.clone().unwrap_or_default(),
                    detail: raise_task.error.detail.clone().unwrap_or_default(),
                    instance: raise_task.error.instance.clone(),
                })
            }

            Task::Wait(wait_task) => {
                let secs = wait_task.seconds.unwrap_or(0)
                    + wait_task.minutes.unwrap_or(0) * 60
                    + wait_task.hours.unwrap_or(0) * 3600
                    + wait_task.days.unwrap_or(0) * 86400;
                if secs > 0 {
                    tokio::time::sleep(std::time::Duration::from_secs(secs)).await;
                }
                Ok(TaskResult::new(input))
            }

            // ── Delegate to registered handlers ──

            Task::Call(call_task) => {
                // Evaluate expressions in call.with before dispatching
                let mut resolved_task = named_task.clone();
                if let Task::Call(ref mut ct) = resolved_task.task {
                    if let Some(ref with_val) = ct.with {
                        ct.with = Some(self.eval_value_recursive(with_val, &input, &ctx.context)?);
                    }
                }

                let handler_name = match &call_task.target {
                    swf_parser::CallTarget::Http(_) => "call:http",
                    swf_parser::CallTarget::Grpc(_) => "call:grpc",
                    swf_parser::CallTarget::OpenApi(_) => "call:openapi",
                    swf_parser::CallTarget::AsyncApi(_) => "call:asyncapi",
                    swf_parser::CallTarget::A2A(_) => "call:a2a",
                    swf_parser::CallTarget::Mcp(_) => "call:mcp",
                    swf_parser::CallTarget::Function(name) => {
                        return self.dispatch_to_handler(&format!("call:{}", name), &resolved_task, input, ctx).await;
                    }
                };
                self.dispatch_to_handler(handler_name, &resolved_task, input, ctx).await
            }

            Task::Run(run_task) => {
                // Evaluate expressions in run task fields before dispatching
                let mut resolved_task = named_task.clone();
                if let Task::Run(ref mut rt) = resolved_task.task {
                    match rt {
                        swf_parser::RunTask::Workflow(ref mut wf) => {
                            if let Some(ref inp) = wf.input {
                                wf.input = Some(self.eval_value_recursive(inp, &input, &ctx.context)?);
                            }
                        }
                        swf_parser::RunTask::Shell(ref mut sh) => {
                            if sh.command.contains("${") {
                                if let Value::String(s) = self.eval_value_recursive(
                                    &Value::String(sh.command.clone()), &input, &ctx.context
                                )? {
                                    sh.command = s;
                                }
                            }
                            if let Some(ref args) = sh.arguments {
                                sh.arguments = Some(self.eval_value_recursive(args, &input, &ctx.context)?);
                            }
                        }
                        swf_parser::RunTask::Script(ref mut sc) => {
                            if let Some(ref args) = sc.arguments {
                                sc.arguments = Some(self.eval_value_recursive(args, &input, &ctx.context)?);
                            }
                        }
                        swf_parser::RunTask::Container(ref mut ct) => {
                            if let Some(ref env) = ct.environment {
                                ct.environment = Some(self.eval_value_recursive(env, &input, &ctx.context)?);
                            }
                        }
                    }
                }

                let handler_name = match run_task {
                    swf_parser::RunTask::Shell(_) => "run:shell",
                    swf_parser::RunTask::Script(_) => "run:script",
                    swf_parser::RunTask::Workflow(_) => "run:workflow",
                    swf_parser::RunTask::Container(_) => "run:container",
                };
                self.dispatch_to_handler(handler_name, &resolved_task, input, ctx).await
            }

            Task::Emit(_) => {
                self.dispatch_to_handler("emit", named_task, input, ctx).await
            }

            Task::Listen(_) => {
                self.dispatch_to_handler("listen", named_task, input, ctx).await
            }
        }
    }

    /// Dispatch to a registered handler by name
    async fn dispatch_to_handler(
        &self,
        handler_name: &str,
        named_task: &NamedTask,
        input: Value,
        ctx: &mut ExecutionContext,
    ) -> Result<TaskResult> {
        match self.handlers.get(handler_name) {
            Some(handler) => {
                // ── E0.2 policy choke point (site B: flow task-handler dispatch) ──
                // Every flow step passes here. AllowAll in CE; CedarHook in EE.
                // E0.4: the principal is the flow's real actor, threaded through
                // ExecutionContext by the flow-start call site (defaults to the
                // runtime service when the caller supplied no better identity).
                // Per-step action+resource: the handler names its real target (MCP
                // tool / subflow / LLM) and the matching action, so policy gates on
                // that rather than the bare task type. Defaults: `tool:invoke` on
                // `Tool{task_type}`.
                let step_action = handler.policy_action();
                // E2 #6: inline AESOP policy declared on the step itself
                // (`metadata.a3t.policy`), evaluated here because this is where
                // the step's expression context lives. Attenuate-only: it can
                // raise the operation bucket and add identity attributes for
                // policy to read, never lower the bucket, rename the entity, or
                // overwrite a handler-owned attribute. This is what lets *any*
                // `call:` step be policy-gated from YAML with no per-handler Rust.
                let step_resource = {
                    let base = handler.policy_resource(named_task);
                    match named_task
                        .metadata
                        .as_ref()
                        .and_then(|m| m.get("a3t"))
                        .and_then(|a| a.get("policy"))
                    {
                        Some(raw) => {
                            let evaluated =
                                self.eval_value_recursive(raw, &input, &ctx.context)?;
                            match flowr_policy::op_policy::StepPolicy::parse(&evaluated) {
                                Some(sp) => sp.apply(base),
                                None => base,
                            }
                        }
                        None => base,
                    }
                };
                // E0.5: the aesop `tool_call` span — this is where the policy
                // decision is made, so it carries the full aesop.* field set and
                // records `aesop.decision`. `aesop.task.source` reflects whether
                // the handler is a compiled-in builtin or a sandboxed WASM plugin,
                // so EE's Cedar policy + the audit trail can distinguish them.
                let span = tracing::info_span!(
                    "tool_call",
                    "aesop.principal.kind" = ctx.principal.kind(),
                    "aesop.principal.id" = %ctx.principal.id(),
                    "aesop.on_behalf_of" = ctx.principal.on_behalf_of().unwrap_or(""),
                    "aesop.action" = step_action.as_str(),
                    "aesop.resource.type" = %step_resource.type_name,
                    "aesop.resource.id" = %step_resource.id,
                    "aesop.flow_id" = %format!("{}/{}", ctx.namespace, ctx.name),
                    "aesop.run_id" = %ctx.instance_id,
                    "aesop.task.source" = handler.source(),
                    "aesop.decision" = tracing::field::Empty,
                );
                async {
                    let policy_req = flowr_policy::ExecutionRequest {
                        principal: ctx.principal.clone(),
                        action: step_action,
                        resource: step_resource,
                        // E2 #2a: the flow's metadata.a3t template data, so policy
                        // resources can gate this task on `principal.template.*`.
                        attributes: ctx.template_meta.clone(),
                        context: flowr_policy::ExecContext {
                            run_id: Some(ctx.instance_id.clone()),
                            flow_id: Some(format!("{}/{}", ctx.namespace, ctx.name)),
                            ..Default::default()
                        },
                        params: input.clone(),
                    };
                    let decision = flowr_policy::policy_hook().check(&policy_req).await;
                    tracing::Span::current().record("aesop.decision", decision.label());
                    flowr_policy::audit::record(&policy_req, &decision);
                    if let flowr_policy::PolicyDecision::Deny { reason, policy_ids } = decision {
                        return Err(RuntimeError::Denied {
                            task: named_task.name.clone(),
                            reason,
                            policy_ids,
                        });
                    }
                    handler.execute(named_task, input, ctx).await
                }
                .instrument(span)
                .await
            }
            None => Err(RuntimeError::TaskFailed {
                task: named_task.name.clone(),
                message: format!("no handler registered for task type '{}'", handler_name),
            }),
        }
    }

    /// Execute a for loop
    #[async_recursion]
    async fn execute_for(
        &self,
        for_task: &ForTask,
        input: Value,
        ctx: &mut ExecutionContext,
        task_path: &str,
    ) -> Result<TaskResult> {
        // Evaluate collection against context-merged input (exports are in context)
        let merged_input = data_flow::merge_context_into_input(&input, &ctx.context);
        let collection = self.eval_expr(&merged_input, &for_task.collection, &ctx.context)?;
        let items = collection.as_array().ok_or_else(|| RuntimeError::TaskFailed {
            task: task_path.to_string(),
            message: format!("'in' expression must evaluate to an array, got: {}", collection),
        })?;

        // Use context-merged input so for-body tasks can access exported state via `.`
        let mut current = merged_input;

        for (idx, item) in items.iter().enumerate() {
            // Check while condition
            if let Some(ref while_expr) = for_task.while_condition {
                let should_continue = self.eval_expr(&current, while_expr, &ctx.context)?;
                if should_continue != Value::Bool(true) {
                    debug!(iteration = idx, "for loop: while condition false, breaking");
                    break;
                }
            }

            // Inject iteration variable into context (both $name and name for flexible access)
            if let Value::Object(ref mut ctx_map) = ctx.context {
                ctx_map.insert(format!("${}", for_task.each), item.clone());
                ctx_map.insert(for_task.each.clone(), item.clone());
                if let Some(ref at) = for_task.at {
                    ctx_map.insert(format!("${}", at), Value::Number(idx.into()));
                    ctx_map.insert(at.clone(), Value::Number(idx.into()));
                }
            }

            // Rewrite loop variable references in task expressions before execution
            // $each_var → $context.each_var so jq can resolve them
            let mut rewritten_tasks = for_task.tasks.clone();
            for t in &mut rewritten_tasks {
                rewrite_loop_vars_in_task(t, &for_task.each, for_task.at.as_deref());
            }

            // Execute loop body
            let iter_path = format!("{}/for[{}]", task_path, idx);
            current = self.execute_task_list(&rewritten_tasks, current.clone(), ctx, &iter_path).await?;
        }

        Ok(TaskResult::new(current))
    }

    /// Execute a switch
    async fn execute_switch(
        &self,
        switch_task: &SwitchTask,
        input: Value,
        ctx: &mut ExecutionContext,
        _task_path: &str,
    ) -> Result<TaskResult> {
        for case in &switch_task.cases {
            let matches = if let Some(ref when_expr) = case.when {
                self.eval_expr(&input, when_expr, &ctx.context)? == Value::Bool(true)
            } else {
                case.default
            };

            if matches {
                let directive = case.then.as_ref().map(|d| {
                    match d {
                        ParsedDirective::Named(s) => match s.as_str() {
                            "continue" => FlowDirective::Continue,
                            "end" => FlowDirective::End,
                            "exit" => FlowDirective::Exit,
                            name => FlowDirective::Goto(name.to_string()),
                        },
                    }
                }).unwrap_or(FlowDirective::Continue);

                return Ok(TaskResult::new(input).with_directive(directive));
            }
        }

        // No case matched
        Ok(TaskResult::new(input))
    }

    /// Execute a try/catch
    #[async_recursion]
    async fn execute_try(
        &self,
        try_task: &TryTask,
        input: Value,
        ctx: &mut ExecutionContext,
        task_path: &str,
    ) -> Result<TaskResult> {
        match self.execute_task_list(&try_task.tasks, input.clone(), ctx, &format!("{}/try", task_path)).await {
            Ok(output) => Ok(TaskResult::new(output)),
            Err(err) => {
                if let Some(ref catch) = try_task.catch {
                    // TODO: match error filter
                    debug!(error = %err, "try caught error, running catch handler");
                    let catch_output = self.execute_task_list(
                        &catch.tasks, input, ctx, &format!("{}/catch", task_path)
                    ).await?;
                    Ok(TaskResult::new(catch_output))
                } else {
                    Err(err)
                }
            }
        }
    }

    /// Execute fork branches (sequential for now — parallel TODO)
    #[async_recursion]
    async fn execute_fork(
        &self,
        fork_task: &ForkTask,
        input: Value,
        ctx: &mut ExecutionContext,
        task_path: &str,
    ) -> Result<TaskResult> {
        let mut results = Vec::new();
        for (i, branch) in fork_task.branches.iter().enumerate() {
            let branch_path = format!("{}/fork[{}]", task_path, i);
            let branch_result = self.execute_single_task(branch, input.clone(), ctx, &branch_path).await?;
            results.push(branch_result.output);
        }
        Ok(TaskResult::new(Value::Array(results)))
    }

    /// Evaluate a jq expression
    fn eval_expr(&self, input: &Value, expr: &str, context: &Value) -> Result<Value> {
        if let Some(ref eval_fn) = self.eval_fn {
            eval_fn(input, expr, context).map_err(|e| RuntimeError::Expression(e))
        } else {
            // No expression engine — return raw value if it's not an expression
            if expr.starts_with("${") {
                Err(RuntimeError::Expression(format!(
                    "expression engine not configured, cannot evaluate: {}", expr
                )))
            } else {
                Ok(Value::String(expr.to_string()))
            }
        }
    }

    /// Recursively evaluate all `${ }` expressions within a JSON value tree.
    /// Used to resolve expressions in `call.with` and `run.input` before handler dispatch.
    fn eval_value_recursive(&self, value: &Value, input: &Value, context: &Value) -> Result<Value> {
        match value {
            Value::String(s) => {
                let trimmed = s.trim();
                if trimmed.starts_with("${") && trimmed.ends_with('}') {
                    // Pure expression: evaluate and return typed result
                    self.eval_expr(input, s, context)
                } else if s.contains("${") {
                    // Interpolated string: replace each ${ ... } span with its evaluated value
                    self.eval_interpolated_string(s, input, context)
                } else {
                    Ok(value.clone())
                }
            }
            Value::Object(map) => {
                let mut result = serde_json::Map::new();
                for (k, v) in map {
                    result.insert(k.clone(), self.eval_value_recursive(v, input, context)?);
                }
                Ok(Value::Object(result))
            }
            Value::Array(arr) => {
                let mut results = Vec::with_capacity(arr.len());
                for v in arr {
                    results.push(self.eval_value_recursive(v, input, context)?);
                }
                Ok(Value::Array(results))
            }
            _ => Ok(value.clone()),
        }
    }

    // ── Extension dispatch ──────────────────────────────────────────────

    /// Run matching before-extensions for a task.
    /// Returns Some(FlowDirective) if a before-task requests Exit/End (skipping the main task).
    async fn run_extensions_before(
        &self,
        ctx: &mut ExecutionContext,
        task_name: &str,
        task_type: &str,
        task_input: &Value,
    ) -> Result<Option<FlowDirective>> {
        if ctx.extensions.is_empty() {
            return Ok(None);
        }

        let eval_fn = ctx.eval_fn.clone();

        // Clone matching extensions to avoid borrow conflict with ctx
        let matching: Vec<_> = ctx.extensions.iter()
            .filter(|e| e.matches(task_type))
            .cloned()
            .collect();

        for ext in &matching {
            // Check when condition
            if let Some(ref when_expr) = ext.inner.when {
                let result = if let Some(ref ef) = eval_fn {
                    ef(task_input, when_expr, &ctx.context).unwrap_or(Value::Bool(false))
                } else {
                    self.eval_expr(task_input, when_expr, &ctx.context)
                        .unwrap_or(Value::Bool(false))
                };
                if result != Value::Bool(true) {
                    continue;
                }
            }

            debug!(extension = %ext.inner.name, task = %task_name, task_type = %task_type, "firing before-extension");

            // Execute before-tasks
            for before_task in &ext.inner.before {
                let result = self.execute_single_task(before_task, task_input.clone(), ctx, &format!("ext:before/{}", ext.inner.name)).await?;

                // Check if before-task requests skip/end
                match result.directive {
                    FlowDirective::Exit => {
                        debug!(extension = %ext.inner.name, "before-extension requested exit");
                        return Ok(Some(FlowDirective::Exit));
                    }
                    FlowDirective::End => {
                        debug!(extension = %ext.inner.name, "before-extension requested end");
                        return Ok(Some(FlowDirective::End));
                    }
                    _ => {
                        // Merge any exports from extension task
                        if let Some(exports) = result.exports {
                            data_flow::merge_into_context(&mut ctx.context, exports);
                        }
                    }
                }
            }
        }
        Ok(None)
    }

    /// Run matching after-extensions for a task.
    async fn run_extensions_after(
        &self,
        ctx: &mut ExecutionContext,
        task_name: &str,
        task_type: &str,
        task_output: &Value,
    ) {
        if ctx.extensions.is_empty() {
            return;
        }

        let eval_fn = ctx.eval_fn.clone();

        // Collect matching extensions to avoid borrow issues
        let matching: Vec<_> = ctx.extensions.iter()
            .filter(|e| e.matches(task_type))
            .cloned()
            .collect();

        for ext in &matching {
            // Check when condition
            if let Some(ref when_expr) = ext.inner.when {
                let result = if let Some(ref ef) = eval_fn {
                    ef(task_output, when_expr, &ctx.context).unwrap_or(Value::Bool(false))
                } else {
                    self.eval_expr(task_output, when_expr, &ctx.context)
                        .unwrap_or(Value::Bool(false))
                };
                if result != Value::Bool(true) {
                    continue;
                }
            }

            debug!(extension = %ext.inner.name, task = %task_name, task_type = %task_type, "firing after-extension");

            // Execute after-tasks
            for after_task in &ext.inner.after {
                match self.execute_single_task(after_task, task_output.clone(), ctx, &format!("ext:after/{}", ext.inner.name)).await {
                    Ok(result) => {
                        if let Some(exports) = result.exports {
                            data_flow::merge_into_context(&mut ctx.context, exports);
                        }
                    }
                    Err(e) => {
                        warn!(extension = %ext.inner.name, task = %after_task.name, error = %e, "after-extension task failed");
                    }
                }
            }
        }
    }

    // ── Expression evaluation ─────────────────────────────────────────

    /// Evaluate an interpolated string like `"prefix ${ expr } suffix"`.
    /// Each `${ ... }` span is evaluated and its result stringified.
    fn eval_interpolated_string(&self, s: &str, input: &Value, context: &Value) -> Result<Value> {
        let mut result = String::new();
        let mut remaining = s;

        while let Some(start) = remaining.find("${") {
            result.push_str(&remaining[..start]);
            remaining = &remaining[start..];

            // Find matching closing brace (simple: first `}` after `${`)
            if let Some(end) = remaining.find('}') {
                let expr_span = &remaining[..=end]; // includes ${ and }
                let val = self.eval_expr(input, expr_span, context)?;
                match val {
                    Value::String(ref sv) => result.push_str(sv),
                    Value::Null => result.push_str("null"),
                    ref other => result.push_str(&other.to_string()),
                }
                remaining = &remaining[end + 1..];
            } else {
                // No closing brace — treat rest as literal
                result.push_str(remaining);
                remaining = "";
            }
        }
        result.push_str(remaining);
        Ok(Value::String(result))
    }
}

impl Default for WorkflowExecutor {
    fn default() -> Self {
        Self::new()
    }
}

/// Rewrite `$each_var` → `$context.each_var` in all expressions within a task.
/// This allows for-loop variables declared via `each:` and `at:` to be referenced
/// as `$_tool_call` in CNCF YAML while the runtime stores them in `$context`.
fn rewrite_loop_vars_in_task(task: &mut NamedTask, each: &str, at: Option<&str>) {
    let rewrite = |s: &mut String| {
        // Replace $each_var with $context.each_var (but not $context.each_var itself)
        let each_pat = format!("${}", each);
        let each_replace = format!("$context.{}", each);
        if s.contains(&each_pat) && !s.contains(&each_replace) {
            *s = s.replace(&each_pat, &each_replace);
        }
        if let Some(at_var) = at {
            let at_pat = format!("${}", at_var);
            let at_replace = format!("$context.{}", at_var);
            if s.contains(&at_pat) && !s.contains(&at_replace) {
                *s = s.replace(&at_pat, &at_replace);
            }
        }
    };

    // Rewrite condition
    if let Some(ref mut cond) = task.condition {
        rewrite(cond);
    }

    // Rewrite set values (they contain expressions)
    if let Task::Set(ref mut set_task) = task.task {
        for val in set_task.values.values_mut() {
            rewrite_value_strings(val, each, at);
        }
    }

    // Rewrite call.with values
    if let Task::Call(ref mut call) = task.task {
        if let Some(ref mut with) = call.with {
            rewrite_value_strings(with, each, at);
        }
    }

    // Rewrite input/output/export expressions
    if let Some(ref mut input) = task.input {
        if let Some(ref mut from) = input.from {
            rewrite(from);
        }
    }
    if let Some(ref mut output) = task.output {
        if let Some(ref mut transform) = output.transform {
            rewrite(transform);
        }
    }
    if let Some(ref mut export) = task.export {
        if let Some(ref mut transform) = export.transform {
            rewrite(transform);
        }
    }

    // Recurse into nested tasks (do, for, try, etc.)
    match &mut task.task {
        Task::Do(ref mut do_task) => {
            for t in &mut do_task.tasks {
                rewrite_loop_vars_in_task(t, each, at);
            }
        }
        Task::For(ref mut for_task) => {
            for t in &mut for_task.tasks {
                rewrite_loop_vars_in_task(t, each, at);
            }
        }
        Task::Try(ref mut try_task) => {
            for t in &mut try_task.tasks {
                rewrite_loop_vars_in_task(t, each, at);
            }
            if let Some(ref mut catch) = try_task.catch {
                for t in &mut catch.tasks {
                    rewrite_loop_vars_in_task(t, each, at);
                }
            }
        }
        Task::Fork(ref mut fork_task) => {
            for t in &mut fork_task.branches {
                rewrite_loop_vars_in_task(t, each, at);
            }
        }
        _ => {}
    }
}

/// Recursively rewrite loop variable references in JSON values
fn rewrite_value_strings(val: &mut Value, each: &str, at: Option<&str>) {
    match val {
        Value::String(s) => {
            let each_pat = format!("${}", each);
            let each_replace = format!("$context.{}", each);
            if s.contains(&each_pat) && !s.contains(&each_replace) {
                *s = s.replace(&each_pat, &each_replace);
            }
            if let Some(at_var) = at {
                let at_pat = format!("${}", at_var);
                let at_replace = format!("$context.{}", at_var);
                if s.contains(&at_pat) && !s.contains(&at_replace) {
                    *s = s.replace(&at_pat, &at_replace);
                }
            }
        }
        Value::Object(map) => {
            for v in map.values_mut() {
                rewrite_value_strings(v, each, at);
            }
        }
        Value::Array(arr) => {
            for v in arr {
                rewrite_value_strings(v, each, at);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod compose_tests {
    use super::compose_template;
    use serde_json::json;

    #[test]
    fn subflow_intersects_and_cannot_widen() {
        let parent = json!({ "exposedTools": ["a", "b", "c"] });
        // Subflow narrows to {b,c,d}; effective must be the intersection {b,c}
        // — d is dropped because the parent never exposed it (no widening).
        let child = json!({ "exposedTools": ["b", "c", "d"] });
        let out = compose_template(Some(parent), child);
        let set = out["exposedTools"].as_array().unwrap();
        let mut names: Vec<&str> = set.iter().map(|v| v.as_str().unwrap()).collect();
        names.sort();
        assert_eq!(names, vec!["b", "c"]);
    }

    #[test]
    fn silent_subflow_inherits_parent_surface() {
        // A subflow that declares no exposedTools inherits the parent's — it
        // cannot escape the ceiling by staying silent.
        let parent = json!({ "exposedTools": ["a", "b"] });
        let child = json!({ "runtimePolicy": "standard" });
        let out = compose_template(Some(parent), child);
        let set = out["exposedTools"].as_array().unwrap();
        assert_eq!(set.len(), 2);
        assert_eq!(out["runtimePolicy"], "standard");
    }

    #[test]
    fn root_flow_keeps_its_own_template() {
        let child = json!({ "exposedTools": ["x"] });
        let out = compose_template(None, child.clone());
        assert_eq!(out, child);
    }
}
