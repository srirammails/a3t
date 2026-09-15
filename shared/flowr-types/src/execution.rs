//! Execution types for workflow instances

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

/// Execution request message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionRequest {
    /// Unique instance ID for this execution
    #[serde(default = "generate_uuid")]
    pub instance_id: Uuid,

    /// Workflow namespace
    pub namespace: String,

    /// Workflow name
    pub name: String,

    /// Workflow version (optional, uses active version if not specified)
    #[serde(default)]
    pub version: Option<String>,

    /// Input data for the workflow
    #[serde(default)]
    pub input: Value,

    /// Requested durability mode
    #[serde(default)]
    pub durability: Option<String>,

    /// Request timestamp
    #[serde(default = "now_utc")]
    pub requested_at: DateTime<Utc>,

    /// Caller/source identifier
    #[serde(default)]
    pub caller: Option<String>,

    /// The initiating principal (E0.4b/E2): who started this flow — a JWT user,
    /// an acting agent, or an internal service. Carried through the queue so the
    /// worker can set it on the root `ExecutionContext` (instead of the
    /// `Service{"flow-runtime"}` default), letting the policy hook gate the
    /// top-level flow as its real caller. `None` → the runtime-service default.
    #[serde(default)]
    pub principal: Option<flowr_policy::Principal>,
}

fn generate_uuid() -> Uuid {
    Uuid::now_v7()
}

fn now_utc() -> DateTime<Utc> {
    Utc::now()
}

impl ExecutionRequest {
    /// Create a new execution request
    pub fn new(namespace: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            instance_id: generate_uuid(),
            namespace: namespace.into(),
            name: name.into(),
            version: None,
            input: Value::Null,
            durability: None,
            requested_at: now_utc(),
            caller: None,
            principal: None,
        }
    }

    /// Set input data
    pub fn with_input(mut self, input: Value) -> Self {
        self.input = input;
        self
    }

    /// Set the initiating principal (who started this flow).
    pub fn with_principal(mut self, principal: flowr_policy::Principal) -> Self {
        self.principal = Some(principal);
        self
    }

    /// Set version
    pub fn with_version(mut self, version: impl Into<String>) -> Self {
        self.version = Some(version.into());
        self
    }

    /// Get workflow key
    pub fn workflow_key(&self) -> String {
        format!("{}/{}", self.namespace, self.name)
    }
}

/// Execution status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ExecutionStatus {
    /// Queued for execution
    Queued,
    /// Currently running
    Running,
    /// Completed successfully
    Completed,
    /// Failed with error
    Failed,
    /// Cancelled by user
    Cancelled,
    /// Suspended (waiting for external event)
    Suspended,
}

impl Default for ExecutionStatus {
    fn default() -> Self {
        Self::Queued
    }
}

/// Execution state for a workflow instance
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionState {
    /// Instance ID
    pub instance_id: Uuid,

    /// Workflow namespace
    pub namespace: String,

    /// Workflow name
    pub name: String,

    /// Workflow version
    pub version: String,

    /// Current status
    pub status: ExecutionStatus,

    /// Input data
    pub input: Value,

    /// Output data (if completed)
    #[serde(default)]
    pub output: Option<Value>,

    /// Error message (if failed)
    #[serde(default)]
    pub error: Option<String>,

    /// Current step (if running)
    #[serde(default)]
    pub current_step: Option<String>,

    /// Started timestamp
    pub started_at: DateTime<Utc>,

    /// Completed timestamp
    #[serde(default)]
    pub completed_at: Option<DateTime<Utc>>,

    /// Duration in microseconds
    #[serde(default)]
    pub duration_us: Option<u64>,

    /// The identity this run was started as, carried over from the
    /// `ExecutionRequest`. Until this existed the execution history could not
    /// answer "who ran this flow" — the principal lived only on the in-flight
    /// request and was dropped at persist time, which left chat transcripts (and
    /// every other run) unattributable after the fact.
    ///
    /// `None` means "not recorded", which is the case for every execution written
    /// before this field existed. It deliberately does not default to the runtime
    /// service principal: unknown must never be readable as a positive claim about
    /// who acted. The whole struct is persisted as JSON in one `state_data`
    /// column, so adding it needs no table migration and old rows still load.
    #[serde(default)]
    pub principal: Option<flowr_policy::Principal>,
}

impl ExecutionState {
    /// Create a new execution state
    pub fn new(request: &ExecutionRequest, version: impl Into<String>) -> Self {
        Self {
            instance_id: request.instance_id,
            namespace: request.namespace.clone(),
            name: request.name.clone(),
            version: version.into(),
            status: ExecutionStatus::Running,
            input: request.input.clone(),
            output: None,
            error: None,
            current_step: None,
            started_at: Utc::now(),
            completed_at: None,
            duration_us: None,
            principal: request.principal.clone(),
        }
    }

    /// Mark as completed
    pub fn complete(mut self, output: Value, duration_us: u64) -> Self {
        self.status = ExecutionStatus::Completed;
        self.output = Some(output);
        self.completed_at = Some(Utc::now());
        self.duration_us = Some(duration_us);
        self
    }

    /// Mark as failed
    pub fn fail(mut self, error: impl Into<String>) -> Self {
        self.status = ExecutionStatus::Failed;
        self.error = Some(error.into());
        self.completed_at = Some(Utc::now());
        self
    }
}

/// Checkpoint data for resumable workflows
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Checkpoint {
    /// Instance ID
    pub instance_id: Uuid,

    /// Workflow ID
    pub workflow_id: String,

    /// Position in workflow (e.g., "do/step1")
    pub position: String,

    /// Step/task index where checkpoint was taken
    pub task_index: usize,

    /// Step name
    pub step_name: String,

    /// Accumulated context at checkpoint
    pub context: Value,

    /// Output at checkpoint
    pub output: Value,

    /// Sequence number for ordering
    pub sequence: u64,

    /// Timestamp
    pub created_at: DateTime<Utc>,
}

impl Checkpoint {
    /// Create a new checkpoint
    pub fn new(
        instance_id: Uuid,
        workflow_id: impl Into<String>,
        position: impl Into<String>,
        context: Value,
        output: Value,
        task_index: usize,
        sequence: u64,
    ) -> Self {
        let pos = position.into();
        let step_name = pos.split('/').last().unwrap_or("unknown").to_string();
        Self {
            instance_id,
            workflow_id: workflow_id.into(),
            position: pos,
            task_index,
            step_name,
            context,
            output,
            sequence,
            created_at: Utc::now(),
        }
    }

    /// Simple constructor for backward compatibility
    pub fn simple(instance_id: Uuid, step_index: usize, step_name: impl Into<String>, context: Value) -> Self {
        Self {
            instance_id,
            workflow_id: String::new(),
            position: String::new(),
            task_index: step_index,
            step_name: step_name.into(),
            context,
            output: Value::Null,
            sequence: 0,
            created_at: Utc::now(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The whole `ExecutionState` is persisted as JSON in one `state_data`
    /// column, so "who ran this" only survives if the field round-trips.
    #[test]
    fn execution_state_carries_the_requesting_principal() {
        let who = flowr_policy::Principal::human("u-123", vec!["SuperAdmin".into()]);
        let req = ExecutionRequest::new("a3t", "a3t-chat-engine")
            .with_principal(who.clone());
        let state = ExecutionState::new(&req, "1.0.0");
        assert_eq!(state.principal.as_ref().map(|p| p.id()), Some("u-123"));

        let json = serde_json::to_string(&state).unwrap();
        let back: ExecutionState = serde_json::from_str(&json).unwrap();
        // `Principal` has no PartialEq, so compare the serialized identity — which
        // is what actually has to survive the state_data column.
        assert_eq!(
            serde_json::to_value(&back.principal).unwrap(),
            serde_json::to_value(&Some(who)).unwrap(),
            "principal must survive persistence",
        );
        assert_eq!(back.principal.as_ref().map(|p| p.kind()), Some("human"));
    }

    /// Rows written before the field existed must still load, and must read as
    /// "not recorded" rather than being attributed to anyone.
    #[test]
    fn a_pre_existing_row_without_a_principal_still_loads_as_unknown() {
        let legacy = serde_json::json!({
            "instance_id": uuid::Uuid::nil(),
            "namespace": "a3t",
            "name": "a3t-chat-engine",
            "version": "1.0.0",
            "status": "completed",
            "input": { "message": "hi" },
            "started_at": "2026-08-04T11:44:55Z"
        });
        let back: ExecutionState = serde_json::from_value(legacy).unwrap();
        assert!(back.principal.is_none(),
                "an unrecorded principal must stay unknown, never default to an identity");
    }

    /// A run with no caller identity stays `None` — it must not silently acquire
    /// the runtime's own service principal.
    #[test]
    fn a_request_without_a_principal_records_none() {
        let state = ExecutionState::new(&ExecutionRequest::new("a3t", "f"), "1.0.0");
        assert!(state.principal.is_none());
    }
}
