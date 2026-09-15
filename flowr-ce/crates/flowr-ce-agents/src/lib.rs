//! ClawdFlow Agents — CNCF workflow agents, triggers, and lifecycle management
//!
//! This crate provides:
//! - Agent types and configuration (`types`)
//! - Trigger system (`triggers`) — first-class webhook/cron/event triggers
//! - Template registry (`templates`) — versioned agent blueprints
//! - Human-in-the-loop input system (`inputs`)

pub mod types;
pub mod triggers;
pub mod templates;
pub mod inputs;

// Re-exports
pub use types::*;
pub use triggers::{trigger_manager, TriggerManager};
pub use triggers::types::{Trigger, TriggerConfig, TriggerTarget, TriggerStatus, TriggerDeliveryResult};
pub use templates::{template_manager, TemplateManager, AgentTemplate, TemplateSummary};
pub use inputs::{input_manager, InputManager, InputRequest, InputResponse, InputStatus};
