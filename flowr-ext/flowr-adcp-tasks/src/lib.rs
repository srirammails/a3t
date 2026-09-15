//! ADCP Protocol Tasks for ClawdFlow
//!
//! Extension crate providing task executors for the Ad Campaign Protocol (ADCP) demo.
//!
//! # Task Types
//!
//! ## Vector Operations
//! | Task | Purpose | Module |
//! |------|---------|--------|
//! | **call:lancedb** | LanceDB vector search/upsert | [`call_lancedb`] |
//! | **call:embed** | Generate embeddings via API | [`call_embed`] |
//!
//! ## Signal Processing
//! | Task | Purpose | Module |
//! |------|---------|--------|
//! | **call:bloom** | Bloom filter operations | [`call_bloom`] |
//!
//! # Example Usage
//!
//! ```yaml
//! # Vector similarity search
//! - name: find-similar-urls
//!   type: call:lancedb
//!   resource: url-vectors
//!   config:
//!     operation: search
//!     table: semantic_urls
//!     vector: ${ .embedding }
//!     limit: 50
//!     filter: "quality_score > 0.7"
//!
//! # Generate embedding
//! - name: embed-text
//!   type: call:embed
//!   resource: embedding-api
//!   config:
//!     model: all-MiniLM-L6-v2
//!     input: ${ .campaign_brief }
//!
//! # Bloom filter check
//! - name: check-signal
//!   type: call:bloom
//!   resource: signal-filter
//!   config:
//!     operation: check
//!     key: ${ .url_hash }
//! ```

// Task modules
mod call_bloom;
mod call_embed;
mod call_lancedb;

// Resource helpers (shared utilities)
mod resource;

// Re-export task executors
pub use call_bloom::{BloomConfig, BloomExecutor, BloomOperation};
pub use call_embed::{EmbedConfig, EmbedExecutor, EmbedModel};
pub use call_lancedb::{LanceDbConfig, LanceDbExecutor, LanceDbOperation};

// Re-export core types for convenience
pub use flowr_ce_core::{FlowDirective, FlowError, FlowResult, TaskContext, TaskExecutor, TaskResult};

/// Register all ADCP task executors with the ClawdFlow runtime.
///
/// Call this during worker initialization to make ADCP tasks available.
///
/// # Example
///
/// ```ignore
/// use flowr_adcp_tasks::register_adcp_tasks;
///
/// // During worker setup
/// register_adcp_tasks(&mut task_registry);
/// ```
pub fn task_types() -> Vec<&'static str> {
    vec!["call:lancedb", "call:bloom", "call:embed"]
}
