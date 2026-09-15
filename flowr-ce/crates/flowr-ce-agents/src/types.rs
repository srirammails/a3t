//! Agent types and shared structures for all agent modes
//!
//! Defines the core types used across ephemeral, session, persistent,
//! and reactive agent implementations.

use serde::{Deserialize, Serialize};
use serde_json::{self, Value};

/// Agent lifecycle mode
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum AgentMode {
    /// Created per step, destroyed after (default)
    Ephemeral,
    /// Lives across workflow calls with TTL, keyed by session_id
    Session,
    /// Long-lived daemon, accepts messages via inbox
    Persistent,
    /// Event-driven, wakes on triggers
    Reactive,
}

impl Default for AgentMode {
    fn default() -> Self {
        Self::Ephemeral
    }
}

/// Trigger definition for reactive agents
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum AgentTrigger {
    /// HTTP webhook trigger
    Webhook {
        path: String,
        #[serde(default)]
        message: Option<String>,
    },
    /// Cron schedule trigger
    Cron {
        expression: String,
        #[serde(default)]
        message: Option<String>,
        #[serde(default)]
        tz: Option<String>,
    },
    /// Internal event trigger (workflow events, etc.)
    Event {
        source: String,
        #[serde(default)]
        filter: Option<String>,
        #[serde(default)]
        message: Option<String>,
    },
}

/// Inbox configuration for persistent/reactive agents
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InboxConfig {
    /// Inbox type: poll, webhook, queue
    #[serde(default = "default_inbox_type")]
    pub r#type: String,
    /// Poll interval in ms (for poll type)
    #[serde(default = "default_poll_interval")]
    pub poll_interval: u64,
}

fn default_inbox_type() -> String {
    "poll".to_string()
}

fn default_poll_interval() -> u64 {
    5000
}

impl Default for InboxConfig {
    fn default() -> Self {
        Self {
            r#type: default_inbox_type(),
            poll_interval: default_poll_interval(),
        }
    }
}

/// Full agent resource configuration (from resource config)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentResourceConfig {
    // =====================================================================
    // Resource bindings (6 types)
    // =====================================================================

    /// LLM provider reference (backward compat: single string)
    #[serde(default)]
    pub llm: Option<String>,

    /// Multi-LLM providers with roles (primary/fallback/specialist)
    #[serde(default)]
    pub llms: Vec<LlmBinding>,

    /// MCP server references for tool discovery
    #[serde(default)]
    pub mcp_servers: Vec<McpServerRef>,

    /// Memory resource reference (ClawDB)
    #[serde(default)]
    pub memory: Option<String>,

    /// Typed variables injected into agent context
    #[serde(default)]
    pub variables: Value,

    /// Bound workflows (procedural memory — agent can execute these)
    #[serde(default)]
    pub flows: Vec<FlowBinding>,

    /// Bound scripts (Lua/JS callable by agent)
    #[serde(default)]
    pub scripts: Vec<ScriptBinding>,

    // =====================================================================
    // Agent template
    // =====================================================================

    /// Template reference (versioned CNCF Workflow)
    #[serde(default)]
    pub template: Option<String>,

    // =====================================================================
    // Agent behavior
    // =====================================================================

    /// System prompt
    #[serde(default = "default_system_prompt")]
    pub system_prompt: String,

    /// Agent lifecycle mode
    #[serde(default)]
    pub mode: AgentMode,

    /// Agent identity (for session/persistent/reactive)
    #[serde(default)]
    pub agent_id: Option<String>,

    /// Session TTL in seconds (for session mode, 0 = no expiry)
    #[serde(default = "default_ttl")]
    pub ttl: u64,

    /// Max tool call loop iterations per turn
    #[serde(default = "default_max_iterations")]
    pub max_iterations: u32,

    /// Max conversation messages to retain in history
    #[serde(default = "default_max_history")]
    pub max_history: usize,

    /// Inbox config (persistent/reactive)
    #[serde(default)]
    pub inbox: Option<InboxConfig>,

    /// Triggers (reactive only — legacy, prefer TriggerManager)
    #[serde(default)]
    pub triggers: Vec<AgentTrigger>,

    /// AQL to run after each turn (store outcome in ClawDB)
    #[serde(default)]
    pub on_complete: Option<String>,

    /// AQL to run before each turn (load context from ClawDB)
    #[serde(default)]
    pub on_before_turn: Option<String>,
}

impl AgentResourceConfig {
    /// Get the primary LLM resource name (backward compat + multi-LLM)
    pub fn primary_llm(&self) -> Option<&str> {
        // First check llms array for primary role
        if let Some(binding) = self.llms.iter().find(|l| l.role == LlmRole::Primary) {
            return Some(&binding.r#ref);
        }
        // Fall back to first in llms array
        if let Some(first) = self.llms.first() {
            return Some(&first.r#ref);
        }
        // Fall back to single llm field
        self.llm.as_deref()
    }

    /// Get fallback LLM resource name
    pub fn fallback_llm(&self) -> Option<&str> {
        self.llms.iter()
            .find(|l| l.role == LlmRole::Fallback)
            .map(|l| l.r#ref.as_str())
    }
}

/// LLM binding with role
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmBinding {
    /// Resource reference name
    pub r#ref: String,
    /// Role: primary, fallback, specialist
    #[serde(default)]
    pub role: LlmRole,
    /// Optional specialist description (when role=specialist)
    #[serde(default)]
    pub description: Option<String>,
}

/// LLM role in multi-LLM setup
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum LlmRole {
    Primary,
    Fallback,
    Specialist,
}

impl Default for LlmRole {
    fn default() -> Self {
        Self::Primary
    }
}

/// MCP server reference
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerRef {
    /// Server URL (for direct URL) or resource name (for ref)
    #[serde(default)]
    pub url: Option<String>,
    /// Resource reference name
    #[serde(default)]
    pub r#ref: Option<String>,
    /// Display name
    #[serde(default)]
    pub name: Option<String>,
}

impl McpServerRef {
    /// Get the URL to use for this MCP server
    pub fn effective_url(&self) -> Option<&str> {
        self.url.as_deref()
    }
}

/// Workflow binding — agent can execute these as procedural memory
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlowBinding {
    /// Workflow reference (namespace/name or just name)
    pub r#ref: String,
    /// Role description (strategy, validation, cleanup, etc.)
    #[serde(default)]
    pub role: Option<String>,
    /// Namespace override
    #[serde(default)]
    pub namespace: Option<String>,
}

/// Script binding — inline or external script callable by agent
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScriptBinding {
    /// Script name (unique within agent)
    pub name: String,
    /// Script language
    pub language: ScriptLanguage,
    /// Inline source code (if inline)
    #[serde(default)]
    pub source: Option<String>,
    /// Resource reference (if external)
    #[serde(default)]
    pub r#ref: Option<String>,
    /// Description (shown to LLM as tool description)
    #[serde(default)]
    pub description: Option<String>,
}

/// Script language
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum ScriptLanguage {
    Lua,
    #[serde(alias = "javascript")]
    Js,
}

impl ScriptBinding {
    pub fn language_str(&self) -> &str {
        match self.language {
            ScriptLanguage::Lua => "lua",
            ScriptLanguage::Js => "js",
        }
    }
}

// =============================================================================
// Agent Observability types
// =============================================================================

/// A single conversation turn (message + response)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentTurn {
    /// Turn number (1-indexed)
    pub turn: u32,
    /// Timestamp
    pub timestamp: String,
    /// User message / prompt
    pub input: String,
    /// Agent response
    pub output: String,
    /// Whether the turn succeeded
    pub success: bool,
    /// Duration in milliseconds
    pub duration_ms: u64,
    /// Source (workflow, webhook, cron, etc.)
    #[serde(default)]
    pub source: Option<String>,
}

/// Per-agent metrics
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AgentMetrics {
    /// Total turns processed
    pub total_turns: u64,
    /// Successful turns
    pub successful_turns: u64,
    /// Failed turns
    pub failed_turns: u64,
    /// Total tool calls made
    pub tool_calls: u64,
    /// Total duration of all turns (ms)
    pub total_duration_ms: u64,
    /// Average turn duration (ms)
    pub avg_duration_ms: u64,
    /// Last turn timestamp
    #[serde(default)]
    pub last_turn_at: Option<String>,
}

/// Agent health status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentHealth {
    pub agent_id: String,
    pub status: AgentStatus,
    pub mode: AgentMode,
    pub uptime_secs: u64,
    pub memory_connected: bool,
    pub tools_count: usize,
    pub metrics: AgentMetrics,
}

/// Agent lifecycle event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentEvent {
    pub agent_id: String,
    pub event_type: AgentEventType,
    pub timestamp: String,
    #[serde(default)]
    pub details: Option<Value>,
}

/// Agent event types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentEventType {
    Spawned,
    TurnStarted,
    TurnCompleted,
    TurnFailed,
    Stopped,
    SessionExpired,
    TriggerFired,
}

fn default_system_prompt() -> String {
    "You are a helpful AI agent. Use the available tools to accomplish tasks.".to_string()
}

fn default_ttl() -> u64 {
    1800 // 30 minutes
}

fn default_max_iterations() -> u32 {
    10
}

fn default_max_history() -> usize {
    50
}

/// Step-level config for call:agent / send:agent / ask:agent
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentStepConfig {
    /// User prompt / message
    #[serde(default)]
    pub prompt: Option<String>,

    /// Session ID override (for session mode)
    #[serde(default)]
    pub session_id: Option<String>,

    /// Agent ID (for send/ask/stop targeting a specific agent)
    #[serde(default)]
    pub agent_id: Option<String>,

    /// Additional context
    #[serde(default)]
    pub context: Option<Value>,

    /// Max iterations override
    #[serde(default)]
    pub max_iterations: Option<u32>,

    /// Message to send (for send:agent)
    #[serde(default)]
    pub message: Option<String>,

    /// Callback URL for async response
    #[serde(default)]
    pub callback_url: Option<String>,
}

/// Message sent to a persistent/session agent
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentMessage {
    /// Unique message ID
    pub id: String,
    /// Message content
    pub content: String,
    /// Optional context
    #[serde(default)]
    pub context: Option<Value>,
    /// Source (workflow instance, webhook, cron, etc.)
    #[serde(default)]
    pub source: Option<String>,
}

/// Response from an agent
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentResponse {
    /// Message ID this responds to
    pub message_id: String,
    /// Agent ID
    pub agent_id: String,
    /// Response content
    pub output: String,
    /// Whether the agent completed successfully
    pub success: bool,
    /// Iteration count for this turn
    #[serde(default)]
    pub iterations: u32,
}

/// Agent status info
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentInfo {
    pub agent_id: String,
    pub mode: AgentMode,
    pub status: AgentStatus,
    pub tools_count: usize,
    pub has_memory: bool,
    pub history_len: usize,
    pub created_at: String,
    pub last_active: String,
    #[serde(default)]
    pub triggers: Vec<AgentTrigger>,
}

/// Agent runtime status
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum AgentStatus {
    Starting,
    Running,
    Idle,
    Processing,
    Stopped,
    Error,
}
