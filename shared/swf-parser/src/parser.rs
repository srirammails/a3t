//! CNCF SWF DSL Parser
//!
//! Parses YAML/JSON into WorkflowDefinition with typed task AST.

use serde_json::Value;

use crate::call::{CallTarget, CallTask, HttpCall, HttpEndpoint};
use crate::document::WorkflowDefinition;
use crate::error::{SwfError, Result};
use crate::event::{EmitTask, ListenTask};
use crate::extension::Extension;
use crate::flow_control::{DoTask, ForTask, ForkTask, SwitchCase, SwitchTask, TryTask, CatchBlock};
use crate::run::{RunTask, ShellProcess, ScriptProcess, WorkflowProcess, ContainerProcess};
use crate::schema::{InputDefinition, OutputDefinition, ExportDefinition};
use crate::task::{NamedTask, Task, SetTask, RaiseTask, RaiseError, WaitTask};
use crate::timeout::Timeout;
use crate::directive::FlowDirective;

/// Parse a YAML string into a WorkflowDefinition
pub fn parse_yaml(yaml: &str) -> Result<WorkflowDefinition> {
    let value: Value = serde_yaml::from_str(yaml)?;
    parse_value(value)
}

/// Parse a JSON string into a WorkflowDefinition
pub fn parse_json(json: &str) -> Result<WorkflowDefinition> {
    let value: Value = serde_json::from_str(json)?;
    parse_value(value)
}

/// Parse a serde_json::Value into a WorkflowDefinition
fn parse_value(value: Value) -> Result<WorkflowDefinition> {
    // Validate required fields
    if value.get("document").is_none() {
        return Err(SwfError::MissingField {
            field: "document".to_string(),
            context: "workflow definition".to_string(),
        });
    }

    if value.get("do").is_none() {
        return Err(SwfError::MissingField {
            field: "do".to_string(),
            context: "workflow definition".to_string(),
        });
    }

    // Deserialize the base structure
    let mut workflow: WorkflowDefinition = serde_json::from_value(value.clone())
        .map_err(|e| SwfError::Parse(format!("Failed to parse workflow: {}", e)))?;

    // Parse the `do` block into typed tasks
    if let Some(raw_do) = workflow.take_raw_do() {
        workflow.tasks = parse_do_block(&raw_do)?;
    }

    Ok(workflow)
}

/// Parse a `do` block: an array of named task entries
/// Format: [{taskName: {taskDefinition}}, ...]
fn parse_do_block(value: &Value) -> Result<Vec<NamedTask>> {
    let arr = value.as_array().ok_or_else(|| {
        SwfError::InvalidValue {
            field: "do".to_string(),
            message: "expected an array of named tasks".to_string(),
        }
    })?;

    let mut tasks = Vec::with_capacity(arr.len());
    for item in arr {
        let task = parse_named_task(item)?;
        tasks.push(task);
    }
    Ok(tasks)
}

/// Parse a single named task entry: {taskName: {taskDefinition}}
fn parse_named_task(value: &Value) -> Result<NamedTask> {
    let obj = value.as_object().ok_or_else(|| {
        SwfError::Parse("expected a named task object".to_string())
    })?;

    if obj.len() != 1 {
        return Err(SwfError::Parse(format!(
            "expected exactly one key in named task, got {}", obj.len()
        )));
    }

    let (name, task_value) = obj.iter().next().unwrap();
    let task_obj = task_value.as_object().ok_or_else(|| {
        SwfError::Parse(format!("task '{}' must be an object", name))
    })?;

    // Extract common fields
    let condition = task_obj.get("if").and_then(|v| v.as_str()).map(String::from);
    let then = task_obj.get("then").and_then(|v| v.as_str()).map(|s| FlowDirective::Named(s.to_string()));
    let timeout = task_obj.get("timeout").and_then(|v| serde_json::from_value::<Timeout>(v.clone()).ok());
    let metadata = task_obj.get("metadata").cloned();

    let input = task_obj.get("input").and_then(|v| serde_json::from_value::<InputDefinition>(v.clone()).ok());
    let output = task_obj.get("output").and_then(|v| serde_json::from_value::<OutputDefinition>(v.clone()).ok());
    let export = task_obj.get("export").and_then(|v| serde_json::from_value::<ExportDefinition>(v.clone()).ok());

    // Determine task type from keys
    let task = parse_task_type(name, task_obj)?;

    Ok(NamedTask {
        name: name.clone(),
        task,
        condition,
        input,
        output,
        export,
        timeout,
        then,
        metadata,
    })
}

/// Determine the task type from the task object's keys
fn parse_task_type(name: &str, obj: &serde_json::Map<String, Value>) -> Result<Task> {
    // Check for each task type keyword
    if let Some(call_value) = obj.get("call") {
        return parse_call_task(call_value, obj);
    }
    if let Some(set_value) = obj.get("set") {
        return parse_set_task(set_value);
    }
    if obj.contains_key("do") && !obj.contains_key("for") && !obj.contains_key("try") {
        return parse_do_task(obj);
    }
    if obj.contains_key("for") {
        return parse_for_task(obj);
    }
    if obj.contains_key("fork") {
        return parse_fork_task(obj);
    }
    if obj.contains_key("switch") {
        return parse_switch_task(obj);
    }
    if obj.contains_key("try") {
        return parse_try_task(obj);
    }
    if obj.contains_key("emit") {
        return parse_emit_task(obj);
    }
    if obj.contains_key("listen") {
        return parse_listen_task(obj);
    }
    if obj.contains_key("raise") {
        return parse_raise_task(obj);
    }
    if obj.contains_key("run") {
        return parse_run_task(obj);
    }
    if obj.contains_key("wait") {
        return parse_wait_task(obj);
    }

    Err(SwfError::InvalidTaskType(format!(
        "task '{}' has no recognized task type keyword (call, set, do, for, fork, switch, try, emit, listen, raise, run, wait)",
        name
    )))
}

fn parse_call_task(call_value: &Value, obj: &serde_json::Map<String, Value>) -> Result<Task> {
    let with = obj.get("with").cloned();
    let auth = obj.get("authentication").and_then(|v| v.as_str()).map(String::from);

    let target = match call_value.as_str() {
        Some("http") => {
            let http_call = if let Some(ref w) = with {
                let method = w.get("method").and_then(|v| v.as_str()).map(String::from);
                let endpoint = w.get("endpoint").and_then(|v| {
                    if let Some(uri) = v.as_str() {
                        Some(HttpEndpoint { uri: uri.to_string(), authentication: None })
                    } else {
                        serde_json::from_value::<HttpEndpoint>(v.clone()).ok()
                    }
                });
                HttpCall { method, endpoint }
            } else {
                HttpCall { method: None, endpoint: None }
            };
            CallTarget::Http(http_call)
        }
        Some("grpc") => CallTarget::Grpc(serde_json::from_value(with.clone().unwrap_or_default()).unwrap_or_default()),
        Some("openapi") => CallTarget::OpenApi(serde_json::from_value(with.clone().unwrap_or_default()).unwrap_or_default()),
        Some("asyncapi") => CallTarget::AsyncApi(serde_json::from_value(with.clone().unwrap_or_default()).unwrap_or_default()),
        Some("a2a") => CallTarget::A2A(serde_json::from_value(with.clone().unwrap_or_default()).unwrap_or_default()),
        Some("mcp") => CallTarget::Mcp(serde_json::from_value(with.clone().unwrap_or_default()).unwrap_or_default()),
        Some(func_ref) => CallTarget::Function(func_ref.to_string()),
        None => return Err(SwfError::InvalidValue {
            field: "call".to_string(),
            message: "call target must be a string".to_string(),
        }),
    };

    Ok(Task::Call(CallTask { target, with, authentication: auth }))
}

fn parse_set_task(set_value: &Value) -> Result<Task> {
    let map = set_value.as_object().ok_or_else(|| {
        SwfError::InvalidValue {
            field: "set".to_string(),
            message: "expected an object of key-value pairs".to_string(),
        }
    })?;
    Ok(Task::Set(SetTask { values: map.clone() }))
}

fn parse_do_task(obj: &serde_json::Map<String, Value>) -> Result<Task> {
    let do_value = obj.get("do").ok_or_else(|| SwfError::MissingField {
        field: "do".to_string(), context: "do task".to_string(),
    })?;
    let tasks = parse_do_block(do_value)?;
    Ok(Task::Do(DoTask { tasks }))
}

fn parse_for_task(obj: &serde_json::Map<String, Value>) -> Result<Task> {
    let for_value = obj.get("for").and_then(|v| v.as_object()).ok_or_else(|| {
        SwfError::MissingField { field: "for".to_string(), context: "for task".to_string() }
    })?;

    let each = for_value.get("each").and_then(|v| v.as_str()).unwrap_or("item").to_string();
    let collection = for_value.get("in").and_then(|v| v.as_str()).ok_or_else(|| {
        SwfError::MissingField { field: "in".to_string(), context: "for task".to_string() }
    })?.to_string();
    let at = for_value.get("at").and_then(|v| v.as_str()).map(String::from);
    let while_condition = for_value.get("while").and_then(|v| v.as_str()).map(String::from);

    let do_value = for_value.get("do").ok_or_else(|| {
        SwfError::MissingField { field: "do".to_string(), context: "for task".to_string() }
    })?;
    let tasks = parse_do_block(do_value)?;

    Ok(Task::For(ForTask { each, collection, at, while_condition, tasks }))
}

fn parse_fork_task(obj: &serde_json::Map<String, Value>) -> Result<Task> {
    let fork_value = obj.get("fork").and_then(|v| v.as_object()).ok_or_else(|| {
        SwfError::MissingField { field: "fork".to_string(), context: "fork task".to_string() }
    })?;

    let compete = fork_value.get("compete").and_then(|v| v.as_bool()).unwrap_or(false);
    let branches_value = fork_value.get("branches").ok_or_else(|| {
        SwfError::MissingField { field: "branches".to_string(), context: "fork task".to_string() }
    })?;

    let branches = parse_do_block(branches_value)?;
    Ok(Task::Fork(ForkTask { branches, compete }))
}

fn parse_switch_task(obj: &serde_json::Map<String, Value>) -> Result<Task> {
    let switch_value = obj.get("switch").and_then(|v| v.as_array()).ok_or_else(|| {
        SwfError::InvalidValue { field: "switch".to_string(), message: "expected array of cases".to_string() }
    })?;

    let mut cases = Vec::new();
    for case_value in switch_value {
        let case_obj = case_value.as_object().ok_or_else(|| {
            SwfError::Parse("switch case must be an object".to_string())
        })?;

        // Each case is {caseName: {when?, then?}}
        for (case_name, case_def) in case_obj {
            let case_def_obj = case_def.as_object().ok_or_else(|| {
                SwfError::Parse(format!("switch case '{}' must be an object", case_name))
            })?;

            let when = case_def_obj.get("when").and_then(|v| v.as_str()).map(String::from);
            let then = case_def_obj.get("then").and_then(|v| v.as_str()).map(|s| FlowDirective::Named(s.to_string()));
            let default = when.is_none(); // If no `when`, it's the default case

            cases.push(SwitchCase {
                name: case_name.clone(),
                when,
                then,
                default,
            });
        }
    }

    Ok(Task::Switch(SwitchTask { cases }))
}

fn parse_try_task(obj: &serde_json::Map<String, Value>) -> Result<Task> {
    let try_value = obj.get("try").and_then(|v| v.as_object()).ok_or_else(|| {
        SwfError::MissingField { field: "try".to_string(), context: "try task".to_string() }
    })?;

    let do_value = try_value.get("do").ok_or_else(|| {
        SwfError::MissingField { field: "do".to_string(), context: "try task".to_string() }
    })?;
    let tasks = parse_do_block(do_value)?;

    let catch = if let Some(catch_value) = try_value.get("catch") {
        let errors = catch_value.get("errors").and_then(|v| {
            serde_json::from_value(v.clone()).ok()
        });
        let retry = catch_value.get("retry").and_then(|v| {
            serde_json::from_value(v.clone()).ok()
        });
        let catch_tasks = catch_value.get("do")
            .map(|v| parse_do_block(v).unwrap_or_default())
            .unwrap_or_default();

        Some(CatchBlock { errors, retry, tasks: catch_tasks })
    } else {
        None
    };

    Ok(Task::Try(TryTask { tasks, catch }))
}

fn parse_emit_task(obj: &serde_json::Map<String, Value>) -> Result<Task> {
    let emit_value = obj.get("emit").ok_or_else(|| {
        SwfError::MissingField { field: "emit".to_string(), context: "emit task".to_string() }
    })?;
    let emit: EmitTask = serde_json::from_value(emit_value.clone())
        .map_err(|e| SwfError::Parse(format!("invalid emit task: {}", e)))?;
    Ok(Task::Emit(emit))
}

fn parse_listen_task(obj: &serde_json::Map<String, Value>) -> Result<Task> {
    let listen_value = obj.get("listen").ok_or_else(|| {
        SwfError::MissingField { field: "listen".to_string(), context: "listen task".to_string() }
    })?;
    let listen: ListenTask = serde_json::from_value(listen_value.clone())
        .map_err(|e| SwfError::Parse(format!("invalid listen task: {}", e)))?;
    Ok(Task::Listen(listen))
}

fn parse_raise_task(obj: &serde_json::Map<String, Value>) -> Result<Task> {
    let raise_value = obj.get("raise").ok_or_else(|| {
        SwfError::MissingField { field: "raise".to_string(), context: "raise task".to_string() }
    })?;
    let raise: RaiseTask = serde_json::from_value(raise_value.clone())
        .map_err(|e| SwfError::Parse(format!("invalid raise task: {}", e)))?;
    Ok(Task::Raise(raise))
}

fn parse_run_task(obj: &serde_json::Map<String, Value>) -> Result<Task> {
    let run_value = obj.get("run").and_then(|v| v.as_object()).ok_or_else(|| {
        SwfError::MissingField { field: "run".to_string(), context: "run task".to_string() }
    })?;

    if let Some(shell) = run_value.get("shell") {
        let process: ShellProcess = serde_json::from_value(shell.clone())
            .map_err(|e| SwfError::Parse(format!("invalid shell process: {}", e)))?;
        return Ok(Task::Run(RunTask::Shell(process)));
    }
    if let Some(script) = run_value.get("script") {
        let process: ScriptProcess = serde_json::from_value(script.clone())
            .map_err(|e| SwfError::Parse(format!("invalid script process: {}", e)))?;
        return Ok(Task::Run(RunTask::Script(process)));
    }
    if let Some(workflow) = run_value.get("workflow") {
        let process: WorkflowProcess = serde_json::from_value(workflow.clone())
            .map_err(|e| SwfError::Parse(format!("invalid workflow process: {}", e)))?;
        return Ok(Task::Run(RunTask::Workflow(process)));
    }
    if let Some(container) = run_value.get("container") {
        let process: ContainerProcess = serde_json::from_value(container.clone())
            .map_err(|e| SwfError::Parse(format!("invalid container process: {}", e)))?;
        return Ok(Task::Run(RunTask::Container(process)));
    }

    Err(SwfError::InvalidValue {
        field: "run".to_string(),
        message: "must contain one of: shell, script, workflow, container".to_string(),
    })
}

fn parse_wait_task(obj: &serde_json::Map<String, Value>) -> Result<Task> {
    let wait_value = obj.get("wait").ok_or_else(|| {
        SwfError::MissingField { field: "wait".to_string(), context: "wait task".to_string() }
    })?;
    let wait: WaitTask = serde_json::from_value(wait_value.clone())
        .map_err(|e| SwfError::Parse(format!("invalid wait task: {}", e)))?;
    Ok(Task::Wait(wait))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_minimal_workflow() {
        let yaml = r#"
document:
  dsl: '1.0.3'
  namespace: test
  name: hello
  version: '1.0.0'
do:
  - greet:
      set:
        message: hello
"#;
        let wf = parse_yaml(yaml).unwrap();
        assert_eq!(wf.document.dsl, "1.0.3");
        assert_eq!(wf.document.name, "hello");
        assert_eq!(wf.document.namespace, "test");
        assert_eq!(wf.tasks.len(), 1);
        assert_eq!(wf.tasks[0].name, "greet");
        assert_eq!(wf.tasks[0].task.type_name(), "set");
    }

    #[test]
    fn test_reject_missing_document() {
        let yaml = r#"
do:
  - greet:
      set:
        message: hello
"#;
        let err = parse_yaml(yaml).unwrap_err();
        assert!(err.to_string().contains("document"));
    }

    #[test]
    fn test_reject_missing_do() {
        let yaml = r#"
document:
  dsl: '1.0.3'
  namespace: test
  name: empty
  version: '1.0.0'
"#;
        let err = parse_yaml(yaml).unwrap_err();
        assert!(err.to_string().contains("do"));
    }

    #[test]
    fn test_parse_all_12_task_types() {
        let yaml = r#"
document:
  dsl: '1.0.3'
  namespace: test
  name: all-tasks
  version: '1.0.0'
do:
  - callHttp:
      call: http
      with:
        method: GET
        endpoint:
          uri: https://example.com
  - doBlock:
      do:
        - inner:
            set:
              x: 1
  - emitEvent:
      emit:
        event:
          with:
            type: test.event
            source: test
  - forLoop:
      for:
        each: item
        in: "${ [1,2,3] }"
        do:
          - step:
              set:
                val: 1
  - forkBranches:
      fork:
        branches:
          - a:
              set:
                x: 1
          - b:
              set:
                y: 2
  - listenForEvent:
      listen:
        to:
          one:
            with:
              type: order.placed
  - raiseError:
      raise:
        error:
          type: https://example.com/error
          status: 400
  - runShell:
      run:
        shell:
          command: echo hello
  - setValue:
      set:
        greeting: hello
  - switchCase:
      switch:
        - caseA:
            when: "${ true }"
            then: continue
        - default:
            then: end
  - tryCatch:
      try:
        do:
          - attempt:
              set:
                tried: true
        catch:
          errors:
            with:
              status: 500
          do:
            - recover:
                set:
                  recovered: true
  - waitDuration:
      wait:
        seconds: 1
"#;
        let wf = parse_yaml(yaml).unwrap();
        assert_eq!(wf.tasks.len(), 12);

        let types: Vec<&str> = wf.tasks.iter().map(|t| t.task.type_name()).collect();
        assert_eq!(types, vec![
            "call", "do", "emit", "for", "fork", "listen",
            "raise", "run", "set", "switch", "try", "wait"
        ]);
    }

    #[test]
    fn test_parse_call_function_ref() {
        let yaml = r#"
document:
  dsl: '1.0.3'
  namespace: test
  name: func-call
  version: '1.0.0'
use:
  functions:
    greetUser:
      set:
        greeting: hello
do:
  - callFunc:
      call: greetUser
      with:
        name: World
"#;
        let wf = parse_yaml(yaml).unwrap();
        assert_eq!(wf.tasks[0].task.type_name(), "call");
        if let Task::Call(ref call) = wf.tasks[0].task {
            assert!(matches!(&call.target, CallTarget::Function(f) if f == "greetUser"));
        }
    }

    #[test]
    fn test_parse_task_with_if_and_output() {
        let yaml = r#"
document:
  dsl: '1.0.3'
  namespace: test
  name: conditional
  version: '1.0.0'
do:
  - expensive:
      if: "${ .should_run }"
      set:
        result: done
      output:
        as: "${ {data: .result} }"
"#;
        let wf = parse_yaml(yaml).unwrap();
        assert_eq!(wf.tasks[0].condition.as_deref(), Some("${ .should_run }"));
        assert!(wf.tasks[0].output.is_some());
    }

    // ── parser_tasks.feature scenarios ──────────────────

    #[test]
    fn test_parse_http_call_full_config() {
        let yaml = r#"
document:
  dsl: '1.0.3'
  namespace: test
  name: http-full
  version: '1.0.0'
do:
  - fetchData:
      call: http
      with:
        method: POST
        endpoint:
          uri: https://api.example.com/data
        headers:
          Authorization: "Bearer token123"
        body:
          query: test
"#;
        let wf = parse_yaml(yaml).unwrap();
        if let Task::Call(ref call) = wf.tasks[0].task {
            if let CallTarget::Http(ref http) = call.target {
                assert_eq!(http.method.as_deref(), Some("POST"));
                assert_eq!(http.endpoint.as_ref().unwrap().uri, "https://api.example.com/data");
            } else { panic!("expected http target"); }
        } else { panic!("expected call task"); }
    }

    #[test]
    fn test_parse_for_with_while() {
        let yaml = r#"
document:
  dsl: '1.0.3'
  namespace: test
  name: for-while
  version: '1.0.0'
do:
  - loop:
      for:
        each: i
        in: "${ [range(100)] }"
        while: "${ $context._continue == true }"
        do:
          - step:
              set:
                count: 1
"#;
        let wf = parse_yaml(yaml).unwrap();
        if let Task::For(ref f) = wf.tasks[0].task {
            assert_eq!(f.each, "i");
            assert_eq!(f.while_condition.as_deref(), Some("${ $context._continue == true }"));
            assert_eq!(f.tasks.len(), 1);
        } else { panic!("expected for task"); }
    }

    #[test]
    fn test_parse_run_variants() {
        let yaml = r#"
document:
  dsl: '1.0.3'
  namespace: test
  name: run-variants
  version: '1.0.0'
do:
  - shellCmd:
      run:
        shell:
          command: echo hello
  - scriptRun:
      run:
        script:
          language: python
          code: "result = 42"
  - nestedFlow:
      run:
        workflow:
          namespace: default
          name: child
          version: '1.0.0'
"#;
        let wf = parse_yaml(yaml).unwrap();
        assert_eq!(wf.tasks.len(), 3);
        assert!(matches!(wf.tasks[0].task, Task::Run(RunTask::Shell(_))));
        assert!(matches!(wf.tasks[1].task, Task::Run(RunTask::Script(_))));
        assert!(matches!(wf.tasks[2].task, Task::Run(RunTask::Workflow(_))));
    }

    // ── parser_remaining_tasks.feature scenarios ────────

    #[test]
    fn test_parse_catalog_function_ref() {
        let yaml = r#"
document:
  dsl: '1.0.3'
  namespace: test
  name: catalog-call
  version: '1.0.0'
use:
  catalogs:
    global:
      endpoint:
        uri: https://github.com/serverlessworkflow/catalog
do:
  - logIt:
      call: "log:0.5.2@global"
      with:
        message: Hello
"#;
        let wf = parse_yaml(yaml).unwrap();
        if let Task::Call(ref call) = wf.tasks[0].task {
            assert!(matches!(&call.target, CallTarget::Function(f) if f == "log:0.5.2@global"));
        } else { panic!("expected call task"); }
        assert!(wf.use_block.as_ref().unwrap().catalogs.is_some());
    }

    #[test]
    fn test_parse_switch_with_cases() {
        let yaml = r#"
document:
  dsl: '1.0.3'
  namespace: test
  name: switch-parse
  version: '1.0.0'
do:
  - route:
      switch:
        - high:
            when: "${ .priority == \"high\" }"
            then: urgent
        - low:
            when: "${ .priority == \"low\" }"
            then: normal
        - fallback:
            then: normal
"#;
        let wf = parse_yaml(yaml).unwrap();
        if let Task::Switch(ref sw) = wf.tasks[0].task {
            assert_eq!(sw.cases.len(), 3);
            assert_eq!(sw.cases[0].name, "high");
            assert!(sw.cases[0].when.is_some());
            assert!(!sw.cases[0].default);
            assert_eq!(sw.cases[2].name, "fallback");
            assert!(sw.cases[2].default);
        } else { panic!("expected switch task"); }
    }

    #[test]
    fn test_parse_try_with_retry() {
        let yaml = r#"
document:
  dsl: '1.0.3'
  namespace: test
  name: try-retry
  version: '1.0.0'
do:
  - resilient:
      try:
        do:
          - callApi:
              call: http
              with:
                method: GET
                endpoint:
                  uri: https://api.example.com
        catch:
          errors:
            with:
              status: 503
          retry:
            delay:
              seconds: 2
            backoff:
              exponential: {}
            limit:
              attempt:
                count: 3
          do:
            - fallback:
                set:
                  error: unavailable
"#;
        let wf = parse_yaml(yaml).unwrap();
        if let Task::Try(ref t) = wf.tasks[0].task {
            assert_eq!(t.tasks.len(), 1);
            let catch = t.catch.as_ref().unwrap();
            assert!(catch.retry.is_some());
            let retry = catch.retry.as_ref().unwrap();
            assert_eq!(retry.limit.as_ref().unwrap().attempt.as_ref().unwrap().count, 3);
        } else { panic!("expected try task"); }
    }

    #[test]
    fn test_parse_emit_cloud_event() {
        let yaml = r#"
document:
  dsl: '1.0.3'
  namespace: test
  name: emit-test
  version: '1.0.0'
do:
  - notify:
      emit:
        event:
          with:
            type: com.example.order.placed
            source: /orders
            data:
              orderId: "123"
"#;
        let wf = parse_yaml(yaml).unwrap();
        if let Task::Emit(ref e) = wf.tasks[0].task {
            assert_eq!(e.event.with.event_type, "com.example.order.placed");
            assert_eq!(e.event.with.source.as_deref(), Some("/orders"));
        } else { panic!("expected emit task"); }
    }

    #[test]
    fn test_parse_listen_event() {
        let yaml = r#"
document:
  dsl: '1.0.3'
  namespace: test
  name: listen-test
  version: '1.0.0'
do:
  - waitApproval:
      listen:
        to:
          one:
            with:
              type: com.example.approval
              source: /approvals
"#;
        let wf = parse_yaml(yaml).unwrap();
        if let Task::Listen(ref l) = wf.tasks[0].task {
            let filter = l.to.one.as_ref().unwrap();
            assert_eq!(filter.with.event_type, "com.example.approval");
        } else { panic!("expected listen task"); }
    }

    #[test]
    fn test_parse_fork_branches() {
        let yaml = r#"
document:
  dsl: '1.0.3'
  namespace: test
  name: fork-test
  version: '1.0.0'
do:
  - parallel:
      fork:
        compete: false
        branches:
          - fetchUser:
              call: http
              with:
                method: GET
                endpoint:
                  uri: https://api.example.com/user
          - fetchOrders:
              call: http
              with:
                method: GET
                endpoint:
                  uri: https://api.example.com/orders
"#;
        let wf = parse_yaml(yaml).unwrap();
        if let Task::Fork(ref f) = wf.tasks[0].task {
            assert_eq!(f.branches.len(), 2);
            assert!(!f.compete);
            assert_eq!(f.branches[0].name, "fetchUser");
            assert_eq!(f.branches[1].name, "fetchOrders");
        } else { panic!("expected fork task"); }
    }

    #[test]
    fn test_parse_raise_rfc7807() {
        let yaml = r#"
document:
  dsl: '1.0.3'
  namespace: test
  name: raise-test
  version: '1.0.0'
do:
  - reject:
      raise:
        error:
          type: https://serverlessworkflow.io/spec/1.0.0/errors/validation
          status: 400
          title: Validation Error
          detail: Name is required
          instance: /do/reject
"#;
        let wf = parse_yaml(yaml).unwrap();
        if let Task::Raise(ref r) = wf.tasks[0].task {
            assert_eq!(r.error.error_type.as_deref(), Some("https://serverlessworkflow.io/spec/1.0.0/errors/validation"));
            assert_eq!(r.error.status, Some(400));
            assert_eq!(r.error.instance.as_deref(), Some("/do/reject"));
        } else { panic!("expected raise task"); }
    }

    #[test]
    fn test_parse_wait_durations() {
        let yaml = r#"
document:
  dsl: '1.0.3'
  namespace: test
  name: wait-test
  version: '1.0.0'
do:
  - pause30s:
      wait:
        seconds: 30
  - pause2m:
      wait:
        minutes: 2
"#;
        let wf = parse_yaml(yaml).unwrap();
        assert_eq!(wf.tasks.len(), 2);
        if let Task::Wait(ref w) = wf.tasks[0].task {
            assert_eq!(w.seconds, Some(30));
        } else { panic!("expected wait"); }
        if let Task::Wait(ref w) = wf.tasks[1].task {
            assert_eq!(w.minutes, Some(2));
        } else { panic!("expected wait"); }
    }

    // ── parser_data_flow.feature scenarios ──────────────

    #[test]
    fn test_parse_input_with_schema() {
        let yaml = r#"
document:
  dsl: '1.0.3'
  namespace: test
  name: input-schema
  version: '1.0.0'
do:
  - process:
      set:
        result: done
      input:
        from: "${ {filtered: .raw} }"
        schema:
          type: object
          required: [filtered]
"#;
        let wf = parse_yaml(yaml).unwrap();
        let input = wf.tasks[0].input.as_ref().unwrap();
        assert_eq!(input.from.as_deref(), Some("${ {filtered: .raw} }"));
        assert!(input.schema.is_some());
    }

    #[test]
    fn test_parse_output_with_schema() {
        let yaml = r#"
document:
  dsl: '1.0.3'
  namespace: test
  name: output-schema
  version: '1.0.0'
do:
  - fetch:
      call: http
      with:
        method: GET
        endpoint:
          uri: https://api.example.com
      output:
        as: "${ {data: .body} }"
        schema:
          type: object
          required: [data]
"#;
        let wf = parse_yaml(yaml).unwrap();
        let output = wf.tasks[0].output.as_ref().unwrap();
        assert_eq!(output.transform.as_deref(), Some("${ {data: .body} }"));
        assert!(output.schema.is_some());
    }

    #[test]
    fn test_parse_export_to_context() {
        let yaml = r#"
document:
  dsl: '1.0.3'
  namespace: test
  name: export-test
  version: '1.0.0'
do:
  - compute:
      set:
        total: 42
      export:
        as: "${ {running_total: .total} }"
"#;
        let wf = parse_yaml(yaml).unwrap();
        let export = wf.tasks[0].export.as_ref().unwrap();
        assert_eq!(export.transform.as_deref(), Some("${ {running_total: .total} }"));
    }

    #[test]
    fn test_parse_timeout() {
        let yaml = r#"
document:
  dsl: '1.0.3'
  namespace: test
  name: timeout-test
  version: '1.0.0'
do:
  - slow:
      call: http
      with:
        method: GET
        endpoint:
          uri: https://slow.example.com
      timeout:
        after:
          seconds: 30
"#;
        let wf = parse_yaml(yaml).unwrap();
        let timeout = wf.tasks[0].timeout.as_ref().unwrap();
        assert_eq!(timeout.after.as_ref().unwrap().seconds, Some(30));
    }

    // ── parser_extensions.feature scenarios ─────────────

    #[test]
    fn test_parse_use_extensions() {
        let yaml = r#"
document:
  dsl: '1.0.3'
  namespace: test
  name: ext-test
  version: '1.0.0'
use:
  extensions:
    - logging:
        extend: all
        before:
          - logBefore:
              call: http
              with:
                method: post
                endpoint: https://logs.example.com
        after:
          - logAfter:
              set:
                logged: true
    - costGuard:
        extend: call
        when: "${ $context._count > 100 }"
        before:
          - block:
              raise:
                error:
                  type: https://example.com/budget
                  status: 429
do:
  - hello:
      set:
        message: hello
"#;
        let wf = parse_yaml(yaml).unwrap();
        let use_block = wf.use_block.as_ref().unwrap();
        assert_eq!(use_block.extensions.len(), 2);
    }

    #[test]
    fn test_parse_use_functions() {
        let yaml = r#"
document:
  dsl: '1.0.3'
  namespace: test
  name: func-test
  version: '1.0.0'
use:
  functions:
    fetchUser:
      call: http
      with:
        method: GET
        endpoint:
          uri: https://api.example.com/users
    computeTotal:
      set:
        total: 42
do:
  - getUser:
      call: fetchUser
  - calc:
      call: computeTotal
"#;
        let wf = parse_yaml(yaml).unwrap();
        let funcs = wf.use_block.as_ref().unwrap().functions.as_ref().unwrap();
        assert_eq!(funcs.len(), 2);
        assert!(funcs.contains_key("fetchUser"));
        assert!(funcs.contains_key("computeTotal"));
    }

    // ── parser_auth_errors.feature scenarios ────────────

    #[test]
    fn test_parse_basic_auth() {
        let yaml = r#"
document:
  dsl: '1.0.3'
  namespace: test
  name: auth-basic
  version: '1.0.0'
use:
  authentications:
    admin:
      basic:
        username: admin
        password: secret
do:
  - secure:
      set:
        auth: configured
"#;
        let wf = parse_yaml(yaml).unwrap();
        let auths = wf.use_block.as_ref().unwrap().authentications.as_ref().unwrap();
        assert!(auths.contains_key("admin"));
    }

    #[test]
    fn test_parse_oauth2_auth() {
        let yaml = r#"
document:
  dsl: '1.0.3'
  namespace: test
  name: auth-oauth2
  version: '1.0.0'
use:
  authentications:
    keycloak:
      oauth2:
        authority: http://keycloak/realms/app/.well-known/openid-configuration
        grant: client-credentials
        client:
          id: workflow-runtime
          secret: secret123
        scopes: [api]
        audiences: [runtime]
do:
  - secure:
      set:
        auth: oauth2
"#;
        let wf = parse_yaml(yaml).unwrap();
        let auths = wf.use_block.as_ref().unwrap().authentications.as_ref().unwrap();
        assert!(auths.contains_key("keycloak"));
    }

    #[test]
    fn test_parse_error_definitions() {
        let yaml = r#"
document:
  dsl: '1.0.3'
  namespace: test
  name: error-model
  version: '1.0.0'
use:
  errors:
    notFound:
      type: https://serverlessworkflow.io/spec/1.0.0/errors/not-found
      status: 404
      title: Not Found
    timeout:
      type: https://serverlessworkflow.io/spec/1.0.0/errors/timeout
      status: 408
do:
  - check:
      set:
        errors: defined
"#;
        let wf = parse_yaml(yaml).unwrap();
        let errors = wf.use_block.as_ref().unwrap().errors.as_ref().unwrap();
        assert_eq!(errors.len(), 2);
        assert!(errors.contains_key("notFound"));
        assert!(errors.contains_key("timeout"));
    }

    // ── Workflow-level input/output schemas ─────────────

    #[test]
    fn test_parse_workflow_input_output_schemas() {
        let yaml = r#"
document:
  dsl: '1.0.3'
  namespace: test
  name: schema-test
  version: '1.0.0'
input:
  schema:
    type: object
    required: [name]
    properties:
      name:
        type: string
output:
  as: "${ {greeting: .message} }"
do:
  - greet:
      set:
        message: "${ \"Hello \" + .name }"
"#;
        let wf = parse_yaml(yaml).unwrap();
        assert!(wf.input.is_some());
        assert!(wf.input.as_ref().unwrap().schema.is_some());
        assert!(wf.output.is_some());
        assert_eq!(wf.output.as_ref().unwrap().transform.as_deref(), Some("${ {greeting: .message} }"));
    }

    // ── Multi-task ordering ────────────────────────────

    #[test]
    fn test_task_order_preserved() {
        let yaml = r#"
document:
  dsl: '1.0.3'
  namespace: default
  name: ordered
  version: '1.0.0'
do:
  - first:
      set:
        step: 1
  - second:
      set:
        step: 2
  - third:
      set:
        step: 3
"#;
        let wf = parse_yaml(yaml).unwrap();
        let names: Vec<&str> = wf.tasks.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(names, vec!["first", "second", "third"]);
    }

    // ── then flow directive ────────────────────────────

    #[test]
    fn test_parse_then_directive() {
        let yaml = r#"
document:
  dsl: '1.0.3'
  namespace: test
  name: directive
  version: '1.0.0'
do:
  - first:
      set:
        step: 1
      then: end
  - second:
      set:
        step: 2
"#;
        let wf = parse_yaml(yaml).unwrap();
        let then = wf.tasks[0].then.as_ref().unwrap();
        assert!(then.is_end());
    }
}

    #[test]
    fn test_parse_set_with_expressions_and_arrays() {
        let yaml = r#"
document:
  dsl: '1.0.3'
  namespace: test
  name: expr-array
  version: '1.0.0'
do:
  - init:
      set:
        _messages: []
        _config: "${ .config // {} }"
        _tools: "${ .config.tools // [] }"
        static_val: 42
      export:
        as: "."
  - append:
      set:
        _messages: "${ $context._messages + [{role: \"user\", content: \"hi\"}] }"
"#;
        let wf = parse_yaml(yaml).unwrap();
        assert_eq!(wf.tasks.len(), 2);
        
        // Check init task values
        if let Task::Set(ref s) = wf.tasks[0].task {
            let msgs = &s.values["_messages"];
            assert!(msgs.is_array(), "_messages should be array, got: {:?}", msgs);
            assert_eq!(msgs.as_array().unwrap().len(), 0);
            
            let config = &s.values["_config"];
            assert!(config.is_string(), "_config should be string expression");
            assert!(config.as_str().unwrap().contains("${"));
            
            let tools = &s.values["_tools"];
            assert!(tools.is_string(), "_tools should be string expression");
            
            let static_v = &s.values["static_val"];
            assert_eq!(static_v, &json!(42));
        } else {
            panic!("expected set task");
        }
}
