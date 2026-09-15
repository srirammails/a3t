//! Suspension types

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::event::EventFilter;

/// Type of suspension
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SuspensionType {
    /// Wait for a duration
    Wait {
        /// When to resume
        resume_at: DateTime<Utc>,
        /// Original duration string (for logging)
        duration: String,
    },

    /// Listen for events
    Listen {
        /// Event filters to match
        filters: Vec<EventFilter>,
        /// Consumption strategy
        strategy: ConsumptionStrategy,
        /// Optional timeout
        timeout_at: Option<DateTime<Utc>>,
    },
}

/// Event consumption strategy
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ConsumptionStrategy {
    /// Wait for all events (AND)
    #[default]
    All,
    /// Wait for any event (OR)
    Any,
    /// Wait for exactly one event
    One,
}

/// A suspended flow instance
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SuspendedFlow {
    /// Unique instance ID
    pub instance_id: Uuid,

    /// Workflow namespace
    pub namespace: String,

    /// Workflow name
    pub name: String,

    /// Workflow version
    pub version: String,

    /// Current step name
    pub step_name: String,

    /// Suspension type and details
    pub suspension: SuspensionType,

    /// Context at time of suspension
    pub context: Value,

    /// When the flow was suspended
    pub suspended_at: DateTime<Utc>,

    /// Correlation ID for tracing
    pub correlation_id: Option<String>,
}

impl SuspendedFlow {
    /// Create a new Wait suspension
    pub fn wait(
        instance_id: Uuid,
        namespace: String,
        name: String,
        version: String,
        step_name: String,
        resume_at: DateTime<Utc>,
        duration: String,
        context: Value,
    ) -> Self {
        Self {
            instance_id,
            namespace,
            name,
            version,
            step_name,
            suspension: SuspensionType::Wait { resume_at, duration },
            context,
            suspended_at: Utc::now(),
            correlation_id: None,
        }
    }

    /// Create a new Listen suspension
    pub fn listen(
        instance_id: Uuid,
        namespace: String,
        name: String,
        version: String,
        step_name: String,
        filters: Vec<EventFilter>,
        strategy: ConsumptionStrategy,
        timeout_at: Option<DateTime<Utc>>,
        context: Value,
    ) -> Self {
        Self {
            instance_id,
            namespace,
            name,
            version,
            step_name,
            suspension: SuspensionType::Listen {
                filters,
                strategy,
                timeout_at,
            },
            context,
            suspended_at: Utc::now(),
            correlation_id: None,
        }
    }

    /// Check if this is a Wait suspension
    pub fn is_wait(&self) -> bool {
        matches!(self.suspension, SuspensionType::Wait { .. })
    }

    /// Check if this is a Listen suspension
    pub fn is_listen(&self) -> bool {
        matches!(self.suspension, SuspensionType::Listen { .. })
    }

    /// Get the resume time for Wait suspensions
    pub fn resume_at(&self) -> Option<DateTime<Utc>> {
        match &self.suspension {
            SuspensionType::Wait { resume_at, .. } => Some(*resume_at),
            SuspensionType::Listen { timeout_at, .. } => *timeout_at,
        }
    }

    /// Get event filters for Listen suspensions
    pub fn event_filters(&self) -> Option<&[EventFilter]> {
        match &self.suspension {
            SuspensionType::Listen { filters, .. } => Some(filters),
            _ => None,
        }
    }

    /// Workflow key
    pub fn workflow_key(&self) -> String {
        format!("{}/{}", self.namespace, self.name)
    }
}
