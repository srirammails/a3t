//! run:script handler — inline script execution (Lua, JavaScript, Python)
//!
//! Uses embedded script runtimes:
//! - Lua via mlua (~20us)
//! - JavaScript via rquickjs (~100us)
//! - Python via rustpython (~200us)

use async_trait::async_trait;
use serde_json::{json, Value};
use std::time::Instant;
use tracing::{debug, info};

use swf_parser::{NamedTask, Task, RunTask};
use swf_runtime::context::ExecutionContext;
use swf_runtime::error::{RuntimeError, Result};
use swf_runtime::traits::{TaskHandler, TaskResult};

use mlua::Lua;

pub struct InlineScriptHandler;

#[async_trait]
impl TaskHandler for InlineScriptHandler {
    fn task_type(&self) -> &str {
        "run:script"
    }

    async fn execute(
        &self,
        task: &NamedTask,
        input: Value,
        _ctx: &mut ExecutionContext,
    ) -> Result<TaskResult> {
        let (language, code) = match &task.task {
            Task::Run(RunTask::Script(script)) => {
                (script.language.as_str(), script.code.as_deref().unwrap_or(""))
            }
            _ => return Err(RuntimeError::task_failed(&task.name, "expected run:script task")),
        };

        debug!(task = %task.name, language = %language, "executing inline script");

        let start = Instant::now();

        let result = match language {
            "lua" => execute_lua(code, &input),
            "js" | "javascript" => execute_javascript(code, &input),
            "py" | "python" => execute_python(code, &input),
            other => Err(format!("unsupported script language: {}", other)),
        }.map_err(|e| RuntimeError::task_failed(&task.name, e))?;

        let duration_us = start.elapsed().as_micros() as u64;
        info!(task = %task.name, language = %language, duration_us = duration_us, "script completed");

        Ok(TaskResult::new(result))
    }
}

// -- Lua (mlua) --

fn execute_lua(code: &str, state: &Value) -> std::result::Result<Value, String> {
    use mlua::{Lua, Value as LuaValue};

    let lua = Lua::new();
    let globals = lua.globals();

    let state_lua = json_to_lua(&lua, state).map_err(|e| format!("Lua state error: {}", e))?;
    globals.set("state", state_lua).map_err(|e| format!("Lua set error: {}", e))?;
    globals.set("result", LuaValue::Nil).map_err(|e| format!("Lua init error: {}", e))?;

    lua.load(code).exec().map_err(|e| format!("Lua error: {}", e))?;

    let result: LuaValue = globals.get("result").map_err(|e| format!("Lua result error: {}", e))?;
    lua_to_json(&result)
}

fn json_to_lua<'a>(lua: &'a Lua, val: &Value) -> std::result::Result<mlua::Value<'a>, mlua::Error> {
    use mlua::Value as LV;
    Ok(match val {
        Value::Null => LV::Nil,
        Value::Bool(b) => LV::Boolean(*b),
        Value::Number(n) => {
            if let Some(i) = n.as_i64() { LV::Integer(i) }
            else { LV::Number(n.as_f64().unwrap_or(0.0)) }
        }
        Value::String(s) => LV::String(lua.create_string(s.as_str())?),
        Value::Array(arr) => {
            let table = lua.create_table()?;
            for (i, v) in arr.iter().enumerate() {
                table.set(i + 1, json_to_lua(lua, v)?)?;
            }
            LV::Table(table)
        }
        Value::Object(map) => {
            let table = lua.create_table()?;
            for (k, v) in map {
                table.set(k.as_str(), json_to_lua(lua, v)?)?;
            }
            LV::Table(table)
        }
    })
}

fn lua_to_json(val: &mlua::Value) -> std::result::Result<Value, String> {
    use mlua::Value as LV;
    Ok(match val {
        LV::Nil => Value::Null,
        LV::Boolean(b) => json!(*b),
        LV::Integer(i) => json!(*i),
        LV::Number(n) => json!(*n),
        LV::String(s) => json!(s.to_str().map_err(|e| format!("{}", e))?),
        LV::Table(t) => {
            let mut map = serde_json::Map::new();
            for pair in t.clone().pairs::<mlua::Value, mlua::Value>() {
                let (k, v) = pair.map_err(|e| format!("{}", e))?;
                let key = match &k {
                    LV::String(s) => s.to_str().map_err(|e| format!("{}", e))?.to_string(),
                    LV::Integer(i) => i.to_string(),
                    _ => continue,
                };
                map.insert(key, lua_to_json(&v)?);
            }
            Value::Object(map)
        }
        _ => Value::Null,
    })
}

// -- JavaScript (rquickjs) --

fn execute_javascript(code: &str, state: &Value) -> std::result::Result<Value, String> {
    use rquickjs::{Runtime, Context};

    let rt = Runtime::new().map_err(|e| format!("JS runtime error: {}", e))?;
    let ctx = Context::full(&rt).map_err(|e| format!("JS context error: {}", e))?;

    ctx.with(|ctx| {
        let _globals = ctx.globals();

        // Inject state as JSON string, parse in JS
        let state_json = serde_json::to_string(state).unwrap_or_else(|_| "{}".to_string());
        let init = format!("var state = JSON.parse('{}'); var result = null;",
            state_json.replace('\\', "\\\\").replace('\'', "\\'"));
        ctx.eval::<(), _>(init.as_bytes()).map_err(|e| format!("JS init error: {}", e))?;

        // Execute user code
        ctx.eval::<(), _>(code.as_bytes()).map_err(|e| format!("JS error: {}", e))?;

        // Get result as JSON
        let get_result = "JSON.stringify(result)";
        let result_str: String = ctx.eval(get_result.as_bytes())
            .map_err(|e| format!("JS result error: {}", e))?;

        serde_json::from_str(&result_str).map_err(|e| format!("JS JSON parse error: {}", e))
    })
}

// -- Python (rustpython) --

fn execute_python(code: &str, state: &Value) -> std::result::Result<Value, String> {
    use rustpython_vm::Interpreter;

    let interp = Interpreter::without_stdlib(Default::default());

    interp.enter(|vm| {
        let scope = vm.new_scope_with_builtins();

        // Inject state fields as Python variables
        if let Value::Object(map) = state {
            for (key, val) in map {
                let py_val = json_to_pyobj(vm, val);
                scope.locals.set_item(key.as_str(), py_val, vm)
                    .map_err(|e| format!("Python set '{}' error: {:?}", key, e))?;
            }
        }
        let state_dict = json_to_pyobj(vm, state);
        scope.locals.set_item("state", state_dict, vm)
            .map_err(|e| format!("Python set state error: {:?}", e))?;
        scope.locals.set_item("result", vm.ctx.none(), vm)
            .map_err(|e| format!("Python init error: {:?}", e))?;

        // Execute
        vm.run_code_string(scope.clone(), code, "<inline>".to_string())
            .map_err(|e| format!("Python error: {:?}", e))?;

        // Extract result
        let result_obj = scope.locals.get_item("result", vm)
            .map_err(|e| format!("Python result error: {:?}", e))?;
        pyobj_to_json(vm, &result_obj)
    })
}

fn json_to_pyobj(vm: &rustpython_vm::VirtualMachine, val: &Value) -> rustpython_vm::PyObjectRef {
    match val {
        Value::Null => vm.ctx.none(),
        Value::Bool(b) => vm.ctx.new_bool(*b).into(),
        Value::Number(n) => {
            if let Some(i) = n.as_i64() { vm.ctx.new_int(i).into() }
            else { vm.ctx.new_float(n.as_f64().unwrap_or(0.0)).into() }
        }
        Value::String(s) => vm.ctx.new_str(s.as_str()).into(),
        Value::Array(arr) => {
            let items: Vec<_> = arr.iter().map(|v| json_to_pyobj(vm, v)).collect();
            vm.ctx.new_list(items).into()
        }
        Value::Object(map) => {
            let dict = vm.ctx.new_dict();
            for (k, v) in map { dict.set_item(k.as_str(), json_to_pyobj(vm, v), vm).ok(); }
            dict.into()
        }
    }
}

fn pyobj_to_json(vm: &rustpython_vm::VirtualMachine, obj: &rustpython_vm::PyObjectRef) -> std::result::Result<Value, String> {
    use rustpython_vm::builtins::*;
    if vm.is_none(obj) { return Ok(Value::Null); }
    if let Some(_) = obj.payload::<PyBool>() {
        return Ok(json!(obj.clone().is_true(vm).unwrap_or(false)));
    }
    if let Some(i) = obj.payload::<PyInt>() {
        if let Ok(v) = i.try_to_primitive::<i64>(vm) { return Ok(json!(v)); }
    }
    if let Some(f) = obj.payload::<PyFloat>() { return Ok(json!(f.to_f64())); }
    if let Some(s) = obj.payload::<PyStr>() { return Ok(json!(s.as_str())); }
    if let Some(l) = obj.payload::<PyList>() {
        let items: std::result::Result<Vec<Value>, _> = l.borrow_vec().iter().map(|i| pyobj_to_json(vm, i)).collect();
        return Ok(Value::Array(items?));
    }
    if let Some(d) = obj.payload::<PyDict>() {
        let mut map = serde_json::Map::new();
        for (k, v) in d {
            let key = k.payload::<PyStr>().map(|s| s.as_str().to_string()).unwrap_or_else(|| format!("{:?}", k));
            map.insert(key, pyobj_to_json(vm, &v)?);
        }
        return Ok(Value::Object(map));
    }
    if let Some(s) = obj.str(vm).ok() { return Ok(json!(s.as_str())); }
    Ok(Value::Null)
}
