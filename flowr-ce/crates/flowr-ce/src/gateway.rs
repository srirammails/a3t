//! The gateway (`.ai/A3T_CLUSTER_ARCHITECTURE.md` §10, G2 + G3): every outbound call
//! of a runtime goes through one pool, `gateway`, which decides and acts.
//!
//! **Client (runtimes, `FLOWR_GATEWAY=pool`).** The outbound handlers
//! ([`OUTBOUND`]) are replaced by [`GatewayClient`]: the step starts a run of
//! `system-gateway/call` (routed to pool `gateway`, see [`crate::router`]) carrying
//! the resolved task, the step input and the caller, and waits for its outcome. The
//! start is deduplicated on the caller's step key, so a step that runs again after a
//! crash finds the same gateway run and nothing is called twice. The runtime holds no
//! downstream credentials.
//!
//! **Gateway (pool `gateway`).** `system-gateway/call` is one step,
//! `call:a3t:gateway` ([`GatewayStep`]): Verify → Resolve → Decide → Act → Record.
//! Decide is a placeholder ([`GatewayPolicy`]): it allows everything and says so;
//! `FLOWR_GATEWAY_DENY` (comma-separated substrings of the target) denies, for
//! tests. Act runs the real handler in this process, as the caller (its namespace
//! and principal). Record writes a `gateway_decisions` row to the platform database.

use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use futures::StreamExt;
use serde_json::{json, Value};
use swf_parser::NamedTask;
use swf_runtime::context::ExecutionContext;
use swf_runtime::error::{Result, RuntimeError};
use swf_runtime::traits::{TaskHandler, TaskResult};
use tracing::{info, warn};

use flowr_storage::engine::{run_topic, Durability, NewRun, NewTask, RunPhase};
use flowr_storage::StorageBackend;

use crate::router::{GATEWAY_NAMESPACE, GATEWAY_POOL};

/// The gateway flow and its one step type.
pub const GATEWAY_FLOW: &str = "call";
pub const GATEWAY_STEP: &str = "call:a3t:gateway";

/// The handlers whose calls leave A3T.
pub const OUTBOUND: &[&str] = &["call:http", "call:mcp", "call:llm"];

/// `FLOWR_GATEWAY=pool`: this process sends its outbound calls to the gateway pool.
pub fn client_enabled() -> bool {
    std::env::var("FLOWR_GATEWAY").map(|v| v == "pool").unwrap_or(false)
}

/// Whether this process serves the gateway pool.
pub fn serves_gateway() -> bool {
    crate::engine_worker::worker_pools().iter().any(|p| p == GATEWAY_POOL)
}

/// A fresh instance of the real handler for an outbound task type.
fn real_handler(task_type: &str) -> Option<Arc<dyn TaskHandler>> {
    Some(match task_type {
        "call:http" => Arc::new(swf_runtime::handlers::call_http::HttpHandler::new()),
        "call:mcp" => Arc::new(crate::handlers::call_mcp::McpHandler::new()),
        "call:llm" => Arc::new(crate::handlers::call_llm::LlmHandler::new()),
        _ => return None,
    })
}

/// Register the gateway: the client wrappers when `FLOWR_GATEWAY=pool`, and the
/// gateway step everywhere (it only ever runs in gateway flows).
pub fn register(executor: &mut swf_runtime::executor::WorkflowExecutor) {
    if client_enabled() {
        for t in OUTBOUND {
            if let Some(inner) = real_handler(t) {
                executor.register_handler(Arc::new(GatewayClient { inner }));
            }
        }
    }
    executor.register_handler(Arc::new(GatewayStep));
}

fn storage_of(ctx: &ExecutionContext) -> Option<Arc<dyn StorageBackend>> {
    ctx.storage.as_ref().and_then(|s| s.downcast_ref::<Arc<dyn StorageBackend>>().cloned())
}

// ------------------------------------------------------------------ client side

/// An outbound handler on a runtime: the call runs in the gateway pool.
pub struct GatewayClient {
    inner: Arc<dyn TaskHandler>,
}

#[async_trait]
impl TaskHandler for GatewayClient {
    fn task_type(&self) -> &str {
        self.inner.task_type()
    }

    // The runtime's own choke point still sees the real target.
    fn policy_resource(&self, task: &NamedTask) -> flowr_policy::Resource {
        self.inner.policy_resource(task)
    }

    fn policy_action(&self) -> flowr_policy::Action {
        self.inner.policy_action()
    }

    async fn execute(&self, task: &NamedTask, input: Value, ctx: &mut ExecutionContext) -> Result<TaskResult> {
        let storage = storage_of(ctx).ok_or_else(|| RuntimeError::Internal("gateway: storage not available".into()))?;
        let engine = storage
            .engine()
            .ok_or_else(|| RuntimeError::task_failed(&task.name, "gateway: this process has no engine store, so it cannot reach the gateway pool"))?;
        let step = swf_runtime::effects::step_key();
        let request = json!({
            "task_type": self.inner.task_type(),
            "task": task,
            "input": input,
            "caller": {
                "run_id": ctx.instance_id,
                "step": step,
                "namespace": ctx.namespace,
                "flow": ctx.name,
                "principal": ctx.principal,
            },
        });
        let run = NewRun {
            run_id: uuid::Uuid::now_v7(),
            namespace: GATEWAY_NAMESPACE.to_string(),
            flow: GATEWAY_FLOW.to_string(),
            version: String::new(),
            def_hash: String::new(),
            parent_run_id: uuid::Uuid::parse_str(&ctx.instance_id).ok(),
            durability: Durability::Durable,
            debug: false,
            breakpoints: None,
            principal: serde_json::to_value(&ctx.principal).unwrap_or(Value::Null),
            trigger: "gateway".into(),
            input: request,
            deadline_at: None,
            // The same gateway run for every attempt of this step.
            dedupe_key: step.as_ref().map(|k| format!("gateway:{}:{k}", ctx.instance_id)),
        };
        // Explicitly the gateway pool: a runtime has no router installed (only the
        // process holding the store does), and without one everything goes to `default`.
        let mut start = NewTask::start(&run);
        start.pool = GATEWAY_POOL.to_string();
        let (id, _) = engine.start(run, start).await.map_err(|e| RuntimeError::task_failed(&task.name, format!("gateway: {e}")))?;
        let mut notices = engine.subscribe(&run_topic(id));
        let t0 = Instant::now();
        loop {
            match engine.get(id).await {
                Ok(Some(row)) => match row.phase {
                    RunPhase::Completed => {
                        tracing::debug!(task = %task.name, gateway_run = %id, took_ms = t0.elapsed().as_millis() as u64, "gateway call done");
                        return Ok(TaskResult::new(row.output.unwrap_or(Value::Null)));
                    }
                    RunPhase::Faulted | RunPhase::Cancelled => {
                        return Err(RuntimeError::task_failed(&task.name, row.error.unwrap_or_else(|| "gateway call failed".into())));
                    }
                    _ => {}
                },
                Ok(None) => return Err(RuntimeError::task_failed(&task.name, format!("gateway run {id} vanished"))),
                Err(e) => warn!(gateway_run = %id, error = %e, "gateway: reading the run failed; retrying"),
            }
            tokio::select! {
                _ = notices.next() => {}
                _ = tokio::time::sleep(Duration::from_millis(500)) => {}
            }
        }
    }
}

// ----------------------------------------------------------------- gateway side

/// The gateway's decision.
#[derive(Debug, Clone)]
pub struct Decision {
    pub allow: bool,
    pub reason: String,
}

/// Decide whether a call may go out. The policy layer (Cedar, identity) comes later;
/// until then [`PlaceholderPolicy`] is the only implementation.
pub trait GatewayPolicy: Send + Sync {
    fn decide(&self, call: &Call) -> Decision;
}

/// Allows everything, unless `FLOWR_GATEWAY_DENY` names a substring of the target.
pub struct PlaceholderPolicy;

impl GatewayPolicy for PlaceholderPolicy {
    fn decide(&self, call: &Call) -> Decision {
        let deny = std::env::var("FLOWR_GATEWAY_DENY").unwrap_or_default();
        if let Some(p) = deny.split(',').map(str::trim).filter(|p| !p.is_empty()).find(|p| call.target.contains(p)) {
            return Decision { allow: false, reason: format!("denied by FLOWR_GATEWAY_DENY ({p})") };
        }
        Decision { allow: true, reason: "policy placeholder: allow".into() }
    }
}

/// One call as the gateway sees it.
#[derive(Debug, Clone)]
pub struct Call {
    pub task_type: String,
    /// What is called: the URL, `mcp:<server>/<tool>`, `llm:<resource>`.
    pub target: String,
    pub caller_run: String,
    pub caller_step: Option<String>,
    pub namespace: String,
    pub flow: String,
    pub principal: Value,
}

fn target_of(task_type: &str, task: &NamedTask) -> String {
    let with = match &task.task {
        swf_parser::Task::Call(c) => c.with.clone().unwrap_or(Value::Null),
        _ => Value::Null,
    };
    let s = |k: &str| with.get(k).and_then(|v| v.as_str()).unwrap_or_default().to_string();
    match task_type {
        "call:http" => {
            let ep = with.get("endpoint");
            ep.and_then(|e| e.as_str().map(String::from).or_else(|| e.get("uri").and_then(|u| u.as_str()).map(String::from)))
                .unwrap_or_default()
        }
        "call:mcp" => format!("mcp:{}/{}", s("server").if_empty(s("resource")), s("tool")),
        "call:llm" => format!("llm:{}", s("resource").if_empty(s("model"))),
        other => other.to_string(),
    }
}

trait IfEmpty {
    fn if_empty(self, other: String) -> String;
}
impl IfEmpty for String {
    fn if_empty(self, other: String) -> String {
        if self.is_empty() {
            other
        } else {
            self
        }
    }
}

/// Where the call goes: today always A3T's own gateway (this process). An org
/// assigned an external gateway (`orgs.settings.gateway.kind = external`) is G-later.
fn resolve(_call: &Call) -> &'static str {
    "a3t"
}

/// `call:a3t:gateway`: the gateway flow's one step.
pub struct GatewayStep;

#[async_trait]
impl TaskHandler for GatewayStep {
    fn task_type(&self) -> &str {
        GATEWAY_STEP
    }

    async fn execute(&self, task: &NamedTask, input: Value, ctx: &mut ExecutionContext) -> Result<TaskResult> {
        let fail = |m: String| RuntimeError::task_failed(&task.name, m);
        // Verify (placeholder: the caller must be named; identity tokens come with AC-11).
        let caller = input.get("caller").cloned().unwrap_or(Value::Null);
        let task_type = input.get("task_type").and_then(|t| t.as_str()).unwrap_or_default().to_string();
        let call_task: NamedTask = serde_json::from_value(input.get("task").cloned().unwrap_or(Value::Null))
            .map_err(|e| fail(format!("gateway: bad request: {e}")))?;
        let caller_run = caller.get("run_id").and_then(|v| v.as_str()).unwrap_or_default().to_string();
        if caller_run.is_empty() {
            return Err(fail("gateway: the request names no caller".into()));
        }
        let call = Call {
            target: target_of(&task_type, &call_task),
            task_type: task_type.clone(),
            caller_run,
            caller_step: caller.get("step").and_then(|v| v.as_str()).map(String::from),
            namespace: caller.get("namespace").and_then(|v| v.as_str()).unwrap_or_default().to_string(),
            flow: caller.get("flow").and_then(|v| v.as_str()).unwrap_or_default().to_string(),
            principal: caller.get("principal").cloned().unwrap_or(Value::Null),
        };
        // Resolve.
        let via = resolve(&call);
        // Decide.
        let decision = PlaceholderPolicy.decide(&call);
        let t0 = Instant::now();
        let outcome: std::result::Result<Value, String> = if !decision.allow {
            Err(format!("denied by the gateway: {}", decision.reason))
        } else {
            // Act, as the caller.
            match real_handler(&task_type) {
                None => Err(format!("gateway: {task_type} is not an outbound call it handles")),
                Some(h) => {
                    let saved = (ctx.namespace.clone(), ctx.principal.clone());
                    ctx.namespace = call.namespace.clone();
                    if let Ok(p) = serde_json::from_value(call.principal.clone()) {
                        ctx.principal = p;
                    }
                    let r = h.execute(&call_task, input.get("input").cloned().unwrap_or(Value::Null), ctx).await;
                    (ctx.namespace, ctx.principal) = saved;
                    r.map(|t| t.output).map_err(|e| e.to_string())
                }
            }
        };
        let took = t0.elapsed();
        info!(
            target = %call.target, task_type = %call.task_type, caller_run = %call.caller_run, namespace = %call.namespace,
            via, allow = decision.allow, reason = %decision.reason, ok = outcome.is_ok(), took_ms = took.as_millis() as u64,
            "gateway decision"
        );
        // Record.
        if let Some(storage) = storage_of(ctx) {
            record(storage.as_ref(), &ctx.instance_id, &call, via, &decision, &outcome, took).await;
        }
        outcome.map(TaskResult::new).map_err(fail)
    }
}

/// A `gateway_decisions` row in the platform database (best effort: a failed write
/// is logged, never fails the call — the gateway run's journal holds the same facts).
async fn record(storage: &dyn StorageBackend, gateway_run: &str, call: &Call, via: &str, d: &Decision, outcome: &std::result::Result<Value, String>, took: Duration) {
    use crate::handlers::call_lancedb::{DEFAULT_PLATFORM_DB_PATH, PLATFORM_DB_RESOURCE};
    let db = crate::platform_db::open_platform_db(storage, Some(PLATFORM_DB_RESOURCE), DEFAULT_PLATFORM_DB_PATH).await;
    let principal = call.principal.get("id").and_then(|v| v.as_str()).map(String::from).unwrap_or_else(|| call.principal.to_string());
    let row = json!({
        "id": gateway_run,
        "at": chrono::Utc::now().to_rfc3339(),
        "namespace": call.namespace,
        "flow": call.flow,
        "caller_run": call.caller_run,
        "caller_step": call.caller_step.clone().unwrap_or_default(),
        "principal": principal,
        "task_type": call.task_type,
        "target": call.target,
        "via": via,
        "decision": if d.allow { "allow" } else { "deny" },
        "reason": d.reason,
        "outcome": match outcome { Ok(_) => "ok".to_string(), Err(e) => format!("error: {e}") },
        "took_ms": took.as_millis().to_string(),
    });
    if let Err(e) = db.insert("gateway_decisions", &[row]).await {
        warn!(error = %e, "gateway: recording the decision failed");
    }
}

/// The gateway flow, seeded into the registry by gateway processes.
pub const GATEWAY_FLOW_YAML: &str = r#"document:
  dsl: '1.0.3'
  namespace: system-gateway
  name: call
  version: '1.0.0'
  summary: One outbound call through the A3T gateway (verify, resolve, decide, act, record).
do:
  - call:
      call: a3t:gateway
      # A gateway that dies mid-call is replaced by another, which runs the call
      # again: HTTP carries the caller step's Idempotency-Key, so the receiver can
      # drop the repeat (MCP and LLM calls have no such key yet).
      metadata: { a3t: { stepClass: idempotent, inDoubt: retry } }
"#;

/// Make sure `system-gateway/call` is in the registry (gateway processes, at start).
pub async fn seed(storage: &dyn StorageBackend) {
    match storage.workflows().get(GATEWAY_NAMESPACE, GATEWAY_FLOW).await {
        Ok(r) if r.raw_yaml == GATEWAY_FLOW_YAML => {}
        _ => match flowr_types::WorkflowRecord::from_yaml(GATEWAY_FLOW_YAML) {
            Ok(rec) => match storage.workflows().register(rec).await {
                Ok(()) => info!("gateway flow system-gateway/call registered"),
                Err(e) => warn!(error = %e, "gateway: registering system-gateway/call failed"),
            },
            Err(e) => warn!(error = %e, "gateway: the built-in flow does not parse"),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn call(target: &str) -> Call {
        Call {
            task_type: "call:http".into(),
            target: target.into(),
            caller_run: "r".into(),
            caller_step: None,
            namespace: "acme".into(),
            flow: "f".into(),
            principal: Value::Null,
        }
    }

    #[test]
    fn the_placeholder_allows_unless_a_deny_pattern_matches() {
        std::env::remove_var("FLOWR_GATEWAY_DENY");
        assert!(PlaceholderPolicy.decide(&call("https://api.example.com/x")).allow);
        std::env::set_var("FLOWR_GATEWAY_DENY", "evil.example, other");
        let d = PlaceholderPolicy.decide(&call("https://evil.example/x"));
        assert!(!d.allow && d.reason.contains("evil.example"));
        assert!(PlaceholderPolicy.decide(&call("https://api.example.com/x")).allow);
        std::env::remove_var("FLOWR_GATEWAY_DENY");
    }

    #[test]
    fn the_gateway_flow_parses() {
        let rec = flowr_types::WorkflowRecord::from_yaml(GATEWAY_FLOW_YAML).expect("parses");
        assert_eq!((rec.namespace(), rec.name()), (GATEWAY_NAMESPACE, GATEWAY_FLOW));
    }
}
