//! Registry API — serves function manifests, templates, and resources
//!
//! REST endpoints:
//!   GET /registry/functions          → list all registered callables
//!   GET /registry/functions/:name    → FunctionManifest with with_schema
//!   GET /registry/templates          → list templates
//!   GET /registry/resources?type=<t> → list resources by type

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Extension, Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::Arc;

use flowr_mcp_core::Principal;
use flowr_storage::StorageBackend;
use flowr_types::Resource as StorageResource;

/// Application state passed to handlers
pub struct RegistryState {
    pub storage: Arc<dyn StorageBackend>,
}

/// Build the registry router
pub fn registry_router(storage: Arc<dyn StorageBackend>) -> Router {
    let state = Arc::new(RegistryState { storage });
    Router::new()
        .route("/functions", get(list_functions))
        .route("/functions/:name", get(get_function))
        .route("/templates", get(list_templates))
        .route("/templates/:ns/:name", get(get_template))
        // Resources: list is open; create/update/delete are policy-gated (E2 #2b).
        // Writing a `type_ref: cedar` policy resource is itself a policy-gated
        // action — in CE the AllowAll hook waves it through; in EE the baseline
        // pack's SoD guardrail forbids it unless the caller is SecurityAdmin.
        .route("/resources", get(list_resources).post(create_resource))
        .route(
            "/resources/:ns/:name",
            get(get_resource).put(update_resource).delete(delete_resource),
        )
        // A3T's platform database: embedded LanceDB or ADB platform tables (FlowR
        // EE, ADB stage G). Switching is a `resource:update` on a3t/a3t-platform-db.
        .route("/platform-db", get(platform_db_status))
        .route("/platform-db/switch", post(platform_db_switch))
        // ADB's embedding settings (ADB W9.10), proxied with FlowR's ADB token.
        .route("/adb-embedding", get(adb_embedding_get).put(adb_embedding_put))
        // ADB's hot cache rules (ADB W10), the same way.
        .route("/adb-cache", get(adb_cache_get).put(adb_cache_put))
        // Read-only SQL over everything in an ADB's Lance (superadmin debug view,
        // A3T Query Console), over Flight with FlowR's ADB token.
        .route("/adb-sql", post(adb_sql))
        .with_state(state)
}

#[derive(Deserialize)]
struct AdbUrlQuery {
    url: String,
}

/// `url`'s `/admin/<what>`, if `url` is an http(s) base.
fn adb_admin_url(url: &str, what: &str) -> Result<String, (StatusCode, Json<Value>)> {
    let base = url.trim().trim_end_matches('/');
    if !(base.starts_with("http://") || base.starts_with("https://")) {
        return Err((StatusCode::BAD_REQUEST, Json(json!({ "error": "url: the ADB's http(s) address" }))));
    }
    Ok(format!("{base}/admin/{what}"))
}

/// The ADB's answer as ours: its `{success, data, error}` body and status.
async fn adb_proxy(req: reqwest::RequestBuilder) -> axum::response::Response {
    match req.timeout(std::time::Duration::from_secs(60)).send().await {
        Ok(r) => {
            let status = StatusCode::from_u16(r.status().as_u16()).unwrap_or(StatusCode::BAD_GATEWAY);
            let body = r.json::<Value>().await.unwrap_or_else(|e| json!({ "success": false, "error": format!("ADB: {e}") }));
            (status, Json(body)).into_response()
        }
        Err(e) => (StatusCode::BAD_GATEWAY, Json(json!({ "success": false, "error": format!("ADB unreachable: {e}") }))).into_response(),
    }
}

/// GET /registry/adb-embedding?url=<adb> — the ADB's embedding settings and how
/// many records still wait for a vector.
async fn adb_embedding_get(Query(q): Query<AdbUrlQuery>) -> impl IntoResponse {
    let url = match adb_admin_url(&q.url, "embedding") {
        Ok(u) => u,
        Err(e) => return e.into_response(),
    };
    adb_proxy(flowr_mcp_core::with_adb_auth(reqwest::Client::new().get(&url), &url)).await
}

/// PUT /registry/adb-embedding — `{url, settings: {provider, model, dimension,
/// url?, api_key_env?}}`: the ADB checks and saves them, then restarts to apply.
/// Gated like editing a resource (Admin in EE).
async fn adb_embedding_put(principal: Option<Extension<Principal>>, Json(body): Json<Value>) -> impl IntoResponse {
    let who = caller(principal);
    if let Err(resp) = authorize_resource_write(&who, flowr_policy::actions::resource_update(), "adb", "embedding").await {
        return resp.into_response();
    }
    let url = match adb_admin_url(body["url"].as_str().unwrap_or_default(), "embedding") {
        Ok(u) => u,
        Err(e) => return e.into_response(),
    };
    adb_proxy(flowr_mcp_core::with_adb_auth(reqwest::Client::new().put(&url), &url).json(&body["settings"])).await
}

/// GET /registry/adb-cache?url=<adb> — the ADB's hot cache: mode, rules, figures.
async fn adb_cache_get(Query(q): Query<AdbUrlQuery>) -> impl IntoResponse {
    let url = match adb_admin_url(&q.url, "cache") {
        Ok(u) => u,
        Err(e) => return e.into_response(),
    };
    adb_proxy(flowr_mcp_core::with_adb_auth(reqwest::Client::new().get(&url), &url)).await
}

/// PUT /registry/adb-cache — `{url, settings: {max_bytes, rules}}`: the ADB saves
/// the rules and rebuilds its cache with them (no restart). Gated like editing a
/// resource (Admin in EE).
async fn adb_cache_put(principal: Option<Extension<Principal>>, Json(body): Json<Value>) -> impl IntoResponse {
    let who = caller(principal);
    if let Err(resp) = authorize_resource_write(&who, flowr_policy::actions::resource_update(), "adb", "cache").await {
        return resp.into_response();
    }
    let url = match adb_admin_url(body["url"].as_str().unwrap_or_default(), "cache") {
        Ok(u) => u,
        Err(e) => return e.into_response(),
    };
    adb_proxy(flowr_mcp_core::with_adb_auth(reqwest::Client::new().put(&url), &url).json(&body["settings"])).await
}

/// GET /registry/platform-db — `{backend, describe, adb_supported, config}`.
async fn platform_db_status(State(state): State<Arc<RegistryState>>) -> impl IntoResponse {
    Json(crate::platform_db::status(state.storage.as_ref()).await)
}

/// POST /registry/platform-db/switch — `{"backend": "adb", "url", "flight_url"?}` or
/// `{"backend": "lancedb"}`: copy every table to the target, then switch. Gated like
/// editing the resource (Admin in EE).
async fn platform_db_switch(
    State(state): State<Arc<RegistryState>>,
    principal: Option<Extension<Principal>>,
    Json(target): Json<Value>,
) -> impl IntoResponse {
    let who = caller(principal);
    if let Err(resp) = authorize_resource_write(&who, flowr_policy::actions::resource_update(), "lancedb", "a3t-platform-db").await {
        return resp.into_response();
    }
    match crate::platform_db::switch(state.storage.as_ref(), &target).await {
        Ok(r) => Json(r).into_response(),
        Err(e) => (StatusCode::BAD_REQUEST, Json(json!({ "error": e }))).into_response(),
    }
}

/// POST /registry/adb-sql — `{resource, sql, limit?}`: read-only SQL over the ADB
/// resource `namespace/name` (its memory tables and platform tables). The endpoint
/// comes from the resource, never the caller. For a signed-in SuperAdmin only —
/// not agents, services or other roles. Answers `{success, data: {columns, rows}}`
/// with every value as text (null stays null).
async fn adb_sql(
    State(state): State<Arc<RegistryState>>,
    principal: Option<Extension<Principal>>,
    Json(body): Json<Value>,
) -> impl IntoResponse {
    let superadmin = matches!(
        principal.as_ref().map(|Extension(p)| p),
        Some(Principal::Human { roles, .. }) if roles.iter().any(|r| r == flowr_policy::roles::SUPER_ADMIN)
    );
    if !superadmin {
        return (StatusCode::FORBIDDEN, Json(json!({ "success": false, "error": "SQL on ADB is for the superadmin" }))).into_response();
    }
    let Some(resource) = body["resource"].as_str().filter(|s| !s.trim().is_empty()) else {
        return (StatusCode::BAD_REQUEST, Json(json!({ "success": false, "error": "resource: the ADB resource, namespace/name" }))).into_response();
    };
    let Some(sql) = body["sql"].as_str().filter(|s| !s.trim().is_empty()) else {
        return (StatusCode::BAD_REQUEST, Json(json!({ "success": false, "error": "sql: the query" }))).into_response();
    };
    let ep = match crate::handlers::call_adb::resolve_adb(state.storage.as_ref(), resource.trim()).await {
        Ok(ep) => ep,
        Err(e) => return (StatusCode::BAD_REQUEST, Json(json!({ "success": false, "error": e }))).into_response(),
    };
    match crate::handlers::call_adb::flight_sql(&ep, sql, body["limit"].as_u64()).await {
        Ok(batches) => match batches_to_text(&batches) {
            Ok((columns, rows)) => Json(json!({ "success": true, "data": { "columns": columns, "rows": rows } })).into_response(),
            Err(e) => (StatusCode::BAD_GATEWAY, Json(json!({ "success": false, "error": e }))).into_response(),
        },
        Err(e) => (StatusCode::BAD_REQUEST, Json(json!({ "success": false, "error": e }))).into_response(),
    }
}

/// Any Arrow batches as column names and rows of display text (null as null).
fn batches_to_text(batches: &[arrow_array::RecordBatch]) -> Result<(Vec<String>, Vec<Value>), String> {
    use arrow_array::Array;
    use arrow_cast::display::{ArrayFormatter, FormatOptions};
    let columns: Vec<String> = batches
        .first()
        .map(|b| b.schema().fields().iter().map(|f| f.name().clone()).collect())
        .unwrap_or_default();
    let options = FormatOptions::default();
    let mut rows = Vec::new();
    for b in batches {
        let formatters: Vec<ArrayFormatter> = b
            .columns()
            .iter()
            .map(|c| ArrayFormatter::try_new(c.as_ref(), &options).map_err(|e| e.to_string()))
            .collect::<Result<_, _>>()?;
        for r in 0..b.num_rows() {
            let mut o = serde_json::Map::new();
            for ((name, f), c) in columns.iter().zip(&formatters).zip(b.columns()) {
                o.insert(name.clone(), if c.is_null(r) { Value::Null } else { Value::String(f.value(r).to_string()) });
            }
            rows.push(Value::Object(o));
        }
    }
    Ok((columns, rows))
}

/// The caller identity, from the auth middleware's `Extension<Principal>`.
/// Absent (CE with no auth) → the trusted MCP service principal, which the
/// AllowAll hook permits; EE always injects a real principal upstream.
fn caller(principal: Option<Extension<Principal>>) -> Principal {
    principal
        .map(|Extension(p)| p)
        .unwrap_or_else(|| Principal::service("registry"))
}

/// Ask the installed policy hook whether `principal` may perform `action` on a
/// `type_ref` registry resource. Returns `Ok(())` on allow, or a 403 response
/// body on deny. This is the single governance choke point for resource writes.
async fn authorize_resource_write(
    principal: &Principal,
    action: flowr_policy::Action,
    type_ref: &str,
    name: &str,
) -> Result<(), (StatusCode, Json<Value>)> {
    let req = flowr_policy::ExecutionRequest {
        principal: principal.clone(),
        action,
        resource: flowr_policy::Resource::registry(type_ref, name),
        context: Default::default(),
        attributes: Default::default(),
        params: serde_json::json!({}),
    };
    match flowr_policy::policy_hook().check(&req).await.non_interactive() {
        flowr_policy::PolicyDecision::Allow => Ok(()),
        flowr_policy::PolicyDecision::Ask { .. } => unreachable!(
            "non_interactive() collapses Ask into Deny"
        ),
        flowr_policy::PolicyDecision::Deny { policy_ids, reason } => Err((
            StatusCode::FORBIDDEN,
            Json(json!({
                "error": "forbidden",
                "reason": reason,
                "policies": policy_ids,
            })),
        )),
    }
}

// ── Function manifests ──

/// Built-in function manifests with JSON Schemas for their `with:` params.
/// These are derived from the Rust handler implementations.
fn get_builtin_manifests() -> Vec<Value> {
    vec![
        json!({
            "name": "a3t:agent",
            "version": "2.0.0",
            "description": "Rust-native agent engine — 10-phase lifecycle with session, memory, LLM reasoning, and tool dispatch",
            "with_schema": {
                "type": "object",
                "properties": {
                    "session_id": { "type": "string", "description": "Session identifier for conversation continuity", "default": "default" },
                    "message": { "type": "string", "description": "User message to process" },
                    "config": {
                        "type": "object",
                        "description": "Agent configuration",
                        "properties": {
                            "system_prompt": { "type": "string", "description": "System instructions for the LLM", "default": "You are a helpful assistant." },
                            "max_iterations": { "type": "integer", "description": "Maximum reasoning loop iterations", "default": 10, "minimum": 1, "maximum": 50 },
                            "max_tokens": { "type": "integer", "description": "Maximum tokens per LLM response", "default": 4096 },
                            "temperature": { "type": "number", "description": "LLM sampling temperature", "default": 0.7, "minimum": 0.0, "maximum": 2.0 },
                            "llm_resource": { "type": "string", "description": "Resource name for the LLM provider (e.g. 'default-llm', 'openai/gpt4')" },
                            "tools": {
                                "type": "array",
                                "description": "Tool definitions available to the agent (OpenAI function format)",
                                "items": {
                                    "type": "object",
                                    "properties": {
                                        "name": { "type": "string" },
                                        "description": { "type": "string" },
                                        "input_schema": { "type": "object" }
                                    },
                                    "required": ["name"]
                                }
                            },
                            "tool_flows": {
                                "type": "object",
                                "description": "Map tool names to workflow references for execution",
                                "additionalProperties": {
                                    "type": "object",
                                    "properties": {
                                        "namespace": { "type": "string" },
                                        "name": { "type": "string" }
                                    },
                                    "required": ["namespace", "name"]
                                }
                            },
                            "mcp_servers": {
                                "type": "array",
                                "description": "MCP server resource names for tool dispatch",
                                "items": { "type": "string" }
                            },
                            "memory_resource": { "type": "string", "description": "ADB resource name for agent memory" },
                            "memory_namespace": { "type": "string", "description": "Memory namespace for isolation", "default": "default" },
                            "memory_config": {
                                "type": "object",
                                "description": "Memory tier configuration",
                                "properties": {
                                    "working": { "type": "object", "properties": { "read": { "type": "boolean", "default": true }, "write": { "type": "boolean", "default": true } } },
                                    "episodic": { "type": "object", "properties": { "read": { "type": "boolean", "default": true }, "write": { "type": "boolean", "default": true } } },
                                    "semantic": { "type": "object", "properties": { "read": { "type": "boolean", "default": true }, "write": { "type": "boolean", "default": false } } },
                                    "procedural": { "type": "object", "properties": { "read": { "type": "boolean", "default": true }, "write": { "type": "boolean", "default": false } } }
                                }
                            },
                            "ttl": { "type": "integer", "description": "Session TTL in seconds", "default": 1800 }
                        },
                        "required": ["system_prompt"]
                    }
                },
                "required": ["message"]
            }
        }),
        json!({
            "name": "llm",
            "version": "1.0.0",
            "description": "Call an LLM provider (OpenAI-compatible API)",
            "with_schema": {
                "type": "object",
                "properties": {
                    "prompt": { "type": "string", "description": "User prompt" },
                    "system_prompt": { "type": "string", "description": "System instructions" },
                    "model": { "type": "string", "description": "Model name override" },
                    "max_tokens": { "type": "integer", "default": 1024 },
                    "temperature": { "type": "number", "default": 0.7 },
                    "messages": { "type": "array", "description": "Message history", "items": { "type": "object" } },
                    "tools": { "type": "array", "description": "Tool definitions", "items": { "type": "object" } },
                    "_resource": { "type": "string", "description": "LLM resource name", "default": "default-llm" }
                },
                "required": ["prompt"]
            }
        }),
        json!({
            "name": "lancedb",
            "version": "1.0.0",
            "description": "LanceDB vector database operations",
            "with_schema": {
                "type": "object",
                "properties": {
                    "operation": { "type": "string", "enum": ["query", "insert", "delete", "create_table", "list_tables"], "description": "Database operation" },
                    "table": { "type": "string", "description": "Table name" },
                    "filter": { "type": "string", "description": "SQL-like filter expression" },
                    "limit": { "type": "integer", "default": 100 },
                    "records": { "type": "array", "description": "Records to insert", "items": { "type": "object" } },
                    "_resource": { "type": "string", "description": "LanceDB resource name" }
                },
                "required": ["operation"]
            }
        }),
        json!({
            "name": "mcp",
            "version": "1.0.0",
            "description": "Call an MCP server tool via JSON-RPC",
            "with_schema": {
                "type": "object",
                "properties": {
                    "tool": { "type": "string", "description": "Tool name to call" },
                    "arguments": { "type": "object", "description": "Tool arguments" },
                    "server": { "type": "string", "description": "MCP server URL" },
                    "_resource": { "type": "string", "description": "MCP resource name" }
                },
                "required": ["tool"]
            }
        }),
        json!({
            "name": "state:get",
            "version": "1.0.0",
            "description": "Get a value from the state store",
            "with_schema": {
                "type": "object",
                "properties": {
                    "key": { "type": "string", "description": "State key to retrieve" }
                },
                "required": ["key"]
            }
        }),
        json!({
            "name": "state:set",
            "version": "1.0.0",
            "description": "Set a value in the state store",
            "with_schema": {
                "type": "object",
                "properties": {
                    "key": { "type": "string", "description": "State key" },
                    "value": { "description": "Value to store" },
                    "ttl": { "type": "integer", "description": "TTL in seconds" }
                },
                "required": ["key", "value"]
            }
        }),
        json!({
            "name": "state:delete",
            "version": "1.0.0",
            "description": "Delete a key from the state store",
            "with_schema": {
                "type": "object",
                "properties": {
                    "key": { "type": "string", "description": "State key to delete" }
                },
                "required": ["key"]
            }
        }),
        json!({
            "name": "adb",
            "version": "1.0.0",
            "description": "ADB AQL query — agent memory operations",
            "with_schema": {
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "AQL query string" },
                    "variables": { "type": "object", "description": "Optional query variables" },
                    "_resource": { "type": "string", "description": "ADB resource name (namespace/name)" }
                },
                "required": ["query"]
            }
        }),
    ]
}

async fn list_functions(State(_state): State<Arc<RegistryState>>) -> impl IntoResponse {
    let manifests = get_builtin_manifests();
    let summary: Vec<Value> = manifests.iter().map(|m| json!({
        "name": m["name"],
        "version": m["version"],
        "description": m["description"],
    })).collect();
    Json(summary)
}

async fn get_function(
    State(_state): State<Arc<RegistryState>>,
    Path(name): Path<String>,
) -> impl IntoResponse {
    let manifests = get_builtin_manifests();
    match manifests.into_iter().find(|m| m["name"].as_str() == Some(&name)) {
        Some(manifest) => (StatusCode::OK, Json(manifest)).into_response(),
        None => (StatusCode::NOT_FOUND, Json(json!({ "error": format!("function '{}' not found", name) }))).into_response(),
    }
}

#[derive(Deserialize)]
struct ResourcesQuery {
    #[serde(rename = "type")]
    type_ref: Option<String>,
}

async fn list_resources(
    State(state): State<Arc<RegistryState>>,
    Query(query): Query<ResourcesQuery>,
    principal: Option<Extension<Principal>>,
) -> impl IntoResponse {
    let who = caller(principal);
    let mut all: Vec<Value> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for ns in &["system", "a3t", "default"] {
        if let Ok(resources) = state.storage.resources().list(ns).await {
            for r in &resources {
                let key = format!("{}/{}", r.namespace, r.name);
                if !seen.insert(key) { continue; }
                if !crate::access::can_read_item(&who, flowr_policy::namespace::ItemKind::Resource, &r.namespace, &r.name, &r.type_ref).await {
                    continue;
                }
                if query.type_ref.as_ref().map_or(true, |t| r.type_ref == *t) {
                    all.push(json!({
                        "name": r.name,
                        "namespace": r.namespace,
                        "type": r.type_ref,
                        "config": flowr_mcp_core::tools::masked_config(&r.type_ref, &r.config),
                        "tags": r.tags,
                    }));
                }
            }
        }
    }
    Json(all).into_response()
}

// ── Resource write path — policy-gated (E2 #2b) ──

#[derive(Deserialize)]
struct ResourceBody {
    name: String,
    #[serde(default = "default_ns")]
    namespace: String,
    #[serde(rename = "type")]
    type_ref: String,
    #[serde(default)]
    config: Value,
    #[serde(default)]
    tags: Vec<String>,
}
fn default_ns() -> String { "default".to_string() }

/// A resource as callers see it: a secret's values hidden, other resources'
/// keys and passwords masked (`flowr_mcp_core::tools::masked_config`).
fn resource_json(r: &StorageResource) -> Value {
    json!({
        "name": r.name,
        "namespace": r.namespace,
        "type": r.type_ref,
        "config": flowr_mcp_core::tools::masked_config(&r.type_ref, &r.config),
        "tags": r.tags,
    })
}

/// GET /registry/resources/:ns/:name — fetch one resource.
async fn get_resource(
    State(state): State<Arc<RegistryState>>,
    Path((ns, name)): Path<(String, String)>,
    principal: Option<Extension<Principal>>,
) -> impl IntoResponse {
    let who = caller(principal);
    match state.storage.resources().get(&ns, &name).await {
        Ok(r) if crate::access::can_read_item(&who, flowr_policy::namespace::ItemKind::Resource, &r.namespace, &r.name, &r.type_ref).await =>
            Json(resource_json(&r)).into_response(),
        // Not there, or in a project the caller can't see: the same answer.
        _ => (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": format!("resource '{}/{}' not found", ns, name) })),
        )
            .into_response(),
    }
}

/// POST /registry/resources — create a resource. Gated by `resource:create`;
/// creating a `type: cedar` policy is refused unless the caller is InfoSec.
async fn create_resource(
    State(state): State<Arc<RegistryState>>,
    principal: Option<Extension<Principal>>,
    Json(body): Json<ResourceBody>,
) -> impl IntoResponse {
    let who = caller(principal);
    if let Err(resp) = crate::access::rest_write(&who, &body.namespace).await {
        return resp.into_response();
    }
    if let Err(resp) = authorize_resource_write(
        &who,
        flowr_policy::actions::resource_create(),
        &body.type_ref,
        &body.name,
    )
    .await
    {
        return resp.into_response();
    }

    let mut resource = StorageResource::new(body.name, body.type_ref, body.config)
        .with_namespace(body.namespace);
    resource.tags = body.tags;

    match state.storage.resources().create(resource).await {
        Ok(r) => (StatusCode::CREATED, Json(resource_json(&r))).into_response(),
        Err(e) => (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}

/// PUT /registry/resources/:ns/:name — update a resource's config. Gated by
/// `resource:update`; editing a `type: cedar` policy needs the InfoSec role.
async fn update_resource(
    State(state): State<Arc<RegistryState>>,
    Path((ns, name)): Path<(String, String)>,
    principal: Option<Extension<Principal>>,
    Json(config): Json<Value>,
) -> impl IntoResponse {
    // The gate keys off the *stored* type, so a caller can't dodge the SoD
    // guardrail by lying about the type in the body.
    let type_ref = match state.storage.resources().get(&ns, &name).await {
        Ok(r) => r.type_ref,
        Err(_) => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": format!("resource '{}/{}' not found", ns, name) })),
            )
                .into_response()
        }
    };

    let who = caller(principal);
    if let Err(resp) = crate::access::rest_write(&who, &crate::access::effective_namespace(
        flowr_policy::namespace::ItemKind::Resource, &ns, &name, &type_ref, "")).await {
        return resp.into_response();
    }
    if let Err(resp) = authorize_resource_write(
        &who,
        flowr_policy::actions::resource_update(),
        &type_ref,
        &name,
    )
    .await
    {
        return resp.into_response();
    }

    // The console edits what it was shown — masked values. Those stay as stored.
    let config = match state.storage.resources().get(&ns, &name).await {
        Ok(old) => flowr_mcp_core::tools::keep_masked(&old.type_ref, &old.config, &config),
        Err(_) => config,
    };
    match state.storage.resources().update(&ns, &name, config).await {
        Ok(r) => Json(resource_json(&r)).into_response(),
        Err(e) => (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}

/// DELETE /registry/resources/:ns/:name — remove a resource. Gated by
/// `resource:delete`; deleting a `type: cedar` policy needs the InfoSec role.
#[derive(Deserialize, Default)]
struct DeleteQuery {
    /// For a runtime: its name again, to confirm the reschedule.
    confirm: Option<String>,
}

async fn delete_resource(
    State(state): State<Arc<RegistryState>>,
    Path((ns, name)): Path<(String, String)>,
    Query(q): Query<DeleteQuery>,
    principal: Option<Extension<Principal>>,
) -> impl IntoResponse {
    let who = caller(principal);

    // Registry resource? (LLM/MCP/db/secret/…)
    if let Ok(r) = state.storage.resources().get(&ns, &name).await {
        if let Err(resp) = crate::access::rest_write(&who, &crate::access::effective_namespace(
            flowr_policy::namespace::ItemKind::Resource, &ns, &name, &r.type_ref, "")).await {
            return resp.into_response();
        }
        if let Err(resp) = authorize_resource_write(
            &who,
            flowr_policy::actions::resource_delete(),
            &r.type_ref,
            &name,
        )
        .await
        {
            return resp.into_response();
        }
        // A runtime is never refused — deleting it is terminating a container, and
        // what ran there is rescheduled. Without `?confirm=<name>` this answers
        // 409 with the plan (moves, fresh runtimes, isolated warnings) so the
        // caller can show it before anything changes.
        if r.type_ref == "runtime" {
            let by = who.id().to_string();
            return match crate::ops::terminate_runtime(state.storage.as_ref(), &name, &by, q.confirm.as_deref()).await {
                Ok(v) if v["preview"].as_bool() == Some(true) => {
                    let has_impact = v["impact"]["plan"].as_array().is_some_and(|a| !a.is_empty())
                        || v["impact"]["pinnedCanvases"].as_array().is_some_and(|a| !a.is_empty());
                    if has_impact {
                        (StatusCode::CONFLICT, Json(json!({
                            "error": "confirm required",
                            "message": format!("Deleting runtime '{name}' reschedules what runs on it. Review the plan, then repeat with ?confirm={name}."),
                            "impact": v["impact"],
                        }))).into_response()
                    } else {
                        match crate::ops::terminate_runtime(state.storage.as_ref(), &name, &by, Some(&name)).await {
                            Ok(_) => StatusCode::NO_CONTENT.into_response(),
                            Err(e) => (StatusCode::BAD_REQUEST, Json(json!({ "error": e }))).into_response(),
                        }
                    }
                }
                Ok(v) => Json(v).into_response(),
                Err(e) => (StatusCode::BAD_REQUEST, Json(json!({ "error": e }))).into_response(),
            };
        }
        // A deployment record is removed by undeploying it, so the ledger the
        // runtime admits against stays in step.
        if r.type_ref == "deployment" {
            return match crate::ops::undeploy(state.storage.as_ref(), &name, &who.id().to_string()).await {
                Ok(_) => StatusCode::NO_CONTENT.into_response(),
                Err(e) => (StatusCode::BAD_REQUEST, Json(json!({ "error": e }))).into_response(),
            };
        }
        if let Err(why) = crate::integrity::check_resource_delete(state.storage.as_ref(), &ns, &name).await {
            return (StatusCode::CONFLICT, Json(json!({ "error": why }))).into_response();
        }
        return match state.storage.resources().delete(&ns, &name).await {
            Ok(()) => StatusCode::NO_CONTENT.into_response(),
            Err(e) => (StatusCode::BAD_REQUEST, Json(json!({ "error": e.to_string() }))).into_response(),
        };
    }

    // Not a registry resource — a workflow lives in a separate store. Deleting it
    // via this admin REST path is the intended destructive route (the MCP
    // `delete_flow` tool is Cedar-blocked for agents/LLMs). Gate it as a
    // `resource:delete` on a `workflow` — permitted for Admin/SecurityAdmin.
    if state.storage.workflows().get(&ns, &name).await.is_ok() {
        if let Err(resp) = crate::access::rest_write(&who, &crate::access::effective_namespace(
            flowr_policy::namespace::ItemKind::Flow, &ns, &name, "", "")).await {
            return resp.into_response();
        }
        if let Err(resp) = authorize_resource_write(
            &who,
            flowr_policy::actions::resource_delete(),
            "workflow",
            &name,
        )
        .await
        {
            return resp.into_response();
        }
        if let Err(why) = crate::integrity::check_flow_delete(&ns, &name) {
            return (StatusCode::CONFLICT, Json(json!({ "error": why }))).into_response();
        }
        return match state.storage.workflows().delete(&ns, &name).await {
            Ok(()) => StatusCode::NO_CONTENT.into_response(),
            Err(e) => (StatusCode::BAD_REQUEST, Json(json!({ "error": e.to_string() }))).into_response(),
        };
    }

    (
        StatusCode::NOT_FOUND,
        Json(json!({ "error": format!("resource '{}/{}' not found", ns, name) })),
    )
        .into_response()
}

async fn list_templates(
    State(state): State<Arc<RegistryState>>,
) -> impl IntoResponse {
    // List workflows from all namespaces, fetch full records to check for agent call
    let mut templates: Vec<Value> = Vec::new();
    for ns in &["system", "a3t", "default"] {
        if let Ok(summaries) = state.storage.workflows().list(ns).await {
            for s in &summaries {
                // Fetch full record to check raw YAML
                if let Ok(record) = state.storage.workflows().get(ns, &s.name).await {
                    if record.raw_yaml.contains("a3t:agent") || record.raw_yaml.contains("call: agent") {
                        templates.push(json!({
                            "name": s.name,
                            "namespace": s.namespace,
                            "version": s.version,
                        }));
                    }
                }
            }
        }
    }
    Json(templates).into_response()
}

/// Get a full TemplateManifest for a specific template workflow.
/// Derives config_schema, resource_slots, extension_slots, integration_slots
/// from the workflow's YAML structure + the function manifest.
async fn get_template(
    State(state): State<Arc<RegistryState>>,
    Path((ns, name)): Path<(String, String)>,
) -> impl IntoResponse {
    // Fetch the workflow
    let workflow = match state.storage.workflows().get(&ns, &name).await {
        Ok(w) => w,
        Err(e) => {
            let msg = e.to_string();
            if msg.contains("not found") {
                return (StatusCode::NOT_FOUND, Json(json!({ "error": format!("template '{}/{}' not found", ns, name) }))).into_response();
            }
            return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": msg }))).into_response();
        }
    };

    // Parse the YAML
    let yaml_str = &workflow.raw_yaml;

    let parsed: Value = match serde_yaml::from_str(yaml_str) {
        Ok(v) => v,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": format!("YAML parse error: {}", e) }))).into_response(),
    };

    // Find the call type and version
    let mut call_type = String::new();
    let mut with_config_schema = json!({});
    if let Some(tasks) = parsed.get("do").and_then(|d| d.as_array()) {
        for task in tasks {
            if let Some(obj) = task.as_object() {
                for (_task_name, def) in obj {
                    if let Some(call) = def.get("call").and_then(|c| c.as_str()) {
                        call_type = call.to_string();
                        // Get the config schema from the function manifest
                        let manifests = get_builtin_manifests();
                        for m in &manifests {
                            if m.get("name").and_then(|n| n.as_str()) == Some(call) {
                                if let Some(ws) = m.get("with_schema") {
                                    if let Some(config_prop) = ws.get("properties").and_then(|p| p.get("config")) {
                                        with_config_schema = config_prop.clone();
                                    }
                                }
                            }
                        }
                        break;
                    }
                }
            }
        }
    }

    // Extract resource_slots from config schema properties that reference resources
    let mut resource_slots = json!({});
    if let Some(props) = with_config_schema.get("properties").and_then(|p| p.as_object()) {
        for (key, prop) in props {
            let desc = prop.get("description").and_then(|d| d.as_str()).unwrap_or("");
            let desc_lower = desc.to_lowercase();
            if key.ends_with("_resource") || key == "_resource" ||
               desc_lower.contains("resource name") || desc_lower.contains("resource ref") {
                let resource_type = if key.contains("llm") { "llm" }
                    else if key.contains("memory") { "adb" }
                    else { "unknown" };
                resource_slots[key] = json!({
                    "required": false,
                    "resource_type": resource_type,
                    "description": desc,
                });
            }
        }
    }

    // Extract extension_slots from use.extensions
    let mut extension_slots = json!({});
    if let Some(extensions) = parsed.get("use").and_then(|u| u.get("extensions")).and_then(|e| e.as_array()) {
        for ext in extensions {
            if let Some(obj) = ext.as_object() {
                for (ext_name, ext_def) in obj {
                    let extend = ext_def.get("extend").and_then(|e| e.as_str()).unwrap_or("all");
                    let when = ext_def.get("when").and_then(|w| w.as_str());
                    extension_slots[ext_name] = json!({
                        "required": false,
                        "description": format!("Extension hook: {}", ext_name),
                        "extends": extend,
                        "when": when,
                        "input_schema": { "type": "object" },
                        "output_schema": { "type": "object" },
                    });
                }
            }
        }
    }

    // Extract integration_slots from config arrays (mcp_servers, tool_flows)
    let mut integration_slots = json!({});
    if let Some(props) = with_config_schema.get("properties").and_then(|p| p.as_object()) {
        if props.contains_key("mcp_servers") {
            integration_slots["mcp_servers"] = json!({
                "required": false,
                "description": "MCP server connections for tool dispatch",
                "per_entry": {
                    "input_schema": { "type": "object", "properties": { "tool": { "type": "string" }, "arguments": { "type": "object" } } },
                    "output_schema": { "type": "object" },
                },
            });
        }
        if props.contains_key("tool_flows") {
            integration_slots["tool_flows"] = json!({
                "required": false,
                "description": "Workflow references for tool dispatch",
                "per_entry": {
                    "input_schema": { "type": "object", "properties": { "namespace": { "type": "string" }, "name": { "type": "string" } } },
                    "output_schema": { "type": "object" },
                },
            });
        }
    }

    // Find function version
    let mut engine_version = "1.0.0".to_string();
    let manifests = get_builtin_manifests();
    for m in &manifests {
        if m.get("name").and_then(|n| n.as_str()) == Some(&call_type) {
            engine_version = m.get("version").and_then(|v| v.as_str()).unwrap_or("1.0.0").to_string();
        }
    }

    let manifest = json!({
        "name": workflow.name(),
        "namespace": workflow.namespace(),
        "version": workflow.version(),
        "engine": {
            "call": call_type,
            "version": engine_version,
        },
        "config_schema": with_config_schema,
        "resource_slots": resource_slots,
        "extension_slots": extension_slots,
        "integration_slots": integration_slots,
    });

    (StatusCode::OK, Json(manifest)).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn status(principal: Option<Principal>, body: Value) -> StatusCode {
        let dir = tempfile::tempdir().unwrap();
        let storage = crate::init_storage(dir.path().to_str().unwrap()).await.unwrap();
        let state = State(Arc::new(RegistryState { storage }));
        adb_sql(state, principal.map(Extension), Json(body)).await.into_response().status()
    }

    /// SQL on ADB is the superadmin's alone: not other roles, agents or services.
    #[tokio::test]
    async fn adb_sql_is_for_the_superadmin_only() {
        let body = json!({ "resource": "a3t/missing", "sql": "SELECT 1" });
        let agent = Principal::Agent {
            agent_id: flowr_policy::AgentId("a".into()),
            on_behalf_of: None,
            flow_id: None,
            roles: vec![flowr_policy::roles::SUPER_ADMIN.into()],
            owner: None,
        };
        for p in [
            None,
            Some(Principal::service("mcp")),
            Some(Principal::human("ada", vec!["Admin".into(), "Operator".into()])),
            Some(agent),
        ] {
            assert_eq!(status(p.clone(), body.clone()).await, StatusCode::FORBIDDEN, "{p:?}");
        }
        // The superadmin gets past the check (then the unknown resource is refused).
        let root = Principal::human("root", flowr_policy::roles::canonicalize("superadmin"));
        assert_eq!(status(Some(root.clone()), body).await, StatusCode::BAD_REQUEST);
        assert_eq!(status(Some(root), json!({ "resource": "a3t/adb", "sql": " " })).await, StatusCode::BAD_REQUEST);
    }
}
