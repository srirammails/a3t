//! MCP Server → ZeroClaw Tool adapter
//!
//! Wraps MCP server tools as ZeroClaw `Tool` trait objects so the agent
//! can invoke them during the tool-call loop.

use async_trait::async_trait;
use serde_json::Value;
use zeroclaw::tools::traits::{Tool, ToolResult};

/// An MCP tool exposed as a ZeroClaw Tool
pub struct McpTool {
    tool_name: String,
    tool_description: String,
    tool_schema: Value,
    server_url: String,
    client: reqwest::Client,
}

impl McpTool {
    pub fn new(
        name: String,
        description: String,
        schema: Value,
        server_url: String,
        client: reqwest::Client,
    ) -> Self {
        Self {
            tool_name: name,
            tool_description: description,
            tool_schema: schema,
            server_url,
            client,
        }
    }
}

#[async_trait]
impl Tool for McpTool {
    fn name(&self) -> &str {
        &self.tool_name
    }

    fn description(&self) -> &str {
        &self.tool_description
    }

    fn parameters_schema(&self) -> Value {
        self.tool_schema.clone()
    }

    async fn execute(&self, args: Value) -> anyhow::Result<ToolResult> {
        let payload = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "tools/call",
            "id": 1,
            "params": {
                "name": self.tool_name,
                "arguments": args
            }
        });

        let resp = self.client
            .post(&format!("{}/message", self.server_url))
            .header("Content-Type", "application/json")
            .header("Accept", "application/json")
            .json(&payload)
            .send()
            .await?;

        let status = resp.status();
        let text = resp.text().await?;
        let body: Value = serde_json::from_str(&text)
            .unwrap_or_else(|_| serde_json::json!({"error": text}));

        if !status.is_success() {
            return Ok(ToolResult {
                success: false,
                output: String::new(),
                error: Some(format!("MCP error ({}): {}", status, body)),
            });
        }

        // Extract content from MCP response
        let content = body
            .pointer("/result/content/0/text")
            .and_then(|v| v.as_str())
            .unwrap_or_else(|| {
                body.pointer("/result/content")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
            });

        Ok(ToolResult {
            success: true,
            output: content.to_string(),
            error: None,
        })
    }
}

/// Discover tools from an MCP server and return them as ZeroClaw Tool objects
pub async fn discover_mcp_tools(
    server_url: &str,
    client: &reqwest::Client,
) -> anyhow::Result<Vec<Box<dyn Tool>>> {
    let payload = serde_json::json!({
        "jsonrpc": "2.0",
        "method": "tools/list",
        "id": 1,
        "params": {}
    });

    let resp = client
        .post(&format!("{}/message", server_url))
        .header("Content-Type", "application/json")
        .header("Accept", "application/json")
        .json(&payload)
        .send()
        .await?;

    let text = resp.text().await?;
    let body: Value = serde_json::from_str(&text)
        .map_err(|e| anyhow::anyhow!("Failed to parse MCP response: {} (body: {})", e, &text[..text.len().min(200)]))?;

    let tools_array = body
        .pointer("/result/tools")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    let mut tools: Vec<Box<dyn Tool>> = Vec::new();

    for tool_def in tools_array {
        let name = tool_def.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string();
        let description = tool_def.get("description").and_then(|v| v.as_str()).unwrap_or("").to_string();
        let schema = tool_def.get("inputSchema").cloned().unwrap_or(serde_json::json!({
            "type": "object",
            "properties": {}
        }));

        if !name.is_empty() {
            tools.push(Box::new(McpTool::new(
                name,
                description,
                schema,
                server_url.to_string(),
                client.clone(),
            )));
        }
    }

    Ok(tools)
}
