//! ClawdFlow Server Library
//!
//! Reusable server components for both `clawdflow` CE and `flowr` EE binaries.
//!
//! Exposes `build_ce_router()` which returns an Axum Router with all CE endpoints.
//! The FlowR EE binary can merge additional routes (UI, etc.) before serving.

pub(crate) mod server;
pub mod approval;
pub mod token_stream;
pub mod canvas;
pub mod handlers;
pub mod registry;

use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{
        sse::{Event, Sse},
        IntoResponse, Json,
    },
    routing::{get, post},
    Extension, Router,
};
use flowr_ce_storage::LanceStorage;
use flowr_mcp_core::{
    error_codes, get_tools, handle_tool_call, CallToolParams, InitializeParams, InitializeResult,
    JsonRpcRequest, JsonRpcResponse, Principal, ServerCapabilities, ServerInfo, ToolsCapability,
    ToolsListResult,
};
use flowr_storage::StorageBackend;
use flowr_types::{ExecutionRequest, ExecutionState, ExecutionStatus};
use futures::stream::{self, Stream};
use serde_json::{json, Value};
use tokio::sync::broadcast;
use tower_http::cors::{Any, CorsLayer};
use tracing::{debug, error, info, warn};
use uuid::Uuid;

// Re-export crates for downstream use
pub use flowr_ce_storage;
pub use flowr_ce_agents;
pub use flowr_storage;
pub use flowr_types;

/// Server configuration
#[derive(Debug, Clone)]
pub struct ServerConfig {
    pub port: u16,
    pub mode: String,
    pub data_dir: String,
    pub ui_enabled: bool,
}

impl Default for ServerConfig {
    fn default() -> Self {
        let data_dir = std::env::var("CLAWDFLOW_DATA_DIR").unwrap_or_else(|_| {
            dirs::data_local_dir()
                .map(|p| p.join("clawdflow").to_string_lossy().to_string())
                .unwrap_or_else(|| ".clawdflow".to_string())
        });
        Self {
            port: 3002,
            mode: "http".to_string(),
            data_dir,
            ui_enabled: false,
        }
    }
}

/// Application state shared across all handlers
pub struct AppState {
    pub storage: Arc<dyn StorageBackend>,
    pub tx: broadcast::Sender<String>,
}

/// Global storage reference for trigger/template persistence
static GLOBAL_STORAGE: std::sync::OnceLock<Arc<dyn StorageBackend>> = std::sync::OnceLock::new();

/// Initialize LanceDB storage
pub async fn init_storage(data_dir: &str) -> anyhow::Result<Arc<dyn StorageBackend>> {
    std::fs::create_dir_all(data_dir)?;
    let db_path = format!("{}/clawdflow.lance", data_dir);
    info!("Using database at: {}", db_path);

    let storage = LanceStorage::new(&db_path)
        .await
        .map_err(|e| anyhow::anyhow!("{}", e))?;
    let storage = Arc::new(storage);
    storage.init().await.map_err(|e| anyhow::anyhow!("{}", e))?;

    info!("Storage initialized successfully");
    Ok(storage)
}

/// Restore persisted triggers from state store
pub async fn restore_triggers(storage: &Arc<dyn StorageBackend>) {
    if let Ok(Some(data)) = storage.state().get("_triggers").await {
        match flowr_ce_agents::trigger_manager().import_all(&data).await {
            Ok(count) => info!(count = count, "Triggers restored"),
            Err(e) => warn!(error = %e, "Failed to restore triggers"),
        }
    }
}

/// Persist triggers to state store
pub async fn persist_triggers() {
    if let Some(storage) = GLOBAL_STORAGE.get() {
        let data = flowr_ce_agents::trigger_manager().export_all().await;
        if let Err(e) = storage.state().set("_triggers", data, None).await {
            warn!(error = %e, "Failed to persist triggers");
        }
    }
}

/// Build the CE router with all ClawdFlow endpoints.
/// Returns a Router that can be extended with additional routes.
pub fn build_ce_router(storage: Arc<dyn StorageBackend>) -> Router {
    let _ = GLOBAL_STORAGE.set(storage.clone());

    let registry_routes = registry::registry_router(storage.clone());
    let canvas_routes = canvas::canvas_router(storage.clone());

    let (tx, _) = broadcast::channel::<String>(100);
    let state = Arc::new(AppState { storage, tx });

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    Router::new()
        // MCP protocol
        .route("/sse", get(sse_handler).post(sse_message_handler))
        .route("/message", post(message_handler))
        // Live agent token stream (CE + EE). Keyed by session_id so the client can
        // attach before its run has an instance id.
        .route("/chat/stream/:session_id", get(chat_stream_handler))
        // AI SDK UI Message Stream (v1) — one request that starts the run and
        // streams it back, for @ai-sdk/svelte's Chat client.
        .route("/chat", post(chat_ui_stream_handler))
        // Chat history, owned by the signed-in principal so it follows the user
        // across devices instead of living in one browser's localStorage.
        // Per-user UI pins (pinned conversations, flows, resources …). Keyed by the
        // caller, so there is nothing cross-user to authorize beyond being signed in.
        .route("/ui/pins", get(list_pins_handler).post(set_pin_handler))
        .route("/chat/sessions", get(list_chat_sessions_handler))
        // Permissions answered in a conversation: the user's own view of them,
        // and the only way to lift a refusal. Deliberately NOT agent-callable.
        .route("/chat/sessions/:session_id/permissions", get(list_permissions_handler))
        .route("/chat/sessions/:session_id/permissions/lift", post(lift_permission_handler))
        .route("/chat/sessions/:session_id", get(get_chat_session_handler)
                                            .delete(delete_chat_session_handler))
        .route("/health", get(health_handler))
        // Agent management
        .route("/agents", get(list_agents_handler))
        .route("/agents/:agent_id", get(get_agent_handler))
        .route("/agents/:agent_id/history", get(agent_history_handler))
        .route("/agents/:agent_id/health", get(agent_health_handler))
        .route("/agents/:agent_id/metrics", get(agent_metrics_handler))
        // Triggers
        .route("/triggers", get(list_triggers_handler).post(create_trigger_handler))
        .route("/triggers/:trigger_id", get(get_trigger_handler).delete(delete_trigger_handler))
        .route("/triggers/:trigger_id/enable", post(enable_trigger_handler))
        .route("/triggers/:trigger_id/pause", post(pause_trigger_handler))
        .route("/triggers/:trigger_id/attach", post(attach_trigger_handler))
        .route("/triggers/:trigger_id/detach", post(detach_trigger_handler))
        // Templates
        .route("/templates", get(list_templates_handler).post(create_template_handler))
        .route("/templates/:name", get(get_template_handler).delete(delete_template_handler))
        .route("/templates/:name/:version", get(get_template_version_handler))
        .route("/templates/:name/activate/:version", post(activate_template_handler))
        // Human-in-the-loop input endpoints
        .route("/inputs", get(list_inputs_handler))
        .route("/inputs/:request_id", get(get_input_handler).post(submit_input_handler))
        // Webhook (exact form + catch-all path form, e.g. /webhook/agent/{id}/chat)
        .route("/webhook", post(webhook_handler))
        .route("/webhook/*path", post(webhook_handler))
        .layer(cors)
        .with_state(state)
        // Registry API (has its own state, merged after with_state)
        .merge(Router::new().nest("/registry", registry_routes))
        // Canvas deploy path (own state)
        .merge(Router::new().nest("/canvas", canvas_routes))
}

/// Spawn the background execution worker
pub fn spawn_worker(storage: Arc<dyn StorageBackend>) {
    tokio::spawn(async move {
        run_worker(storage).await;
    });
}

/// Run the HTTP server with the given router
pub async fn serve(router: Router, port: u16) -> anyhow::Result<()> {
    let addr = format!("0.0.0.0:{}", port);
    let listener = tokio::net::TcpListener::bind(&addr).await?;

    info!(addr = %addr, "HTTP/SSE server listening");
    info!(
        "Claude Desktop config: {{ \"mcpServers\": {{ \"clawdflow\": {{ \"url\": \"http://<host>:{}/sse\" }} }} }}",
        port
    );

    axum::serve(listener, router).await?;
    Ok(())
}

/// Run the stdio MCP server
pub async fn run_stdio(storage: Arc<dyn StorageBackend>) -> anyhow::Result<()> {
    run_stdio_server(storage).await
}

// =============================================================================
// All handler implementations below. These are used by build_ce_router().
// =============================================================================

// Include all handlers, worker, SSE, MCP protocol handlers
// These were previously in main.rs — now they live here so both
// clawdflow (CE binary) and flowr (EE binary) can use them.

async fn health_handler() -> impl IntoResponse {
    Json(json!({
        "status": "ok",
        "service": "clawdflow",
        "version": env!("CARGO_PKG_VERSION")
    }))
}

// --- Agent handlers ---

async fn list_agents_handler() -> impl IntoResponse {
    Json(json!({ "agents": [], "count": 0, "note": "CNCF workflow agents — use MCP tools (list_agents, send_message) instead" }))
}

async fn get_agent_handler(
    axum::extract::Path(agent_id): axum::extract::Path<String>,
) -> impl IntoResponse {
    (StatusCode::OK, Json(json!({"agent_id": agent_id, "note": "Use get_agent MCP tool"})))
}

async fn agent_history_handler(
    axum::extract::Path(agent_id): axum::extract::Path<String>,
) -> impl IntoResponse {
    (StatusCode::OK, Json(json!({"agent_id": agent_id, "note": "Use get_agent_history MCP tool"})))
}

async fn agent_health_handler(
    axum::extract::Path(agent_id): axum::extract::Path<String>,
) -> impl IntoResponse {
    (StatusCode::OK, Json(json!({"agent_id": agent_id, "status": "healthy"})))
}

async fn agent_metrics_handler(
    axum::extract::Path(agent_id): axum::extract::Path<String>,
) -> impl IntoResponse {
    (StatusCode::OK, Json(json!({"agent_id": agent_id, "metrics": {}})))
}

// --- Human input handlers ---

/// List all pending input requests
async fn list_inputs_handler() -> impl IntoResponse {
    let pending = flowr_ce_agents::input_manager().list_pending().await;
    let count = pending.len();
    Json(json!({ "inputs": pending, "count": count }))
}

/// Get a specific pending input request
async fn get_input_handler(
    axum::extract::Path(request_id): axum::extract::Path<String>,
) -> impl IntoResponse {
    match flowr_ce_agents::input_manager().get_pending(&request_id).await {
        Some(req) => (StatusCode::OK, Json(json!(req))),
        None => (StatusCode::NOT_FOUND, Json(json!({"error": format!("Input request '{}' not found or expired", request_id)}))),
    }
}

/// Submit a response to a pending input request (resumes the workflow)
async fn submit_input_handler(
    axum::extract::Path(request_id): axum::extract::Path<String>,
    principal: Option<Extension<Principal>>,
    Json(body): Json<Value>,
) -> axum::response::Response {
    // An `authz` request is an authorization decision, so answering one is too.
    // Previously any caller holding a request id could answer any pending
    // request, unauthenticated — which for approvals would mean granting an
    // agent a permission on someone else's behalf. The request names the
    // principal entitled to answer it; anyone else is told it does not exist,
    // so ids cannot be probed.
    let pending = flowr_ce_agents::input_manager().get_pending(&request_id).await;
    if let Some(req) = &pending {
        if req.request_type == "authz" {
            let expected = req
                .context
                .as_ref()
                .and_then(|c| c.get("approver"))
                .and_then(|v| v.as_str());
            let caller = principal.as_ref().map(|Extension(p)| p.id());
            match (expected, caller) {
                (Some(want), Some(got)) if want == got => {}
                _ => {
                    warn!(request_id = %request_id,
                          "rejected approval answer from a principal that is not the approver");
                    return (StatusCode::NOT_FOUND,
                        Json(json!({"error": "input request not found"}))).into_response();
                }
            }
        }
    }
    let responded_by = principal.as_ref().map(|Extension(p)| p.id().to_string());

    let response = flowr_ce_agents::InputResponse {
        request_id: request_id.clone(),
        data: body,
        responded_at: chrono::Utc::now().to_rfc3339(),
        responded_by,
    };

    match flowr_ce_agents::input_manager().submit_response(response).await {
        Ok(()) => {
            info!(request_id = %request_id, "Human input submitted, workflow resumed");
            Json(json!({"submitted": true, "request_id": request_id})).into_response()
        }
        Err(e) => {
            Json(json!({"error": e.to_string(), "request_id": request_id})).into_response()
        }
    }
}

// --- Webhook handler ---

async fn webhook_handler(
    path: Option<axum::extract::Path<String>>,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
    // Attached by the auth middleware when the webhook carried a valid token
    // (opportunistic auth on the public /webhook path). Absent for external webhooks.
    principal: Option<axum::Extension<flowr_policy::Principal>>,
    Json(body): Json<Value>,
) -> impl IntoResponse {
    // Prefer the catch-all path segment (/webhook/agent/x/chat), then ?path=, then body._trigger_path.
    let trigger_path = path
        .map(|p| if p.0.starts_with('/') { p.0.clone() } else { format!("/{}", p.0) })
        .or_else(|| params.get("path").cloned())
        .or_else(|| body.get("_trigger_path").and_then(|v| v.as_str()).map(|s| s.to_string()))
        .unwrap_or_else(|| "/webhook".to_string());
    info!(path = %trigger_path, "Webhook received");

    let agent_delivered: Vec<String> = Vec::new();

    let trigger_adapter = TriggerAgentAdapter;
    let trigger_wf_adapter = TriggerWorkflowAdapter {
        principal: principal.map(|axum::Extension(p)| p),
    };
    let trigger_results = flowr_ce_agents::trigger_manager().deliver_webhook(
        &trigger_path, body, &trigger_adapter, &trigger_wf_adapter
    ).await;

    let mut total: Vec<String> = agent_delivered;
    for r in &trigger_results {
        total.extend(r.delivered_to.iter().cloned());
    }

    Json(json!({"delivered": total.len(), "targets": total, "path": trigger_path}))
}

// --- Trigger handlers ---

async fn list_triggers_handler() -> impl IntoResponse {
    let triggers = flowr_ce_agents::trigger_manager().list().await;
    Json(json!({ "triggers": triggers, "count": triggers.len() }))
}

async fn create_trigger_handler(Json(body): Json<Value>) -> impl IntoResponse {
    let trigger: flowr_ce_agents::Trigger = match serde_json::from_value(body) {
        Ok(t) => t,
        Err(e) => return (StatusCode::BAD_REQUEST, Json(json!({"error": format!("Invalid trigger: {}", e)}))),
    };
    let name = trigger.name.clone();
    match flowr_ce_agents::trigger_manager().create(trigger).await {
        Ok(id) => { persist_triggers().await; (StatusCode::CREATED, Json(json!({"id": id, "name": name, "created": true}))) }
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e.to_string()}))),
    }
}

async fn get_trigger_handler(axum::extract::Path(id): axum::extract::Path<String>) -> impl IntoResponse {
    match flowr_ce_agents::trigger_manager().get(&id).await {
        Some(t) => (StatusCode::OK, Json(json!(t))),
        None => (StatusCode::NOT_FOUND, Json(json!({"error": format!("Trigger '{}' not found", id)}))),
    }
}

async fn delete_trigger_handler(axum::extract::Path(id): axum::extract::Path<String>) -> impl IntoResponse {
    match flowr_ce_agents::trigger_manager().delete(&id).await {
        Ok(()) => { persist_triggers().await; Json(json!({"deleted": true, "id": id})) }
        Err(e) => Json(json!({"error": e.to_string()})),
    }
}

async fn enable_trigger_handler(axum::extract::Path(id): axum::extract::Path<String>) -> impl IntoResponse {
    match flowr_ce_agents::trigger_manager().enable(&id).await {
        Ok(()) => { persist_triggers().await; Json(json!({"enabled": true, "id": id})) }
        Err(e) => Json(json!({"error": e.to_string()})),
    }
}

async fn pause_trigger_handler(axum::extract::Path(id): axum::extract::Path<String>) -> impl IntoResponse {
    match flowr_ce_agents::trigger_manager().pause(&id).await {
        Ok(()) => { persist_triggers().await; Json(json!({"paused": true, "id": id})) }
        Err(e) => Json(json!({"error": e.to_string()})),
    }
}

async fn attach_trigger_handler(
    axum::extract::Path(id): axum::extract::Path<String>,
    Json(target): Json<flowr_ce_agents::TriggerTarget>,
) -> impl IntoResponse {
    match flowr_ce_agents::trigger_manager().attach(&id, target).await {
        Ok(()) => { persist_triggers().await; Json(json!({"attached": true, "trigger_id": id})) }
        Err(e) => Json(json!({"error": e.to_string()})),
    }
}

async fn detach_trigger_handler(
    axum::extract::Path(id): axum::extract::Path<String>,
    Json(body): Json<Value>,
) -> impl IntoResponse {
    let target_id = body.get("target_id").and_then(|v| v.as_str()).unwrap_or("");
    match flowr_ce_agents::trigger_manager().detach(&id, target_id).await {
        Ok(()) => { persist_triggers().await; Json(json!({"detached": true, "trigger_id": id})) }
        Err(e) => Json(json!({"error": e.to_string()})),
    }
}

// --- Trigger adapters ---

struct TriggerAgentAdapter;

#[async_trait::async_trait]
impl flowr_ce_agents::triggers::AgentMessageSender for TriggerAgentAdapter {
    async fn send_to_agent(&self, agent_id: &str, content: &str, context: Option<Value>, source: &str) -> anyhow::Result<String> {
        let _ = (agent_id, content, context, source);
        anyhow::bail!("Agent inbox not available. Use workflow triggers instead.")
    }
}

struct TriggerWorkflowAdapter {
    /// The authenticated caller, when the webhook carried a valid token (e.g. the
    /// console chat). `None` for external, unauthenticated webhooks.
    principal: Option<flowr_policy::Principal>,
}

#[async_trait::async_trait]
impl flowr_ce_agents::triggers::WorkflowExecutor for TriggerWorkflowAdapter {
    async fn execute_workflow(&self, namespace: &str, name: &str, input: Value) -> anyhow::Result<String> {
        let storage = GLOBAL_STORAGE.get()
            .ok_or_else(|| anyhow::anyhow!("Storage not initialized"))?;
        let instance_id = Uuid::now_v7();
        let request = flowr_types::ExecutionRequest {
            instance_id,
            namespace: namespace.to_string(),
            name: name.to_string(),
            version: None,
            input,
            durability: None,
            requested_at: chrono::Utc::now(),
            caller: Some("trigger".to_string()),
            // Run on behalf of the authenticated caller when present (so an agent
            // in the flow gets `on_behalf_of` and its policy-gated tool calls are
            // permitted). External webhooks with no token run as the trigger
            // Service, gated by the Service-tier baseline permits.
            principal: Some(
                self.principal
                    .clone()
                    .unwrap_or_else(|| flowr_policy::Principal::service("trigger")),
            ),
        };
        let storage_clone = storage.clone();
        let n = name.to_string();
        tokio::spawn(async move {
            match execute_workflow(storage_clone, &reqwest::Client::new(), request).await {
                Ok(state) => info!(workflow = %n, instance_id = %instance_id, status = ?state.status, "Trigger workflow completed"),
                Err(e) => warn!(workflow = %n, error = %e, "Trigger workflow failed"),
            }
        });
        Ok(instance_id.to_string())
    }
}

// --- Template handlers ---

async fn list_templates_handler(
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> impl IntoResponse {
    let ns = params.get("namespace").map(|s| s.as_str());
    let templates = flowr_ce_agents::template_manager().list(ns).await;
    Json(json!({ "templates": templates, "count": templates.len() }))
}

async fn create_template_handler(Json(body): Json<Value>) -> impl IntoResponse {
    let template: flowr_ce_agents::AgentTemplate = match serde_json::from_value(body) {
        Ok(t) => t,
        Err(e) => return (StatusCode::BAD_REQUEST, Json(json!({"error": format!("Invalid template: {}", e)}))),
    };
    let name = template.name.clone();
    match flowr_ce_agents::template_manager().create(template).await {
        Ok(v) => (StatusCode::CREATED, Json(json!({"name": name, "version": v, "created": true}))),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e.to_string()}))),
    }
}

async fn get_template_handler(
    axum::extract::Path(name): axum::extract::Path<String>,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> impl IntoResponse {
    let ns = params.get("namespace").map(|s| s.as_str()).unwrap_or("default");
    match flowr_ce_agents::template_manager().get_active(ns, &name).await {
        Some(t) => (StatusCode::OK, Json(json!(t))),
        None => (StatusCode::NOT_FOUND, Json(json!({"error": format!("Template '{}' not found", name)}))),
    }
}

async fn get_template_version_handler(
    axum::extract::Path((name, version)): axum::extract::Path<(String, String)>,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> impl IntoResponse {
    let ns = params.get("namespace").map(|s| s.as_str()).unwrap_or("default");
    match flowr_ce_agents::template_manager().get(ns, &name, &version).await {
        Some(t) => (StatusCode::OK, Json(json!(t))),
        None => (StatusCode::NOT_FOUND, Json(json!({"error": format!("Template '{}:{}' not found", name, version)}))),
    }
}

async fn activate_template_handler(
    axum::extract::Path((name, version)): axum::extract::Path<(String, String)>,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> impl IntoResponse {
    let ns = params.get("namespace").map(|s| s.as_str()).unwrap_or("default");
    match flowr_ce_agents::template_manager().activate(ns, &name, &version).await {
        Ok(()) => Json(json!({"activated": true, "name": name, "version": version})),
        Err(e) => Json(json!({"error": e.to_string()})),
    }
}

async fn delete_template_handler(
    axum::extract::Path(name): axum::extract::Path<String>,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> impl IntoResponse {
    let ns = params.get("namespace").map(|s| s.as_str()).unwrap_or("default");
    match flowr_ce_agents::template_manager().delete_all(ns, &name).await {
        Ok(count) => Json(json!({"deleted": true, "name": name, "versions_removed": count})),
        Err(e) => Json(json!({"error": e.to_string()})),
    }
}

// --- SSE/MCP handlers ---
// These are complex — include from the existing main.rs logic

use std::convert::Infallible;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::StreamExt;

async fn sse_handler(
    State(state): State<Arc<AppState>>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let session_id = Uuid::now_v7().to_string();
    info!(session_id = %session_id, "SSE client connected");

    let rx = state.tx.subscribe();
    let endpoint = format!("/message?session_id={}", session_id);
    let init_event = Event::default()
        .event("endpoint")
        .data(endpoint);

    let stream = BroadcastStream::new(rx)
        .filter_map(|result| result.ok())
        .map(|msg| Ok(Event::default().event("message").data(msg)));

    let init_stream = stream::once(async { Ok(init_event) });
    Sse::new(init_stream.chain(stream))
}

/// SSE stream of an agent session's live tokens. Replays anything the client
/// missed (`Last-Event-ID`, or `?after=`) before switching to live chunks, so a
/// reconnect after a navigation or a dropped connection does not lose tokens.
async fn chat_stream_handler(
    axum::extract::Path(session_id): axum::extract::Path<String>,
    axum::extract::Query(q): axum::extract::Query<std::collections::HashMap<String, String>>,
    headers: axum::http::HeaderMap,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let after: u64 = headers
        .get("last-event-id")
        .and_then(|v| v.to_str().ok())
        .or_else(|| q.get("after").map(|s| s.as_str()))
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);

    let (replay, rx) = crate::token_stream::subscribe(&session_id, after);
    info!(session_id = %session_id, after, replayed = replay.len(), "chat stream attached");

    let to_event = |c: crate::token_stream::StreamChunk| {
        Ok(Event::default().id(c.seq.to_string()).event(c.kind).data(c.data))
    };
    let replay_stream = stream::iter(replay.into_iter().map(to_event));
    let live = BroadcastStream::new(rx)
        .filter_map(|r| r.ok())
        .map(move |c| to_event(c));

    // Keep-alive so intermediaries do not drop an idle connection between turns.
    Sse::new(replay_stream.chain(live))
        .keep_alive(axum::response::sse::KeepAlive::new().interval(std::time::Duration::from_secs(15)))
}


/// Ask the policy whether `principal` may act on this session row.
///
/// The decision is Cedar's, not this function's: it only supplies the facts the
/// policy reads — the row's `owner` and whether the caller *is* that owner, which
/// is knowable only here. `baseline.cedar` permits owners for read+delete and adds
/// an InfoSec read exception. In CE (`allow_all`) this is a no-op, so CE keeps its
/// existing behaviour; the ownership *filter* below is independent of policy and
/// applies in both editions.
async fn session_policy_allows(
    principal: &Principal,
    action: flowr_policy::Action,
    session_id: &str,
    owner: Option<&str>,
) -> bool {
    let is_owner = owner.is_some_and(|o| o == principal.id());
    let mut resource = flowr_policy::Resource::new("Session", session_id)
        .with_attr("is_owner", is_owner);
    if let Some(o) = owner {
        resource = resource.with_attr("owner", o);
    }
    let req = flowr_policy::ExecutionRequest {
        principal: principal.clone(),
        action,
        resource,
        context: Default::default(),
        attributes: Default::default(),
        params: serde_json::Value::Null,
    };
    match flowr_policy::policy_hook().check(&req).await.non_interactive() {
        flowr_policy::PolicyDecision::Allow => true,
        flowr_policy::PolicyDecision::Deny { reason, policy_ids } => {
            info!(session_id, principal = %principal.id(), %reason, ?policy_ids,
                  "session access denied by policy");
            false
        }
        // Reading someone's transcript is not a thing to prompt about mid-request.
        flowr_policy::PolicyDecision::Ask { .. } => false,
    }
}

/// State key for one user's pin set. Derived from the caller, never from input,
/// so one user can neither read nor write another's pins.
fn pins_key(owner: &str) -> String {
    format!("ui:pins:{}", owner)
}

/// `GET /ui/pins` — the caller's pinned items, grouped by kind.
///
/// Shape: `{ "pins": { "chat": ["id", …], "flows": [...], … } }`. A kind with no
/// pins is simply absent.
async fn list_pins_handler(
    State(state): State<Arc<AppState>>,
    principal: Option<Extension<Principal>>,
) -> axum::response::Response {
    let Some(Extension(principal)) = principal else {
        return (StatusCode::UNAUTHORIZED,
            Json(json!({"error": "authentication required"}))).into_response();
    };
    let row = state.storage.state().get(&pins_key(principal.id())).await.ok().flatten();
    let pins = row.and_then(|v| v.get("pins").cloned()).unwrap_or_else(|| json!({}));
    Json(json!({ "pins": pins })).into_response()
}

/// `POST /ui/pins` — pin or unpin one item.
///
/// Body: `{ "kind": "chat", "id": "<id>", "pinned": true }`. Idempotent, and
/// returns the whole updated set so the client never has to guess at merge order.
async fn set_pin_handler(
    State(state): State<Arc<AppState>>,
    principal: Option<Extension<Principal>>,
    Json(body): Json<Value>,
) -> axum::response::Response {
    let Some(Extension(principal)) = principal else {
        return (StatusCode::UNAUTHORIZED,
            Json(json!({"error": "authentication required"}))).into_response();
    };
    let (Some(kind), Some(id)) = (
        body.get("kind").and_then(|v| v.as_str()),
        body.get("id").and_then(|v| v.as_str()),
    ) else {
        return (StatusCode::BAD_REQUEST,
            Json(json!({"error": "kind and id are required"}))).into_response();
    };
    let pinned = body.get("pinned").and_then(|v| v.as_bool()).unwrap_or(true);

    let key = pins_key(principal.id());
    let mut pins = state.storage.state().get(&key).await.ok().flatten()
        .and_then(|v| v.get("pins").cloned())
        .and_then(|v| v.as_object().cloned())
        .unwrap_or_default();

    let mut ids: Vec<String> = pins.get(kind).and_then(|v| v.as_array())
        .map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect())
        .unwrap_or_default();
    ids.retain(|x| x != id);
    if pinned {
        // Newest pin first, so a UI showing only the top N shows the latest.
        ids.insert(0, id.to_string());
        ids.truncate(PIN_MAX_PER_KIND);
    }
    if ids.is_empty() { pins.remove(kind); } else { pins.insert(kind.to_string(), json!(ids)); }

    // No TTL: a pin is durable until the user removes it.
    if let Err(e) = state.storage.state()
        .set(&key, json!({ "owner": principal.id(), "pins": pins }), None).await {
        error!(error = %e, "failed to save pins");
        return (StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "failed to save pins"}))).into_response();
    }
    Json(json!({ "pins": pins })).into_response()
}

/// Cap per kind, so one user cannot grow the row without bound.
const PIN_MAX_PER_KIND: usize = 50;

/// `GET /chat/sessions` — the signed-in principal's own conversations, newest
/// first. Reads that principal's index only, so one user can never enumerate
/// another's sessions regardless of policy.
async fn list_chat_sessions_handler(
    State(state): State<Arc<AppState>>,
    principal: Option<Extension<Principal>>,
) -> axum::response::Response {
    let Some(Extension(principal)) = principal else {
        return (StatusCode::UNAUTHORIZED,
            Json(json!({"error": "authentication required"}))).into_response();
    };
    let sessions = crate::handlers::call_agent::list_owner_sessions(
        state.storage.as_ref(), principal.id()).await;
    Json(json!({ "sessions": sessions, "owner": principal.id() })).into_response()
}

/// `GET /chat/sessions/:session_id` — one transcript, as renderable turns.
///
/// Tool traffic stays server-side: only user/assistant messages with text are
/// returned, matching what the chat renders.
async fn get_chat_session_handler(
    State(state): State<Arc<AppState>>,
    principal: Option<Extension<Principal>>,
    Path(session_id): Path<String>,
) -> axum::response::Response {
    let Some(Extension(principal)) = principal else {
        return (StatusCode::UNAUTHORIZED,
            Json(json!({"error": "authentication required"}))).into_response();
    };
    let key = crate::handlers::call_agent::session_key(&session_id);
    let Ok(Some(row)) = state.storage.state().get(&key).await else {
        return (StatusCode::NOT_FOUND, Json(json!({"error": "session not found"}))).into_response();
    };
    let owner = row.get("owner").and_then(|v| v.as_str());
    if !session_policy_allows(&principal, flowr_policy::actions::session_read(),
                              &session_id, owner).await {
        // 404, not 403: confirming a session exists but is somebody else's is
        // itself a disclosure.
        return (StatusCode::NOT_FOUND, Json(json!({"error": "session not found"}))).into_response();
    }
    let messages: Vec<Value> = row.get("messages").and_then(|v| v.as_array())
        .map(|msgs| msgs.iter().filter(|m| {
            let role = m.get("role").and_then(|v| v.as_str()).unwrap_or("");
            let has_text = m.get("content").and_then(|v| v.as_str())
                .is_some_and(|c| !c.trim().is_empty());
            (role == "user" || role == "assistant") && has_text
        }).cloned().collect())
        .unwrap_or_default();
    Json(json!({
        "session_id": session_id,
        "owner": owner,
        "updated_at": row.get("updated_at"),
        "messages": messages,
    })).into_response()
}

/// `GET /chat/sessions/:session_id/permissions` — what was allowed or declined
/// in this conversation.
///
/// Gated by the same owner check as reading the transcript: the answers are as
/// personal as the conversation that produced them.
async fn list_permissions_handler(
    State(state): State<Arc<AppState>>,
    principal: Option<Extension<Principal>>,
    Path(session_id): Path<String>,
) -> axum::response::Response {
    let Some(Extension(principal)) = principal else {
        return (StatusCode::UNAUTHORIZED,
            Json(json!({"error": "authentication required"}))).into_response();
    };
    if !session_permission_allowed(&state, &principal, &session_id,
                                   flowr_policy::actions::session_read()).await {
        return (StatusCode::NOT_FOUND, Json(json!({"error": "session not found"}))).into_response();
    }
    let all = flowr_policy::grants::list(&session_id);
    Json(json!({ "permissions": all, "count": all.len() })).into_response()
}

/// `POST /chat/sessions/:session_id/permissions/lift` — reopen a refusal.
///
/// Body: `{ "key": "<decision key>" }`.
///
/// This REOPENS the question rather than granting it: the next matching call
/// asks again. Granting outright from here would let a stray click hand out a
/// permission with none of the context the prompt provides.
///
/// There is no agent-facing equivalent, and that is the point — an agent able to
/// lift its own refusals could defeat a sticky denial by lifting and re-asking
/// until the user gave in.
async fn lift_permission_handler(
    State(state): State<Arc<AppState>>,
    principal: Option<Extension<Principal>>,
    Path(session_id): Path<String>,
    Json(body): Json<Value>,
) -> axum::response::Response {
    let Some(Extension(principal)) = principal else {
        return (StatusCode::UNAUTHORIZED,
            Json(json!({"error": "authentication required"}))).into_response();
    };
    // Lifting changes what the agent may do, so it takes the DELETE-tier check,
    // not the read one: an InfoSec reader auditing a transcript must not be able
    // to alter the permissions in it.
    if !session_permission_allowed(&state, &principal, &session_id,
                                   flowr_policy::actions::session_delete()).await {
        return (StatusCode::NOT_FOUND, Json(json!({"error": "session not found"}))).into_response();
    }
    let Some(key) = body.get("key").and_then(|v| v.as_str()) else {
        return (StatusCode::BAD_REQUEST, Json(json!({"error": "key is required"}))).into_response();
    };
    // Forgets either verdict: a grant is as revocable as a refusal is liftable.
    // Both re-open the question rather than flipping it, so a click in a list
    // never silently grants or silently blocks anything.
    let lifted = flowr_policy::grants::forget(&session_id, key);
    Json(json!({
        "lifted": lifted,
        "key": key,
        "note": if lifted {
            "The question is open again — the agent will ask next time it needs this."
        } else {
            "Nothing was recorded for that key."
        },
    }))
    .into_response()
}

/// Shared owner check for the permission endpoints: resolve the session's owner
/// and run the same Cedar gate the transcript routes use.
async fn session_permission_allowed(
    state: &Arc<AppState>,
    principal: &Principal,
    session_id: &str,
    action: flowr_policy::Action,
) -> bool {
    let key = crate::handlers::call_agent::session_key(session_id);
    let row = state.storage.state().get(&key).await.ok().flatten();
    let owner = row.as_ref().and_then(|r| r.get("owner")).and_then(|v| v.as_str());
    session_policy_allows(principal, action, session_id, owner).await
}

/// `DELETE /chat/sessions/:session_id` — drop a conversation and its index entry.
///
/// This is the only thing that removes a transcript: sessions are kept until the
/// user deletes them (there is no TTL), so deletion has to be explicit.
async fn delete_chat_session_handler(
    State(state): State<Arc<AppState>>,
    principal: Option<Extension<Principal>>,
    Path(session_id): Path<String>,
) -> axum::response::Response {
    let Some(Extension(principal)) = principal else {
        return (StatusCode::UNAUTHORIZED,
            Json(json!({"error": "authentication required"}))).into_response();
    };
    let key = crate::handlers::call_agent::session_key(&session_id);
    let row = state.storage.state().get(&key).await.ok().flatten();
    let owner = row.as_ref().and_then(|r| r.get("owner")).and_then(|v| v.as_str());
    if !session_policy_allows(&principal, flowr_policy::actions::session_delete(),
                              &session_id, owner).await {
        return (StatusCode::NOT_FOUND, Json(json!({"error": "session not found"}))).into_response();
    }
    let _ = state.storage.state().delete(&key).await;
    // Clear the index entry for the row's real owner, so an InfoSec delete does
    // not leave a dangling pointer in somebody else's sidebar.
    let index_owner = owner.unwrap_or_else(|| principal.id()).to_string();
    if let Err(e) = crate::handlers::call_agent::remove_from_session_index(
        state.storage.as_ref(), &index_owner, &session_id).await {
        warn!(error = %e, "failed to prune chat session index");
    }
    // Permissions must not outlive the transcript that justified them. Whatever
    // the user allowed this agent to do, they allowed it in the context of THIS
    // conversation; a deleted conversation leaves no standing consent behind.
    flowr_policy::grants::clear(&session_id);
    crate::token_stream::close(&session_id);
    Json(json!({ "deleted": true, "session_id": session_id })).into_response()
}

/// `POST /chat` — AI SDK **UI Message Stream protocol v1** over SSE.
///
/// Bridges an impedance mismatch: `Chat.sendMessage()` expects one request whose
/// body streams the reply, but an A3T chat turn is a queued *workflow* execution
/// decoupled from the HTTP request. So this enqueues the flow, then streams the
/// agent's live chunks translated into protocol frames.
///
/// Authorization is unchanged and not re-implemented here: the run is enqueued
/// **with the caller's principal**, so `flow:start`, `tool:invoke` and every
/// `mcp:call` are gated by Cedar exactly as they are on the webhook path, with
/// `on_behalf_of` intact. An unauthenticated request is refused rather than
/// falling back to a service identity — defaulting a user-facing chat route to a
/// shared identity is the identity-collapse trap, not a convenience.
async fn chat_ui_stream_handler(
    State(state): State<Arc<AppState>>,
    principal: Option<Extension<Principal>>,
    Json(body): Json<Value>,
) -> axum::response::Response {
    let Some(Extension(principal)) = principal else {
        return (StatusCode::UNAUTHORIZED,
            Json(json!({"error": "authentication required"}))).into_response();
    };
    let Some(session_id) = body.get("session_id").and_then(|v| v.as_str()).map(str::to_string) else {
        return (StatusCode::BAD_REQUEST,
            Json(json!({"error": "session_id is required"}))).into_response();
    };
    let message = body.get("message").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let namespace = body.get("namespace").and_then(|v| v.as_str()).unwrap_or("a3t").to_string();
    let name = body.get("name").and_then(|v| v.as_str()).unwrap_or("a3t-chat-engine").to_string();

    // Attach *before* enqueueing so nothing this run emits can be missed, and
    // take a live-only subscription: the ring still holds the previous turn
    // (kept so Last-Event-ID resume works), and replaying it here would re-send
    // the last answer as soon as a second message was sent.
    let rx = crate::token_stream::subscribe_live(&session_id);
    let replay: Vec<crate::token_stream::StreamChunk> = Vec::new();

    let mut input = json!({ "message": message, "session_id": session_id });
    for k in ["llm_resource", "enabled_tools", "context", "ui_context"] {
        if let Some(v) = body.get(k) { input[k] = v.clone(); }
    }
    let request = flowr_types::ExecutionRequest::new(&namespace, &name)
        .with_input(input)
        .with_principal(principal.clone());
    let instance_id = request.instance_id;
    if let Err(e) = state.storage.queue().enqueue(request).await {
        return (StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": format!("failed to start chat run: {}", e)}))).into_response();
    }
    info!(%instance_id, session_id = %session_id, principal = %principal.id(),
          "chat run started (ui-message-stream)");

    // ── Watchdog: a failed run must end the stream, not hang it ──
    //
    // The SSE ends when a `done` chunk arrives, and the agent publishes that on
    // its way out. A run that dies BEFORE that point — an LLM 429, a denied step,
    // a panic in a nested flow — publishes nothing, so the client sat on a typing
    // indicator forever with no way to know it had failed. Observed live: an
    // upstream provider rate-limit left the chat spinning indefinitely.
    //
    // So watch the execution record and, if it reaches a terminal failure without
    // the stream having closed itself, say so in the conversation and close it.
    // Completion needs no handling here: a run that completes has already sent
    // `done`.
    {
        let storage = state.storage.clone();
        let sid = session_id.clone();
        tokio::spawn(async move {
            // Generous cap: long tool chains are legitimate. This only bounds how
            // long the watchdog itself lives, not the run.
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(900);
            let mut ticker = tokio::time::interval(std::time::Duration::from_secs(2));
            ticker.tick().await;
            while std::time::Instant::now() < deadline {
                ticker.tick().await;
                let Ok(Some(exec)) = storage.executions().get(instance_id).await else { continue };
                match exec.status {
                    flowr_types::ExecutionStatus::Failed
                    | flowr_types::ExecutionStatus::Cancelled => {
                        let reason = exec.error.unwrap_or_else(|| "the run failed".to_string());
                        warn!(%instance_id, session_id = %sid, %reason, "chat run ended without closing its stream");
                        crate::token_stream::publish(&sid, "token",
                            format!("\n\n**The run stopped.** {reason}"));
                        crate::token_stream::publish(&sid, "done", "");
                        return;
                    }
                    flowr_types::ExecutionStatus::Completed => return,
                    _ => {}
                }
            }
        });
    }

    let sse = Sse::new(ui_message_stream(replay, rx))
        .keep_alive(axum::response::sse::KeepAlive::new()
            .interval(std::time::Duration::from_secs(15)));

    // Exactly the header set ai@6's UI_MESSAGE_STREAM_HEADERS expects.
    ([
        ("x-vercel-ai-ui-message-stream", "v1"),
        ("cache-control", "no-cache"),
        ("connection", "keep-alive"),
    ], sse).into_response()
}

/// Translate hub chunks into UI Message Stream v1 frames.
///
/// One agent iteration maps to a `start-step`/`finish-step` pair, and text is
/// bracketed by `text-start`/`text-end` — a multi-iteration run is several steps
/// in one message, which is what lets the client render tool calls inline.
fn ui_message_stream(
    replay: Vec<crate::token_stream::StreamChunk>,
    rx: tokio::sync::broadcast::Receiver<crate::token_stream::StreamChunk>,
) -> impl Stream<Item = Result<Event, Infallible>> {
    struct St {
        rx: tokio::sync::broadcast::Receiver<crate::token_stream::StreamChunk>,
        replay: std::collections::VecDeque<crate::token_stream::StreamChunk>,
        pending: std::collections::VecDeque<Event>,
        text_open: bool,
        step_open: bool,
        text_id: String,
        finished: bool,
    }

    fn frame(v: Value) -> Event { Event::default().data(v.to_string()) }

    /// End the open text part, if any.
    ///
    /// A text part must never be REOPENED after another kind of part interrupts
    /// it. The client settles the text when a tool call or data part arrives, so
    /// a later delta naming the same id is a delta for a part it no longer has —
    /// "Received text-delta for missing text part". Minting a fresh id per block
    /// was not enough on its own, because `text_open` only cleared at `done`, so
    /// text that resumed AFTER a tool call kept writing into the settled part.
    ///
    /// Closing here makes each contiguous run of text a complete
    /// start → deltas → end triple, which is the invariant the protocol wants.
    fn close_text(st: &mut St) {
        if st.text_open {
            st.pending.push_back(frame(json!({"type":"text-end","id":st.text_id})));
            st.text_open = false;
        }
    }

    fn translate(st: &mut St, c: crate::token_stream::StreamChunk) {
        // Anything that is not more text ends the current text part first.
        if c.kind != "token" {
            close_text(st);
        }
        match c.kind.as_str() {
            "step_start" => {
                if st.step_open { st.pending.push_back(frame(json!({"type":"finish-step"}))); }
                st.pending.push_back(frame(json!({"type":"start-step"})));
                st.step_open = true;
            }
            "token" => {
                if !st.text_open {
                    // A FRESH id per text block. `text_id` used to be minted once
                    // per connection, so a turn that produced text, ended it, then
                    // produced more — an approval pause, a tool result followed by
                    // a second answer — reopened a part the client had already
                    // closed with `text-end`. The AI SDK then rejected the stream
                    // with "Received text-delta for missing text part". Suspending
                    // mid-turn for approval makes a second text block the norm
                    // rather than an edge case.
                    st.text_id = format!("msg_{}", Uuid::now_v7());
                    st.pending.push_back(frame(json!({"type":"text-start","id":st.text_id})));
                    st.text_open = true;
                }
                st.pending.push_back(frame(json!({
                    "type":"text-delta","id":st.text_id,"delta":c.data
                })));
            }
            "tool_input" => {
                if let Ok(v) = serde_json::from_str::<Value>(&c.data) {
                    st.pending.push_back(frame(json!({
                        "type":"tool-input-available",
                        "toolCallId": v.get("toolCallId").cloned().unwrap_or(Value::Null),
                        "toolName": v.get("toolName").cloned().unwrap_or(Value::Null),
                        "input": v.get("input").cloned().unwrap_or(json!({})),
                    })));
                }
            }
            "tool_output" => {
                if let Ok(v) = serde_json::from_str::<Value>(&c.data) {
                    st.pending.push_back(frame(json!({
                        "type":"tool-output-available",
                        "toolCallId": v.get("toolCallId").cloned().unwrap_or(Value::Null),
                        "output": v.get("output").cloned().unwrap_or(Value::Null),
                    })));
                }
            }
            // A permission question, and its answer. Surfaced as data parts so the
            // client renders the approval card and can retire it in place once
            // it is answered — the same DataUIPart path the widgets already use.
            //
            // `transient: false` matters here: the card must survive a reload.
            // A question the agent is still suspended on, lost because the tab
            // refreshed, would leave the run hanging with nothing on screen.
            "authz-request" | "authz-resolved" => {
                if let Ok(v) = serde_json::from_str::<Value>(&c.data) {
                    st.pending.push_back(frame(json!({
                        "type": format!("data-{}", c.kind),
                        "id": v.get("request_id").cloned().unwrap_or(Value::Null),
                        "data": v,
                    })));
                }
            }
            // Generic data part: {"name":"flow","data":{…}} → `data-flow` frame.
            // The AI SDK surfaces these as DataUIPart entries in message.parts,
            // which the client renders as widgets.
            "data" => {
                if let Ok(v) = serde_json::from_str::<Value>(&c.data) {
                    if let Some(name) = v.get("name").and_then(|n| n.as_str()) {
                        st.pending.push_back(frame(json!({
                            "type": format!("data-{}", name),
                            "data": v.get("data").cloned().unwrap_or(Value::Null),
                        })));
                    }
                }
            }
            "done" => {
                // close_text() above has already ended any open text part.
                if st.step_open {
                    st.pending.push_back(frame(json!({"type":"finish-step"})));
                    st.step_open = false;
                }
                st.pending.push_back(frame(json!({"type":"finish"})));
                st.pending.push_back(Event::default().data("[DONE]"));
                st.finished = true;
            }
            "error" => {
                st.pending.push_back(frame(json!({"type":"error","errorText":c.data})));
                st.pending.push_back(Event::default().data("[DONE]"));
                st.finished = true;
            }
            _ => {}
        }
    }

    let mut init = std::collections::VecDeque::new();
    init.push_back(Event::default().data(json!({"type":"start"}).to_string()));

    let st = St {
        rx,
        replay: replay.into_iter().collect(),
        pending: init,
        text_open: false,
        step_open: false,
        text_id: format!("msg_{}", Uuid::now_v7()),
        finished: false,
    };

    stream::unfold(st, |mut st| async move {
        loop {
            if let Some(ev) = st.pending.pop_front() {
                return Some((Ok(ev), st));
            }
            if st.finished { return None; }
            if let Some(c) = st.replay.pop_front() {
                translate(&mut st, c);
                continue;
            }
            match st.rx.recv().await {
                Ok(c) => translate(&mut st, c),
                // Lagged: keep going, the client reconciles from the transcript.
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                Err(_) => return None,
            }
        }
    })
}

async fn sse_message_handler(
    State(state): State<Arc<AppState>>,
    principal: Option<Extension<Principal>>,
    Json(request): Json<JsonRpcRequest>,
) -> impl IntoResponse {
    let response = handle_request(&state.storage, caller(principal), request).await;
    let response_json = serde_json::to_string(&response).unwrap_or_default();
    let _ = state.tx.send(response_json.clone());
    Json(response)
}

async fn message_handler(
    State(state): State<Arc<AppState>>,
    // E0.4a: the AuthN middleware inserts the caller's Principal into request
    // extensions. Absent (unauthenticated/internal) → the mcp service identity.
    principal: Option<Extension<Principal>>,
    Json(request): Json<JsonRpcRequest>,
) -> impl IntoResponse {
    info!("Executing tool tool={}", request.method);
    let response = handle_request(&state.storage, caller(principal), request).await;
    Json(response)
}

/// The caller identity for a request: the middleware-supplied Principal, or the
/// mcp service identity when none was authenticated.
fn caller(principal: Option<Extension<Principal>>) -> Principal {
    principal.map(|Extension(p)| p).unwrap_or_else(|| Principal::service("mcp"))
}

async fn handle_request(
    storage: &Arc<dyn StorageBackend>,
    principal: Principal,
    request: JsonRpcRequest,
) -> JsonRpcResponse {
    let id = request.id.clone();
    match request.method.as_str() {
        "initialize" => {
            info!("MCP client connected");
            handle_initialize(id)
        }
        "tools/list" => {
            let tools = get_tools();
            let result = ToolsListResult { tools };
            JsonRpcResponse::success(id, serde_json::to_value(result).unwrap())
        }
        "tools/call" => {
            let params: Option<CallToolParams> = request.params
                .and_then(|p| serde_json::from_value(p).ok());
            match params {
                Some(p) => {
                    // Try agent-specific tools first (need flowr-ce-agents)
                    // Direct MCP call: no approver here, so the dispatcher makes the
                    // target decisions itself (pre_authorized = false).
                    let tool_result = match handle_agent_tool_call(storage, &principal, &p.name, &p.arguments, false).await {
                        Some(result) => result,
                        // Fall through to shared handler — authorize the caller.
                        None => handle_tool_call(storage.as_ref(), &principal, &p.name, p.arguments).await,
                    };
                    JsonRpcResponse::success(id, serde_json::to_value(tool_result).unwrap())
                }
                None => JsonRpcResponse::error(id, error_codes::INVALID_PARAMS, "Invalid params".to_string()),
            }
        }
        method => {
            warn!("Unknown method method={}", method);
            JsonRpcResponse::error(id, error_codes::METHOD_NOT_FOUND, format!("Unknown method: {}", method))
        }
    }
}

fn handle_initialize(id: Option<Value>) -> JsonRpcResponse {
    let result = InitializeResult {
        protocol_version: "2024-11-05".to_string(),
        capabilities: ServerCapabilities {
            tools: ToolsCapability { list_changed: false },
        },
        server_info: ServerInfo {
            name: "clawdflow".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
        },
    };
    JsonRpcResponse::success(id, serde_json::to_value(result).unwrap())
}

// --- Agent MCP tool handlers ---
// These require flowr-ce-agents (TemplateManager, TriggerManager) and can't
// live in the shared flowr-mcp-core crate.

use flowr_mcp_core::protocol::CallToolResult;

/// Handle agent-specific MCP tools. Returns None if tool name doesn't match.
/// Agent-runtime tools (templates, agents, send_message) live in this layer.
/// `pub(crate)` so the in-process agent loop can offer its own tool calls here
/// too — it dispatches straight to `flowr_mcp_core` and otherwise never reaches
/// this interceptor, which is why the chat agent could not list agents.
pub(crate) async fn handle_agent_tool_call(
    storage: &Arc<dyn StorageBackend>,
    principal: &Principal,
    name: &str,
    arguments: &Option<Value>,
    // True when the caller has ALREADY made the target decision — the agent
    // loop does, because only it can put an approval to a person. Direct MCP
    // calls (`/message`) pass false and are gated here, non-interactively.
    // Without this flag it was either gate-twice (refusing a call the user had
    // just approved) or gate-never (direct MCP could deploy to prod unchecked).
    pre_authorized: bool,
) -> Option<CallToolResult> {
    let args = arguments.clone().unwrap_or(json!({}));

    // ── Target decisions for direct MCP callers ──
    // `flow:deploy` on the Runtime and `resource:create` for a new pool. There is
    // no approver on this path, so an Ask degrades to a denial.
    if !pre_authorized && (name == "deploy_flow" || name == "create_runtime") {
        let req = if name == "deploy_flow" {
            let ns = args.get("namespace").and_then(|v| v.as_str()).unwrap_or("a3t");
            let flow = args.get("name").and_then(|v| v.as_str()).unwrap_or_default();
            let rt = args.get("runtime").and_then(|v| v.as_str());
            crate::canvas::deploy_request(storage.as_ref(), principal, ns, flow, rt, &args)
                .await
                .map(|(req, ..)| req)
        } else {
            let rt_name = args.get("name").and_then(|v| v.as_str()).unwrap_or_default();
            Some(crate::canvas::runtime_create_request(principal, rt_name, &args))
        };
        if let Some(req) = req {
            let decision = flowr_policy::policy_hook().check(&req).await;
            flowr_policy::audit::record(&req, &decision);
            if let flowr_policy::PolicyDecision::Deny { reason, policy_ids } =
                decision.non_interactive()
            {
                return Some(CallToolResult::error(format!(
                    "denied: {name} — {reason} (policies: {})",
                    policy_ids.join(",")
                )));
            }
        }
    }

    // E2 #5 Phase C: these agent/flow *authoring* tools are dispatched here and
    // historically bypassed the site-A policy hook entirely. Gate the write
    // operations through the hook so authorization (RBAC — e.g. authoring needs
    // the Admin role) actually applies. Reads (list_/get_) fall through untouched.
    const GATED_WRITES: &[&str] =
        &["create_flow", "create_agent", "create_agent_template", "delete_agent"];
    if GATED_WRITES.contains(&name) {
        let req = flowr_policy::ExecutionRequest {
            principal: principal.clone(),
            action: flowr_policy::actions::mcp_call(),
            resource: flowr_policy::Resource::tool(name),
            attributes: Default::default(),
            context: Default::default(),
            params: args.clone(),
        };
        let decision = flowr_policy::policy_hook().check(&req).await;
        flowr_policy::audit::record(&req, &decision);
        // Authoring writes are an HTTP-dispatched path with no approver attached,
        // so `Ask` fails closed here (`non_interactive`) rather than silently
        // behaving like `Allow`.
        if let flowr_policy::PolicyDecision::Deny { reason, policy_ids } =
            decision.non_interactive()
        {
            return Some(CallToolResult::error(format!(
                "denied: {name} — {reason} (policies: {})",
                policy_ids.join(",")
            )));
        }
    }

    // ── Deployment (canvas) ──
    // These read/write canvas configuration; `deploy_flow` is the only one that
    // acts, and it is gated separately on `flow:deploy` in `call_mcp_tool`,
    // where the session is known and an approval can be put to the user.
    let ns_arg = |d: &str| args.get("namespace").and_then(|v| v.as_str()).unwrap_or(d).to_string();
    match name {
        "list_canvas_presets" => {
            return Some(CallToolResult::json(
                crate::canvas::op_list_presets(storage.as_ref()).await,
            ));
        }
        "list_runtimes" => {
            return Some(CallToolResult::json(
                crate::canvas::op_list_runtimes(storage.as_ref()).await,
            ));
        }
        "get_canvas" => {
            let Some(n) = args.get("name").and_then(|v| v.as_str()) else {
                return Some(CallToolResult::error("name is required".to_string()));
            };
            return Some(match crate::canvas::op_get_canvas(storage.as_ref(), &ns_arg("a3t"), n).await {
                Ok(v) => CallToolResult::json(v),
                Err(e) => CallToolResult::error(e),
            });
        }
        "preview_placement" => {
            let Some(n) = args.get("name").and_then(|v| v.as_str()) else {
                return Some(CallToolResult::error("name is required".to_string()));
            };
            let chosen = args.get("runtime").and_then(|v| v.as_str());
            return Some(
                match crate::canvas::compute_placement_with(storage.as_ref(), &ns_arg("a3t"), n, chosen).await {
                    Ok(v) => CallToolResult::json(v),
                    Err(e) => CallToolResult::error(e),
                },
            );
        }
        "update_canvas" => {
            let Some(n) = args.get("name").and_then(|v| v.as_str()) else {
                return Some(CallToolResult::error("name is required".to_string()));
            };
            // Pass the whole argument object: op_update_canvas reads `spec`
            // (editor save, replace) or `patch` (agent edit, merge) from it.
            return Some(
                match crate::canvas::op_update_canvas(storage.as_ref(), &ns_arg("a3t"), n, &args).await {
                    Ok(v) => CallToolResult::json(v),
                    Err(e) => CallToolResult::error(e),
                },
            );
        }
        "create_runtime" => {
            // Authorized in `call_mcp_tool` (resource:create), including any
            // approval. Reaching here means it was allowed.
            let Some(n) = args.get("name").and_then(|v| v.as_str()) else {
                return Some(CallToolResult::error("name is required".to_string()));
            };
            let labels = args.get("labels").and_then(|v| v.as_object()).cloned().unwrap_or_default();
            let url = args.get("url").and_then(|v| v.as_str());
            return Some(
                match crate::canvas::op_create_runtime(storage.as_ref(), &ns_arg("default"), n, &labels, url).await {
                    Ok(v) => CallToolResult::json(v),
                    Err(e) => CallToolResult::error(e),
                },
            );
        }
        "clone_canvas" => {
            let (Some(preset), Some(new_name)) = (
                args.get("preset").and_then(|v| v.as_str()),
                args.get("name").and_then(|v| v.as_str()),
            ) else {
                return Some(CallToolResult::error("preset and name are required".to_string()));
            };
            let pns = args.get("preset_namespace").and_then(|v| v.as_str()).unwrap_or("a3t").to_string();
            let patch = args.get("patch").cloned().unwrap_or_else(|| json!({}));
            return Some(
                match crate::canvas::op_clone_preset(
                    storage.as_ref(), &pns, preset, &ns_arg("a3t"), new_name, &patch,
                )
                .await
                {
                    Ok(v) => CallToolResult::json(v),
                    Err(e) => CallToolResult::error(e),
                },
            );
        }
        "bind_canvas" => {
            let (Some(n), Some(flow)) = (
                args.get("name").and_then(|v| v.as_str()),
                args.get("flow").and_then(|v| v.as_str()),
            ) else {
                return Some(CallToolResult::error("name and flow are required".to_string()));
            };
            return Some(
                match crate::canvas::op_bind_canvas(storage.as_ref(), &ns_arg("a3t"), n, flow).await {
                    Ok(v) => CallToolResult::json(v),
                    Err(e) => CallToolResult::error(e),
                },
            );
        }
        "deploy_flow" => {
            let Some(n) = args.get("name").and_then(|v| v.as_str()) else {
                return Some(CallToolResult::error("name is required".to_string()));
            };
            // Authorization already happened in `call_mcp_tool` — including any
            // approval the user was asked for. Reaching here means it was allowed.
            return Some(
                match crate::canvas::op_deploy_flow(
                    storage.as_ref(), &ns_arg("a3t"), n, args.get("runtime").and_then(|v| v.as_str()),
                ).await {
                    Ok(v) => CallToolResult::json(v),
                    Err(e) => CallToolResult::error(e),
                },
            );
        }
        _ => {}
    }

    match name {
        // Intercept create_flow to handle CNCF DSL 1.0.3 YAML
        "create_flow" => {
            // Check if this is CNCF format (has document: block)
            let yaml_str = args.get("yaml").and_then(|v| v.as_str());
            let json_val = args.get("json");

            let is_cncf = yaml_str.map(|y| y.contains("document:")).unwrap_or(false)
                || json_val.and_then(|j| j.get("document")).is_some();

            if is_cncf {
                Some(mcp_create_flow_cncf(storage, principal, &args).await)
            } else {
                None // Fall through to legacy create_flow
            }
        }
        "create_agent_template" => Some(mcp_create_agent_template(&args).await),
        "list_agent_templates" => Some(mcp_list_agent_templates(storage, &args).await),
        "get_agent_template" => Some(mcp_get_agent_template(storage, &args).await),
        "create_agent" => Some(mcp_create_agent(storage, principal, &args).await),
        "list_agents" => Some(mcp_list_agents(storage, &args).await),
        "get_agent" => Some(mcp_get_agent(storage, &args).await),
        "delete_agent" => Some(mcp_delete_agent(storage, &args).await),
        "send_message" => Some(mcp_send_message(storage, principal, &args).await),
        _ => None,
    }
}

/// Create a workflow from CNCF DSL 1.0.3 YAML (clawdflow layer interceptor)
async fn mcp_create_flow_cncf(
    storage: &Arc<dyn StorageBackend>,
    principal: &Principal,
    args: &Value,
) -> CallToolResult {
    let yaml_str = if let Some(y) = args.get("yaml").and_then(|v| v.as_str()) {
        y.to_string()
    } else if let Some(json_obj) = args.get("json") {
        serde_yaml::to_string(json_obj).unwrap_or_default()
    } else {
        return CallToolResult::error("Either 'yaml' or 'json' is required".to_string());
    };

    // Parse with swf-parser, store as WorkflowRecord
    let mut record = match flowr_types::WorkflowRecord::from_yaml(&yaml_str) {
        Ok(r) => r,
        Err(e) => return CallToolResult::error(format!("Invalid CNCF SWF YAML: {}", e)),
    };

    // This path is save-over-existing (delete + register below), so it is also
    // the EDIT path. Ownership must survive an edit: whoever created the flow
    // stays its owner, or anyone able to save would silently take it over —
    // and with it the authority bounding what the flow's agent may be granted.
    // Only a genuinely new flow takes the current caller as owner.
    let existing_owner = storage
        .workflows()
        .get(record.namespace(), record.name())
        .await
        .ok()
        .and_then(|r| r.metadata.owner.clone());
    record.metadata.owner = existing_owner.or_else(|| Some(principal.id().to_string()));

    let namespace = record.namespace().to_string();
    let name = record.name().to_string();
    let version = record.version().to_string();
    let dsl = record.metadata.dsl.clone().unwrap_or_else(|| "1.0.3".to_string());

    let _ = storage.workflows().delete(&namespace, &name).await;
    match storage.workflows().register(record).await {
        Ok(_) => {
            info!(namespace = %namespace, name = %name, version = %version, "CNCF workflow created");
            CallToolResult::text(format!(
                "Workflow created successfully!\n\n- Namespace: {}\n- Name: {}\n- Version: {}\n- DSL: {}\n\nUse `execute_flow` to run it.",
                namespace, name, version, dsl
            ))
        }
        Err(e) => CallToolResult::error(format!("Failed to create workflow: {}", e)),
    }
}

async fn mcp_create_agent_template(args: &Value) -> CallToolResult {
    let template: flowr_ce_agents::AgentTemplate = match serde_json::from_value(args.clone()) {
        Ok(t) => t,
        Err(e) => return CallToolResult::error(format!("Invalid template: {}", e)),
    };
    let name = template.name.clone();
    match flowr_ce_agents::template_manager().create(template).await {
        Ok(v) => CallToolResult::json(json!({"name": name, "version": v, "created": true})),
        Err(e) => CallToolResult::error(format!("Failed to create template: {}", e)),
    }
}

async fn mcp_list_agent_templates(storage: &Arc<dyn StorageBackend>, args: &Value) -> CallToolResult {
    // Same story as agents: the console reads `template_registry` through the
    // a3t-templates flow, so the tool reads it too rather than the in-process
    // template manager, which only knows templates registered this run.
    let org = args.get("org_id").and_then(|v| v.as_str()).unwrap_or(DEFAULT_ORG);
    let filter = format!("org_id = {}", crate::handlers::call_lancedb::sql_quote(org));
    match crate::handlers::call_lancedb::query_platform_table(
        storage.as_ref(), "template_registry", Some(filter), 200).await
    {
        Ok(rows) => CallToolResult::json(json!({"templates": rows, "count": rows.len()})),
        Err(e) => CallToolResult::error(format!("Failed to list templates: {}", e)),
    }
}

async fn mcp_get_agent_template(storage: &Arc<dyn StorageBackend>, args: &Value) -> CallToolResult {
    let name = match args.get("name").and_then(|v| v.as_str()) {
        Some(n) => n,
        None => return CallToolResult::error("name is required".to_string()),
    };
    let ns = args.get("namespace").and_then(|v| v.as_str()).unwrap_or("default");

    // Registry first — that is what list_agent_templates returns and what the
    // console writes. The in-process manager only knows templates registered in
    // this process, so it stays as a fallback rather than the source of truth.
    let q = crate::handlers::call_lancedb::sql_quote(name);
    // Agents reference their template by id (`tpl-…`), the UI usually by name —
    // accept either so a lookup from an agent resolves.
    if let Ok(rows) = crate::handlers::call_lancedb::query_platform_table(
        storage.as_ref(), "template_registry", Some(format!("id = {q} OR name = {q}")), 1).await {
        if let Some(row) = rows.into_iter().next() {
            return CallToolResult::json(row);
        }
    }

    match flowr_ce_agents::template_manager().resolve(ns, name).await {
        Some(t) => CallToolResult::json(serde_json::to_value(t).unwrap_or(json!({}))),
        None => CallToolResult::error(format!("Template '{}' not found", name)),
    }
}

async fn mcp_create_agent(
    storage: &Arc<dyn StorageBackend>,
    principal: &Principal,
    args: &Value,
) -> CallToolResult {
    let template_ref = match args.get("template").and_then(|v| v.as_str()) {
        Some(t) => t,
        None => return CallToolResult::error("template is required".to_string()),
    };
    let ns = args.get("namespace").and_then(|v| v.as_str()).unwrap_or("default");
    let agent_id = args.get("agent_id").and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .unwrap_or_else(|| Uuid::now_v7().to_string());
    let config = args.get("config").cloned().unwrap_or(json!({}));
    let triggers = args.get("triggers").and_then(|v| v.as_array()).cloned().unwrap_or_default();

    // Resolve template
    let template = match flowr_ce_agents::template_manager().resolve(ns, template_ref).await {
        Some(t) => t,
        None => return CallToolResult::error(format!("Template '{}' not found", template_ref)),
    };

    // Merge template defaults with instance config
    let mut merged_config = template.defaults.clone().unwrap_or(json!({}));
    if let (Value::Object(base), Value::Object(overrides)) = (&mut merged_config, &config) {
        for (k, v) in overrides {
            base.insert(k.clone(), v.clone());
        }
    }

    // Store agent registration in state
    // `owner` comes from the authenticated caller, never from `args` — an agent
    // that could name its own owner could name whose authority bounds its grants.
    // It is also written into `config.owner` so the agent handler sees it on the
    // principal without a second lookup (AgentSettings::owner).
    let owner = principal.id().to_string();
    if let Value::Object(cfg) = &mut merged_config {
        cfg.insert("owner".to_string(), Value::String(owner.clone()));
    }
    let agent_state = json!({
        "agent_id": agent_id,
        "template": template_ref,
        "template_version": template.version,
        "namespace": ns,
        "config": merged_config,
        "status": "active",
        "owner": owner,
        "created_by": owner,
        "created_at": chrono::Utc::now().to_rfc3339(),
    });

    if let Err(e) = storage.state().set(&format!("agent:config:{}", agent_id), agent_state, None).await {
        return CallToolResult::error(format!("Failed to save agent config: {}", e));
    }

    // Create triggers
    let mut trigger_ids = Vec::new();
    for trigger_def in &triggers {
        let kind = trigger_def.get("kind").and_then(|v| v.as_str()).unwrap_or("webhook");
        let trigger = match kind {
            "webhook" => {
                let default_path = format!("/agent/{}", agent_id);
                let path = trigger_def.get("path").and_then(|v| v.as_str())
                    .unwrap_or(&default_path);
                flowr_ce_agents::Trigger {
                    id: Uuid::now_v7().to_string(),
                    name: format!("{}-webhook", agent_id),
                    namespace: "system".to_string(),
                    config: flowr_ce_agents::TriggerConfig::Webhook {
                        path: path.to_string(),
                        method: None,
                        secret: None,
                        message_template: None,
                    },
                    targets: vec![flowr_ce_agents::TriggerTarget::Workflow {
                        namespace: "system".to_string(),
                        name: "agent-template".to_string(),
                        input_mapping: None,
                    }],
                    enabled: true,
                    status: flowr_ce_agents::TriggerStatus::Active,
                    description: Some(format!("Agent {} webhook", agent_id)),
                }
            }
            "cron" => {
                let expression = trigger_def.get("expression").and_then(|v| v.as_str())
                    .unwrap_or("*/5 * * * *");
                flowr_ce_agents::Trigger {
                    id: Uuid::now_v7().to_string(),
                    name: format!("{}-cron", agent_id),
                    namespace: "system".to_string(),
                    config: flowr_ce_agents::TriggerConfig::Cron {
                        expression: expression.to_string(),
                        timezone: None,
                        message: format!("Cron trigger: {}", expression),
                    },
                    targets: vec![flowr_ce_agents::TriggerTarget::Workflow {
                        namespace: "system".to_string(),
                        name: "agent-template".to_string(),
                        input_mapping: None,
                    }],
                    enabled: true,
                    status: flowr_ce_agents::TriggerStatus::Active,
                    description: Some(format!("Agent {} cron", agent_id)),
                }
            }
            _ => continue,
        };

        let tid = trigger.id.clone();
        match flowr_ce_agents::trigger_manager().create(trigger).await {
            Ok(_) => trigger_ids.push(tid),
            Err(e) => warn!(error = %e, "Failed to create trigger for agent"),
        }
    }

    if !trigger_ids.is_empty() {
        persist_triggers().await;
    }

    info!(agent_id = %agent_id, template = %template_ref, triggers = trigger_ids.len(), "Agent created");

    CallToolResult::json(json!({
        "agent_id": agent_id,
        "template": template_ref,
        "template_version": template.version,
        "triggers": trigger_ids,
        "created": true,
    }))
}

/// Default tenant. The registry rows are scoped by `org_id`, matching what the
/// console passes to the `a3t-agents` CRUD flow.
const DEFAULT_ORG: &str = "org-default";

async fn mcp_list_agents(storage: &Arc<dyn StorageBackend>, args: &Value) -> CallToolResult {
    // Reads the SAME table the console's a3t-agents flow reads. This used to scan
    // the state key `agent:registry`, which nothing ever writes, so the tool
    // answered "no agents" while the console listed several.
    let org = args.get("org_id").and_then(|v| v.as_str()).unwrap_or(DEFAULT_ORG);
    let filter = format!("org_id = {}", crate::handlers::call_lancedb::sql_quote(org));
    match crate::handlers::call_lancedb::query_platform_table(
        storage.as_ref(), "agent_configs", Some(filter), 200).await
    {
        Ok(rows) => CallToolResult::json(json!({"agents": rows, "count": rows.len()})),
        Err(e) => CallToolResult::error(format!("Failed to list agents: {}", e)),
    }
}

async fn mcp_get_agent(storage: &Arc<dyn StorageBackend>, args: &Value) -> CallToolResult {
    let agent_id = match args.get("agent_id").and_then(|v| v.as_str()) {
        Some(id) => id,
        None => return CallToolResult::error("agent_id is required".to_string()),
    };

    let config_key = format!("agent:config:{}", agent_id);
    let session_key = format!("agent:session:{}", agent_id);

    let mut config = storage.state().get(&config_key).await.ok().flatten();
    let session = storage.state().get(&session_key).await.ok().flatten();

    // Agents created through the console live in the registry table, not in the
    // state store — without this, list_agents would show an agent that get_agent
    // then claimed did not exist. Accept the id under either column.
    if config.is_none() {
        let q = crate::handlers::call_lancedb::sql_quote(agent_id);
        let filter = format!("id = {q} OR name = {q}");
        if let Ok(rows) = crate::handlers::call_lancedb::query_platform_table(
            storage.as_ref(), "agent_configs", Some(filter), 1).await {
            config = rows.into_iter().next();
        }
    }

    match config {
        Some(cfg) => {
            let message_count = session.as_ref()
                .and_then(|s| s.get("messages"))
                .and_then(|m| m.as_array())
                .map(|a| a.len())
                .unwrap_or(0);

            CallToolResult::json(json!({
                "agent_id": agent_id,
                "config": cfg,
                "session": {
                    "active": session.is_some(),
                    "message_count": message_count,
                    "last_response": session.as_ref().and_then(|s| s.get("last_response")),
                    "updated_at": session.as_ref().and_then(|s| s.get("updated_at")),
                }
            }))
        }
        None => CallToolResult::error(format!("Agent '{}' not found", agent_id)),
    }
}

async fn mcp_delete_agent(storage: &Arc<dyn StorageBackend>, args: &Value) -> CallToolResult {
    let agent_id = match args.get("agent_id").and_then(|v| v.as_str()) {
        Some(id) => id,
        None => return CallToolResult::error("agent_id is required".to_string()),
    };

    // Delete config + session state
    let _ = storage.state().delete(&format!("agent:config:{}", agent_id)).await;
    let _ = storage.state().delete(&format!("agent:session:{}", agent_id)).await;

    // Delete triggers
    let triggers = flowr_ce_agents::trigger_manager().list().await;
    let mut removed = 0;
    for t in &triggers {
        if t.name.starts_with(agent_id) {
            if flowr_ce_agents::trigger_manager().delete(&t.id).await.is_ok() {
                removed += 1;
            }
        }
    }

    if removed > 0 {
        persist_triggers().await;
    }

    info!(agent_id = %agent_id, triggers_removed = removed, "Agent deleted");

    CallToolResult::json(json!({
        "agent_id": agent_id,
        "deleted": true,
        "triggers_removed": removed,
    }))
}

async fn mcp_send_message(storage: &Arc<dyn StorageBackend>, principal: &Principal, args: &Value) -> CallToolResult {
    let agent_id = match args.get("agent_id").and_then(|v| v.as_str()) {
        Some(id) => id,
        None => return CallToolResult::error("agent_id is required".to_string()),
    };
    let message = match args.get("message").and_then(|v| v.as_str()) {
        Some(m) => m,
        None => return CallToolResult::error("message is required".to_string()),
    };
    let context = args.get("context").cloned();

    // Load agent config to get template defaults
    let agent_config = match storage.state().get(&format!("agent:config:{}", agent_id)).await {
        Ok(Some(cfg)) => cfg.get("config").cloned().unwrap_or(json!({})),
        Ok(None) => {
            // No registered agent — use agent_id as session, execute with defaults
            json!({})
        }
        Err(e) => return CallToolResult::error(format!("Failed to load agent config: {}", e)),
    };

    // Build execution input for system/agent-template
    let mut template_input = json!({
        "session_id": agent_id,
        "message": message,
    });
    if let Some(ctx) = context {
        template_input["context"] = ctx;
    }
    template_input["config"] = agent_config;

    // Execute the agent-template workflow as the message sender (E0.4b/E2 #3):
    // the flow runs under the caller's identity, so the agent it spawns acts
    // on-behalf-of a real principal (not ambient) and its tool calls are gated
    // by the template ceiling rather than refused as ambient-agent.
    let request = ExecutionRequest::new("system", "agent-template")
        .with_input(template_input)
        .with_principal(principal.clone());
    let instance_id = request.instance_id;

    match storage.queue().enqueue(request).await {
        Ok(_) => {
            info!(agent_id = %agent_id, instance_id = %instance_id, "Agent message queued");

            // Wait for execution to complete (poll with timeout)
            let start = std::time::Instant::now();
            let timeout = std::time::Duration::from_secs(60);

            loop {
                if start.elapsed() > timeout {
                    return CallToolResult::json(json!({
                        "agent_id": agent_id,
                        "instance_id": instance_id.to_string(),
                        "status": "timeout",
                        "message": "Agent execution timed out. Use get_execution_status to check."
                    }));
                }

                tokio::time::sleep(std::time::Duration::from_millis(200)).await;

                match storage.executions().get(instance_id).await {
                    Ok(Some(state)) => {
                        match state.status {
                            ExecutionStatus::Completed => {
                                let output = state.output.unwrap_or(json!({}));
                                return CallToolResult::json(json!({
                                    "agent_id": agent_id,
                                    "content": output.get("content").cloned().unwrap_or(json!(null)),
                                    "session_id": output.get("session_id").cloned().unwrap_or(json!(agent_id)),
                                    "tool_calls_made": output.get("tool_calls_made").cloned().unwrap_or(json!(0)),
                                    "iterations": output.get("iterations").cloned().unwrap_or(json!(0)),
                                    "procedural_hit": output.get("procedural_hit").cloned().unwrap_or(json!(false)),
                                }));
                            }
                            ExecutionStatus::Failed => {
                                return CallToolResult::error(format!(
                                    "Agent execution failed: {}",
                                    state.error.unwrap_or_else(|| "unknown error".to_string())
                                ));
                            }
                            _ => continue, // Still running
                        }
                    }
                    Ok(None) | Err(_) => continue,
                }
            }
        }
        Err(e) => CallToolResult::error(format!("Failed to queue agent execution: {}", e)),
    }
}

// --- Stdio server ---

async fn run_stdio_server(storage: Arc<dyn StorageBackend>) -> anyhow::Result<()> {
    let stdin = tokio::io::stdin();
    let mut stdout = tokio::io::stdout();
    let reader = BufReader::new(stdin);
    let mut lines = reader.lines();

    while let Ok(Some(line)) = lines.next_line().await {
        if line.trim().is_empty() { continue; }
        match serde_json::from_str::<JsonRpcRequest>(&line) {
            Ok(request) => {
                // Local stdio channel: no HTTP auth, so the runtime service is
                // the caller identity.
                let response = handle_request(&storage, Principal::service("mcp-stdio"), request).await;
                write_response(&mut stdout, &response).await?;
            }
            Err(e) => {
                let response = JsonRpcResponse::error(None, error_codes::PARSE_ERROR, format!("Parse error: {}", e));
                write_response(&mut stdout, &response).await?;
            }
        }
    }
    Ok(())
}

async fn write_response<W: AsyncWriteExt + Unpin>(writer: &mut W, response: &JsonRpcResponse) -> anyhow::Result<()> {
    let json = serde_json::to_string(response)?;
    writer.write_all(json.as_bytes()).await?;
    writer.write_all(b"\n").await?;
    writer.flush().await?;
    Ok(())
}

// --- Execution worker ---

async fn run_worker(storage: Arc<dyn StorageBackend>) {
    info!("Execution worker started");

    loop {
        // Poll the queue for execution requests
        match storage.queue().dequeue(std::time::Duration::from_millis(100)).await {
            Ok(Some(request)) => {
                let storage_clone = storage.clone();
                let http_client = reqwest::Client::new();

                info!(
                    instance_id = %request.instance_id,
                    namespace = %request.namespace,
                    name = %request.name,
                    "Processing execution request"
                );

                tokio::spawn(async move {
                    match execute_workflow(storage_clone, &http_client, request).await {
                        Ok(state) => {
                            info!(
                                instance_id = %state.instance_id,
                                status = ?state.status,
                                "Workflow execution completed"
                            );
                        }
                        Err(e) => {
                            error!(error = %e, "Workflow execution failed");
                        }
                    }
                });
            }
            Ok(None) => {
                // No pending requests — sleep briefly
                tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            }
            Err(e) => {
                error!(error = %e, "Failed to dequeue execution request");
                tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            }
        }
    }
}

/// Execute a workflow using CNCF SWF runtime (swf-parser + swf-runtime)
pub async fn execute_workflow(
    storage: Arc<dyn StorageBackend>,
    _http_client: &reqwest::Client,
    request: flowr_types::ExecutionRequest,
) -> anyhow::Result<ExecutionState> {
    let start = std::time::Instant::now();

    // Load WorkflowRecord from storage
    let record = match storage.workflows().get(&request.namespace, &request.name).await {
        Ok(r) => r,
        Err(e) => {
            let error_msg = format!("Failed to load workflow: {}", e);
            let mut state = ExecutionState::new(&request, "unknown");
            state = state.fail(error_msg.clone());
            let _ = storage.executions().save(state).await;
            return Err(anyhow::anyhow!("{}", error_msg));
        }
    };

    let mut state = ExecutionState::new(&request, record.version());
    if let Err(e) = storage.executions().save(state.clone()).await {
        error!(error = %e, "Failed to save initial state");
    }

    // Parse stored CNCF YAML via swf-parser
    let swf_workflow = match record.parse() {
        Ok(wf) => wf,
        Err(e) => {
            let error_msg = format!("Failed to parse workflow: {}", e);
            state = state.fail(error_msg.clone());
            let _ = storage.executions().save(state.clone()).await;
            return Err(anyhow::anyhow!("{}", error_msg));
        }
    };

    // Create executor with all handlers
    let mut executor = swf_runtime::WorkflowExecutor::new();
    swf_runtime::handlers::register_builtin_handlers(&mut executor);
    handlers::register_clawdflow_handlers(&mut executor);

    // Wire flowr-expr jq engine for runtime expressions
    let expr_engine = flowr_core::ExpressionEngine::new();
    let wf_namespace = request.namespace.clone();
    let wf_name = request.name.clone();
    let wf_version = record.version().to_string();
    let wf_instance_id = request.instance_id.to_string();
    executor.set_eval_fn(move |input, expr, context| {
        use flowr_core::Scope;
        let scope = Scope::builder()
            .context(context.clone())
            .workflow(serde_json::json!({
                "id": wf_instance_id,
                "name": wf_name,
                "namespace": wf_namespace,
                "version": wf_version,
            }))
            .runtime(serde_json::json!({
                "name": "clawdflow",
                "version": env!("CARGO_PKG_VERSION"),
            }))
            .build();
        expr_engine.eval(input, expr, &scope)
            .map_err(|e| e.to_string())
    });

    // Create execution context
    let mut exec_ctx = swf_runtime::ExecutionContext {
        instance_id: request.instance_id.to_string(),
        namespace: request.namespace.clone(),
        name: request.name.clone(),
        version: record.version().to_string(),
        context: serde_json::Value::Null,
        secrets: Default::default(),
        runtime: swf_runtime::RuntimeInfo::default(),
        storage: {
            let sb: Arc<dyn StorageBackend> = storage.clone();
            Some(Arc::new(sb) as Arc<dyn std::any::Any + Send + Sync>)
        },
        http_client: None,
        extensions: if let Some(ref use_block) = swf_workflow.use_block {
            swf_runtime::ResolvedExtension::resolve_all(&use_block.extensions)
        } else {
            Vec::new()
        },
        eval_fn: None,
        // E0.4b/E2: the top-level principal is whoever initiated the flow, carried
        // on the queued request (JWT user, acting agent, or trigger service). Falls
        // back to the runtime-service default when the enqueuer set no identity.
        principal: request
            .principal
            .clone()
            .unwrap_or_else(swf_runtime::default_flow_principal),
        template_meta: Default::default(),
    };

    // Execute
    let result = executor.execute(&swf_workflow, request.input.clone(), &mut exec_ctx).await;

    let duration_us = start.elapsed().as_micros() as u64;
    state = match result {
        Ok(output) => state.complete(output, duration_us),
        Err(e) => state.fail(e.to_string()),
    };

    storage.executions().save(state.clone()).await
        .map_err(|e| anyhow::anyhow!("Failed to save execution state: {}", e))?;

    Ok(state)
}

// Old execute_workflow DELETED — replaced by execute_workflow (now renamed to execute_workflow above)
