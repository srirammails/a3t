//! Flow durability - checkpoint behavior

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

/// Flow durability mode
///
/// Controls when checkpoints are written to Iggy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum FlowDurability {
    /// Write to Iggy only at workflow completion (fire-and-forget)
    ///
    /// - Fastest execution (~1µs async append)
    /// - No crash recovery mid-workflow
    /// - Suitable for idempotent, short-lived flows
    #[default]
    Completion,

    /// Write to Iggy at steps marked as checkpoints
    ///
    /// - Sync write at checkpoints (~1ms with ack)
    /// - Crash recovery resumes from last checkpoint
    /// - Suitable for long-running or non-idempotent flows
    Checkpoint,
}

impl FlowDurability {
    /// Check if this mode supports mid-workflow recovery
    pub fn supports_recovery(&self) -> bool {
        matches!(self, Self::Checkpoint)
    }
}

/// Checkpoint data persisted to Iggy
///
/// Represents the state of a workflow at a specific point in execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Checkpoint {
    /// Unique workflow instance ID
    pub instance_id: Uuid,

    /// Workflow definition identifier (namespace/name:version)
    pub workflow_id: String,

    /// Position in the workflow (e.g., "do/step1/fork/branch2")
    pub step_position: String,

    /// Workflow context at this checkpoint
    pub context: Value,

    /// Task output at this checkpoint (for memoization)
    pub output: Option<Value>,

    /// When this checkpoint was created
    pub timestamp: DateTime<Utc>,

    /// Sequence number for ordering
    pub sequence: u64,
}

impl Checkpoint {
    /// Create a new checkpoint
    pub fn new(
        instance_id: Uuid,
        workflow_id: impl Into<String>,
        step_position: impl Into<String>,
        context: Value,
        sequence: u64,
    ) -> Self {
        Self {
            instance_id,
            workflow_id: workflow_id.into(),
            step_position: step_position.into(),
            context,
            output: None,
            timestamp: Utc::now(),
            sequence,
        }
    }

    /// Create checkpoint with output (for memoization)
    pub fn with_output(mut self, output: Value) -> Self {
        self.output = Some(output);
        self
    }

    /// Serialize to JSON bytes for Iggy
    pub fn to_bytes(&self) -> Result<Vec<u8>, serde_json::Error> {
        serde_json::to_vec(self)
    }

    /// Deserialize from JSON bytes
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, serde_json::Error> {
        serde_json::from_slice(bytes)
    }
}

/// Checkpoint metadata stored in Dragonfly for fast lookup
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckpointIndex {
    /// Instance ID
    pub instance_id: Uuid,

    /// Last checkpoint position
    pub last_position: String,

    /// Last checkpoint sequence
    pub last_sequence: u64,

    /// Iggy partition where checkpoints are stored
    pub partition_id: u32,

    /// Iggy offset of last checkpoint
    pub last_offset: u64,
}
