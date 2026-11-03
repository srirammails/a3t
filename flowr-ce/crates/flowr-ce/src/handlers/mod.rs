//! ClawdFlow-specific TaskHandler implementations
//!
//! These handle task types that aren't in the CNCF spec but are
//! supported by ClawdFlow as function-call extensions:
//! - call:llm — LLM provider calls (OpenAI, Anthropic, Ollama)
//! - call:adb — AQL agent memory on ADB (`call:clawdb` is an alias)
//! - call:mcp — MCP server tool calls
//! - call:registry — moving a namespace (the A3T project flows)
//! - call:state:get/set/delete — state store operations
//! - run:script — inline script execution (Lua, JS, Python)
//! - emit / listen — CloudEvents between runs

pub mod agent_memory;
pub mod call_adb;
pub mod call_agent;
pub mod call_lancedb;
pub mod call_llm;
pub mod call_rotate;
pub mod events;
pub mod call_registry;
pub mod call_mcp;
pub mod run_script;
pub mod run_workflow;
pub mod state;

use std::sync::Arc;
use swf_runtime::executor::WorkflowExecutor;

/// Register all FlowR-CE task handlers. This is the single canonical handler
/// set (consolidated from the former flowr-mcp crate); CE registers it directly
/// and EE inherits it — no runtime injection.
pub fn register_clawdflow_handlers(executor: &mut WorkflowExecutor) {
    executor.register_handler(Arc::new(call_llm::LlmHandler::new()));
    executor.register_handler(Arc::new(call_mcp::McpHandler::new()));
    executor.register_handler(Arc::new(run_script::InlineScriptHandler));
    executor.register_handler(Arc::new(run_workflow::RunWorkflowHandler));
    executor.register_handler(Arc::new(state::StateGetHandler));
    executor.register_handler(Arc::new(state::StateSetHandler));
    executor.register_handler(Arc::new(state::StateDeleteHandler));
    executor.register_handler(Arc::new(call_lancedb::LanceDbHandler));
    executor.register_handler(Arc::new(call_adb::AdbHandler::new("call:adb")));
    executor.register_handler(Arc::new(call_adb::AdbHandler::new("call:clawdb")));
    executor.register_handler(Arc::new(call_rotate::RotateHandler));
    executor.register_handler(Arc::new(call_registry::RegistryHandler));
    executor.register_handler(Arc::new(call_agent::AgentHandler));
    executor.register_handler(Arc::new(events::EmitHandler));
    executor.register_handler(Arc::new(events::ListenHandler));
    // Last: with FLOWR_GATEWAY=pool it replaces the outbound handlers above.
    crate::gateway::register(executor);
}
