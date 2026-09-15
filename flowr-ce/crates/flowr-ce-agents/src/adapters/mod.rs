//! ZeroClaw integration adapters
//!
//! Bridges between ClawdFlow resources and ZeroClaw's trait system:
//! - MCP tools → ZeroClaw `Tool` trait
//! - ClawDB memory → ZeroClaw `Memory` trait
//! - ClawdFlow LLM resource → ZeroClaw `Provider` factory

pub mod mcp_tool;
pub mod clawdb_memory;
