//! MCP Server implementation for ClawdFlow
//!
//! Implements Model Context Protocol over stdio for Claude Desktop integration.
//! Uses the shared flowr-mcp-core crate for protocol and tools.

use std::io::{BufRead, BufReader, Write};
use std::sync::Arc;

use flowr_mcp_core::{
    error_codes, get_tools, handle_tool_call,
    CallToolParams, InitializeParams, InitializeResult, JsonRpcRequest, JsonRpcResponse,
    ServerCapabilities, ServerInfo, ToolsCapability, ToolsListResult,
};
use flowr_storage::StorageBackend;
use serde_json::{json, Value};
use tracing::{debug, error, info, warn};

/// MCP Server state
pub struct McpServer<S: StorageBackend> {
    /// Storage backend
    storage: Arc<S>,
}

impl<S: StorageBackend + 'static> McpServer<S> {
    /// Create a new MCP server with the given storage backend
    pub fn new(storage: Arc<S>) -> Self {
        Self { storage }
    }

    /// Start the MCP server (stdio transport)
    pub async fn run(&self) -> anyhow::Result<()> {
        info!("Starting ClawdFlow MCP server");

        let stdin = std::io::stdin();
        let mut stdout = std::io::stdout();
        let reader = BufReader::new(stdin.lock());

        for line in reader.lines() {
            match line {
                Ok(line) if line.trim().is_empty() => continue,
                Ok(line) => {
                    debug!(request = %line, "Received request");

                    let response = self.handle_request(&line).await;

                    let response_json = serde_json::to_string(&response)?;
                    debug!(response = %response_json, "Sending response");

                    writeln!(stdout, "{}", response_json)?;
                    stdout.flush()?;
                }
                Err(e) => {
                    error!(error = %e, "Failed to read from stdin");
                    break;
                }
            }
        }

        info!("MCP server shutting down");
        Ok(())
    }

    /// Handle a single JSON-RPC request
    async fn handle_request(&self, line: &str) -> JsonRpcResponse {
        // Parse request
        let request: JsonRpcRequest = match serde_json::from_str(line) {
            Ok(req) => req,
            Err(e) => {
                return JsonRpcResponse::error(None, error_codes::PARSE_ERROR, format!("Parse error: {}", e));
            }
        };

        // Route to handler
        match request.method.as_str() {
            "initialize" => self.handle_initialize(request.id, request.params).await,
            "initialized" => self.handle_initialized(request.id).await,
            "tools/list" => self.handle_tools_list(request.id).await,
            "tools/call" => self.handle_tools_call(request.id, request.params).await,
            "ping" => self.handle_ping(request.id).await,
            "shutdown" => {
                info!("Shutdown requested");
                JsonRpcResponse::success(request.id, json!(null))
            }
            _ => {
                warn!(method = %request.method, "Unknown method");
                JsonRpcResponse::error(
                    request.id,
                    error_codes::METHOD_NOT_FOUND,
                    format!("Method not found: {}", request.method),
                )
            }
        }
    }

    /// Handle initialize request
    async fn handle_initialize(
        &self,
        id: Option<Value>,
        params: Option<Value>,
    ) -> JsonRpcResponse {
        let _params: InitializeParams = match params {
            Some(p) => match serde_json::from_value(p) {
                Ok(p) => p,
                Err(e) => {
                    return JsonRpcResponse::error(
                        id,
                        error_codes::INVALID_PARAMS,
                        format!("Invalid params: {}", e),
                    );
                }
            },
            None => {
                return JsonRpcResponse::error(id, error_codes::INVALID_PARAMS, "Missing params".to_string());
            }
        };

        info!("MCP client connected");

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

    /// Handle initialized notification
    async fn handle_initialized(&self, id: Option<Value>) -> JsonRpcResponse {
        info!("MCP initialization complete");
        JsonRpcResponse::success(id, json!({}))
    }

    /// Handle tools/list request
    async fn handle_tools_list(&self, id: Option<Value>) -> JsonRpcResponse {
        let tools = get_tools();
        let result = ToolsListResult { tools };
        JsonRpcResponse::success(id, serde_json::to_value(result).unwrap())
    }

    /// Handle tools/call request
    async fn handle_tools_call(
        &self,
        id: Option<Value>,
        params: Option<Value>,
    ) -> JsonRpcResponse {
        let params: CallToolParams = match params {
            Some(p) => match serde_json::from_value(p) {
                Ok(p) => p,
                Err(e) => {
                    return JsonRpcResponse::error(
                        id,
                        error_codes::INVALID_PARAMS,
                        format!("Invalid params: {}", e),
                    );
                }
            },
            None => {
                return JsonRpcResponse::error(id, error_codes::INVALID_PARAMS, "Missing params".to_string());
            }
        };

        info!(tool = %params.name, "Executing tool");

        // Use the shared handle_tool_call with our storage backend. This server
        // carries no HTTP auth context, so the caller is the mcp service (E0.4a).
        let caller = flowr_mcp_core::Principal::service("mcp");
        let result = handle_tool_call(self.storage.as_ref(), &caller, &params.name, params.arguments).await;

        JsonRpcResponse::success(id, serde_json::to_value(result).unwrap())
    }

    /// Handle ping request
    async fn handle_ping(&self, id: Option<Value>) -> JsonRpcResponse {
        JsonRpcResponse::success(id, json!({}))
    }
}
