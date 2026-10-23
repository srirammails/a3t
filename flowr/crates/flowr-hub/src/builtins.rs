//! Built-in Resource Types
//!
//! Pre-defined resource types that ship with FlowR.

use serde_json::json;

use crate::types::{ConnectionKind, ResourceCategory, ResourceType};

/// Get all built-in resource types
pub fn builtin_types() -> Vec<ResourceType> {
    vec![
        mysql(),
        postgres(),
        redis_type(),
        rest_api(),
        qdrant(),
        graphql(),
        grpc(),
        s3(),
        kafka(),
        mcp_server(),
        llm(),
        adb(),
        input_channel(),
        output_channel(),
        agent(),
        human(),
        secret(),
        credential(),
    ]
}

/// MySQL database resource type
pub fn mysql() -> ResourceType {
    ResourceType::new("mysql", "MySQL Database")
        .with_category(ResourceCategory::Database)
        .with_connection_kind(ConnectionKind::Pool)
        .with_icon("database")
        .with_schema(json!({
            "type": "object",
            "properties": {
                "host": {
                    "type": "string",
                    "description": "Database host"
                },
                "port": {
                    "type": "integer",
                    "default": 3306,
                    "description": "Database port"
                },
                "username": {
                    "type": "string",
                    "description": "Database username"
                },
                "password": {
                    "type": "string",
                    "description": "Database password (use ${secrets.xxx} for secrets)",
                    "format": "password"
                },
                "database": {
                    "type": "string",
                    "description": "Database name"
                },
                "ssl_mode": {
                    "type": "string",
                    "enum": ["disabled", "preferred", "required"],
                    "default": "preferred"
                },
                "pool_size": {
                    "type": "integer",
                    "default": 10,
                    "minimum": 1,
                    "maximum": 100
                }
            },
            "required": ["host", "username", "password", "database"]
        }))
        .as_builtin()
}

/// PostgreSQL database resource type
pub fn postgres() -> ResourceType {
    ResourceType::new("postgres", "PostgreSQL Database")
        .with_category(ResourceCategory::Database)
        .with_connection_kind(ConnectionKind::Pool)
        .with_icon("database")
        .with_schema(json!({
            "type": "object",
            "properties": {
                "host": {
                    "type": "string",
                    "description": "Database host"
                },
                "port": {
                    "type": "integer",
                    "default": 5432,
                    "description": "Database port"
                },
                "username": {
                    "type": "string",
                    "description": "Database username"
                },
                "password": {
                    "type": "string",
                    "description": "Database password",
                    "format": "password"
                },
                "database": {
                    "type": "string",
                    "description": "Database name"
                },
                "schema": {
                    "type": "string",
                    "default": "public",
                    "description": "Default schema"
                },
                "ssl_mode": {
                    "type": "string",
                    "enum": ["disable", "allow", "prefer", "require", "verify-ca", "verify-full"],
                    "default": "prefer"
                },
                "pool_size": {
                    "type": "integer",
                    "default": 10,
                    "minimum": 1,
                    "maximum": 100
                }
            },
            "required": ["host", "username", "password", "database"]
        }))
        .as_builtin()
}

/// Redis/Dragonfly resource type
pub fn redis_type() -> ResourceType {
    ResourceType::new("redis", "Redis / Dragonfly")
        .with_category(ResourceCategory::KeyValue)
        .with_connection_kind(ConnectionKind::Pool)
        .with_icon("key")
        .with_schema(json!({
            "type": "object",
            "properties": {
                "url": {
                    "type": "string",
                    "description": "Redis URL (redis://host:port or redis://user:pass@host:port/db)",
                    "pattern": "^redis(s)?://"
                },
                "host": {
                    "type": "string",
                    "description": "Redis host (alternative to url)"
                },
                "port": {
                    "type": "integer",
                    "default": 6379,
                    "description": "Redis port"
                },
                "password": {
                    "type": "string",
                    "description": "Redis password",
                    "format": "password"
                },
                "database": {
                    "type": "integer",
                    "default": 0,
                    "minimum": 0,
                    "maximum": 15
                },
                "tls": {
                    "type": "boolean",
                    "default": false
                },
                "pool_size": {
                    "type": "integer",
                    "default": 10
                }
            },
            "oneOf": [
                { "required": ["url"] },
                { "required": ["host"] }
            ]
        }))
        .as_builtin()
}

/// REST API resource type
pub fn rest_api() -> ResourceType {
    ResourceType::new("rest_api", "REST API")
        .with_category(ResourceCategory::Api)
        .with_connection_kind(ConnectionKind::OnDemand)
        .with_icon("api")
        .with_schema(json!({
            "type": "object",
            "properties": {
                "base_url": {
                    "type": "string",
                    "description": "Base URL for the API",
                    "format": "uri"
                },
                "auth_type": {
                    "type": "string",
                    "enum": ["none", "api_key", "bearer", "basic", "oauth2"],
                    "default": "none"
                },
                "api_key": {
                    "type": "string",
                    "description": "API key (for api_key auth)",
                    "format": "password"
                },
                "api_key_header": {
                    "type": "string",
                    "default": "X-API-Key",
                    "description": "Header name for API key"
                },
                "bearer_token": {
                    "type": "string",
                    "description": "Bearer token",
                    "format": "password"
                },
                "basic_username": {
                    "type": "string"
                },
                "basic_password": {
                    "type": "string",
                    "format": "password"
                },
                "headers": {
                    "type": "object",
                    "additionalProperties": { "type": "string" },
                    "description": "Default headers to include in requests"
                },
                "timeout_ms": {
                    "type": "integer",
                    "default": 30000,
                    "description": "Request timeout in milliseconds"
                }
            },
            "required": ["base_url"]
        }))
        .as_builtin()
}

/// Qdrant vector database resource type
pub fn qdrant() -> ResourceType {
    ResourceType::new("qdrant", "Qdrant Vector Database")
        .with_category(ResourceCategory::Vector)
        .with_connection_kind(ConnectionKind::Pool)
        .with_icon("vector")
        .with_schema(json!({
            "type": "object",
            "properties": {
                "url": {
                    "type": "string",
                    "description": "Qdrant server URL",
                    "format": "uri"
                },
                "api_key": {
                    "type": "string",
                    "description": "API key for authentication",
                    "format": "password"
                },
                "collection": {
                    "type": "string",
                    "description": "Default collection name"
                },
                "timeout_ms": {
                    "type": "integer",
                    "default": 30000
                },
                "https": {
                    "type": "boolean",
                    "default": true
                }
            },
            "required": ["url"]
        }))
        .as_builtin()
}

/// GraphQL API resource type
pub fn graphql() -> ResourceType {
    ResourceType::new("graphql", "GraphQL API")
        .with_category(ResourceCategory::Api)
        .with_connection_kind(ConnectionKind::OnDemand)
        .with_icon("graphql")
        .with_schema(json!({
            "type": "object",
            "properties": {
                "endpoint": {
                    "type": "string",
                    "description": "GraphQL endpoint URL",
                    "format": "uri"
                },
                "auth_type": {
                    "type": "string",
                    "enum": ["none", "api_key", "bearer"],
                    "default": "none"
                },
                "api_key": {
                    "type": "string",
                    "format": "password"
                },
                "bearer_token": {
                    "type": "string",
                    "format": "password"
                },
                "headers": {
                    "type": "object",
                    "additionalProperties": { "type": "string" }
                }
            },
            "required": ["endpoint"]
        }))
        .as_builtin()
}

/// gRPC service resource type
pub fn grpc() -> ResourceType {
    ResourceType::new("grpc", "gRPC Service")
        .with_category(ResourceCategory::Api)
        .with_connection_kind(ConnectionKind::Persistent)
        .with_icon("grpc")
        .with_schema(json!({
            "type": "object",
            "properties": {
                "host": {
                    "type": "string",
                    "description": "gRPC server host"
                },
                "port": {
                    "type": "integer",
                    "default": 50051
                },
                "tls": {
                    "type": "boolean",
                    "default": false
                },
                "proto_path": {
                    "type": "string",
                    "description": "Path to .proto file or URL"
                },
                "service_name": {
                    "type": "string",
                    "description": "Service name in proto"
                },
                "metadata": {
                    "type": "object",
                    "additionalProperties": { "type": "string" },
                    "description": "gRPC metadata (headers)"
                }
            },
            "required": ["host"]
        }))
        .as_builtin()
}

/// S3-compatible object storage
pub fn s3() -> ResourceType {
    ResourceType::new("s3", "S3 / Object Storage")
        .with_category(ResourceCategory::Storage)
        .with_connection_kind(ConnectionKind::OnDemand)
        .with_icon("storage")
        .with_schema(json!({
            "type": "object",
            "properties": {
                "endpoint": {
                    "type": "string",
                    "description": "S3 endpoint (leave empty for AWS)"
                },
                "region": {
                    "type": "string",
                    "default": "us-east-1"
                },
                "access_key_id": {
                    "type": "string"
                },
                "secret_access_key": {
                    "type": "string",
                    "format": "password"
                },
                "bucket": {
                    "type": "string",
                    "description": "Default bucket"
                },
                "path_style": {
                    "type": "boolean",
                    "default": false,
                    "description": "Use path-style URLs (for MinIO)"
                }
            },
            "required": ["access_key_id", "secret_access_key"]
        }))
        .as_builtin()
}

/// Kafka message queue
pub fn kafka() -> ResourceType {
    ResourceType::new("kafka", "Apache Kafka")
        .with_category(ResourceCategory::Queue)
        .with_connection_kind(ConnectionKind::Persistent)
        .with_icon("queue")
        .with_schema(json!({
            "type": "object",
            "properties": {
                "brokers": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Kafka broker addresses"
                },
                "sasl_mechanism": {
                    "type": "string",
                    "enum": ["PLAIN", "SCRAM-SHA-256", "SCRAM-SHA-512"],
                    "description": "SASL mechanism"
                },
                "username": {
                    "type": "string"
                },
                "password": {
                    "type": "string",
                    "format": "password"
                },
                "ssl": {
                    "type": "boolean",
                    "default": false
                },
                "group_id": {
                    "type": "string",
                    "description": "Consumer group ID"
                }
            },
            "required": ["brokers"]
        }))
        .as_builtin()
}

/// MCP (Model Context Protocol) server resource type
pub fn mcp_server() -> ResourceType {
    ResourceType::new("mcp", "MCP Server")
        .with_category(ResourceCategory::Mcp)
        .with_connection_kind(ConnectionKind::Persistent)
        .with_icon("mcp")
        .with_schema(json!({
            "type": "object",
            "properties": {
                "transport": {
                    "type": "string",
                    "enum": ["stdio", "sse", "websocket"],
                    "default": "stdio",
                    "description": "Transport protocol"
                },
                "command": {
                    "type": "string",
                    "description": "Command to start MCP server (for stdio transport)"
                },
                "args": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Command arguments"
                },
                "url": {
                    "type": "string",
                    "description": "Server URL (for sse/websocket transport)",
                    "format": "uri"
                },
                "env": {
                    "type": "object",
                    "additionalProperties": { "type": "string" },
                    "description": "Environment variables for the server process"
                },
                "tools": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Allowed tools (empty = all)"
                },
                "timeout_ms": {
                    "type": "integer",
                    "default": 30000,
                    "description": "Tool call timeout"
                }
            },
            "oneOf": [
                { "required": ["command"] },
                { "required": ["url"] }
            ]
        }))
        .as_builtin()
}

/// LLM provider resource type
///
/// Stateless LLM calls. For full agents with memory and learning,
/// use the `agent` composite resource type.
pub fn llm() -> ResourceType {
    ResourceType::new("llm", "LLM Provider")
        .with_category(ResourceCategory::Ai)
        .with_connection_kind(ConnectionKind::OnDemand)
        .with_icon("brain")
        .with_schema(json!({
            "type": "object",
            "properties": {
                "provider": {
                    "type": "string",
                    "enum": ["anthropic", "openai", "ollama", "local"],
                    "default": "anthropic",
                    "description": "LLM provider"
                },
                "model": {
                    "type": "string",
                    "description": "Model identifier (e.g., claude-sonnet-4-20250514)"
                },
                "api_key": {
                    "type": "string",
                    "description": "API key for the provider (use api_key_ref instead for secret reference)",
                    "format": "password"
                },
                "api_key_ref": {
                    "type": "string",
                    "description": "Reference to a secret resource (namespace/name) containing the API key"
                },
                "api_url": {
                    "type": "string",
                    "description": "Custom API endpoint URL",
                    "format": "uri"
                },
                "system_prompt": {
                    "type": "string",
                    "description": "Default system prompt"
                },
                "max_tokens": {
                    "type": "integer",
                    "default": 4096,
                    "description": "Maximum tokens in response"
                },
                "temperature": {
                    "type": "number",
                    "default": 0.7,
                    "minimum": 0,
                    "maximum": 2
                }
            },
            "required": ["provider", "model"]
        }))
        .as_builtin()
}

/// ADB (Agent Database) — agent memory over AQL
///
/// Memory types WORKING, EPISODIC, PROCEDURAL, SEMANTIC and TOOLS. FlowR's
/// `call:adb` task and the DB console send AQL to `POST {url}/query`; agents can
/// also use ADB's MCP server at `{url}/mcp/sse` through an `mcp` resource.
/// Replaces the `clawdb` type (still accepted by `call:clawdb`).
pub fn adb() -> ResourceType {
    ResourceType::new("adb", "ADB (Agent Memory)")
        .with_category(ResourceCategory::Memory)
        .with_connection_kind(ConnectionKind::Persistent)
        .with_icon("memory")
        .with_schema(json!({
            "type": "object",
            "properties": {
                "url": {
                    "type": "string",
                    "description": "ADB server URL, e.g. http://adb:3000",
                    "format": "uri"
                },
                "namespace": {
                    "type": "string",
                    "description": "Agent namespace for multi-agent isolation"
                },
                "scope": {
                    "type": "string",
                    "enum": ["private", "shared", "cluster"],
                    "default": "private",
                    "description": "Memory isolation scope"
                },
                "default_ttl": {
                    "type": "string",
                    "description": "Default TTL for WORKING memory (e.g. '30s')"
                }
            },
            "required": ["url"]
        }))
        .as_builtin()
}

/// Input channel resource type
///
/// Sources of events for agents: streams, webhooks, files.
pub fn input_channel() -> ResourceType {
    ResourceType::new("input_channel", "Input Channel")
        .with_category(ResourceCategory::Channel)
        .with_connection_kind(ConnectionKind::Persistent)
        .with_icon("input")
        .with_schema(json!({
            "type": "object",
            "properties": {
                "type": {
                    "type": "string",
                    "enum": ["iggy", "kafka", "webhook", "file", "stdin"],
                    "description": "Channel type"
                },
                "topic": {
                    "type": "string",
                    "description": "Topic/stream name (for iggy/kafka)"
                },
                "consumer_group": {
                    "type": "string",
                    "description": "Consumer group ID"
                },
                "path": {
                    "type": "string",
                    "description": "File path (for file type)"
                },
                "format": {
                    "type": "string",
                    "enum": ["json", "text", "binary"],
                    "default": "json"
                },
                "batch_size": {
                    "type": "integer",
                    "default": 1,
                    "description": "Batch size for reading"
                }
            },
            "required": ["type"]
        }))
        .as_builtin()
}

/// Output channel resource type
///
/// Destinations for agent actions: streams, webhooks, notifications.
pub fn output_channel() -> ResourceType {
    ResourceType::new("output_channel", "Output Channel")
        .with_category(ResourceCategory::Channel)
        .with_connection_kind(ConnectionKind::Persistent)
        .with_icon("output")
        .with_schema(json!({
            "type": "object",
            "properties": {
                "type": {
                    "type": "string",
                    "enum": ["iggy", "kafka", "webhook", "file", "stdout", "slack", "discord"],
                    "description": "Channel type"
                },
                "topic": {
                    "type": "string",
                    "description": "Topic/stream name (for iggy/kafka)"
                },
                "url": {
                    "type": "string",
                    "description": "Webhook URL",
                    "format": "uri"
                },
                "path": {
                    "type": "string",
                    "description": "File path (for file type)"
                },
                "format": {
                    "type": "string",
                    "enum": ["json", "text", "binary"],
                    "default": "json"
                }
            },
            "required": ["type"]
        }))
        .as_builtin()
}

/// Agent resource type (composite)
///
/// A complete autonomous agent: LLM + Memory (ADB) + Input/Output + Tools.
/// This is NOT just an LLM call - for simple LLM calls use the `llm` resource.
///
/// Agents have:
/// - Memory (ADB with 4 cognitive types: WORKING, SEMANTIC, PROCEDURAL, EPISODIC)
/// - Learning (ontology building via LINK, procedural variable tuning)
/// - Input channels (event streams, webhooks)
/// - Output channels (actions, notifications)
/// - Tools (MCP servers, native tools)
pub fn agent() -> ResourceType {
    ResourceType::new("agent", "AI Agent")
        .with_category(ResourceCategory::Agent)
        .with_connection_kind(ConnectionKind::Persistent)
        .with_icon("agent")
        .with_schema(json!({
            "type": "object",
            "properties": {
                "namespace": {
                    "type": "string",
                    "description": "Agent namespace for identification"
                },
                "description": {
                    "type": "string",
                    "description": "Agent description"
                },
                "llm": {
                    "type": "string",
                    "description": "Reference to LLM resource for reasoning"
                },
                "memory": {
                    "type": "string",
                    "description": "Reference to ADB resource for memory"
                },
                "inputs": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "References to input_channel resources"
                },
                "outputs": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "References to output_channel resources"
                },
                "tools": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Tool names or references"
                },
                "mcp_servers": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "References to MCP server resources"
                },
                "system_prompt": {
                    "type": "string",
                    "description": "System prompt for the agent"
                },
                "max_iterations": {
                    "type": "integer",
                    "default": 10,
                    "description": "Maximum agentic loop iterations"
                },
                "timeout": {
                    "type": "string",
                    "default": "30s",
                    "description": "Agent execution timeout"
                }
            },
            "required": ["llm"]
        }))
        .as_builtin()
}

/// Human approval resource type
///
/// Human-in-the-loop for approvals, decisions, and input collection.
/// Supports multiple channels: webhook, Slack, Discord, form UI.
pub fn human() -> ResourceType {
    ResourceType::new("human", "Human Approval")
        .with_category(ResourceCategory::Agent)
        .with_connection_kind(ConnectionKind::OnDemand)
        .with_icon("user")
        .with_schema(json!({
            "type": "object",
            "properties": {
                "channel": {
                    "type": "string",
                    "enum": ["webhook", "slack", "discord", "form"],
                    "default": "webhook",
                    "description": "Channel for human interaction"
                },
                "webhook_url": {
                    "type": "string",
                    "format": "uri",
                    "description": "Webhook URL for approval requests"
                },
                "callback_url": {
                    "type": "string",
                    "format": "uri",
                    "description": "URL where human response will be sent"
                },
                "slack_channel": {
                    "type": "string",
                    "description": "Slack channel ID for notifications"
                },
                "slack_token": {
                    "type": "string",
                    "description": "Slack bot token"
                },
                "discord_webhook": {
                    "type": "string",
                    "format": "uri",
                    "description": "Discord webhook URL"
                },
                "timeout": {
                    "type": "string",
                    "default": "24h",
                    "description": "Timeout for human response"
                },
                "reminder_interval": {
                    "type": "string",
                    "default": "1h",
                    "description": "Interval for sending reminders"
                },
                "escalation_after": {
                    "type": "string",
                    "description": "Escalate if no response after this duration"
                },
                "escalation_channel": {
                    "type": "string",
                    "description": "Channel for escalation notifications"
                }
            },
            "required": ["channel"]
        }))
        .as_builtin()
}

/// Secret resource type — stores a single sensitive value (API key, token, password)
pub fn secret() -> ResourceType {
    ResourceType::new("secret", "Secret")
        .with_category(ResourceCategory::Security)
        .with_connection_kind(ConnectionKind::OnDemand)
        .with_icon("lock")
        .with_schema(json!({
            "type": "object",
            "properties": {
                "value": {
                    "type": "string",
                    "format": "password",
                    "description": "The secret value (API key, token, password)"
                },
                "description": {
                    "type": "string",
                    "description": "What this secret is for"
                }
            },
            "required": ["value"]
        }))
        .as_builtin()
}

/// Credential resource type — stores a username/password or token pair
pub fn credential() -> ResourceType {
    ResourceType::new("credential", "Credential")
        .with_category(ResourceCategory::Security)
        .with_connection_kind(ConnectionKind::OnDemand)
        .with_icon("key")
        .with_schema(json!({
            "type": "object",
            "properties": {
                "auth_type": {
                    "type": "string",
                    "enum": ["basic", "bearer", "api_key"],
                    "default": "bearer",
                    "description": "Authentication type"
                },
                "username": {
                    "type": "string",
                    "description": "Username (for basic auth)"
                },
                "password": {
                    "type": "string",
                    "format": "password",
                    "description": "Password (for basic auth)"
                },
                "token": {
                    "type": "string",
                    "format": "password",
                    "description": "Token or API key value"
                },
                "description": {
                    "type": "string",
                    "description": "What this credential is for"
                }
            },
            "required": ["auth_type"]
        }))
        .as_builtin()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_all_builtins_valid() {
        let types = builtin_types();
        assert!(!types.is_empty());

        for rt in types {
            assert!(rt.builtin, "{} should be marked as builtin", rt.name);
            assert!(!rt.name.is_empty());
            assert!(!rt.description.is_empty());
            // Schema should be a valid JSON object
            assert!(rt.config_schema.is_object());
        }
    }

    #[test]
    fn test_mysql_schema() {
        let mysql = mysql();
        let schema = &mysql.config_schema;

        // Check required fields exist
        let required = schema["required"].as_array().unwrap();
        assert!(required.iter().any(|v| v == "host"));
        assert!(required.iter().any(|v| v == "database"));
    }
}
