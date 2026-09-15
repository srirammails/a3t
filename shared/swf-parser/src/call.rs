//! Call task — call services, functions, or cataloged resources

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Call task — calls a service or function
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallTask {
    /// Call target: protocol name, function ref, or catalog ref
    pub target: CallTarget,

    /// Arguments passed to the call
    #[serde(default)]
    pub with: Option<Value>,

    /// Authentication reference
    #[serde(default)]
    pub authentication: Option<String>,
}

/// What the call targets
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CallTarget {
    Http(HttpCall),
    Grpc(GrpcCall),
    OpenApi(OpenApiCall),
    AsyncApi(AsyncApiCall),
    A2A(A2ACall),
    Mcp(McpCall),
    /// Reference to a use.functions entry or cataloged function
    Function(String),
}

/// HTTP call configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HttpCall {
    #[serde(default)]
    pub method: Option<String>,
    #[serde(default)]
    pub endpoint: Option<HttpEndpoint>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HttpEndpoint {
    pub uri: String,
    #[serde(default)]
    pub authentication: Option<EndpointAuth>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EndpointAuth {
    #[serde(rename = "use")]
    pub use_ref: String,
}

/// gRPC call configuration
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GrpcCall {
    #[serde(default)]
    pub proto: Option<String>,
    #[serde(default)]
    pub service: Option<String>,
    #[serde(default)]
    pub method: Option<String>,
}

/// OpenAPI call configuration
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct OpenApiCall {
    #[serde(default)]
    pub document: Option<String>,
    #[serde(rename = "operationId")]
    #[serde(default)]
    pub operation_id: Option<String>,
}

/// AsyncAPI call configuration
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AsyncApiCall {
    #[serde(default)]
    pub document: Option<String>,
    #[serde(default)]
    pub channel: Option<String>,
    #[serde(default)]
    pub message: Option<Value>,
}

/// A2A (Agent-to-Agent) call configuration
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct A2ACall {
    #[serde(default)]
    pub agent: Option<String>,
    #[serde(default)]
    pub skill: Option<String>,
}

/// MCP (Model Context Protocol) call configuration — FlowR extension
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct McpCall {
    #[serde(default)]
    pub server: Option<String>,
    #[serde(default)]
    pub tool: Option<String>,
    #[serde(default)]
    pub arguments: Option<Value>,
}
