//! call:a3t:agent handler — Rust-native agent engine
//!
//! Implements the 10-phase agent lifecycle as a single TaskHandler:
//!   1. init_turn      — extract config, initialize state
//!   2. load_session   — restore conversation history from state store
//!   3. recall_memory  — query ClawDB memory tiers (working/episodic/semantic/procedural)
//!   4. procedural_check — short-circuit if procedural memory matches
//!   5. build_prompt   — assemble system prompt
//!   6. append_message — add user message to history
//!   7. reasoning_loop — LLM call + tool dispatch loop
//!   8. store_memory   — persist to working + episodic memory
//!   9. save_session   — persist conversation to state store
//!  10. emit_response  — return final response
//!
//! Extensions fire at each phase via ExecutionContext.extensions.
//! The handler calls run_extensions_before/after at each named phase,
//! allowing wrapper workflows to inject hooks (audit, budget, etc).

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Arc;
use tracing::{debug, info, warn, error};

use swf_parser::{NamedTask, Task};
use swf_runtime::context::ExecutionContext;
use swf_runtime::error::{RuntimeError, Result};
use swf_runtime::traits::{TaskHandler, TaskResult, FlowDirective};
use swf_runtime::executor::extensions::ResolvedExtension;

use flowr_storage::StorageBackend;

// ═══════════════════════════════════════════════════════════════════
// Config types — extracted from call.with
// ═══════════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Deserialize)]
struct AgentConfig {
    #[serde(default = "default_session_id")]
    session_id: String,
    #[serde(default)]
    message: String,

    /// What the user is looking at in the console when they sent this turn
    /// (open artifact + active form section). The form is the discovery
    /// surface — it is how a user learns what is configurable — so a request
    /// like "tighten this" only resolves if the agent can see the same screen.
    #[serde(default)]
    ui_context: Option<Value>,

    #[serde(default)]
    config: AgentSettings,
}

fn default_session_id() -> String { "default".to_string() }

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct AgentSettings {
    #[serde(default = "default_system_prompt")]
    system_prompt: String,
    #[serde(default = "default_max_iterations")]
    max_iterations: usize,
    #[serde(default = "default_max_tokens")]
    max_tokens: u32,
    #[serde(default = "default_temperature")]
    temperature: f64,
    #[serde(default)]
    llm_resource: Option<String>,
    #[serde(default)]
    tools: Vec<Value>,
    // E2 #3: instance-enabled subset of the exposed tools. Empty = no narrowing
    // (every exposed tool is enabled). When set, the template ceiling forbids any
    // tool not in this set even if the agent is offered it.
    #[serde(default)]
    /// The instance-enabled tool set.
    ///
    /// `Option` is load-bearing: **absent and empty are different things.**
    /// `None` means the agent was never given a tool restriction, so it is not
    /// narrowed (CE parity). `Some([])` means someone configured it down to
    /// nothing — every tool then falls outside the ceiling and prompts its owner.
    ///
    /// Conflating the two was the reported bug: removing the last tool from an
    /// agent made it LESS restricted rather than more, because an empty set read
    /// as "no restriction configured".
    enabled_tools: Option<Vec<String>>,
    #[serde(default)]
    tool_flows: HashMap<String, ToolFlowRef>,
    #[serde(default)]
    mcp_servers: Vec<String>,
    #[serde(default)]
    memory_resource: Option<String>,
    #[serde(default = "default_memory_namespace")]
    memory_namespace: String,
    #[serde(default)]
    memory_config: MemoryTierConfig,
    #[serde(default = "default_ttl")]
    ttl: u64,
    /// Role ceiling: the most authority this agent may ever carry, regardless of
    /// who runs it. Attenuate-only — the effective roles are the caller's roles
    /// INTERSECTED with this, so naming a role here can never grant it. `None`
    /// (the default) means "do not narrow", matching how `enabled_tools` treats
    /// an empty set.
    #[serde(default)]
    role_ceiling: Option<Vec<String>>,
    /// The principal that created this agent. Default authority over what the
    /// agent may be granted; anything beyond the owner's own reach must be
    /// requested from InfoSec rather than self-granted.
    #[serde(default)]
    owner: Option<String>,
}

fn default_system_prompt() -> String { "You are a helpful assistant.".to_string() }
fn default_max_iterations() -> usize { 10 }
fn default_max_tokens() -> u32 { 4096 }
fn default_temperature() -> f64 { 0.7 }
fn default_memory_namespace() -> String { "default".to_string() }
fn default_ttl() -> u64 { 1800 }

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct MemoryTierConfig {
    #[serde(default)]
    working: TierMode,
    #[serde(default)]
    episodic: TierMode,
    #[serde(default)]
    semantic: TierMode,
    #[serde(default)]
    procedural: TierMode,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TierMode {
    #[serde(default = "bool_true")]
    read: bool,
    #[serde(default = "bool_true")]
    write: bool,
}
fn bool_true() -> bool { true }

impl Default for TierMode {
    fn default() -> Self { Self { read: true, write: true } }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ToolFlowRef {
    namespace: String,
    name: String,
}

// ═══════════════════════════════════════════════════════════════════
// Agent state — mutable through the loop
// ═══════════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Serialize)]
struct Message {
    role: String,
    content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_call_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_calls: Option<Vec<Value>>,
}

/// Rebuild the conversation from a persisted session. This must be the exact
/// inverse of `save_session`, which serializes `Message` as-is — in particular
/// the assistant's `tool_calls` MUST round-trip. Dropping them (the previous
/// behaviour) left every `tool` result orphaned from the call that produced it,
/// so strict OpenAI-compatible providers rejected the *next* turn with
/// "No tool call found for function call output with call_id …". A missing or
/// non-string `content` is tolerated as "" rather than dropping the message,
/// which would orphan its tool results the same way.
fn messages_from_session(session: &Value) -> Vec<Message> {
    let Some(msgs) = session.get("messages").and_then(|v| v.as_array()) else {
        return Vec::new();
    };
    msgs.iter()
        .filter_map(|msg| {
            let role = msg.get("role")?.as_str()?;
            let content = msg.get("content").and_then(|v| v.as_str()).unwrap_or("");
            Some(Message {
                role: role.to_string(),
                content: content.to_string(),
                tool_call_id: msg.get("tool_call_id").and_then(|v| v.as_str()).map(String::from),
                tool_calls: msg.get("tool_calls").and_then(|v| v.as_array()).cloned(),
            })
        })
        .collect()
}

/// State-store key for one conversation's transcript.
pub fn session_key(session_id: &str) -> String {
    format!("agent:session:{}", session_id)
}

/// State-store key for one user's list of conversations. The transcripts are the
/// source of truth; this is a pointer list so the sidebar can be built without
/// scanning every key in the state table.
/// Look up who owns the agent behind a session.
///
/// `session_id` is `<agent-id>:session-…` when a chat runs a configured agent,
/// and a bare session id otherwise (no agent, so no owner). Prefers the stamped
/// `owner`, falling back to `created_by` for records written before ownership was
/// recorded.
async fn resolve_agent_owner(
    storage: &Arc<dyn StorageBackend>,
    session_id: &str,
) -> Option<String> {
    let agent_id = session_id.split(':').next().filter(|s| !s.is_empty())?;
    if agent_id == session_id {
        return None; // no agent prefix — a plain chat session
    }
    // Agents live in the platform LanceDB table, NOT the state store. The
    // `agent:config:<id>` key only exists for agents created through
    // `mcp_create_agent`; everything the console writes goes to `agent_configs`.
    // Reading state first therefore found nothing for every real agent, which
    // made this resolver — and with it the whole owner check — silently inert.
    let q = crate::handlers::call_lancedb::sql_quote(agent_id);
    let rows = crate::handlers::call_lancedb::query_platform_table(
        storage.as_ref(), "agent_configs", Some(format!("id = {q}")), 1,
    )
    .await
    .ok()?;
    let row = rows.into_iter().next()?;
    let cfg = row.get("config").and_then(|c| c.as_object());
    row.get("owner")
        .and_then(|v| v.as_str())
        .or_else(|| row.get("created_by").and_then(|v| v.as_str()))
        .or_else(|| cfg.and_then(|c| c.get("owner")).and_then(|v| v.as_str()))
        .or_else(|| cfg.and_then(|c| c.get("created_by")).and_then(|v| v.as_str()))
        .map(str::to_string)
}

pub fn session_index_key(owner: &str) -> String {
    format!("agent:sessions:{}", owner)
}

/// Cap on index entries per owner, newest first. Bounds the row so one user
/// cannot grow it without limit; the transcripts themselves are never trimmed.
const SESSION_INDEX_MAX: usize = 500;

/// First user message, trimmed — a title for the sidebar so the other device
/// shows something recognisable rather than "Conversation".
fn derive_title(messages: &[Message]) -> String {
    let first = messages.iter()
        .find(|m| m.role == "user" && !m.content.trim().is_empty())
        .map(|m| m.content.trim())
        .unwrap_or("New conversation");
    let one_line: String = first.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut out: String = one_line.chars().take(60).collect();
    if one_line.chars().count() > 60 { out.push('…'); }
    out
}

/// Add or refresh this session in its owner's index, newest first.
async fn upsert_session_index(
    storage: &dyn flowr_storage::StorageBackend,
    owner: &str,
    session_id: &str,
    messages: &[Message],
) -> Result<()> {
    let key = session_index_key(owner);
    let mut entries: Vec<Value> = storage.state().get(&key).await
        .ok().flatten()
        .and_then(|v| v.get("sessions").and_then(|s| s.as_array()).cloned())
        .unwrap_or_default();
    // Drop any existing entry for this session so the refreshed one sorts first.
    entries.retain(|e| e.get("session_id").and_then(|v| v.as_str()) != Some(session_id));
    entries.insert(0, json!({
        "session_id": session_id,
        "title": derive_title(messages),
        "message_count": messages.iter().filter(|m| m.role == "user" || m.role == "assistant").count(),
        "updated_at": chrono::Utc::now().to_rfc3339(),
    }));
    entries.truncate(SESSION_INDEX_MAX);
    storage.state()
        .set(&key, json!({ "owner": owner, "sessions": entries }), None).await
        .map_err(|e| RuntimeError::Communication(format!("Failed to save session index: {}", e)))?;
    Ok(())
}

/// Remove one session from its owner's index. Used when a conversation is deleted.
pub async fn remove_from_session_index(
    storage: &dyn flowr_storage::StorageBackend,
    owner: &str,
    session_id: &str,
) -> Result<()> {
    let key = session_index_key(owner);
    let Some(existing) = storage.state().get(&key).await.ok().flatten() else { return Ok(()) };
    let mut entries: Vec<Value> = existing.get("sessions")
        .and_then(|s| s.as_array()).cloned().unwrap_or_default();
    entries.retain(|e| e.get("session_id").and_then(|v| v.as_str()) != Some(session_id));
    storage.state()
        .set(&key, json!({ "owner": owner, "sessions": entries }), None).await
        .map_err(|e| RuntimeError::Communication(format!("Failed to save session index: {}", e)))?;
    Ok(())
}

/// List one owner's conversations, newest first.
pub async fn list_owner_sessions(
    storage: &dyn flowr_storage::StorageBackend,
    owner: &str,
) -> Vec<Value> {
    storage.state().get(&session_index_key(owner)).await
        .ok().flatten()
        .and_then(|v| v.get("sessions").and_then(|s| s.as_array()).cloned())
        .unwrap_or_default()
}

struct AgentState {
    iteration: u32,
    should_continue: bool,
    tool_calls_made: u32,
    agent_response: Option<String>,
    procedural_hit: bool,
    messages: Vec<Message>,
    system_prompt: String,
}

impl AgentState {
    fn new(config: &AgentConfig) -> Self {
        Self {
            iteration: 0,
            should_continue: true,
            tool_calls_made: 0,
            agent_response: None,
            procedural_hit: false,
            messages: Vec::new(),
            system_prompt: Self::prompt_with_ui_context(config),
        }
    }

    /// Append the console context to the system prompt. Kept out of the message
    /// text so the visible transcript stays exactly what the user typed.
    /// How the platform wants an agent to behave around permissions.
    ///
    /// Appended to every agent's prompt because it describes the RUNTIME, not the
    /// agent's job, and getting it wrong deadlocks the feature: a model that
    /// checks whether it is allowed and then asks the user to go and grant
    /// something never attempts the call, so the approval request is never
    /// raised and there is nothing for the user to approve. Observed live — the
    /// agent called `list_permissions`, saw an empty list, and told the user to
    /// approve in a panel that had nothing in it.
    const PERMISSION_NOTE: &'static str = "\n\n## Tool permissions\n\
Some tools need the user's approval. You do not have to arrange that, and you \
must not try to: just call the tool you need. If approval is required the \
platform pauses and asks the user at that moment, and your call proceeds by \
itself once they allow it. Never check your permissions before calling \
something, and never tell the user to go and grant a permission — attempting \
the call is what raises the request they would approve. If a call comes back \
refused, say so plainly and do not retry it in the same turn.\n\n\
A refusal is never permanent — the user can re-enable it at any time. Each tool \
carries a \"permission NOW\" note that is RECOMPUTED FROM THE LIVE PERMISSION \
LIST EVERY TURN. Trust that note over anything that happened earlier in this \
conversation: if a tool you were once refused no longer says declined, the user \
has re-enabled it, so just call it. Never tell the user a tool is still blocked \
on the strength of an older message.";

    fn prompt_with_ui_context(config: &AgentConfig) -> String {
        let base = format!("{}{}", config.config.system_prompt, Self::PERMISSION_NOTE);
        let Some(ui) = config.ui_context.as_ref().and_then(|v| v.as_object()) else {
            return base;
        };
        let mut bits: Vec<String> = Vec::new();
        if let (Some(k), Some(n)) = (
            ui.get("kind").and_then(|v| v.as_str()),
            ui.get("name").and_then(|v| v.as_str()),
        ) {
            bits.push(format!("{k} `{n}`"));
        }
        if let Some(sec) = ui.get("section").and_then(|v| v.as_str()) {
            bits.push(format!("the \"{sec}\" section"));
        }
        if bits.is_empty() { return base; }
        let mut out = format!(
            "{base}\n\n## What the user is looking at\n\
             The console is currently showing {}. When the user says \"this\", \
             \"here\" or names no target, assume they mean that. Read it before \
             changing it.",
            bits.join(", viewing ")
        );

        // A designer's AI tab: the document on screen IS the subject, and changes
        // go back to the editor as proposals — the user saves, not the agent.
        if ui.get("mode").and_then(|v| v.as_str()) == Some("designer") {
            out.push_str(
                "\n\n## You are the assistant inside this designer\n\
                 The user is editing it in a designer and talking to you from its AI tab. \
                 To change it, call `propose_edit` with the COMPLETE updated document — keep \
                 everything you were not asked to change exactly as it is, including keys, \
                 order and comments where you can. The edit is applied to the editor on \
                 screen and is NOT saved; the user reviews it and presses Save. Never call \
                 create_*/update_*/delete_* tools or deploy from here, and never say it is \
                 saved. If the request is a question, answer it without proposing an edit.",
            );
            if let Some(txt) = ui.get("draft_text").and_then(|v| v.as_str()) {
                out.push_str("\n\n### The document in the editor (current, possibly unsaved)\n```\n");
                out.push_str(txt);
                if !txt.ends_with('\n') { out.push('\n'); }
                out.push_str("```");
                if ui.get("draft_truncated").and_then(|v| v.as_bool()).unwrap_or(false) {
                    out.push_str("\n\n(Truncated — the beginning of the document only. Do not propose an edit that would drop the rest; say it is too large.)");
                }
            }
            return out;
        }

        // Unsaved editor edits. Without this the agent reads the registry copy
        // and answers about a version the user has already changed underneath
        // it — so say plainly which text wins.
        let dirty = ui.get("dirty").and_then(|v| v.as_bool()).unwrap_or(false);
        if dirty {
            out.push_str(
                "\n\nThe user has edited it in the editor and has NOT saved yet, \
                 so the stored copy is out of date: a get_* tool would return the \
                 old version. The text below is what is on screen and is the one \
                 to reason about. Do not re-read it with a tool. If a change has \
                 to be persisted, say that it needs saving (the user presses Save \
                 in the pane) rather than writing the stored copy behind them.",
            );
            if let Some(txt) = ui.get("draft_text").and_then(|v| v.as_str()) {
                out.push_str("\n\n### Unsaved editor contents\n```\n");
                out.push_str(txt);
                if !txt.ends_with('\n') { out.push('\n'); }
                out.push_str("```");
                if ui.get("draft_truncated").and_then(|v| v.as_bool()).unwrap_or(false) {
                    out.push_str(
                        "\n\n(Truncated — this is the beginning of the buffer, not \
                         all of it. Say so if the answer depends on the rest.)",
                    );
                }
            }
        }
        out
    }

    fn to_context_value(&self, config: &AgentConfig) -> Value {
        json!({
            "session_id": config.session_id,
            "message": config.message,
            "_iteration": self.iteration,
            "_should_continue": self.should_continue,
            "_tool_calls_made": self.tool_calls_made,
            "_agent_response": self.agent_response,
            "_procedural_hit": self.procedural_hit,
            "_messages": self.messages,
            "_system_prompt": self.system_prompt,
            "_config": config.config,
        })
    }
}

// ═══════════════════════════════════════════════════════════════════
// Handler implementation
// ═══════════════════════════════════════════════════════════════════

pub struct AgentHandler;

#[async_trait]
impl TaskHandler for AgentHandler {
    fn task_type(&self) -> &str {
        "call:a3t:agent"
    }

    async fn execute(
        &self,
        task: &NamedTask,
        _input: Value,
        ctx: &mut ExecutionContext,
    ) -> Result<TaskResult> {
        let with = match &task.task {
            Task::Call(c) => c.with.clone().unwrap_or(json!({})),
            _ => return Err(RuntimeError::task_failed(&task.name, "expected call task")),
        };

        let config: AgentConfig = serde_json::from_value(with)
            .map_err(|e| RuntimeError::task_failed(&task.name, format!("invalid agent config: {}", e)))?;

        info!(task = %task.name, session = %config.session_id, "call:a3t:agent starting");

        let storage = get_storage(ctx)?;
        let http_client = reqwest::Client::new();

        // ── Admission: a paused agent does not run, however it was reached ──
        // Chat, send_message, triggers and other agents all arrive here, which
        // makes this the one place an agent pause cannot be routed around —
        // including agents nested inside another flow, which never pass the
        // worker's own check under their own name.
        if let Some(agent) = crate::ops::agent_of_session(&config.session_id) {
            if let Err(b) = crate::ops::admit(storage.as_ref(), None, Some(agent)).await {
                return Err(RuntimeError::task_failed(&task.name, format!("refused: {b}")));
            }
        }

        // ── E0.4c: the agent is a first-class principal ──
        // It acts on behalf of whoever started the flow (ctx.principal). Local
        // tool calls are dispatched in-process through the gated choke point
        // (site A) as this identity — see call_mcp_tool / list_mcp_tools.
        //
        // It also inherits the caller's ROLES, attenuated by any ceiling the
        // agent declares (`config.role_ceiling`). Carrying `on_behalf_of` alone
        // was not enough: policy could see whose name the agent acted under but
        // not what that person was actually allowed to do, so an agent run by a
        // Viewer had the same authority as one run by an Admin.
        // Resolve the agent's OWNER from the registry, server-side.
        //
        // It is deliberately not taken from the step config: a flow that could
        // name its own owner could name whose authority bounds its grants. The
        // session id carries the agent id (`<agent-id>:session-…`), so the record
        // can be looked up and trusted.
        //
        // Agents created before ownership was recorded have no `owner`, but they
        // do carry `created_by` — real provenance, not an invention — so that is
        // the fallback. Without it every pre-existing agent could never produce a
        // permission prompt, because the owner check would always fail.
        let owner = match config.config.owner.clone() {
            Some(o) => Some(o),
            None => resolve_agent_owner(&storage, &config.session_id).await,
        };
        let role_ceiling = config.config.role_ceiling.as_deref();
        let agent_principal = flowr_mcp_core::Principal::agent_for(
            flowr_mcp_core::AgentId(format!("agent:{}", config.session_id)),
            &ctx.principal,
            Some(format!("{}/{}", ctx.namespace, ctx.name)),
            owner,
            role_ceiling,
        );

        let mut state = AgentState::new(&config);

        // Sync state to context for extension visibility
        ctx.context = state.to_context_value(&config);

        // ── Phase 1: INIT TURN ─────────────────────────────
        fire_before(self, ctx, "init_turn", "set").await;
        debug!("phase 1: init_turn");
        fire_after(self, ctx, "init_turn", "set").await;

        // ── Phase 2: LOAD SESSION ──────────────────────────
        fire_before(self, ctx, "load_session", "call:state:get").await;
        let session_key = session_key(&config.session_id);
        if let Ok(Some(session)) = storage.state().get(&session_key).await {
            state.messages.extend(messages_from_session(&session));
            debug!(restored_messages = state.messages.len(), "session restored");
        }
        ctx.context = state.to_context_value(&config);
        fire_after(self, ctx, "load_session", "call:state:get").await;

        // ── Phase 3: RECALL MEMORY ─────────────────────────
        fire_before(self, ctx, "recall_memory", "call:clawdb").await;
        // Memory recall via ClawDB is optional — only if memory_resource is configured
        // TODO: implement ClawDB AQL queries for working/episodic/semantic/procedural
        if config.config.memory_resource.is_some() {
            debug!("memory recall: skipped (ClawDB integration pending in call:a3t:agent)");
        }
        fire_after(self, ctx, "recall_memory", "call:clawdb").await;

        // ── Phase 4: PROCEDURAL CHECK ──────────────────────
        // Procedural memory short-circuit (if recall returned a match)
        if state.procedural_hit {
            debug!("procedural hit — skipping reasoning loop");
        }

        if !state.procedural_hit {
            // ── Phase 5: BUILD PROMPT ──────────────────────
            // System prompt is already set from config

            // ── Phase 6: APPEND USER MESSAGE ───────────────
            state.messages.push(Message {
                role: "user".to_string(),
                content: config.message.clone(),
                tool_call_id: None,
                tool_calls: None,
            });

            // ── Phase 7: REASONING LOOP ────────────────────
            let llm_resource = config.config.llm_resource.as_deref().unwrap_or("default-llm");

            // Build tool list: merge inline tools + tool_flows (fetch flow metadata)
            // The same ceiling the gate will apply, so the advertised permission
            // state matches what actually happens when the tool is called.
            let ceiling_for_hints = build_ceiling_attributes(&config.config, &ctx.template_meta);
            let mut all_tools: Vec<Value> = config.config.tools.clone();
            for (tool_name, flow_ref) in &config.config.tool_flows {
                match storage.workflows().get(&flow_ref.namespace, &flow_ref.name).await {
                    Ok(record) => {
                        // Extract description and input schema from the flow
                        let description = record.metadata.description.clone()
                            .unwrap_or_else(|| format!("Execute workflow {}/{}", flow_ref.namespace, flow_ref.name));
                        let input_schema = match record.parse() {
                            Ok(swf) => {
                                // Try to get input.schema from the parsed workflow
                                if let Some(input) = swf.input {
                                    input.schema.unwrap_or(json!({"type": "object"}))
                                } else {
                                    json!({"type": "object", "properties": {}, "description": "No input schema defined"})
                                }
                            }
                            Err(_) => json!({"type": "object"}),
                        };
                        all_tools.push(json!({
                            "name": tool_name,
                            "description": description,
                            "input_schema": input_schema,
                        }));
                        info!(tool = %tool_name, flow = %flow_ref.name, "registered tool_flow as LLM tool");
                    }
                    Err(e) => {
                        warn!(tool = %tool_name, flow = %flow_ref.name, error = %e, "failed to load tool_flow — skipping");
                    }
                }
            }

            // Discover tools from MCP servers (tools/list) and advertise them to the LLM.
            // Tool calls the LLM makes for these names are dispatched via call_mcp_tool.
            for server in &config.config.mcp_servers {
                match list_mcp_tools(&http_client, &storage, &agent_principal, server).await {
                    Ok(mcp_tools) => {
                        let count = mcp_tools.len();
                        for t in mcp_tools {
                            let name = t.get("name").and_then(|v| v.as_str()).unwrap_or("");
                            if name.is_empty() { continue; }
                            let description = t.get("description").and_then(|v| v.as_str()).unwrap_or("");
                            let input_schema = t.get("inputSchema")
                                .or_else(|| t.get("input_schema"))
                                .cloned()
                                .unwrap_or_else(|| json!({"type": "object"}));
                            all_tools.push(json!({
                                "name": name,
                                "description": description,
                                "input_schema": input_schema,
                            }));
                        }
                        info!(server = %server, count = count, "discovered MCP tools for LLM");
                    }
                    Err(e) => warn!(server = %server, error = %e, "MCP tools/list failed — skipping"),
                }
            }

            // Say, on the tool itself, what will happen if it is called.
            //
            // A sentence in the system prompt was not enough: the model kept
            // deciding for itself that it lacked permission and telling the user
            // to grant it manually, which can never work — the request a user
            // approves is raised BY the call. Annotating the tool description is
            // far harder to reason past, because the model reads it at the moment
            // it is choosing the tool.
            for t in all_tools.iter_mut() {
                let Some(name) = t.get("name").and_then(|v| v.as_str()).map(str::to_string) else {
                    continue;
                };
                let perm = flowr_mcp_core::tool_permission(
                    &agent_principal, &config.session_id, &name, ceiling_for_hints.clone(),
                ).await;
                if let Some(hint) = perm.hint() {
                    if let Some(d) = t.get_mut("description").and_then(|v| v.as_str()).map(str::to_string) {
                        t["description"] = Value::String(format!("{d} {hint}"));
                    } else {
                        t["description"] = Value::String(hint.to_string());
                    }
                }
            }
            info!(total_tools = all_tools.len(), "tools available for LLM");

            for _iter in 0..config.config.max_iterations {
                if !state.should_continue { break; }
                crate::token_stream::publish(&config.session_id, "step_start", "");

                // Fire before_llm extension
                ctx.context = state.to_context_value(&config);
                fire_before(self, ctx, "call_llm", "call:llm").await;

                // Call LLM
                let llm_result = call_llm(
                    &http_client,
                    &storage,
                    llm_resource,
                    &state.system_prompt,
                    &state.messages,
                    &all_tools,
                    config.config.max_tokens,
                    config.config.temperature,
                    Some(config.session_id.as_str()),
                ).await.map_err(|e| RuntimeError::Communication(format!("LLM call failed: {}", e)))?;

                state.iteration += 1;

                // Fire on_llm_response extension
                ctx.context = state.to_context_value(&config);
                ctx.context["_llm_result"] = llm_result.clone();
                fire_after(self, ctx, "call_llm", "call:llm").await;

                let tool_calls = llm_result.get("tool_calls")
                    .and_then(|v| v.as_array())
                    .filter(|arr| !arr.is_empty());

                if let Some(tool_calls) = tool_calls {
                    // Append assistant message WITH tool_calls (required by OpenAI API)
                    let raw_tool_calls = llm_result.get("raw_tool_calls")
                        .and_then(|v| v.as_array())
                        .cloned();
                    info!(has_raw_tool_calls = raw_tool_calls.is_some(), count = raw_tool_calls.as_ref().map(|v| v.len()).unwrap_or(0), "assistant tool_calls for message history");
                    let assistant_content = llm_result.get("content")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    state.messages.push(Message {
                        role: "assistant".to_string(),
                        content: assistant_content,
                        tool_call_id: None,
                        tool_calls: raw_tool_calls,
                    });

                    // Fire on_tool_call extension
                    fire_before(self, ctx, "dispatch_tools", "call:mcp").await;

                    for tc in tool_calls {
                        let tool_name = tc.get("name").and_then(|v| v.as_str()).unwrap_or("unknown");
                        let tool_input = tc.get("input").cloned().unwrap_or(json!({}));
                        let tool_id = tc.get("id").and_then(|v| v.as_str()).unwrap_or("tool");

                        info!(tool = %tool_name, "dispatching tool call");
                        // Faithful tool part for the UI stream: the SDK protocol
                        // needs the call id and its input, not just a name.
                        crate::token_stream::publish(&config.session_id, "tool_input",
                            json!({"toolCallId": tool_id, "toolName": tool_name, "input": tool_input}).to_string());

                        // Check if tool maps to a workflow
                        let tool_result = if let Some(flow_ref) = config.config.tool_flows.get(tool_name) {
                            // Execute as nested workflow
                            execute_tool_flow(&storage, &flow_ref.namespace, &flow_ref.name, tool_input.clone(), ctx).await
                        } else {
                            // Execute via MCP
                            let mcp_resource = config.config.mcp_servers.first()
                                .map(|s| s.as_str())
                                .unwrap_or("agent-flows");
                            // E2 #3: build the agent's template ceiling from its
                            // config — exposedTools = the tools it declares, enabledTools
                            // = the instance-enabled subset. Threaded to site A so the
                            // hook gates this call as the agent (§6.5). Merges over any
                            // flow-level metadata.a3t already on the context.
                            let ceiling_attrs = build_ceiling_attributes(&config.config, &ctx.template_meta);
                            call_mcp_tool(&http_client, &storage, &agent_principal, &config.session_id, mcp_resource, tool_name, &tool_input, ceiling_attrs).await
                        };

                        let tool_ok = tool_result.is_ok();
                        // Text for widgets: a tool returning a JSON *string* (get_flow
                        // hands back markdown) must be unwrapped, or the widget renders
                        // the escaped form — literal \n instead of newlines.
                        let tool_text = match &tool_result {
                            Ok(v) => v.as_str().map(|s| s.to_string()).unwrap_or_else(|| v.to_string()),
                            Err(_) => String::new(),
                        };
                        // Structured results (list_flows) need the value, not its text.
                        let tool_value: Option<Value> = tool_result.as_ref().ok().cloned();
                        let result_str = match tool_result {
                            Ok(v) => { info!(tool = %tool_name, result = %v.to_string().chars().take(200).collect::<String>(), "tool result"); v.to_string() },
                            Err(e) => {
                                warn!(tool = %tool_name, error = %e, "tool call failed");
                                json!({"error": e.to_string()}).to_string()
                            }
                        };

                        crate::token_stream::publish(&config.session_id, "tool_output",
                            json!({"toolCallId": tool_id, "output": result_str}).to_string());

                        // Generative UI: a tool that returns an editable artifact also
                        // emits a typed data part, so the client can render a widget
                        // instead of a wall of YAML. The channel is generic — `name`
                        // becomes the `data-<name>` frame — so new widget types need
                        // no change to the stream adapter.
                        if tool_ok {
                            // Table-driven so a new artifact type is one row, not
                            // another match arm. Lists carry their rows; details
                            // carry the text/JSON the pane will render.
                            const LIST_WIDGETS: &[(&str, &str, &str)] = &[
                                ("list_flows", "flows", "workflows"),
                                ("list_agents", "agents", "agents"),
                                ("list_agent_templates", "templates", "templates"),
                                ("list_resources", "resources", "resources"),
                                ("list_triggers", "triggers", "triggers"),
                                // Enumerating MCP servers yields plain "ns/name"
                                // strings; they are resources, so they open the
                                // same way.
                                ("list_mcp_tools", "resources", "mcp_servers"),
                                // Runtime pools are registry resources, so a row
                                // opens the same way any resource does.
                                ("list_runtimes", "runtimes", "runtimes"),
                            ];
                            const DETAIL_WIDGETS: &[(&str, &str)] = &[
                                ("get_flow", "flow"),
                                ("get_agent", "agent"),
                                ("get_agent_template", "template"),
                                ("get_resource", "resource"),
                                ("get_trigger", "trigger"),
                            ];

                            let widget = if let Some((_, kind, key)) =
                                LIST_WIDGETS.iter().find(|(t, _, _)| *t == tool_name)
                            {
                                let items: Vec<Value> = tool_value
                                    .as_ref()
                                    .and_then(|v| v.get(*key))
                                    .and_then(|w| w.as_array())
                                    .map(|arr| arr.iter().map(|raw_it| {
                                        // Some tools list bare "namespace/name"
                                        // strings rather than objects.
                                        let owned;
                                        let it: &Value = if let Some(txt) = raw_it.as_str() {
                                            let (ns, nm) = txt.split_once('/').unwrap_or(("default", txt));
                                            owned = json!({ "namespace": ns, "name": nm });
                                            &owned
                                        } else { raw_it };
                                        // Identity is what the row needs to open the
                                        // artifact; the rest rides along for display.
                                        let ns = it.get("namespace").and_then(|x| x.as_str())
                                            .unwrap_or_else(|| tool_input.get("namespace")
                                                .and_then(|x| x.as_str()).unwrap_or("default"));
                                        let name = it.get("name").and_then(|x| x.as_str())
                                            .or_else(|| it.get("id").and_then(|x| x.as_str()))
                                            .unwrap_or("");
                                        // `id` is carried separately: some get_*
                                        // tools key on a UUID (get_trigger) rather
                                        // than the display name.
                                        json!({
                                            "namespace": ns,
                                            "name": name,
                                            "id": it.get("id").and_then(|x| x.as_str()).unwrap_or(""),
                                            "meta": it,
                                        })
                                    }).collect())
                                    .unwrap_or_default();
                                if items.is_empty() { None } else {
                                    Some((*kind, json!({
                                        "namespace": tool_input.get("namespace")
                                            .and_then(|v| v.as_str()).unwrap_or(""),
                                        "items": items,
                                    })))
                                }
                            } else if tool_name == "propose_edit" {
                                // The designer applies the proposed document to its
                                // editor; nothing is stored until the user saves.
                                let ok = tool_value.as_ref()
                                    .and_then(|v| v.get("applied_to_editor"))
                                    .and_then(|v| v.as_bool())
                                    .unwrap_or(false);
                                if !ok { None } else {
                                    Some(("edit_draft", json!({
                                        "raw": tool_input.get("yaml").and_then(|v| v.as_str()).unwrap_or(""),
                                        "summary": tool_input.get("summary").and_then(|v| v.as_str()).unwrap_or(""),
                                    })))
                                }
                            } else if tool_name == "propose_flow" {
                                // A draft: the artifact is the YAML the model
                                // proposed, not the validator's response.
                                let ok = tool_value.as_ref()
                                    .and_then(|v| v.get("valid"))
                                    .and_then(|v| v.as_bool())
                                    .unwrap_or(false);
                                if !ok { None } else {
                                    Some(("flow_draft", json!({
                                        "namespace": tool_value.as_ref()
                                            .and_then(|v| v.get("namespace"))
                                            .and_then(|v| v.as_str()).unwrap_or("default"),
                                        "name": tool_value.as_ref()
                                            .and_then(|v| v.get("name"))
                                            .and_then(|v| v.as_str()).unwrap_or(""),
                                        "raw": tool_input.get("yaml")
                                            .and_then(|v| v.as_str()).unwrap_or(""),
                                    })))
                                }
                            } else if let Some((_, kind)) =
                                DETAIL_WIDGETS.iter().find(|(t, _)| *t == tool_name)
                            {
                                // get_flow returns markdown; the rest return JSON.
                                let raw = tool_value
                                    .as_ref()
                                    .filter(|v| !v.is_string())
                                    .map(|v| serde_json::to_string_pretty(v)
                                        .unwrap_or_else(|_| v.to_string()))
                                    .unwrap_or_else(|| tool_text.clone());
                                Some((*kind, json!({
                                    "namespace": tool_input.get("namespace")
                                        .and_then(|v| v.as_str()).unwrap_or("default"),
                                    "name": tool_input.get("name")
                                        .and_then(|v| v.as_str())
                                        .or_else(|| tool_input.get("id").and_then(|v| v.as_str()))
                                        .unwrap_or(""),
                                    "raw": raw,
                                })))
                            } else { None };

                            if let Some((kind, data)) = widget {
                                crate::token_stream::publish(&config.session_id, "data",
                                    json!({"name": kind, "data": data}).to_string());
                            }

                            // Deployment results render as cards the user DECIDES on,
                            // not as JSON in the execution log. Choosing a deployment
                            // shape is the user's call — "prod-ha" says nothing about
                            // isolation or scale, so the card shows what they weigh.
                            let deploy_card = match tool_name {
                                "list_canvas_presets" => tool_value.as_ref()
                                    .and_then(|v| v.get("presets")).cloned()
                                    .filter(|p| p.as_array().is_some_and(|a| !a.is_empty()))
                                    .map(|p| ("canvas-presets", json!({ "presets": p }))),
                                "get_canvas" => tool_value.as_ref()
                                    .and_then(|v| v.get("summary")).cloned()
                                    .map(|s| ("canvas-presets", json!({ "presets": [s] }))),
                                "clone_canvas" => tool_value.as_ref()
                                    .and_then(|v| v.get("summary")).cloned()
                                    .map(|s| ("canvas-presets", json!({ "presets": [s], "cloned": true }))),
                                "preview_placement" | "deploy_flow" => tool_value.as_ref()
                                    .filter(|v| v.get("placedRuntime").is_some())
                                    .cloned()
                                    .map(|v| ("canvas-placement", v)),
                                _ => None,
                            };
                            if let Some((kind, data)) = deploy_card {
                                crate::token_stream::publish(&config.session_id, "data",
                                    json!({"name": kind, "data": data}).to_string());
                            }

                            // The canvas itself as an ARTIFACT — the same "Open" widget a
                            // flow gets, so it can be edited in the side pane, pointed at a
                            // runtime and deployed from there. The raw payload is exactly
                            // what get_canvas returns, so the pane's stale-write check
                            // compares like with like.
                            let canvas_ref: Option<(String, String)> = match tool_name {
                                "get_canvas" | "update_canvas" => Some((
                                    tool_input.get("namespace").and_then(|v| v.as_str()).unwrap_or("a3t").to_string(),
                                    tool_input.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                                )),
                                "clone_canvas" | "bind_canvas" => tool_value.as_ref()
                                    .and_then(|v| v.get("canvas")).and_then(|v| v.as_str())
                                    .and_then(|c| c.split_once('/'))
                                    .map(|(a, b)| (a.to_string(), b.to_string())),
                                _ => None,
                            };
                            if let Some((cns, cname)) = canvas_ref.filter(|(_, n)| !n.is_empty()) {
                                if let Ok(raw) = crate::canvas::op_get_canvas(storage.as_ref(), &cns, &cname).await {
                                    crate::token_stream::publish(&config.session_id, "data",
                                        json!({"name": "canvas", "data": {
                                            "namespace": cns, "name": cname, "raw": raw,
                                        }}).to_string());
                                }
                            }
                        }

                        state.tool_calls_made += 1;
                        state.messages.push(Message {
                            role: "tool".to_string(),
                            content: result_str,
                            tool_call_id: Some(tool_id.to_string()),
                            tool_calls: None,
                        });
                    }

                    // Fire after dispatch_tools extension
                    ctx.context = state.to_context_value(&config);
                    fire_after(self, ctx, "dispatch_tools", "call:mcp").await;
                } else {
                    // No tool calls — LLM produced final response
                    let content = llm_result.get("content")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    state.agent_response = Some(content.clone());
                    state.should_continue = false;

                    // Append final assistant message
                    state.messages.push(Message {
                        role: "assistant".to_string(),
                        content,
                        tool_call_id: None,
                        tool_calls: None,
                    });
                }
            }
        }

        // ── Phase 7b: GUARANTEE A FINAL ANSWER ─────────────
        // The loop above can exhaust `max_iterations` while the model is still
        // issuing tool calls, which left `agent_response` unset and emitted an
        // empty response — the chat rendered a blank bubble with no error. Ask
        // once more with tools withheld so the model must answer from what it
        // already gathered; if even that yields nothing, say so explicitly.
        let mut stopped_reason: Option<String> = None;
        if state.agent_response.is_none() {
            stopped_reason = Some("max_iterations".to_string());
            warn!(
                iterations = state.iteration,
                tool_calls = state.tool_calls_made,
                "agent hit max_iterations without a final answer — forcing a tool-free response"
            );
            state.messages.push(Message {
                role: "user".to_string(),
                content: format!(
                    "You have reached the {} step limit and can make no further tool calls. \
                     Answer now using only what you have already gathered. If the task is \
                     incomplete, say what you found and what remains.",
                    config.config.max_iterations
                ),
                tool_call_id: None,
                tool_calls: None,
            });
            // Same resolution as the reasoning loop above (that binding is scoped
            // to the loop's block, which has already closed here).
            let llm_resource = config.config.llm_resource.as_deref().unwrap_or("default-llm");
            match call_llm(
                &http_client,
                &storage,
                llm_resource,
                &state.system_prompt,
                &state.messages,
                &[], // no tools — forces prose
                config.config.max_tokens,
                config.config.temperature,
                Some(config.session_id.as_str()),
            ).await {
                Ok(res) => {
                    let content = res.get("content").and_then(|v| v.as_str()).unwrap_or("").trim().to_string();
                    if !content.is_empty() {
                        state.messages.push(Message {
                            role: "assistant".to_string(),
                            content: content.clone(),
                            tool_call_id: None,
                            tool_calls: None,
                        });
                        state.agent_response = Some(content);
                    }
                }
                Err(e) => warn!(error = %e, "forced final LLM call failed"),
            }
            // Last resort: an explicit message beats a blank bubble.
            if state.agent_response.as_deref().map_or(true, |c| c.trim().is_empty()) {
                state.agent_response = Some(format!(
                    "I stopped after {} steps and {} tool calls without reaching a final answer. \
                     Please narrow the request or ask me to continue.",
                    state.iteration, state.tool_calls_made
                ));
            }
        }

        // ── Phase 8: STORE MEMORY ──────────────────────────
        ctx.context = state.to_context_value(&config);
        fire_before(self, ctx, "store_memory", "call:clawdb").await;
        if config.config.memory_resource.is_some() {
            debug!("memory store: skipped (ClawDB integration pending in call:a3t:agent)");
        }
        fire_after(self, ctx, "store_memory", "call:clawdb").await;

        // ── Phase 9: SAVE SESSION ──────────────────────────
        fire_before(self, ctx, "save_session", "call:state:set").await;
        // `owner` is what makes a transcript belong to somebody. Without it a
        // session row was reachable only by whoever still remembered its random
        // id, so signing in on another device showed an empty sidebar even though
        // the history was sitting in LanceDB.
        let owner = ctx.principal.id().to_string();
        let session_value = json!({
            "messages": state.messages,
            "last_response": state.agent_response,
            "iteration_count": state.iteration,
            "tool_calls_made": state.tool_calls_made,
            "owner": owner,
            "owner_kind": ctx.principal.kind(),
            "updated_at": chrono::Utc::now().to_rfc3339(),
        });
        // No TTL: a conversation is kept until it is explicitly deleted. The
        // former 1800s expiry silently discarded transcripts after 30 minutes,
        // which is not what a chat history is for. `config.ttl` still bounds the
        // agent's own working state elsewhere.
        storage.state().set(&session_key, session_value, None).await
            .map_err(|e| RuntimeError::Communication(format!("Failed to save session: {}", e)))?;
        // Maintain the owner's index so the sessions can be *listed* without a
        // key scan: the transcript rows are the source of truth, this is only a
        // pointer list.
        if let Err(e) = upsert_session_index(
            storage.as_ref(), &owner, &config.session_id, &state.messages,
        ).await {
            // A lost index entry costs one sidebar row, never the transcript, so
            // it must not fail the turn.
            warn!(error = %e, owner = %owner, "failed to update chat session index");
        }
        fire_after(self, ctx, "save_session", "call:state:set").await;

        // ── Phase 10: EMIT RESPONSE ────────────────────────
        let response = json!({
            "content": state.agent_response.clone().unwrap_or_default(),
            "session_id": config.session_id,
            "tool_calls_made": state.tool_calls_made,
            "iterations": state.iteration,
            "procedural_hit": state.procedural_hit,
            "stopped_reason": stopped_reason,
        });

        // Terminal marker for any attached stream: lets the client finalize the
        // message (and reconcile if it missed deltas) instead of waiting.
        crate::token_stream::publish(&config.session_id, "done",
            state.agent_response.clone().unwrap_or_default());

        fire_before(self, ctx, "emit_response", "set").await;
        ctx.context = state.to_context_value(&config);
        fire_after(self, ctx, "emit_response", "set").await;

        info!(task = %task.name, iterations = state.iteration, tools = state.tool_calls_made, "call:a3t:agent completed");

        Ok(TaskResult::new(response))
    }
}

// ═══════════════════════════════════════════════════════════════════
// Extension helpers
// ═══════════════════════════════════════════════════════════════════

/// Fire matching before-extensions for a named phase
async fn fire_before(executor: &AgentHandler, ctx: &mut ExecutionContext, phase_name: &str, task_type: &str) {
    if ctx.extensions.is_empty() { return; }

    let eval_fn = ctx.eval_fn.clone();
    let matching: Vec<_> = ctx.extensions.iter()
        .filter(|e| e.matches(task_type))
        .cloned()
        .collect();

    for ext in &matching {
        if let Some(ref when_expr) = ext.inner.when {
            let result = if let Some(ref ef) = eval_fn {
                ef(&ctx.context, when_expr, &ctx.context).unwrap_or(Value::Bool(false))
            } else {
                Value::Bool(false)
            };
            if result != Value::Bool(true) { continue; }
        }

        debug!(extension = %ext.inner.name, phase = %phase_name, "agent before-extension");
        for before_task in &ext.inner.before {
            // Extension before-tasks are typically Set tasks
            if let Task::Set(set_task) = &before_task.task {
                // Evaluate set values and merge into context
                for (key, val) in &set_task.values {
                    if let Some(ref ef) = eval_fn {
                        if let Ok(evaluated) = ef(&ctx.context, &val.to_string(), &ctx.context) {
                            if let Some(ctx_obj) = ctx.context.as_object_mut() {
                                ctx_obj.insert(key.clone(), evaluated);
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Fire matching after-extensions for a named phase
async fn fire_after(executor: &AgentHandler, ctx: &mut ExecutionContext, phase_name: &str, task_type: &str) {
    if ctx.extensions.is_empty() { return; }

    let eval_fn = ctx.eval_fn.clone();
    let matching: Vec<_> = ctx.extensions.iter()
        .filter(|e| e.matches(task_type))
        .cloned()
        .collect();

    for ext in &matching {
        if let Some(ref when_expr) = ext.inner.when {
            let result = if let Some(ref ef) = eval_fn {
                ef(&ctx.context, when_expr, &ctx.context).unwrap_or(Value::Bool(false))
            } else {
                Value::Bool(false)
            };
            if result != Value::Bool(true) { continue; }
        }

        debug!(extension = %ext.inner.name, phase = %phase_name, "agent after-extension");
        for after_task in &ext.inner.after {
            if let Task::Set(set_task) = &after_task.task {
                for (key, val) in &set_task.values {
                    if let Some(ref ef) = eval_fn {
                        if let Ok(evaluated) = ef(&ctx.context, &val.to_string(), &ctx.context) {
                            if let Some(ctx_obj) = ctx.context.as_object_mut() {
                                ctx_obj.insert(key.clone(), evaluated);
                            }
                        }
                    }
                }
            }
        }
    }
}

// ═══════════════════════════════════════════════════════════════════
// Infrastructure helpers
// ═══════════════════════════════════════════════════════════════════

fn get_storage(ctx: &ExecutionContext) -> Result<Arc<dyn StorageBackend>> {
    ctx.storage.as_ref()
        .and_then(|s| s.downcast_ref::<Arc<dyn StorageBackend>>().cloned())
        .ok_or_else(|| RuntimeError::Internal("Storage not available".to_string()))
}

/// Resolve a secret resource reference (namespace/name) to its value
async fn resolve_secret_ref(storage: &Arc<dyn StorageBackend>, secret_ref: &str) -> Option<String> {
    let (ns, name) = if secret_ref.contains('/') {
        let parts: Vec<&str> = secret_ref.splitn(2, '/').collect();
        (parts[0], parts[1])
    } else {
        ("default", secret_ref)
    };
    let secret = storage.resources().get(ns, name).await.ok()?;
    secret.config.get("value")
        .or_else(|| secret.config.get("token"))
        .or_else(|| secret.config.get("api_key"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
}

/// Call LLM via OpenAI-compatible API
async fn call_llm(
    client: &reqwest::Client,
    storage: &Arc<dyn StorageBackend>,
    resource_name: &str,
    system_prompt: &str,
    messages: &[Message],
    tools: &[Value],
    max_tokens: u32,
    temperature: f64,
    // When set, request `stream: true` and publish each text delta to the token
    // hub under this session id so the browser renders tokens as they arrive.
    // The returned value is identical either way, so callers are unaffected.
    stream_session: Option<&str>,
) -> std::result::Result<Value, String> {
    // Resolve resource
    let (ns, name) = if resource_name.contains('/') {
        let parts: Vec<&str> = resource_name.splitn(2, '/').collect();
        (parts[0], parts[1])
    } else {
        ("default", resource_name)
    };

    let resource = storage.resources().get(ns, name).await
        .map_err(|e| format!("LLM resource '{}' not found: {}", resource_name, e))?;

    let api_url = resource.config.get("api_url")
        .or_else(|| resource.config.get("base_url"))
        .and_then(|v| v.as_str())
        .unwrap_or("https://api.openai.com/v1");

    // Resolve API key: prefer api_key_ref (secret resource reference) over embedded api_key
    let resolved_key = if let Some(secret_ref) = resource.config.get("api_key_ref").and_then(|v| v.as_str()) {
        resolve_secret_ref(storage, secret_ref).await.unwrap_or_default()
    } else {
        resource.config.get("api_key").and_then(|v| v.as_str()).unwrap_or("").to_string()
    };
    let api_key = resolved_key.as_str();

    let model = resource.config.get("model")
        .and_then(|v| v.as_str())
        .unwrap_or("gpt-4o-mini");

    // Build messages
    let mut api_messages = vec![json!({"role": "system", "content": system_prompt})];
    for msg in messages {
        let mut m = json!({"role": msg.role, "content": msg.content});
        if let Some(ref tid) = msg.tool_call_id {
            m["tool_call_id"] = json!(tid);
        }
        if let Some(ref tcs) = msg.tool_calls {
            m["tool_calls"] = json!(tcs);
        }
        api_messages.push(m);
    }

    // Build request
    let mut body = json!({
        "model": model,
        "messages": api_messages,
        "max_tokens": max_tokens,
        "temperature": temperature,
    });
    if !tools.is_empty() {
        let openai_tools: Vec<Value> = tools.iter().map(|t| {
            json!({
                "type": "function",
                "function": {
                    "name": t.get("name").and_then(|v| v.as_str()).unwrap_or(""),
                    "description": t.get("description").and_then(|v| v.as_str()).unwrap_or(""),
                    "parameters": t.get("input_schema").or_else(|| t.get("parameters")).cloned().unwrap_or(json!({})),
                }
            })
        }).collect();
        body["tools"] = json!(openai_tools);
    }

    let url = format!("{}/chat/completions", api_url.trim_end_matches('/'));

    if let Some(session) = stream_session {
        body["stream"] = json!(true);
        let resp = client.post(&url)
            .header("Authorization", format!("Bearer {}", api_key))
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("HTTP error: {}", e))?;
        if !resp.status().is_success() {
            let status = resp.status();
            let detail = resp.text().await.unwrap_or_default();
            return Err(format!("OpenAI API error ({}): {}", status, detail));
        }
        return read_stream(resp, session).await;
    }

    let resp = client.post(&url)
        .header("Authorization", format!("Bearer {}", api_key))
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("HTTP error: {}", e))?;

    let resp_text = resp.text().await
        .map_err(|e| format!("Response read error: {}", e))?;
    debug!(response = %resp_text, "LLM raw response");
    let resp_json: Value = serde_json::from_str(&resp_text)
        .map_err(|e| format!("JSON parse error: {}", e))?;

    if let Some(err) = resp_json.get("error") {
        return Err(format!("OpenAI API error: {}", err));
    }

    // Extract response
    let choice = &resp_json["choices"][0]["message"];
    let content = choice.get("content").and_then(|v| v.as_str()).unwrap_or("").to_string();

    let tool_calls: Option<Vec<Value>> = choice.get("tool_calls")
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().map(|tc| {
            json!({
                "id": tc.get("id").and_then(|v| v.as_str()).unwrap_or(""),
                "name": tc["function"]["name"],
                "input": serde_json::from_str::<Value>(
                    tc["function"]["arguments"].as_str().unwrap_or("{}")
                ).unwrap_or(json!({})),
            })
        }).collect());

    // Preserve raw tool_calls for assistant message (OpenAI format)
    let raw_tool_calls = choice.get("tool_calls").cloned();

    Ok(json!({
        "content": content,
        "tool_calls": tool_calls,
        "raw_tool_calls": raw_tool_calls,
        "stop_reason": resp_json["choices"][0].get("finish_reason"),
    }))
}


/// Reassembles an OpenAI-compatible streamed completion.
///
/// With `stream: true` *everything* arrives fragmented, tool calls included: a
/// call's `id`/`name` may land in one frame and its `arguments` across many
/// more, keyed only by `index`. Reassembly therefore has to be stateful, and it
/// must reproduce the exact OpenAI wire shape in `raw_tool_calls` — the message
/// history replays that verbatim, so an error here would silently break tool
/// calling on the next turn. Kept separate from the HTTP layer so it can be
/// tested against real frame sequences.
#[derive(Default)]
struct StreamAccumulator {
    content: String,
    calls: std::collections::BTreeMap<u64, PartialCall>,
    finish_reason: Option<String>,
    done: bool,
}

#[derive(Default)]
struct PartialCall {
    id: String,
    name: String,
    arguments: String,
    announced: bool,
}

impl StreamAccumulator {
    /// Feed one raw SSE line. Returns chunks to publish (text deltas, and a
    /// `tool` notice the first time a call becomes identifiable).
    fn push_line(&mut self, line: &str) -> Vec<(&'static str, String)> {
        let mut out = Vec::new();
        let Some(payload) = line.trim_end_matches(['\n', '\r']).strip_prefix("data:") else {
            return out;
        };
        let payload = payload.trim();
        if payload.is_empty() { return out; }
        if payload == "[DONE]" { self.done = true; return out; }
        let Ok(ev) = serde_json::from_str::<Value>(payload) else { return out };
        let choice = &ev["choices"][0];
        if let Some(fr) = choice.get("finish_reason").and_then(|v| v.as_str()) {
            self.finish_reason = Some(fr.to_string());
        }
        let delta = &choice["delta"];
        if let Some(text) = delta.get("content").and_then(|v| v.as_str()) {
            if !text.is_empty() {
                self.content.push_str(text);
                out.push(("token", text.to_string()));
            }
        }
        if let Some(tcs) = delta.get("tool_calls").and_then(|v| v.as_array()) {
            for tc in tcs {
                let idx = tc.get("index").and_then(|v| v.as_u64()).unwrap_or(0);
                let e = self.calls.entry(idx).or_default();
                if let Some(id) = tc.get("id").and_then(|v| v.as_str()) {
                    if !id.is_empty() { e.id = id.to_string(); }
                }
                if let Some(n) = tc["function"].get("name").and_then(|v| v.as_str()) {
                    if !n.is_empty() { e.name.push_str(n); }
                }
                if let Some(a) = tc["function"].get("arguments").and_then(|v| v.as_str()) {
                    e.arguments.push_str(a);
                }
                if !e.announced && !e.name.is_empty() {
                    e.announced = true;
                    out.push(("tool", e.name.clone()));
                }
            }
        }
        out
    }

    /// Build the same value shape the buffered (non-streaming) path returns.
    fn finish(self) -> Value {
        let raw_tool_calls: Option<Vec<Value>> = if self.calls.is_empty() {
            None
        } else {
            Some(self.calls.values().map(|c| json!({
                "id": c.id,
                "type": "function",
                "function": { "name": c.name, "arguments": c.arguments },
            })).collect())
        };
        let tool_calls: Option<Vec<Value>> = raw_tool_calls.as_ref().map(|arr| arr.iter().map(|tc| json!({
            "id": tc.get("id").and_then(|v| v.as_str()).unwrap_or(""),
            "name": tc["function"]["name"],
            "input": serde_json::from_str::<Value>(
                tc["function"]["arguments"].as_str().unwrap_or("{}")
            ).unwrap_or(json!({})),
        })).collect());
        json!({
            "content": self.content,
            "tool_calls": tool_calls,
            "raw_tool_calls": raw_tool_calls,
            "stop_reason": self.finish_reason,
        })
    }
}

/// Consume a streamed completion, publishing deltas live to the token hub.
async fn read_stream(
    resp: reqwest::Response,
    session: &str,
) -> std::result::Result<Value, String> {
    use futures::StreamExt;

    let mut acc = StreamAccumulator::default();
    let mut buf = String::new();
    let mut body = resp.bytes_stream();

    while let Some(chunk) = body.next().await {
        let bytes = chunk.map_err(|e| format!("stream read error: {}", e))?;
        buf.push_str(&String::from_utf8_lossy(&bytes));
        // Frames are newline-delimited, but a network chunk may split one anywhere.
        while let Some(nl) = buf.find('\n') {
            let line: String = buf.drain(..=nl).collect();
            for (kind, data) in acc.push_line(&line) {
                crate::token_stream::publish(session, kind, data);
            }
        }
        if acc.done { break; }
    }
    // A final frame with no trailing newline still has to be processed.
    if !buf.is_empty() {
        for (kind, data) in acc.push_line(&buf.clone()) {
            crate::token_stream::publish(session, kind, data);
        }
    }
    Ok(acc.finish())
}

/// True when an MCP resource URL points back at this same server (loopback),
/// so the tool call can be dispatched in-process instead of over HTTP.
/// Resolve an agent's MCP server reference to the URL its JSON-RPC requests are POSTed to.
///
/// `ns/name` names a resource directly. A bare `name` (how the A3T UI saves an agent's
/// servers) is looked up in `default`, then in the A3T platform namespace `a3t`. A
/// missing resource is an error: this used to fall back silently to this FlowR's own
/// `/message`, so an agent configured with an external server quietly got FlowR's tools
/// instead. A resource without a URL still means this FlowR (as `list_mcp_tools` treats
/// it). `…/sse` URLs map to their `…/message` endpoint, as in `call:mcp`.
async fn resolve_agent_mcp_url(
    storage: &Arc<dyn StorageBackend>,
    resource_name: &str,
) -> std::result::Result<String, String> {
    let candidates: Vec<(&str, &str)> = match resource_name.split_once('/') {
        Some((ns, name)) => vec![(ns, name)],
        None => vec![("default", resource_name), ("a3t", resource_name)],
    };
    for (ns, name) in &candidates {
        if let Ok(r) = storage.resources().get(ns, name).await {
            let url = r.config.get("url").or_else(|| r.config.get("endpoint")).and_then(|v| v.as_str());
            return Ok(match url {
                Some(u) if !u.is_empty() => flowr_mcp_core::mcp_message_url(u),
                _ => "http://localhost:3002/message".to_string(),
            });
        }
    }
    let tried: Vec<String> = candidates.iter().map(|(ns, n)| format!("{ns}/{n}")).collect();
    Err(format!("MCP server resource not found: {}", tried.join(" or ")))
}

fn is_local_mcp_url(url: &str) -> bool {
    url.contains("localhost") || url.contains("127.0.0.1") || url.contains("0.0.0.0")
}

/// Convert a `CallToolResult` (in-process handler output) into the `Value` the
/// agent loop expects, mirroring the HTTP JSON-RPC extraction.
fn callresult_to_value(result: flowr_mcp_core::CallToolResult) -> std::result::Result<Value, String> {
    let text: String = result.content.iter().map(|c| match c {
        flowr_mcp_core::ToolContent::Text { text } => text.clone(),
    }).collect();
    if result.is_error == Some(true) {
        return Err(text);
    }
    Ok(serde_json::from_str(&text).unwrap_or(json!(text)))
}

/// Call an MCP tool. Local-server tools (loopback) are dispatched **in-process**
/// through the gated choke point (site A) as the agent principal — no HTTP, no
/// re-auth (E0.4c). Remote MCP servers still use JSON-RPC over HTTP.
/// E2 #3: assemble the policy `attributes` carrying the agent's template ceiling.
/// `exposedTools` = the tool names the agent declares in its config; `enabledTools`
/// = the instance-enabled subset (omitted when empty → no narrowing). Any
/// `template.*` already present from the flow's `metadata.a3t` is preserved and
/// the config-derived sets layer over it.
fn build_ceiling_attributes(
    settings: &AgentSettings,
    flow_meta: &serde_json::Map<String, Value>,
) -> serde_json::Map<String, Value> {
    let mut template = flow_meta
        .get("template")
        .and_then(|v| v.as_object())
        .cloned()
        .unwrap_or_default();

    let exposed: Vec<Value> = settings
        .tools
        .iter()
        .filter_map(|t| t.get("name").and_then(|v| v.as_str()))
        .map(|s| Value::String(s.to_string()))
        .collect();
    if !exposed.is_empty() {
        template.insert("exposedTools".to_string(), Value::Array(exposed));
    }
    // Present-but-empty must still narrow — see AgentSettings::enabled_tools.
    if let Some(enabled_list) = &settings.enabled_tools {
        let enabled: Vec<Value> = enabled_list
            .iter()
            .map(|s| Value::String(s.clone()))
            .collect();
        template.insert("enabledTools".to_string(), Value::Array(enabled));
    }

    let mut attrs = flow_meta.clone();
    attrs.insert("template".to_string(), Value::Object(template));
    attrs
}

#[allow(clippy::too_many_arguments)]
async fn call_mcp_tool(
    client: &reqwest::Client,
    storage: &Arc<dyn StorageBackend>,
    caller: &flowr_mcp_core::Principal,
    session_id: &str,
    resource_name: &str,
    tool_name: &str,
    arguments: &Value,
    attributes: serde_json::Map<String, Value>,
) -> std::result::Result<Value, String> {
    // ── Permission introspection ──
    //
    // Handled before the gate, and READ-ONLY on purpose. The user must always be
    // able to find out what is blocked — gating the question behind the same
    // policy that produced the block would be a dead end.
    //
    // There is deliberately no matching "lift" tool. If the agent could reopen
    // its own refusals it could defeat a sticky denial by lifting and re-asking,
    // turning "no" into prompt spam until the user gave in. Lifting is a user
    // action, from the permissions panel (see the /permissions endpoints).
    if tool_name == "list_permissions" {
        let only = arguments.get("only").and_then(|v| v.as_str()).unwrap_or("all");
        let all = flowr_policy::grants::list(session_id);
        let filtered: Vec<_> = all
            .into_iter()
            .filter(|d| match only {
                "denied" => d.verdict == flowr_policy::grants::Verdict::Denied,
                "granted" => d.verdict == flowr_policy::grants::Verdict::Granted,
                _ => true,
            })
            .collect();
        return Ok(json!({
            "permissions": filtered,
            "count": filtered.len(),
            "note": "Only the user can change these, from the permissions panel in the chat.",
        }));
    }

    // ── Deployment needs its own decision, on the target ──
    //
    // `mcp:call` on Tool::deploy_flow says whether the agent may USE the tool.
    // It does not say whether this placement is allowed — that is `flow:deploy`
    // on the Runtime, which is where "prod needs Admin" lives. Both must pass.
    //
    // It happens HERE, not inside the tool, because this is the layer that knows
    // the session: an owner deploying to prod gets the approval card rather than
    // a flat refusal, which is the whole point of deploying from a conversation.
    if tool_name == "deploy_flow" {
        let flow_ns = arguments.get("namespace").and_then(|v| v.as_str()).unwrap_or("a3t");
        let flow_name = arguments.get("name").and_then(|v| v.as_str()).unwrap_or_default();
        // Authorize against the runtime the user CHOSE, if they chose one — the
        // decision must be about the placement that will actually happen.
        let chosen = arguments.get("runtime").and_then(|v| v.as_str());
        if let Some((req, rt, eff_ns, mode)) = crate::canvas::deploy_request(
            storage.as_ref(), caller, flow_ns, flow_name, chosen, arguments,
        ).await
        {
            let decision = flowr_policy::policy_hook().check(&req).await;
            flowr_policy::audit::record(&req, &decision);

            // Answer already given in this conversation, if any — same ledger the
            // tool gate uses, keyed on the placement rather than the tool, so
            // approving a deploy to prod does not silently cover a different pool.
            let deploy_id = format!("{}/{}", rt.name, mode);
            let decision = flowr_mcp_core::apply_session_decisions(
                session_id, decision, flowr_policy::actions::FLOW_DEPLOY, "Runtime", &deploy_id,
            );
            let decision = match decision {
                flowr_policy::PolicyDecision::Ask { reason, .. } => {
                    // A deploy approval is deliberately ONE-SHOT: approving this
                    // placement must not silently authorize the next one.
                    crate::approval::request_approval(
                        session_id, caller, Some(&rt.name), "deploy_flow", arguments, &reason,
                        &[flowr_policy::GrantScope::Once],
                    )
                    .await
                }
                settled => settled,
            };
            if let flowr_policy::PolicyDecision::Deny { reason, policy_ids } = decision {
                return Ok(json!({
                    "error": format!("deployment denied: {reason}"),
                    "denied": true,
                    "runtime": rt.name,
                    "namespace": eff_ns,
                    "mode": mode,
                    "policies": policy_ids,
                }));
            }
        }
    }

    // ── Operating deployments and runtimes ──
    //
    // Same shape as deploy: decided on the Runtime, answered from the session
    // ledger if the user already answered, otherwise put to them as a ONE-SHOT
    // approval — approving one pause must not authorize the next.
    if let Some((req, action, decision_id)) = crate::ops::ops_request(caller, tool_name, arguments) {
        let decision = flowr_policy::policy_hook().check(&req).await;
        flowr_policy::audit::record(&req, &decision);
        let decision = flowr_mcp_core::apply_session_decisions(
            session_id, decision, action, "Runtime", &decision_id,
        );
        let decision = match decision {
            flowr_policy::PolicyDecision::Ask { reason, .. } => {
                crate::approval::request_approval(
                    session_id, caller, Some(&req.resource.id), tool_name, arguments, &reason,
                    &[flowr_policy::GrantScope::Once],
                )
                .await
            }
            settled => settled,
        };
        if let flowr_policy::PolicyDecision::Deny { reason, policy_ids } = decision {
            return Ok(json!({
                "error": format!("{tool_name} was denied: {reason}"),
                "denied": true,
                "policies": policy_ids,
            }));
        }
    }

    // ── Registering a runtime is registry governance ──
    //
    // `resource:create` on a `runtime`-typed resource, the same rule that governs
    // every registry write (baseline: Admin). Checked here, where the session is
    // known, so an Admin's agent proposing a new pool raises the approval card
    // instead of failing — the registry permit is scoped to `principal is User`.
    if tool_name == "create_runtime" {
        let rt_name = arguments.get("name").and_then(|v| v.as_str()).unwrap_or_default();
        let req = crate::canvas::runtime_create_request(caller, rt_name, arguments);
        let decision = flowr_policy::policy_hook().check(&req).await;
        flowr_policy::audit::record(&req, &decision);
        let decision = flowr_mcp_core::apply_session_decisions(
            session_id, decision, flowr_policy::actions::RESOURCE_CREATE, "Resource",
            &format!("runtime/{rt_name}"),
        );
        let decision = match decision {
            flowr_policy::PolicyDecision::Ask { reason, .. } => {
                // One-shot, like deploy: approving one new pool must not
                // authorize the next.
                crate::approval::request_approval(
                    session_id, caller, None, "create_runtime", arguments, &reason,
                    &[flowr_policy::GrantScope::Once],
                )
                .await
            }
            settled => settled,
        };
        if let flowr_policy::PolicyDecision::Deny { reason, policy_ids } = decision {
            return Ok(json!({
                "error": format!("creating a runtime was denied: {reason}"),
                "denied": true,
                "policies": policy_ids,
            }));
        }
    }

    // ── The policy gate, for BOTH routes ──
    //
    // This used to sit inside the local-dispatch branch, so a tool served by a
    // REMOTE MCP server reached the network without ever passing the hook —
    // authorization depended on where a tool happened to be hosted. Hoisting it
    // here makes the check unconditional; routing can no longer mean skipping.
    //
    // The server is part of the identity of the capability: two servers can both
    // expose `query` and they are not the same thing, so the decision is recorded
    // against `server/tool` (see `mcp_decision_id`).
    let decision = flowr_mcp_core::authorize_mcp_tool_call(
        caller, Some(resource_name), tool_name, arguments, attributes.clone(),
    ).await;
    let decision_id = flowr_mcp_core::mcp_decision_id(Some(resource_name), tool_name);
    // What the user already answered in this conversation, if anything: a prior
    // grant allows without re-asking, a prior refusal denies without re-asking.
    let decision = flowr_mcp_core::apply_session_decisions(
        session_id, decision, flowr_policy::actions::MCP_CALL, "Tool", &decision_id,
    );
    let decision = match decision {
        flowr_policy::PolicyDecision::Ask { reason, scopes, .. } => {
            // Stop the turn and put it to the human. Blocks until they answer or
            // it times out.
            crate::approval::request_approval(
                session_id, caller, Some(resource_name), tool_name, arguments, &reason, &scopes,
            ).await
        }
        settled => settled,
    };
    if let flowr_policy::PolicyDecision::Deny { reason, policy_ids } = decision {
        return Ok(json!({
            "error": format!("denied: {tool_name} — {reason}"),
            "denied": true,
            "policies": policy_ids,
        }));
    }

    // Resolve MCP server URL from resource
    let server_url = resolve_agent_mcp_url(storage, resource_name).await?;

    // E0.4c: local tools → in-process, authorized as the agent principal.
    // E2 #3: pass the template ceiling attributes so the hook can gate here.
    if is_local_mcp_url(&server_url) {
        // The agent-runtime tools (list_agents, list_agent_templates, …) are
        // implemented in this crate, not in flowr-mcp-core. Dispatching straight
        // to the shared handler landed on its "requires the agent runtime" arm,
        // so the chat agent could not see agents or templates even though the
        // identical tools worked over /message, which tries this layer first.
        //
        // The policy gate is applied HERE, with the same attributes (the flow's
        // capability ceiling), so routing a tool elsewhere never means skipping
        // authorization.
        if flowr_mcp_core::is_agent_runtime_tool(tool_name) {
            // Already authorized at the top of this function — one gate for both
            // the local and remote routes.
            if let Some(result) = crate::handle_agent_tool_call(
                storage, caller, tool_name, &Some(arguments.clone()), true,
            ).await {
                return callresult_to_value(result);
            }
        }
        // Already authorized at the top of this function, including resolving any
        // interactive approval. Gating again here would re-ask the policy without
        // the session context, see `Ask`, and fail closed — refusing a call the
        // user had just granted.
        let result = flowr_mcp_core::dispatch_tool_call(
            storage.as_ref(), caller, tool_name, Some(arguments.clone()),
        ).await;
        return callresult_to_value(result);
    }

    let payload = json!({
        "jsonrpc": "2.0",
        "method": "tools/call",
        "id": 1,
        "params": {
            "name": tool_name,
            "arguments": arguments,
        }
    });

    let resp = client.post(&server_url)
        .header("Content-Type", "application/json")
        .json(&payload)
        .send()
        .await
        .map_err(|e| format!("MCP HTTP error: {}", e))?;

    let resp_json: Value = resp.json().await
        .map_err(|e| format!("MCP JSON error: {}", e))?;

    // Extract result from JSON-RPC response
    if let Some(result) = resp_json.get("result") {
        if let Some(content) = result.get("content").and_then(|v| v.as_array()) {
            if let Some(first) = content.first() {
                if let Some(text) = first.get("text").and_then(|v| v.as_str()) {
                    return serde_json::from_str(text).unwrap_or(Ok(json!(text)));
                }
            }
        }
        Ok(result.clone())
    } else if let Some(err) = resp_json.get("error") {
        Err(format!("MCP error: {}", err))
    } else {
        Ok(json!(null))
    }
}

/// List tools exposed by an MCP server (tools/list), so they can be advertised to the LLM.
/// Local discovery is served in-process; remote servers use HTTP (E0.4c).
async fn list_mcp_tools(
    client: &reqwest::Client,
    storage: &Arc<dyn StorageBackend>,
    _caller: &flowr_mcp_core::Principal,
    resource_name: &str,
) -> std::result::Result<Vec<Value>, String> {
    let server_url = resolve_agent_mcp_url(storage, resource_name).await?;

    // E0.4c: local tool discovery is in-process (no loopback HTTP / re-auth).
    if is_local_mcp_url(&server_url) {
        return Ok(flowr_mcp_core::get_tools()
            .into_iter()
            .map(|t| serde_json::to_value(t).unwrap_or(json!({})))
            .collect());
    }

    let payload = json!({"jsonrpc": "2.0", "method": "tools/list", "id": 1});

    let resp = client.post(&server_url)
        .header("Content-Type", "application/json")
        .json(&payload)
        .send()
        .await
        .map_err(|e| format!("MCP tools/list HTTP error: {}", e))?;

    let resp_json: Value = resp.json().await
        .map_err(|e| format!("MCP tools/list JSON error: {}", e))?;

    let tools = resp_json.get("result")
        .and_then(|r| r.get("tools"))
        .and_then(|t| t.as_array())
        .cloned()
        .unwrap_or_default();
    Ok(tools)
}

/// Execute a tool as a nested workflow
async fn execute_tool_flow(
    storage: &Arc<dyn StorageBackend>,
    namespace: &str,
    name: &str,
    input: Value,
    ctx: &mut ExecutionContext,
) -> std::result::Result<Value, String> {
    let record = storage.workflows().get(namespace, name).await
        .map_err(|e| format!("Tool workflow {}/{} not found: {}", namespace, name, e))?;

    let swf_workflow = record.parse()
        .map_err(|e| format!("Failed to parse tool workflow: {}", e))?;

    let mut child_executor = swf_runtime::WorkflowExecutor::new();
    swf_runtime::handlers::register_builtin_handlers(&mut child_executor);
    crate::handlers::register_clawdflow_handlers(&mut child_executor);

    let expr_engine = flowr_core::ExpressionEngine::new();
    child_executor.set_eval_fn(move |input, expr, context| {
        use flowr_core::Scope;
        let scope = Scope::builder().context(context.clone()).build();
        expr_engine.eval(input, expr, &scope).map_err(|e| e.to_string())
    });

    let mut child_ctx = ExecutionContext {
        instance_id: format!("{}/tool-{}-{}", ctx.instance_id, namespace, name),
        namespace: namespace.to_string(),
        name: name.to_string(),
        version: record.version().to_string(),
        context: Value::Null,
        secrets: ctx.secrets.clone(),
        runtime: ctx.runtime.clone(),
        storage: ctx.storage.clone(),
        http_client: ctx.http_client.clone(),
        extensions: Vec::new(),
        eval_fn: ctx.eval_fn.clone(),
        // E0.4: a spawned agent tool runs as the flow's principal (inherited).
        principal: ctx.principal.clone(),
        // E2 #4: inherit the parent's capability template (attenuate-only compose).
        template_meta: ctx.template_meta.clone(),
    };

    child_executor.execute(&swf_workflow, input, &mut child_ctx).await
        .map_err(|e| format!("Tool workflow failed: {}", e))
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── The capability ceiling: absent is not the same as empty ──
    //
    // Reported bug: removing `list_flows` from an agent left it still able to
    // list flows. The empty tool set read as "no restriction configured", so the
    // agent came back UNRESTRICTED — removing a tool made it more powerful, not
    // less. These pin the three cases apart.

    fn settings_with(enabled: Option<Vec<&str>>, tools: &[&str]) -> AgentSettings {
        let mut s: AgentSettings = serde_json::from_value(json!({})).expect("defaults");
        s.tools = tools.iter().map(|t| json!({ "name": t })).collect();
        s.enabled_tools = enabled.map(|v| v.into_iter().map(String::from).collect());
        s
    }

    #[test]
    fn no_restriction_configured_does_not_narrow() {
        // CE parity: an agent that was never given a tool list is not narrowed,
        // so `enabledTools` must be absent from the ceiling entirely.
        let attrs = build_ceiling_attributes(&settings_with(None, &[]), &Default::default());
        let template = attrs.get("template").and_then(|v| v.as_object()).unwrap();
        assert!(!template.contains_key("enabledTools"),
                "absent means unconstrained: {template:?}");
    }

    #[test]
    fn an_emptied_tool_list_still_narrows() {
        // The bug. Configuring the list down to nothing must produce an EMPTY
        // enabledTools, not a missing one — an empty allow-list permits nothing,
        // so every tool falls outside the ceiling and prompts the owner.
        let attrs = build_ceiling_attributes(&settings_with(Some(vec![]), &[]), &Default::default());
        let template = attrs.get("template").and_then(|v| v.as_object()).unwrap();
        let enabled = template.get("enabledTools").expect("an emptied list must still be declared");
        assert_eq!(enabled.as_array().unwrap().len(), 0);
    }

    #[test]
    fn a_narrowed_list_is_carried_through() {
        let attrs = build_ceiling_attributes(
            &settings_with(Some(vec!["get_flow"]), &["get_flow", "list_flows"]),
            &Default::default(),
        );
        let template = attrs.get("template").and_then(|v| v.as_object()).unwrap();
        let enabled: Vec<&str> = template["enabledTools"].as_array().unwrap()
            .iter().map(|v| v.as_str().unwrap()).collect();
        assert_eq!(enabled, ["get_flow"], "removing list_flows must leave it out");
        let exposed: Vec<&str> = template["exposedTools"].as_array().unwrap()
            .iter().map(|v| v.as_str().unwrap()).collect();
        assert!(exposed.contains(&"list_flows"), "exposed is what it declares, enabled is the subset");
    }

    fn cfg_with_ui(ui: Option<Value>) -> AgentConfig {
        AgentConfig {
            session_id: "s".into(),
            message: "m".into(),
            ui_context: ui,
            config: AgentSettings { system_prompt: "BASE".into(), ..Default::default() },
        }
    }

    /// A designer's AI tab always gets the document and the propose-don't-save
    /// rule, whether or not the buffer is dirty.
    #[test]
    fn designer_mode_sends_the_document_and_forbids_saving() {
        let p = AgentState::prompt_with_ui_context(&cfg_with_ui(Some(json!({
            "kind": "canvas", "name": "a3t/orders", "mode": "designer",
            "draft_text": "placement:\n  mode: shared\n"
        }))));
        assert!(p.contains("propose_edit"), "must name the tool to use");
        assert!(p.contains("NOT saved"), "must say edits are not persisted");
        assert!(p.contains("mode: shared"), "must include the document");
    }

    /// A clean pane must not claim anything about unsaved edits.
    #[test]
    fn ui_context_without_dirty_says_nothing_about_unsaved_edits() {
        let p = AgentState::prompt_with_ui_context(&cfg_with_ui(Some(json!({
            "kind": "flow", "name": "a3t/hello", "section": "form"
        }))));
        assert!(p.starts_with("BASE"));
        assert!(p.contains("flow `a3t/hello`"));
        assert!(p.contains("the \"form\" section"));
        assert!(!p.to_lowercase().contains("not saved"));
        assert!(!p.contains("Unsaved editor contents"));
    }

    /// Regression: an edit the user has not saved must reach the LLM verbatim,
    /// with the stored copy explicitly marked stale — otherwise the agent reads
    /// the registry with get_flow and answers about the version the user just
    /// changed underneath it.
    #[test]
    fn dirty_ui_context_sends_the_unsaved_buffer_and_marks_storage_stale() {
        let draft = "document:\n  name: hello\ndo:\n- a:\n    set:\n      x: 1\n";
        let p = AgentState::prompt_with_ui_context(&cfg_with_ui(Some(json!({
            "kind": "flow", "name": "a3t/hello", "section": "code",
            "dirty": true, "draft_text": draft
        }))));
        assert!(p.contains("has NOT saved yet"), "must say the edit is unsaved");
        assert!(p.contains("out of date"), "must mark the stored copy stale");
        assert!(p.contains("### Unsaved editor contents"));
        assert!(p.contains(draft), "the buffer must be sent verbatim");
        assert!(!p.contains("(Truncated"), "not truncated, so no truncation notice");
    }

    /// A buffer the UI had to cut must be announced as partial, so the model does
    /// not answer confidently about a tail it never received.
    #[test]
    fn truncated_buffer_is_announced() {
        let p = AgentState::prompt_with_ui_context(&cfg_with_ui(Some(json!({
            "kind": "flow", "name": "a3t/big", "section": "code",
            "dirty": true, "draft_text": "partial", "draft_truncated": true
        }))));
        assert!(p.contains("(Truncated"));
    }

    /// `dirty` alone, with no buffer, still has to warn — it must never silently
    /// fall back to "storage is authoritative".
    #[test]
    fn dirty_without_draft_text_still_warns() {
        let p = AgentState::prompt_with_ui_context(&cfg_with_ui(Some(json!({
            "kind": "flow", "name": "a3t/hello", "dirty": true
        }))));
        assert!(p.contains("has NOT saved yet"));
        assert!(!p.contains("### Unsaved editor contents"));
    }

    /// Regression: turn-2 history must keep the assistant `tool_calls` that the
    /// persisted `tool` result refers to. Uses the exact JSON `save_session` writes.
    #[test]
    fn session_round_trip_preserves_tool_calls_and_leaves_no_orphans() {
        let tc = json!([{"id": "call_abc", "type": "function",
                         "function": {"name": "list_flows", "arguments": "{}"}}]);
        let msgs = vec![
            Message { role: "user".into(), content: "hi".into(), tool_call_id: None, tool_calls: None },
            Message { role: "assistant".into(), content: "".into(), tool_call_id: None,
                      tool_calls: Some(tc.as_array().unwrap().clone()) },
            Message { role: "tool".into(), content: "[]".into(),
                      tool_call_id: Some("call_abc".into()), tool_calls: None },
        ];
        let session = json!({ "messages": msgs }); // what save_session persists

        let restored = messages_from_session(&session);
        assert_eq!(restored.len(), 3);
        assert_eq!(restored[1].tool_calls.as_ref().map(|v| v.len()), Some(1),
                   "assistant tool_calls must survive a session reload");
        assert_eq!(restored[1].tool_calls.as_ref().unwrap()[0]["id"], "call_abc");
        assert_eq!(restored[2].tool_call_id.as_deref(), Some("call_abc"));

        // Every tool result must have its parent tool_call earlier in the history.
        for (i, m) in restored.iter().enumerate() {
            if let Some(id) = &m.tool_call_id {
                let has_parent = restored[..i].iter().any(|p| p.tool_calls.as_ref()
                    .map_or(false, |tcs| tcs.iter().any(|t| t["id"] == *id)));
                assert!(has_parent, "orphaned tool result {id}");
            }
        }
    }

    #[test]
    fn assistant_message_with_null_content_is_not_dropped() {
        let session = json!({ "messages": [
            {"role": "assistant", "content": null,
             "tool_calls": [{"id": "call_x", "type": "function", "function": {"name": "f", "arguments": "{}"}}]},
            {"role": "tool", "content": "ok", "tool_call_id": "call_x"}
        ]});
        let restored = messages_from_session(&session);
        assert_eq!(restored.len(), 2, "null-content assistant message must be kept");
        assert_eq!(restored[0].content, "");
        assert!(restored[0].tool_calls.is_some());
    }

    // ── streamed completion reassembly ──

    fn feed(acc: &mut StreamAccumulator, lines: &[&str]) -> Vec<(&'static str, String)> {
        lines.iter().flat_map(|l| acc.push_line(l)).collect()
    }

    #[test]
    fn text_deltas_accumulate_and_emit_tokens_in_order() {
        let mut acc = StreamAccumulator::default();
        let emitted = feed(&mut acc, &[
            r#"data: {"choices":[{"delta":{"content":"Hel"}}]}"#,
            r#"data: {"choices":[{"delta":{"content":"lo "}}]}"#,
            r#"data: {"choices":[{"delta":{"content":"world"},"finish_reason":"stop"}]}"#,
            "data: [DONE]",
        ]);
        assert_eq!(emitted.iter().map(|(k, d)| (*k, d.as_str())).collect::<Vec<_>>(),
                   vec![("token", "Hel"), ("token", "lo "), ("token", "world")]);
        assert!(acc.done);
        let v = acc.finish();
        assert_eq!(v["content"], "Hello world");
        assert_eq!(v["stop_reason"], "stop");
        assert!(v["raw_tool_calls"].is_null());
    }

    #[test]
    fn tool_call_fragments_reassemble_into_the_openai_wire_shape() {
        // Arguments split across frames is the normal case, not an edge case.
        let mut acc = StreamAccumulator::default();
        let emitted = feed(&mut acc, &[
            r#"data: {"choices":[{"delta":{"tool_calls":[{"index":0,"id":"call_1","function":{"name":"list_flows","arguments":""}}]}}]}"#,
            r#"data: {"choices":[{"delta":{"tool_calls":[{"index":0,"function":{"arguments":"{\"names"}}]}}]}"#,
            r#"data: {"choices":[{"delta":{"tool_calls":[{"index":0,"function":{"arguments":"pace\":\"a3t\"}"}}]}}]}"#,
            r#"data: {"choices":[{"delta":{},"finish_reason":"tool_calls"}]}"#,
        ]);
        // Announced once, as soon as the name was known.
        assert_eq!(emitted, vec![("tool", "list_flows".to_string())]);
        let v = acc.finish();
        let raw = v["raw_tool_calls"].as_array().expect("raw_tool_calls present");
        assert_eq!(raw.len(), 1);
        assert_eq!(raw[0]["id"], "call_1");
        assert_eq!(raw[0]["type"], "function");
        assert_eq!(raw[0]["function"]["name"], "list_flows");
        assert_eq!(raw[0]["function"]["arguments"], r#"{"namespace":"a3t"}"#);
        // And the parsed form the dispatcher uses.
        assert_eq!(v["tool_calls"][0]["name"], "list_flows");
        assert_eq!(v["tool_calls"][0]["input"]["namespace"], "a3t");
        assert_eq!(v["stop_reason"], "tool_calls");
    }

    #[test]
    fn parallel_tool_calls_stay_separated_by_index() {
        let mut acc = StreamAccumulator::default();
        feed(&mut acc, &[
            r#"data: {"choices":[{"delta":{"tool_calls":[{"index":0,"id":"a","function":{"name":"one","arguments":"{}"}}]}}]}"#,
            r#"data: {"choices":[{"delta":{"tool_calls":[{"index":1,"id":"b","function":{"name":"two","arguments":"{"}}]}}]}"#,
            r#"data: {"choices":[{"delta":{"tool_calls":[{"index":1,"function":{"arguments":"}"}}]}}]}"#,
        ]);
        let v = acc.finish();
        let raw = v["raw_tool_calls"].as_array().unwrap();
        assert_eq!(raw.len(), 2);
        assert_eq!((raw[0]["id"].as_str(), raw[0]["function"]["name"].as_str()), (Some("a"), Some("one")));
        assert_eq!((raw[1]["id"].as_str(), raw[1]["function"]["arguments"].as_str()), (Some("b"), Some("{}")));
    }

    #[test]
    fn noise_frames_are_ignored() {
        let mut acc = StreamAccumulator::default();
        let emitted = feed(&mut acc, &["", ": keep-alive", "event: ping", "data: ", "data: not-json"]);
        assert!(emitted.is_empty());
        assert_eq!(acc.finish()["content"], "");
    }
}
