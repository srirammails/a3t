//! Lifecycle event emitter
//!
//! Emits standardized CloudEvents for workflow and task state changes.
//! Events are published to Iggy for audit, observability, and external integrations.

use std::str::FromStr;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use iggy::client::MessageClient;
use iggy::clients::client::IggyClient;
use iggy::identifier::Identifier;
use iggy::messages::send_messages::{Message, Partitioning};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tracing::{debug, instrument};
use uuid::Uuid;

/// Iggy topic for lifecycle events
pub const LIFECYCLE_EVENTS_TOPIC: &str = "events";

/// CNCF Serverless Workflow event type prefixes
pub mod event_types {
    /// Workflow event prefix
    pub const WORKFLOW_PREFIX: &str = "io.serverlessworkflow.workflow";
    /// Task event prefix
    pub const TASK_PREFIX: &str = "io.serverlessworkflow.task";

    // Workflow events
    pub const WORKFLOW_STARTED: &str = "io.serverlessworkflow.workflow.started.v1";
    pub const WORKFLOW_SUSPENDED: &str = "io.serverlessworkflow.workflow.suspended.v1";
    pub const WORKFLOW_RESUMED: &str = "io.serverlessworkflow.workflow.resumed.v1";
    pub const WORKFLOW_CANCELLED: &str = "io.serverlessworkflow.workflow.cancelled.v1";
    pub const WORKFLOW_FAULTED: &str = "io.serverlessworkflow.workflow.faulted.v1";
    pub const WORKFLOW_COMPLETED: &str = "io.serverlessworkflow.workflow.completed.v1";

    // Task events
    pub const TASK_CREATED: &str = "io.serverlessworkflow.task.created.v1";
    pub const TASK_STARTED: &str = "io.serverlessworkflow.task.started.v1";
    pub const TASK_SUSPENDED: &str = "io.serverlessworkflow.task.suspended.v1";
    pub const TASK_RESUMED: &str = "io.serverlessworkflow.task.resumed.v1";
    pub const TASK_RETRIED: &str = "io.serverlessworkflow.task.retried.v1";
    pub const TASK_SKIPPED: &str = "io.serverlessworkflow.task.skipped.v1";
    pub const TASK_CANCELLED: &str = "io.serverlessworkflow.task.cancelled.v1";
    pub const TASK_FAULTED: &str = "io.serverlessworkflow.task.faulted.v1";
    pub const TASK_COMPLETED: &str = "io.serverlessworkflow.task.completed.v1";
}

/// Lifecycle CloudEvent
///
/// Follows the CloudEvents specification v1.0
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LifecycleEvent {
    /// Event ID (UUIDv7)
    pub id: String,

    /// Event type (e.g., "io.serverlessworkflow.workflow.started.v1")
    #[serde(rename = "type")]
    pub event_type: String,

    /// Event source (e.g., "flowr://namespace/workflow-name")
    pub source: String,

    /// Event subject (instance ID)
    pub subject: String,

    /// CloudEvents spec version
    pub specversion: String,

    /// Data content type
    pub datacontenttype: String,

    /// Event timestamp
    pub time: DateTime<Utc>,

    /// Event data
    pub data: LifecycleEventData,
}

impl LifecycleEvent {
    /// Create a new lifecycle event
    pub fn new(
        event_type: impl Into<String>,
        source: impl Into<String>,
        subject: impl Into<String>,
        data: LifecycleEventData,
    ) -> Self {
        Self {
            id: Uuid::now_v7().to_string(),
            event_type: event_type.into(),
            source: source.into(),
            subject: subject.into(),
            specversion: "1.0".to_string(),
            datacontenttype: "application/json".to_string(),
            time: Utc::now(),
            data,
        }
    }

    /// Serialize to JSON bytes for Iggy
    pub fn to_bytes(&self) -> Result<Vec<u8>, serde_json::Error> {
        serde_json::to_vec(self)
    }
}

/// Event data payload
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum LifecycleEventData {
    /// Workflow event data
    Workflow(WorkflowEventData),
    /// Task event data
    Task(TaskEventData),
}

/// Workflow-specific event data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowEventData {
    /// Workflow instance ID
    pub instance_id: String,

    /// Workflow namespace
    pub namespace: String,

    /// Workflow name
    pub name: String,

    /// Workflow version
    pub version: String,

    /// Workflow input (for started events)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input: Option<Value>,

    /// Workflow output (for completed events)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output: Option<Value>,

    /// Error details (for faulted events)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<ErrorData>,

    /// Suspension details (for suspended events)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suspension: Option<SuspensionData>,

    /// Duration in microseconds (for completed events)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_us: Option<u64>,
}

impl WorkflowEventData {
    /// Create new workflow event data
    pub fn new(
        instance_id: impl Into<String>,
        namespace: impl Into<String>,
        name: impl Into<String>,
        version: impl Into<String>,
    ) -> Self {
        Self {
            instance_id: instance_id.into(),
            namespace: namespace.into(),
            name: name.into(),
            version: version.into(),
            input: None,
            output: None,
            error: None,
            suspension: None,
            duration_us: None,
        }
    }

    /// Set workflow input
    pub fn with_input(mut self, input: Value) -> Self {
        self.input = Some(input);
        self
    }

    /// Set workflow output
    pub fn with_output(mut self, output: Value) -> Self {
        self.output = Some(output);
        self
    }

    /// Set error details
    pub fn with_error(mut self, error: ErrorData) -> Self {
        self.error = Some(error);
        self
    }

    /// Set suspension details
    pub fn with_suspension(mut self, suspension: SuspensionData) -> Self {
        self.suspension = Some(suspension);
        self
    }

    /// Set duration in microseconds
    pub fn with_duration(mut self, duration_us: u64) -> Self {
        self.duration_us = Some(duration_us);
        self
    }
}

/// Task-specific event data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskEventData {
    /// Workflow instance ID
    pub instance_id: String,

    /// Workflow namespace
    pub namespace: String,

    /// Workflow name
    pub workflow_name: String,

    /// Task name
    pub task_name: String,

    /// Task type (e.g., "call:http", "set", "fork")
    pub task_type: String,

    /// Task position in workflow
    pub position: String,

    /// Task input (for started events)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input: Option<Value>,

    /// Task output (for completed events)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output: Option<Value>,

    /// Error details (for faulted events)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<ErrorData>,

    /// Retry attempt number (for retried events)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retry_attempt: Option<u32>,

    /// Duration in microseconds
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_us: Option<u64>,
}

impl TaskEventData {
    /// Create new task event data
    pub fn new(
        instance_id: impl Into<String>,
        namespace: impl Into<String>,
        workflow_name: impl Into<String>,
        task_name: impl Into<String>,
        task_type: impl Into<String>,
        position: impl Into<String>,
    ) -> Self {
        Self {
            instance_id: instance_id.into(),
            namespace: namespace.into(),
            workflow_name: workflow_name.into(),
            task_name: task_name.into(),
            task_type: task_type.into(),
            position: position.into(),
            input: None,
            output: None,
            error: None,
            retry_attempt: None,
            duration_us: None,
        }
    }

    /// Set task input
    pub fn with_input(mut self, input: Value) -> Self {
        self.input = Some(input);
        self
    }

    /// Set task output
    pub fn with_output(mut self, output: Value) -> Self {
        self.output = Some(output);
        self
    }

    /// Set error details
    pub fn with_error(mut self, error: ErrorData) -> Self {
        self.error = Some(error);
        self
    }

    /// Set retry attempt
    pub fn with_retry(mut self, attempt: u32) -> Self {
        self.retry_attempt = Some(attempt);
        self
    }

    /// Set duration in microseconds
    pub fn with_duration(mut self, duration_us: u64) -> Self {
        self.duration_us = Some(duration_us);
        self
    }
}

/// Error details for faulted events
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorData {
    /// Error type/code
    #[serde(rename = "type")]
    pub error_type: String,

    /// Error message
    pub message: String,

    /// Additional error details
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<Value>,
}

impl ErrorData {
    /// Create new error data
    pub fn new(error_type: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            error_type: error_type.into(),
            message: message.into(),
            details: None,
        }
    }

    /// Add error details
    pub fn with_details(mut self, details: Value) -> Self {
        self.details = Some(details);
        self
    }
}

/// Suspension details
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SuspensionData {
    /// Suspension type ("wait" or "listen")
    #[serde(rename = "type")]
    pub suspension_type: String,

    /// Resume at time (for wait suspensions)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resume_at: Option<DateTime<Utc>>,

    /// Event filters (for listen suspensions)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub event_filters: Option<Vec<String>>,
}

impl SuspensionData {
    /// Create wait suspension data
    pub fn wait(resume_at: DateTime<Utc>) -> Self {
        Self {
            suspension_type: "wait".to_string(),
            resume_at: Some(resume_at),
            event_filters: None,
        }
    }

    /// Create listen suspension data
    pub fn listen(event_filters: Vec<String>) -> Self {
        Self {
            suspension_type: "listen".to_string(),
            resume_at: None,
            event_filters: Some(event_filters),
        }
    }
}

/// Lifecycle event emitter
///
/// Publishes CloudEvents to Iggy for workflow and task state transitions.
#[derive(Clone)]
pub struct LifecycleEventEmitter {
    /// Iggy client
    iggy: Arc<IggyClient>,

    /// Source URI base (e.g., "flowr://")
    source_base: String,

    /// Whether event emission is enabled
    enabled: bool,
}

impl LifecycleEventEmitter {
    /// Create a new lifecycle event emitter
    pub fn new(iggy: Arc<IggyClient>) -> Self {
        Self {
            iggy,
            source_base: "flowr://".to_string(),
            enabled: true,
        }
    }

    /// Set the source base URI
    pub fn with_source_base(mut self, base: impl Into<String>) -> Self {
        self.source_base = base.into();
        self
    }

    /// Enable or disable event emission
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    /// Check if event emission is enabled
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// Build source URI for a workflow
    fn workflow_source(&self, namespace: &str, name: &str) -> String {
        format!("{}{}/{}", self.source_base, namespace, name)
    }

    /// Calculate partition ID from instance ID
    fn partition_for_instance(instance_id: &Uuid) -> u32 {
        const PARTITION_COUNT: u32 = 8;
        let bytes = instance_id.as_bytes();
        let hash = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
        (hash % PARTITION_COUNT) + 1
    }

    /// Emit a lifecycle event to Iggy
    #[instrument(skip(self, event), fields(event_type = %event.event_type, subject = %event.subject))]
    async fn emit(&self, event: LifecycleEvent, instance_id: Uuid) -> Result<(), String> {
        if !self.enabled {
            debug!("lifecycle events disabled, skipping emission");
            return Ok(());
        }

        let payload = event
            .to_bytes()
            .map_err(|e| format!("failed to serialize event: {}", e))?;

        let message = Message::from_str(&String::from_utf8_lossy(&payload))
            .map_err(|e| format!("failed to create message: {}", e))?;

        let stream_id = Identifier::from_str("flowr")
            .map_err(|e| format!("invalid stream id: {}", e))?;
        let topic_id = Identifier::from_str(LIFECYCLE_EVENTS_TOPIC)
            .map_err(|e| format!("invalid topic id: {}", e))?;

        let partition_id = Self::partition_for_instance(&instance_id);

        self.iggy
            .send_messages(
                &stream_id,
                &topic_id,
                &Partitioning::partition_id(partition_id),
                &mut [message],
            )
            .await
            .map_err(|e| format!("failed to send event to Iggy: {}", e))?;

        debug!(event_type = %event.event_type, "lifecycle event emitted");
        Ok(())
    }

    // ==================== Workflow Events ====================

    /// Emit workflow.started event
    #[instrument(skip(self, input))]
    pub async fn workflow_started(
        &self,
        instance_id: Uuid,
        namespace: &str,
        name: &str,
        version: &str,
        input: Option<Value>,
    ) -> Result<(), String> {
        let mut data = WorkflowEventData::new(
            instance_id.to_string(),
            namespace,
            name,
            version,
        );
        if let Some(input) = input {
            data = data.with_input(input);
        }

        let event = LifecycleEvent::new(
            event_types::WORKFLOW_STARTED,
            self.workflow_source(namespace, name),
            instance_id.to_string(),
            LifecycleEventData::Workflow(data),
        );

        self.emit(event, instance_id).await
    }

    /// Emit workflow.completed event
    #[instrument(skip(self, output))]
    pub async fn workflow_completed(
        &self,
        instance_id: Uuid,
        namespace: &str,
        name: &str,
        version: &str,
        output: Option<Value>,
        duration_us: Option<u64>,
    ) -> Result<(), String> {
        let mut data = WorkflowEventData::new(
            instance_id.to_string(),
            namespace,
            name,
            version,
        );
        if let Some(output) = output {
            data = data.with_output(output);
        }
        if let Some(duration) = duration_us {
            data = data.with_duration(duration);
        }

        let event = LifecycleEvent::new(
            event_types::WORKFLOW_COMPLETED,
            self.workflow_source(namespace, name),
            instance_id.to_string(),
            LifecycleEventData::Workflow(data),
        );

        self.emit(event, instance_id).await
    }

    /// Emit workflow.faulted event
    #[instrument(skip(self))]
    pub async fn workflow_faulted(
        &self,
        instance_id: Uuid,
        namespace: &str,
        name: &str,
        version: &str,
        error: ErrorData,
    ) -> Result<(), String> {
        let data = WorkflowEventData::new(
            instance_id.to_string(),
            namespace,
            name,
            version,
        )
        .with_error(error);

        let event = LifecycleEvent::new(
            event_types::WORKFLOW_FAULTED,
            self.workflow_source(namespace, name),
            instance_id.to_string(),
            LifecycleEventData::Workflow(data),
        );

        self.emit(event, instance_id).await
    }

    /// Emit workflow.suspended event
    #[instrument(skip(self))]
    pub async fn workflow_suspended(
        &self,
        instance_id: Uuid,
        namespace: &str,
        name: &str,
        version: &str,
        suspension: SuspensionData,
    ) -> Result<(), String> {
        let data = WorkflowEventData::new(
            instance_id.to_string(),
            namespace,
            name,
            version,
        )
        .with_suspension(suspension);

        let event = LifecycleEvent::new(
            event_types::WORKFLOW_SUSPENDED,
            self.workflow_source(namespace, name),
            instance_id.to_string(),
            LifecycleEventData::Workflow(data),
        );

        self.emit(event, instance_id).await
    }

    /// Emit workflow.resumed event
    #[instrument(skip(self))]
    pub async fn workflow_resumed(
        &self,
        instance_id: Uuid,
        namespace: &str,
        name: &str,
        version: &str,
    ) -> Result<(), String> {
        let data = WorkflowEventData::new(
            instance_id.to_string(),
            namespace,
            name,
            version,
        );

        let event = LifecycleEvent::new(
            event_types::WORKFLOW_RESUMED,
            self.workflow_source(namespace, name),
            instance_id.to_string(),
            LifecycleEventData::Workflow(data),
        );

        self.emit(event, instance_id).await
    }

    /// Emit workflow.cancelled event
    #[instrument(skip(self))]
    pub async fn workflow_cancelled(
        &self,
        instance_id: Uuid,
        namespace: &str,
        name: &str,
        version: &str,
        reason: Option<String>,
    ) -> Result<(), String> {
        let mut data = WorkflowEventData::new(
            instance_id.to_string(),
            namespace,
            name,
            version,
        );
        if let Some(reason) = reason {
            data = data.with_error(ErrorData::new("cancelled", reason));
        }

        let event = LifecycleEvent::new(
            event_types::WORKFLOW_CANCELLED,
            self.workflow_source(namespace, name),
            instance_id.to_string(),
            LifecycleEventData::Workflow(data),
        );

        self.emit(event, instance_id).await
    }

    // ==================== Task Events ====================

    /// Emit task.created event (before if-check)
    #[instrument(skip(self))]
    pub async fn task_created(
        &self,
        instance_id: Uuid,
        namespace: &str,
        workflow_name: &str,
        task_name: &str,
        task_type: &str,
        position: &str,
    ) -> Result<(), String> {
        let data = TaskEventData::new(
            instance_id.to_string(),
            namespace,
            workflow_name,
            task_name,
            task_type,
            position,
        );

        let event = LifecycleEvent::new(
            event_types::TASK_CREATED,
            self.workflow_source(namespace, workflow_name),
            instance_id.to_string(),
            LifecycleEventData::Task(data),
        );

        self.emit(event, instance_id).await
    }

    /// Emit task.skipped event (when if-condition evaluates to false)
    #[instrument(skip(self))]
    pub async fn task_skipped(
        &self,
        instance_id: Uuid,
        namespace: &str,
        workflow_name: &str,
        task_name: &str,
        task_type: &str,
        position: &str,
    ) -> Result<(), String> {
        let data = TaskEventData::new(
            instance_id.to_string(),
            namespace,
            workflow_name,
            task_name,
            task_type,
            position,
        );

        let event = LifecycleEvent::new(
            event_types::TASK_SKIPPED,
            self.workflow_source(namespace, workflow_name),
            instance_id.to_string(),
            LifecycleEventData::Task(data),
        );

        self.emit(event, instance_id).await
    }

    /// Emit task.started event
    #[instrument(skip(self, input))]
    pub async fn task_started(
        &self,
        instance_id: Uuid,
        namespace: &str,
        workflow_name: &str,
        task_name: &str,
        task_type: &str,
        position: &str,
        input: Option<Value>,
    ) -> Result<(), String> {
        let mut data = TaskEventData::new(
            instance_id.to_string(),
            namespace,
            workflow_name,
            task_name,
            task_type,
            position,
        );
        if let Some(input) = input {
            data = data.with_input(input);
        }

        let event = LifecycleEvent::new(
            event_types::TASK_STARTED,
            self.workflow_source(namespace, workflow_name),
            instance_id.to_string(),
            LifecycleEventData::Task(data),
        );

        self.emit(event, instance_id).await
    }

    /// Emit task.completed event
    #[instrument(skip(self, output))]
    pub async fn task_completed(
        &self,
        instance_id: Uuid,
        namespace: &str,
        workflow_name: &str,
        task_name: &str,
        task_type: &str,
        position: &str,
        output: Option<Value>,
        duration_us: Option<u64>,
    ) -> Result<(), String> {
        let mut data = TaskEventData::new(
            instance_id.to_string(),
            namespace,
            workflow_name,
            task_name,
            task_type,
            position,
        );
        if let Some(output) = output {
            data = data.with_output(output);
        }
        if let Some(duration) = duration_us {
            data = data.with_duration(duration);
        }

        let event = LifecycleEvent::new(
            event_types::TASK_COMPLETED,
            self.workflow_source(namespace, workflow_name),
            instance_id.to_string(),
            LifecycleEventData::Task(data),
        );

        self.emit(event, instance_id).await
    }

    /// Emit task.faulted event
    #[instrument(skip(self))]
    pub async fn task_faulted(
        &self,
        instance_id: Uuid,
        namespace: &str,
        workflow_name: &str,
        task_name: &str,
        task_type: &str,
        position: &str,
        error: ErrorData,
    ) -> Result<(), String> {
        let data = TaskEventData::new(
            instance_id.to_string(),
            namespace,
            workflow_name,
            task_name,
            task_type,
            position,
        )
        .with_error(error);

        let event = LifecycleEvent::new(
            event_types::TASK_FAULTED,
            self.workflow_source(namespace, workflow_name),
            instance_id.to_string(),
            LifecycleEventData::Task(data),
        );

        self.emit(event, instance_id).await
    }

    /// Emit task.retried event
    #[instrument(skip(self))]
    pub async fn task_retried(
        &self,
        instance_id: Uuid,
        namespace: &str,
        workflow_name: &str,
        task_name: &str,
        task_type: &str,
        position: &str,
        attempt: u32,
        error: ErrorData,
    ) -> Result<(), String> {
        let data = TaskEventData::new(
            instance_id.to_string(),
            namespace,
            workflow_name,
            task_name,
            task_type,
            position,
        )
        .with_retry(attempt)
        .with_error(error);

        let event = LifecycleEvent::new(
            event_types::TASK_RETRIED,
            self.workflow_source(namespace, workflow_name),
            instance_id.to_string(),
            LifecycleEventData::Task(data),
        );

        self.emit(event, instance_id).await
    }

    /// Emit task.suspended event
    #[instrument(skip(self))]
    pub async fn task_suspended(
        &self,
        instance_id: Uuid,
        namespace: &str,
        workflow_name: &str,
        task_name: &str,
        task_type: &str,
        position: &str,
    ) -> Result<(), String> {
        let data = TaskEventData::new(
            instance_id.to_string(),
            namespace,
            workflow_name,
            task_name,
            task_type,
            position,
        );

        let event = LifecycleEvent::new(
            event_types::TASK_SUSPENDED,
            self.workflow_source(namespace, workflow_name),
            instance_id.to_string(),
            LifecycleEventData::Task(data),
        );

        self.emit(event, instance_id).await
    }

    /// Emit task.resumed event
    #[instrument(skip(self))]
    pub async fn task_resumed(
        &self,
        instance_id: Uuid,
        namespace: &str,
        workflow_name: &str,
        task_name: &str,
        task_type: &str,
        position: &str,
    ) -> Result<(), String> {
        let data = TaskEventData::new(
            instance_id.to_string(),
            namespace,
            workflow_name,
            task_name,
            task_type,
            position,
        );

        let event = LifecycleEvent::new(
            event_types::TASK_RESUMED,
            self.workflow_source(namespace, workflow_name),
            instance_id.to_string(),
            LifecycleEventData::Task(data),
        );

        self.emit(event, instance_id).await
    }

    /// Emit task.cancelled event
    #[instrument(skip(self))]
    pub async fn task_cancelled(
        &self,
        instance_id: Uuid,
        namespace: &str,
        workflow_name: &str,
        task_name: &str,
        task_type: &str,
        position: &str,
        reason: Option<String>,
    ) -> Result<(), String> {
        let mut data = TaskEventData::new(
            instance_id.to_string(),
            namespace,
            workflow_name,
            task_name,
            task_type,
            position,
        );
        if let Some(reason) = reason {
            data = data.with_error(ErrorData::new("cancelled", reason));
        }

        let event = LifecycleEvent::new(
            event_types::TASK_CANCELLED,
            self.workflow_source(namespace, workflow_name),
            instance_id.to_string(),
            LifecycleEventData::Task(data),
        );

        self.emit(event, instance_id).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lifecycle_event_serialization() {
        let data = WorkflowEventData::new(
            "test-instance",
            "default",
            "test-workflow",
            "1.0.0",
        )
        .with_input(json!({"key": "value"}));

        let event = LifecycleEvent::new(
            event_types::WORKFLOW_STARTED,
            "flowr://default/test-workflow",
            "test-instance",
            LifecycleEventData::Workflow(data),
        );

        let bytes = event.to_bytes().unwrap();
        let json: Value = serde_json::from_slice(&bytes).unwrap();

        assert_eq!(json["type"], event_types::WORKFLOW_STARTED);
        assert_eq!(json["specversion"], "1.0");
        assert_eq!(json["source"], "flowr://default/test-workflow");
    }

    #[test]
    fn test_task_event_data() {
        let data = TaskEventData::new(
            "instance-1",
            "default",
            "my-workflow",
            "step-1",
            "call:http",
            "do/step-1",
        )
        .with_input(json!({"url": "https://example.com"}))
        .with_output(json!({"status": 200}))
        .with_duration(150);

        assert_eq!(data.task_name, "step-1");
        assert_eq!(data.task_type, "call:http");
        assert_eq!(data.duration_us, Some(150));
    }

    #[test]
    fn test_error_data() {
        let error = ErrorData::new("http_error", "Connection refused")
            .with_details(json!({"code": 503}));

        assert_eq!(error.error_type, "http_error");
        assert_eq!(error.message, "Connection refused");
    }
}
