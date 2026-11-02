//! Run tools: read and steer runs on the durable engine store
//! (`.ai/FLOWR_CONSOLE_RUN_DEBUG_UI_SPEC.md` §11). The console's run panel,
//! monitor, debugger and checkpoint views use these; so can agents.
//!
//! Access: reading a run needs what `get_execution_status` needs (Read on its
//! project; a platform flow's runs only to whoever started them). Starting a
//! run needs Run; a debug run, a command, an edit, breakpoints, cancel and
//! re-run need Write. Sending an event needs Run on its project (it can wake
//! that project's runs). Values are masked on the way out.

use serde_json::{json, Value};
use uuid::Uuid;

use flowr_policy::namespace::{namespace_guard, ItemKind, NsAccess};
use flowr_policy::Principal;
use flowr_storage::engine::*;
use flowr_storage::StorageBackend;

use crate::protocol::{CallToolResult, Tool};

pub fn tools() -> Vec<Tool> {
    let run_id = json!({ "type": "string", "description": "Run (instance) id" });
    vec![
        Tool {
            name: "list_runs".into(),
            description: "List runs newest first, with cursor paging. Filter by project(s), flow, status.".into(),
            input_schema: json!({ "type": "object", "properties": {
                "namespace": { "type": "string" },
                "namespaces": { "type": "array", "items": { "type": "string" } },
                "name": { "type": "string", "description": "Flow name" },
                "status": { "type": "array", "items": { "type": "string" }, "description": "pending, running, waiting, suspended, completed, faulted, cancelled" },
                "limit": { "type": "integer", "default": 50 },
                "cursor": { "type": "string" }
            }}),
        },
        Tool {
            name: "get_run".into(),
            description: "One run: status, input, output, error, its steps (status, attempts, timing), and where it is paused.".into(),
            input_schema: json!({ "type": "object", "properties": { "run_id": run_id }, "required": ["run_id"] }),
        },
        Tool {
            name: "get_run_journal".into(),
            description: "A run's journal records after a sequence number: what happened, step by step.".into(),
            input_schema: json!({ "type": "object", "properties": { "run_id": run_id, "after_seq": { "type": "integer", "default": 0 } }, "required": ["run_id"] }),
        },
        Tool {
            name: "start_run".into(),
            description: "Start a run of a flow, optionally as a debug run with breakpoints.".into(),
            input_schema: json!({ "type": "object", "properties": {
                "namespace": { "type": "string" }, "name": { "type": "string" }, "input": {},
                "debug": { "type": "boolean" },
                "breakpoints": { "type": "array", "description": "[{step_key, when: before|after|on_error, condition?, hit?}]" },
                "pause_on_start": { "type": "boolean" }
            }, "required": ["namespace", "name"] }),
        },
        Tool {
            name: "debug_run".into(),
            description: "Continue a paused debug run: continue, step_over, step_into, step_out, run_to (with step_key), or stop.".into(),
            input_schema: json!({ "type": "object", "properties": { "run_id": run_id, "command": { "type": "string" }, "step_key": { "type": "string" } }, "required": ["run_id", "command"] }),
        },
        Tool {
            name: "set_breakpoints".into(),
            description: "Replace a debug run's breakpoints (used from its next resume).".into(),
            input_schema: json!({ "type": "object", "properties": { "run_id": run_id, "breakpoints": { "type": "array" } }, "required": ["run_id", "breakpoints"] }),
        },
        Tool {
            name: "edit_step_input".into(),
            description: "While a debug run is paused before a step, replace that step's input.".into(),
            input_schema: json!({ "type": "object", "properties": { "run_id": run_id, "step_key": { "type": "string" }, "input": {} }, "required": ["run_id", "step_key", "input"] }),
        },
        Tool {
            name: "cancel_run".into(),
            description: "Cancel a run: stopped at its next step if running, at once otherwise.".into(),
            input_schema: json!({ "type": "object", "properties": { "run_id": run_id, "reason": { "type": "string" } }, "required": ["run_id"] }),
        },
        Tool {
            name: "send_event".into(),
            description: "Send a CloudEvent into a project: every run whose `listen` step it completes is woken. Give at least `type`; `source`, `subject`, `data` and extensions are matched by the listeners' filters.".into(),
            input_schema: json!({ "type": "object", "properties": {
                "namespace": { "type": "string", "description": "Project the event is sent in" },
                "event": { "type": "object", "description": "CloudEvent attributes: {type, source?, subject?, data?, …}" }
            }, "required": ["namespace", "event"] }),
        },
        Tool {
            name: "rerun_run".into(),
            description: "Start a new run like this one — from the start, or from a journal point (steps up to it replay, later ones run again).".into(),
            input_schema: json!({ "type": "object", "properties": {
                "run_id": run_id, "from_seq": { "type": "integer" }, "input": {},
                "debug": { "type": "boolean" }, "breakpoints": { "type": "array" }
            }, "required": ["run_id"] }),
        },
    ]
}

/// Handle a run tool; `None` if `name` is not one.
pub async fn call(storage: &dyn StorageBackend, principal: &Principal, name: &str, args: &Value) -> Option<CallToolResult> {
    let result = match name {
        "list_runs" => list_runs(storage, principal, args).await,
        "get_run" => get_run(storage, principal, args).await,
        "get_run_journal" => get_journal(storage, principal, args).await,
        "start_run" => start_run(storage, principal, args).await,
        "debug_run" => debug_run(storage, principal, args).await,
        "set_breakpoints" => set_breakpoints(storage, principal, args).await,
        "edit_step_input" => edit_step_input(storage, principal, args).await,
        "cancel_run" => cancel_run(storage, principal, args).await,
        "rerun_run" => rerun_run(storage, principal, args).await,
        "send_event" => send_event(storage, principal, args).await,
        _ => return None,
    };
    Some(result.unwrap_or_else(CallToolResult::error))
}

type R = Result<CallToolResult, String>;

/// Where `send_event` hands an event when the server also starts flows from
/// events (FlowR's event triggers); without one, it only wakes listening runs.
pub type EventSink = std::sync::Arc<
    dyn Fn(String, Value, Principal) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Value, String>> + Send>> + Send + Sync,
>;

static EVENT_SINK: std::sync::OnceLock<EventSink> = std::sync::OnceLock::new();

pub fn set_event_sink(sink: EventSink) {
    let _ = EVENT_SINK.set(sink);
}

fn engine(storage: &dyn StorageBackend) -> Result<std::sync::Arc<dyn EngineStore>, String> {
    storage.engine().ok_or_else(|| "this server runs without the run store (FLOWR_STORE=lance)".to_string())
}

fn s<'a>(args: &'a Value, k: &str) -> Option<&'a str> {
    args.get(k).and_then(|v| v.as_str()).filter(|v| !v.is_empty())
}

fn run_id(args: &Value) -> Result<RunId, String> {
    s(args, "run_id").and_then(|i| Uuid::parse_str(i).ok()).ok_or_else(|| "run_id is required".to_string())
}

/// Hide secrets in a value shown to a person: fields named like keys,
/// secrets, passwords, tokens, credentials or authorization (not `*_ref`).
pub fn mask(v: &Value) -> Value {
    match v {
        Value::Object(o) => Value::Object(
            o.iter()
                .map(|(k, v)| {
                    let l = k.to_lowercase();
                    let secret = !l.ends_with("_ref")
                        && l != "step_key"
                        && ["key", "secret", "password", "passwd", "token", "credential", "authorization"].iter().any(|w| l.contains(w));
                    (k.clone(), if secret && !v.is_null() { Value::String("****".into()) } else { mask(v) })
                })
                .collect(),
        ),
        Value::Array(a) => Value::Array(a.iter().map(mask).collect()),
        other => other.clone(),
    }
}

fn flow_ns(ns: &str, name: &str) -> String {
    namespace_guard().effective_namespace(ItemKind::Flow, ns, name, "", "")
}

async fn may(principal: &Principal, ns: &str, name: &str, need: NsAccess) -> bool {
    let g = namespace_guard();
    !g.enforcing() || matches!(principal, Principal::Service { .. }) || g.access(principal, &flow_ns(ns, name)).await >= need
}

async fn visible(principal: &Principal, r: &RunRow) -> bool {
    let started_by: Option<Principal> = serde_json::from_value(r.principal.clone()).ok();
    crate::ns_gate::can_see(principal, &r.namespace, &r.flow, started_by.as_ref()).await
}

/// The run, if the caller may see it (else "not found", so ids can't be probed).
async fn readable(storage: &dyn StorageBackend, principal: &Principal, args: &Value) -> Result<(std::sync::Arc<dyn EngineStore>, RunRow), String> {
    let e = engine(storage)?;
    let id = run_id(args)?;
    match e.get(id).await.map_err(|e| e.to_string())? {
        Some(r) if visible(principal, &r).await => Ok((e, r)),
        _ => Err(format!("Run `{id}` not found.")),
    }
}

async fn writable(storage: &dyn StorageBackend, principal: &Principal, args: &Value) -> Result<(std::sync::Arc<dyn EngineStore>, RunRow), String> {
    let (e, r) = readable(storage, principal, args).await?;
    if !may(principal, &r.namespace, &r.flow, NsAccess::Write).await {
        return Err(format!("Forbidden: steering a run needs write access to project '{}'.", r.namespace));
    }
    Ok((e, r))
}

fn who(principal: &Principal) -> String {
    flowr_policy::namespace::acting_user(principal).map(|u| u.to_string()).unwrap_or_else(|| principal.id().to_string())
}

fn summary(r: &RunRow) -> Value {
    let duration_us = match (r.started_at, r.completed_at) {
        (Some(s), Some(c)) => (c - s).num_microseconds(),
        _ => None,
    };
    json!({
        "run_id": r.run_id, "namespace": r.namespace, "flow": r.flow, "version": r.version,
        "phase": r.phase, "status": r.phase.legacy_status(), "current_step": r.current_step,
        "trigger": r.trigger, "debug": r.debug, "durability": r.durability,
        "principal_id": serde_json::from_value::<Principal>(r.principal.clone()).ok().map(|p| p.id().to_string()),
        "created_at": r.created_at, "started_at": r.started_at, "completed_at": r.completed_at,
        "duration_us": duration_us, "error": r.error, "epoch": r.epoch, "resume_count": r.resume_count,
        "parent_run_id": r.parent_run_id,
    })
}

async fn list_runs(storage: &dyn StorageBackend, principal: &Principal, args: &Value) -> R {
    let e = engine(storage)?;
    let mut namespaces: Option<Vec<String>> = args.get("namespaces").and_then(|v| v.as_array())
        .map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect());
    if let Some(ns) = s(args, "namespace") {
        namespaces = Some(vec![ns.to_string()]);
    }
    let phases = args.get("status").and_then(|v| v.as_array()).map(|a| {
        a.iter().filter_map(|x| x.as_str()).filter_map(|x| {
            let x = match x.to_lowercase().as_str() { "queued" => "pending".to_string(), "failed" => "faulted".to_string(), o => o.to_string() };
            serde_json::from_value::<RunPhase>(Value::String(x)).ok()
        }).collect()
    }).unwrap_or_default();
    let limit = args.get("limit").and_then(|v| v.as_u64()).unwrap_or(50).clamp(1, 200) as usize;
    let page = e
        .list(RunQuery { namespaces, phases, flow: s(args, "name").map(String::from), cursor: s(args, "cursor").map(String::from), limit, ..Default::default() })
        .await
        .map_err(|e| e.to_string())?;
    let mut runs = Vec::new();
    for r in &page.items {
        if visible(principal, r).await {
            runs.push(summary(r));
        }
    }
    Ok(CallToolResult::json(json!({ "runs": runs, "next": page.next })))
}

/// A step's start and end: the end it recorded (older records: the time its
/// journal entry was written), and the start its duration puts before that.
fn span(written: chrono::DateTime<chrono::Utc>, at: Option<chrono::DateTime<chrono::Utc>>, duration_us: u64)
    -> (chrono::DateTime<chrono::Utc>, chrono::DateTime<chrono::Utc>) {
    let end = at.unwrap_or(written);
    (end - chrono::Duration::microseconds(duration_us.min(i64::MAX as u64) as i64), end)
}

/// Steps in the order they first appear in the journal.
fn steps_of(entries: &[JournalEntry]) -> (Vec<Value>, Option<Value>) {
    let mut order: Vec<String> = Vec::new();
    let mut steps: std::collections::HashMap<String, serde_json::Map<String, Value>> = Default::default();
    let mut paused: Option<Value> = None;
    type Steps = std::collections::HashMap<String, serde_json::Map<String, Value>>;
    fn entry(steps: &mut Steps, k: &str, order: &mut Vec<String>) -> serde_json::Map<String, Value> {
        if !steps.contains_key(k) {
            order.push(k.to_string());
        }
        steps.remove(k).unwrap_or_else(|| {
            let mut m = serde_json::Map::new();
            m.insert("step_key".into(), json!(k));
            m.insert("status".into(), json!("started"));
            m.insert("attempts".into(), json!(0));
            m
        })
    }
    for e in entries {
        match &e.record {
            Record::StepStarted { step_key, attempt, .. } => {
                let mut m = entry(&mut steps, step_key, &mut order);
                m.insert("status".into(), json!("started"));
                m.insert("attempts".into(), json!(attempt + 1));
                m.entry("started_at").or_insert(json!(e.at));
                m.insert("seq".into(), json!(e.seq));
                steps.insert(step_key.clone(), m);
            }
            Record::StepCompleted { step_key, class, duration_us, attempt, at, .. } => {
                let mut m = entry(&mut steps, step_key, &mut order);
                let (start, end) = span(e.at, *at, *duration_us);
                m.insert("status".into(), json!("completed"));
                m.insert("class".into(), json!(class));
                m.insert("duration_us".into(), json!(duration_us));
                m.insert("attempts".into(), json!(attempt + 1));
                m.insert("completed_at".into(), json!(end));
                m.entry("started_at").or_insert(json!(start));
                m.insert("seq".into(), json!(e.seq));
                steps.insert(step_key.clone(), m);
            }
            Record::StepFailed { step_key, error, duration_us, at, .. } => {
                let mut m = entry(&mut steps, step_key, &mut order);
                let (start, end) = span(e.at, *at, *duration_us);
                m.insert("status".into(), json!("failed"));
                m.insert("error".into(), error.clone());
                m.insert("duration_us".into(), json!(duration_us));
                m.insert("completed_at".into(), json!(end));
                m.entry("started_at").or_insert(json!(start));
                m.insert("seq".into(), json!(e.seq));
                steps.insert(step_key.clone(), m);
            }
            Record::TimerSet { wait_id, fire_at } => {
                let mut m = entry(&mut steps, wait_id, &mut order);
                m.insert("status".into(), json!("waiting"));
                m.insert("fire_at".into(), json!(fire_at));
                m.entry("started_at").or_insert(json!(e.at));
                steps.insert(wait_id.clone(), m);
            }
            Record::WaitForEvents { wait_id, correlation } => {
                let mut m = entry(&mut steps, wait_id, &mut order);
                m.insert("status".into(), json!("waiting"));
                m.insert("listening".into(), correlation.clone());
                m.entry("started_at").or_insert(json!(e.at));
                steps.insert(wait_id.clone(), m);
            }
            Record::ApprovalRequested { wait_id, request } => {
                let k = request.get("step_key").and_then(|k| k.as_str()).unwrap_or(wait_id).to_string();
                let mut m = entry(&mut steps, &k, &mut order);
                m.insert("status".into(), json!("waiting"));
                m.insert("asking".into(), json!({
                    "request_id": wait_id,
                    "tool": request.pointer("/context/tool"),
                    "approver": request.pointer("/context/approver"),
                    "reason": request.get("message"),
                    "expires_at": request.get("expires_at"),
                }));
                m.entry("started_at").or_insert(json!(e.at));
                steps.insert(k, m);
            }
            Record::ApprovalDecided { wait_id, decision, by } => {
                if let Some(m) = steps.values_mut().find(|m| m.get("asking").and_then(|a| a.get("request_id")).and_then(|v| v.as_str()) == Some(wait_id)) {
                    m.insert("status".into(), json!("completed"));
                    m.insert("answered".into(), json!({ "decision": decision, "by": by, "at": e.at }));
                    m.insert("completed_at".into(), json!(e.at));
                }
            }
            Record::DebugEdit { step_key, .. } => {
                let mut m = entry(&mut steps, step_key, &mut order);
                m.insert("edited".into(), json!(true));
                steps.insert(step_key.clone(), m);
            }
            Record::Paused { step_key, when, reason, input } => {
                paused = Some(json!({ "step_key": step_key, "when": when, "reason": reason, "seq": e.seq, "at": e.at, "input": mask(input) }));
            }
            Record::DebugCommand { .. } => paused = None,
            _ => {}
        }
    }
    (order.into_iter().filter_map(|k| steps.remove(&k).map(Value::Object)).collect(), paused)
}

async fn get_run(storage: &dyn StorageBackend, principal: &Principal, args: &Value) -> R {
    let (e, r) = readable(storage, principal, args).await?;
    let entries = e.read_from(r.run_id, 0).await.map_err(|e| e.to_string())?;
    let (steps, paused) = steps_of(&entries);
    let paused = if r.phase == RunPhase::Suspended { paused } else { None };
    let mut out = summary(&r);
    out["input"] = mask(&r.input);
    out["output"] = r.output.as_ref().map(mask).unwrap_or(Value::Null);
    out["breakpoints"] = r.breakpoints.clone().unwrap_or(Value::Null);
    out["steps"] = json!(steps);
    out["paused"] = paused.unwrap_or(Value::Null);
    out["last_seq"] = json!(r.last_seq);
    out["can_steer"] = json!(may(principal, &r.namespace, &r.flow, NsAccess::Write).await);
    Ok(CallToolResult::json(out))
}

async fn get_journal(storage: &dyn StorageBackend, principal: &Principal, args: &Value) -> R {
    let (e, r) = readable(storage, principal, args).await?;
    let after = args.get("after_seq").and_then(|v| v.as_u64()).unwrap_or(0);
    let entries = e.read_from(r.run_id, after).await.map_err(|e| e.to_string())?;
    let rows: Vec<Value> = entries
        .iter()
        .map(|e| {
            let mut rec = serde_json::to_value(&e.record).unwrap_or(Value::Null);
            for k in ["output", "context", "input", "error", "events", "request", "decision"] {
                if let Some(v) = rec.get(k).cloned() {
                    rec[k] = mask(&v);
                }
            }
            json!({ "seq": e.seq, "at": e.at, "epoch": e.epoch, "record": rec })
        })
        .collect();
    Ok(CallToolResult::json(json!({ "entries": rows, "last_seq": r.last_seq, "phase": r.phase })))
}

fn new_run(namespace: &str, name: &str, input: Value, principal: &Principal, debug: bool, breakpoints: Option<Value>) -> NewRun {
    NewRun {
        run_id: Uuid::now_v7(),
        namespace: namespace.to_string(),
        flow: name.to_string(),
        version: String::new(),
        def_hash: String::new(),
        parent_run_id: None,
        durability: Durability::Durable,
        debug,
        breakpoints: if debug { Some(breakpoints.unwrap_or_else(|| json!([]))) } else { None },
        principal: serde_json::to_value(principal).unwrap_or(Value::Null),
        trigger: if debug { "debug".into() } else { "api".into() },
        input,
        deadline_at: None,
        dedupe_key: None,
    }
}

async fn start_run(storage: &dyn StorageBackend, principal: &Principal, args: &Value) -> R {
    let e = engine(storage)?;
    let ns = s(args, "namespace").ok_or("namespace is required")?;
    let name = s(args, "name").ok_or("name is required")?;
    let debug = args.get("debug").and_then(|v| v.as_bool()).unwrap_or(false);
    let need = if debug { NsAccess::Write } else { NsAccess::Run };
    if !may(principal, ns, name, need).await {
        return Err(format!("Forbidden: this needs {} access to project '{ns}'.", if debug { "write" } else { "run" }));
    }
    storage.workflows().get(ns, name).await.map_err(|_| format!("Flow `{ns}/{name}` not found."))?;
    let mut bps = args.get("breakpoints").cloned().filter(|b| b.is_array()).unwrap_or_else(|| json!([]));
    if args.get("pause_on_start").and_then(|v| v.as_bool()).unwrap_or(false) {
        bps.as_array_mut().unwrap().push(json!({ "step_key": "*", "when": "before", "hit": 1 }));
    }
    let run = new_run(ns, name, args.get("input").cloned().unwrap_or(json!({})), principal, debug, Some(bps));
    let task = NewTask::start(&run);
    let (id, _) = e.start(run, task).await.map_err(|e| e.to_string())?;
    Ok(CallToolResult::json(json!({ "run_id": id, "debug": debug })))
}

async fn debug_run(storage: &dyn StorageBackend, principal: &Principal, args: &Value) -> R {
    let (e, r) = writable(storage, principal, args).await?;
    let command = s(args, "command").ok_or("command is required")?;
    if command == "stop" {
        e.request(r.run_id, RunRequest::Cancel { by: who(principal), reason: "stopped from the debugger".into() }).await.map_err(|e| e.to_string())?;
        return Ok(CallToolResult::json(json!({ "ok": true, "stopped": true })));
    }
    if !["continue", "step_over", "step_into", "step_out", "run_to"].contains(&command) {
        return Err(format!("unknown command `{command}`"));
    }
    let mut cmd = json!({ "command": command });
    if let Some(k) = s(args, "step_key") {
        cmd["step_key"] = json!(k);
    }
    e.request(r.run_id, RunRequest::Debug { command: cmd, by: who(principal) }).await.map_err(|e| e.to_string())?;
    Ok(CallToolResult::json(json!({ "ok": true })))
}

async fn set_breakpoints(storage: &dyn StorageBackend, principal: &Principal, args: &Value) -> R {
    let (e, r) = writable(storage, principal, args).await?;
    let bps = args.get("breakpoints").cloned().filter(|b| b.is_array()).ok_or("breakpoints must be an array")?;
    e.request(r.run_id, RunRequest::Breakpoints(bps)).await.map_err(|e| e.to_string())?;
    Ok(CallToolResult::json(json!({ "ok": true })))
}

async fn edit_step_input(storage: &dyn StorageBackend, principal: &Principal, args: &Value) -> R {
    let (e, r) = writable(storage, principal, args).await?;
    let key = s(args, "step_key").ok_or("step_key is required")?;
    let input = args.get("input").cloned().ok_or("input is required")?;
    let entries = e.read_from(r.run_id, 0).await.map_err(|e| e.to_string())?;
    let (_, paused) = steps_of(&entries);
    let at = paused.filter(|_| r.phase == RunPhase::Suspended);
    let ok = at.as_ref().is_some_and(|p| p["step_key"] == key && p["when"] == "before");
    if !ok {
        return Err(format!("the run is not paused before {key}; an input can be edited only there"));
    }
    e.append(r.run_id, r.epoch, &[Record::DebugEdit { step_key: key.to_string(), input, by: who(principal) }])
        .await
        .map_err(|e| format!("the run moved on; try again ({e})"))?;
    Ok(CallToolResult::json(json!({ "ok": true })))
}

async fn cancel_run(storage: &dyn StorageBackend, principal: &Principal, args: &Value) -> R {
    let (e, r) = writable(storage, principal, args).await?;
    let reason = s(args, "reason").unwrap_or("cancelled").to_string();
    e.request(r.run_id, RunRequest::Cancel { by: who(principal), reason }).await.map_err(|e| e.to_string())?;
    Ok(CallToolResult::json(json!({ "ok": true })))
}

async fn send_event(storage: &dyn StorageBackend, principal: &Principal, args: &Value) -> R {
    let e = engine(storage)?;
    let ns = s(args, "namespace").ok_or("namespace is required")?;
    let given = args.get("event").filter(|v| v.is_object()).ok_or("event must be an object")?;
    if given.get("type").and_then(|t| t.as_str()).is_none_or(str::is_empty) {
        return Err("event.type is required".into());
    }
    let g = namespace_guard();
    let allowed = !g.enforcing() || matches!(principal, Principal::Service { .. }) || g.access(principal, ns).await >= NsAccess::Run;
    if !allowed {
        return Err(format!("Forbidden: this needs run access to project '{ns}'."));
    }
    let event = cloud_event(given, &format!("/a3t/{}", who(principal)));
    if let Some(sink) = EVENT_SINK.get() {
        let out = sink(ns.to_string(), event, principal.clone()).await?;
        return Ok(CallToolResult::json(out));
    }
    let woke = e.deliver_event(ns, event.clone()).await.map_err(|e| e.to_string())?;
    Ok(CallToolResult::json(json!({ "id": event["id"], "woke": woke, "started": [] })))
}

async fn rerun_run(storage: &dyn StorageBackend, principal: &Principal, args: &Value) -> R {
    let (e, src) = writable(storage, principal, args).await?;
    let debug = args.get("debug").and_then(|v| v.as_bool()).unwrap_or(true);
    let input = args.get("input").cloned().unwrap_or_else(|| src.input.clone());
    let from = args.get("from_seq").and_then(|v| v.as_u64()).unwrap_or(0);
    let mut run = new_run(&src.namespace, &src.flow, input, principal, debug, args.get("breakpoints").cloned());
    run.parent_run_id = None;
    run.trigger = format!("rerun:{}", src.run_id);
    // Steps up to `from` replay in the new run: copy their records before any
    // worker can claim it (the task becomes visible a moment later).
    let seed: Vec<Record> = if from > 0 {
        e.read_from(src.run_id, 0)
            .await
            .map_err(|e| e.to_string())?
            .into_iter()
            .filter(|x| x.seq <= from)
            .map(|x| x.record)
            .filter(|r| matches!(r, Record::RunStarted { .. } | Record::StepStarted { .. } | Record::StepCompleted { .. } | Record::StepFailed { .. } | Record::TimerSet { .. }))
            .collect()
    } else {
        Vec::new()
    };
    let mut task = NewTask::start(&run);
    task.visible_at = chrono::Utc::now() + chrono::Duration::milliseconds(if seed.is_empty() { 0 } else { 1500 });
    let (id, _) = e.start(run, task).await.map_err(|e| e.to_string())?;
    if !seed.is_empty() {
        e.append(id, 0, &seed).await.map_err(|e| e.to_string())?;
    }
    Ok(CallToolResult::json(json!({ "run_id": id, "from_seq": from, "replayed_records": seed.len(), "debug": debug })))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn masks_secrets_but_not_references() {
        let v = json!({ "api_key": "sk-123456789", "token_ref": "a3t/tok", "nested": [{ "password": "x" }], "step_key": "/a", "n": 1 });
        let m = mask(&v);
        assert_eq!(m["api_key"], "****");
        assert_eq!(m["token_ref"], "a3t/tok");
        assert_eq!(m["nested"][0]["password"], "****");
        assert_eq!(m["step_key"], "/a");
        assert_eq!(m["n"], 1);
    }
}
