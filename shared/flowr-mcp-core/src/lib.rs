//! FlowR MCP Core
//!
//! Shared MCP protocol and tools for workflow engines.
//!
//! This crate provides backend-agnostic MCP functionality that can be used by:
//! - ClawdFlow (single-process with LanceDB)
//! - FlowR (distributed with Redis + Iggy)
//!
//! # Usage
//!
//! ```rust,ignore
//! use flowr_mcp_core::{protocol, tools};
//! use flowr_storage::StorageBackend;
//!
//! // Get tool definitions
//! let tools = tools::get_tools();
//!
//! // Handle tool calls with your storage backend
//! let result = tools::handle_tool_call(&storage, "list_flows", args).await;
//! ```

pub mod protocol;
pub mod tools;

pub use protocol::{
    CallToolParams, CallToolResult, ClientCapabilities, ClientInfo,
    InitializeParams, InitializeResult, JsonRpcError, JsonRpcRequest,
    JsonRpcResponse, RootsCapability, ServerCapabilities, ServerInfo,
    Tool, ToolContent, ToolsCapability, ToolsListResult, error_codes,
};

pub use tools::{
    get_tools, handle_tool_call, handle_tool_call_with_attributes,
    // The site-A gate on its own, plus the set of tools the agent runtime
    // owns — so a dispatcher can route those correctly and still authorize.
    authorize_tool_call, is_agent_runtime_tool, AGENT_RUNTIME_TOOLS,
    // The interactive-approval path: the decision-returning gate, the session
    // ledger resolution that sits between policy and the prompt, and the
    // per-tool permission state the agent's tool list is annotated with.
    authorize_tool_call_decision, apply_session_decisions, tool_permission, ToolPermission,
    authorize_mcp_tool_call, mcp_decision_id,
};

// Re-export the identity type so MCP-server call sites can name the principal
// they pass to `handle_tool_call` without a direct flowr-policy dependency (E0.4a).
pub use flowr_policy::{Principal, AgentId};
