//! call:mcp handler — calls MCP server tools
//!
//! In CNCF syntax: `call: mcp` with `with:` containing tool, arguments, server.
//! The MCP server URL can come from a named resource via `_resource`.

use async_trait::async_trait;
use serde_json::{json, Value};
use tracing::info;

use swf_parser::{NamedTask, Task};
use swf_runtime::context::ExecutionContext;
use swf_runtime::error::{RuntimeError, Result};
use swf_runtime::traits::{TaskHandler, TaskResult};

pub struct McpHandler {
    client: reqwest::Client,
}

impl McpHandler {
    pub fn new() -> Self {
        Self { client: reqwest::Client::new() }
    }
}

#[async_trait]
impl TaskHandler for McpHandler {
    fn task_type(&self) -> &str {
        "call:mcp"
    }

    /// Gate on the *MCP tool* this step invokes (`with.tool`), not the bare
    /// `call:mcp` task type — so a policy can allow `search_docs` but forbid
    /// `delete_flow`, and the destructive-tool guardrail sees the real tool name.
    /// `task_type: "call:mcp"` is kept as an attribute for mechanism-level rules
    /// (e.g. forbidding raw `call:mcp` outside sanctioned flows).
    fn policy_resource(&self, task: &NamedTask) -> flowr_policy::Resource {
        let tool = match &task.task {
            Task::Call(c) => c
                .with
                .as_ref()
                .and_then(|w| w.get("tool"))
                .and_then(|v| v.as_str())
                .unwrap_or("call:mcp")
                .to_string(),
            _ => "call:mcp".to_string(),
        };
        flowr_policy::Resource::tool(tool).with_attr("task_type", "call:mcp")
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

        let method = with.get("method").and_then(|v| v.as_str()).unwrap_or("tools/call");
        let tool_name = with.get("tool").and_then(|v| v.as_str()).unwrap_or("").to_string();
        let arguments = with.get("arguments").cloned().unwrap_or(json!({}));

        if method == "tools/call" && tool_name.is_empty() {
            return Err(RuntimeError::task_failed(&task.name, "call:mcp requires 'tool' in with"));
        }

        // Resolve MCP server URL: from with.server, or from _resource config, or default
        let server_url = if let Some(server) = with.get("server").and_then(|v| v.as_str()) {
            server.to_string()
        } else if let Some(resource_name) = with.get("_resource").and_then(|v| v.as_str()) {
            resolve_mcp_url_from_resource(ctx, resource_name).await
                .unwrap_or_else(|| format!("http://localhost:{}/message", 3099))
        } else {
            format!("http://localhost:{}/message", 3099)
        };

        // Ensure URL ends with /message for JSON-RPC
        let server_url = flowr_mcp_core::mcp_message_url(&server_url);

        info!(task = %task.name, method = %method, tool = %tool_name, server = %server_url, "calling MCP");

        let mcp_request = if method == "tools/list" {
            json!({
                "jsonrpc": "2.0",
                "method": "tools/list",
                "id": 1,
                "params": {}
            })
        } else {
            json!({
                "jsonrpc": "2.0",
                "method": "tools/call",
                "id": 1,
                "params": {
                    "name": tool_name,
                    "arguments": arguments,
                }
            })
        };

        let resp = flowr_mcp_core::with_adb_auth(self.client.post(&server_url), &server_url)
            .header("Content-Type", "application/json")
            .json(&mcp_request)
            .send()
            .await
            .map_err(|e| RuntimeError::Communication(format!("MCP call failed: {}", e)))?;

        let resp_body: Value = resp.json().await
            .map_err(|e| RuntimeError::Communication(format!("MCP response parse error: {}", e)))?;

        // Extract result from MCP response
        let result = if method == "tools/list" {
            // tools/list returns {tools: [{name, description, inputSchema}]}
            // Convert to our format: [{name, description, input_schema}]
            resp_body.get("result")
                .and_then(|r| r.get("tools"))
                .and_then(|t| t.as_array())
                .map(|tools| {
                    let converted: Vec<Value> = tools.iter().map(|t| {
                        json!({
                            "name": t.get("name"),
                            "description": t.get("description"),
                            "input_schema": t.get("inputSchema").or_else(|| t.get("input_schema"))
                        })
                    }).collect();
                    json!(converted)
                })
                .unwrap_or(json!([]))
        } else {
            resp_body.get("result")
                .and_then(|r| r.get("content"))
                .and_then(|c| c.as_array())
                .and_then(|arr| arr.first())
                .and_then(|item| item.get("text"))
                .cloned()
                .unwrap_or(resp_body.clone())
        };

        Ok(TaskResult::new(result))
    }
}

/// Resolve MCP server URL from a named resource in storage
async fn resolve_mcp_url_from_resource(ctx: &ExecutionContext, resource_name: &str) -> Option<String> {
    let storage_any = ctx.storage.as_ref()?;
    let storage = storage_any.downcast_ref::<std::sync::Arc<dyn flowr_storage::StorageBackend>>()?;

    let (ns, name) = if resource_name.contains('/') {
        let parts: Vec<&str> = resource_name.splitn(2, '/').collect();
        (parts[0], parts[1])
    } else {
        ("default", resource_name)
    };

    let resource = storage.resources().get(ns, name).await.ok()?;
    // Try url field, then endpoint, then server
    resource.config.get("url").and_then(|v| v.as_str())
        .or_else(|| resource.config.get("endpoint").and_then(|v| v.as_str()))
        .or_else(|| resource.config.get("server").and_then(|v| v.as_str()))
        .map(|s| s.to_string())
}
