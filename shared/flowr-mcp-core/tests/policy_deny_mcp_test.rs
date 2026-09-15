//! E0.3 — Deny semantics at the MCP tool choke point (site A).
//!
//! Isolated test binary: installs a process-wide `DenyAll` hook, so every test
//! here expects denial. The Allow path is the default (`AllowAll`) exercised by
//! the rest of the suite in separate binaries.
//!
//! What we prove:
//!   1. A denied MCP tool call returns a `CallToolResult` with `is_error = true`
//!      and a message naming the tool, the reason, and the policy ids.
//!   2. Denial happens *before* storage is touched: the `StorageBackend` passed
//!      in has every accessor wired to `unimplemented!()`, so if the choke point
//!      ever fell through to a handler, the test would panic instead of pass.

use std::sync::Arc;

use flowr_mcp_core::handle_tool_call;
use flowr_mcp_core::protocol::ToolContent;
use flowr_storage::{
    CheckpointStore, EventBus, ExecutionQueue, ExecutionStore, ResourceStore, StateStore,
    StorageBackend, TriggerStore, WorkflowStore,
};
use flowr_types::error::FlowResult;
use serde_json::json;

/// A `StorageBackend` that must never be called. The deny path returns before
/// any `storage.*()` access, so every method here diverges — reaching one is a
/// bug the test will catch as a panic.
struct PanicStorage;

#[async_trait::async_trait]
impl StorageBackend for PanicStorage {
    fn workflows(&self) -> &dyn WorkflowStore {
        unimplemented!("storage must not be reached on a denied call")
    }
    fn resources(&self) -> &dyn ResourceStore {
        unimplemented!("storage must not be reached on a denied call")
    }
    fn state(&self) -> &dyn StateStore {
        unimplemented!("storage must not be reached on a denied call")
    }
    fn checkpoints(&self) -> &dyn CheckpointStore {
        unimplemented!("storage must not be reached on a denied call")
    }
    fn events(&self) -> &dyn EventBus {
        unimplemented!("storage must not be reached on a denied call")
    }
    fn queue(&self) -> &dyn ExecutionQueue {
        unimplemented!("storage must not be reached on a denied call")
    }
    fn executions(&self) -> &dyn ExecutionStore {
        unimplemented!("storage must not be reached on a denied call")
    }
    fn triggers(&self) -> &dyn TriggerStore {
        unimplemented!("storage must not be reached on a denied call")
    }
    async fn init(&self) -> FlowResult<()> {
        unimplemented!("storage must not be reached on a denied call")
    }
    async fn close(&self) -> FlowResult<()> {
        unimplemented!("storage must not be reached on a denied call")
    }
}

fn install_deny_all() {
    let _ = flowr_policy::set_policy_hook(Arc::new(flowr_policy::DenyAll));
}

fn result_text(r: &flowr_mcp_core::protocol::CallToolResult) -> String {
    r.content
        .iter()
        .map(|c| match c {
            ToolContent::Text { text } => text.clone(),
        })
        .collect()
}

#[tokio::test]
async fn denied_mcp_call_is_error_and_names_policy() {
    install_deny_all();
    let storage = PanicStorage;

    let caller = flowr_policy::Principal::human("user::sriram", vec!["admin".into()]);
    let result = handle_tool_call(
        &storage,
        &caller,
        "create_flow",
        Some(json!({ "name": "anything" })),
    )
    .await;

    assert_eq!(result.is_error, Some(true), "denied call must be an error result");
    let text = result_text(&result);
    assert!(text.contains("denied"), "message says denied: {text}");
    assert!(text.contains("create_flow"), "message names the tool: {text}");
    assert!(text.contains("deny-all"), "message carries reason: {text}");
    assert!(text.contains("policies:"), "message lists policy ids: {text}");
}

#[tokio::test]
async fn denied_before_storage_for_read_tools_too() {
    // Even a pure read tool (no mutation) is gated: the choke point is
    // unconditional, so `list_flows` is denied without touching storage.
    install_deny_all();
    let storage = PanicStorage;

    let caller = flowr_policy::Principal::service("mcp");
    let result = handle_tool_call(&storage, &caller, "list_flows", Some(json!({}))).await;

    assert_eq!(result.is_error, Some(true));
    assert!(result_text(&result).contains("denied"));
}
