//! Execution contexts

use std::str::FromStr;
use std::sync::Arc;

use iggy::client::{StreamClient, TopicClient};
use iggy::clients::client::IggyClient;
use iggy::compression::compression_algorithm::CompressionAlgorithm;
use iggy::identifier::Identifier;
use iggy::utils::expiry::IggyExpiry;
use iggy::utils::topic_size::MaxTopicSize;
use serde_json::Value;
use tracing::{debug, info};
use uuid::Uuid;

use flowr_expr::{ExpressionEngine, Scope};

use crate::lifecycle::{LifecycleEventEmitter, LIFECYCLE_EVENTS_TOPIC};

/// Iggy stream name for FlowR
const FLOWR_STREAM: &str = "flowr";
/// Iggy topic for checkpoints
const CHECKPOINT_TOPIC: &str = "checkpoints";
/// Iggy topic for completion events
const COMPLETION_TOPIC: &str = "completions";
/// Default number of partitions
const DEFAULT_PARTITIONS: u32 = 8;

/// Shared services for workflow execution
///
/// Contains connections to Dragonfly, Iggy, HTTP client, and expression engine.
#[derive(Clone)]
pub struct FlowContext {
    /// Iggy client for checkpoints and events
    pub iggy: Arc<IggyClient>,

    /// Redis/Dragonfly connection
    pub redis: redis::aio::MultiplexedConnection,

    /// HTTP client for Call:http tasks
    pub http: reqwest::Client,

    /// Expression engine for jq evaluation
    pub expr: Arc<ExpressionEngine>,

    /// Lifecycle event emitter for CloudEvents
    pub lifecycle: Arc<LifecycleEventEmitter>,
}

impl FlowContext {
    /// Create a new flow context
    pub fn new(
        iggy: Arc<IggyClient>,
        redis: redis::aio::MultiplexedConnection,
        http: reqwest::Client,
    ) -> Self {
        let lifecycle = Arc::new(LifecycleEventEmitter::new(iggy.clone()));
        Self {
            iggy,
            redis,
            http,
            expr: Arc::new(ExpressionEngine::new()),
            lifecycle,
        }
    }

    /// Create a flow context with a custom expression engine
    pub fn with_expr(mut self, expr: Arc<ExpressionEngine>) -> Self {
        self.expr = expr;
        self
    }

    /// Create a flow context with lifecycle events disabled
    pub fn without_lifecycle_events(mut self) -> Self {
        let mut emitter = LifecycleEventEmitter::new(self.iggy.clone());
        emitter.set_enabled(false);
        self.lifecycle = Arc::new(emitter);
        self
    }

    /// Initialize Iggy stream and topics for FlowR
    ///
    /// Creates the stream and topics if they don't exist.
    /// Should be called once during application startup.
    pub async fn init_iggy(&self) -> Result<(), String> {
        info!("Initializing FlowR Iggy stream and topics");

        let stream_id = Identifier::from_str(FLOWR_STREAM)
            .map_err(|e| format!("invalid stream id: {}", e))?;

        // Try to create stream (ignore error if exists)
        match self.iggy.create_stream(FLOWR_STREAM, None).await {
            Ok(_) => info!("Created stream: {}", FLOWR_STREAM),
            Err(e) => debug!("Stream may already exist: {}", e),
        }

        // Create checkpoints topic
        // API: create_topic(stream_id, name, partitions, compression, replication, expiry, max_size)
        match self
            .iggy
            .create_topic(
                &stream_id,
                CHECKPOINT_TOPIC,
                DEFAULT_PARTITIONS,
                CompressionAlgorithm::None,
                None, // replication
                None, // topic_id
                IggyExpiry::NeverExpire,
                MaxTopicSize::Unlimited,
            )
            .await
        {
            Ok(_) => info!("Created topic: {}", CHECKPOINT_TOPIC),
            Err(e) => debug!("Topic may already exist: {}", e),
        }

        // Create completions topic
        match self
            .iggy
            .create_topic(
                &stream_id,
                COMPLETION_TOPIC,
                DEFAULT_PARTITIONS,
                CompressionAlgorithm::None,
                None, // replication
                None, // topic_id
                IggyExpiry::NeverExpire,
                MaxTopicSize::Unlimited,
            )
            .await
        {
            Ok(_) => info!("Created topic: {}", COMPLETION_TOPIC),
            Err(e) => debug!("Topic may already exist: {}", e),
        }

        // Create lifecycle events topic
        match self
            .iggy
            .create_topic(
                &stream_id,
                LIFECYCLE_EVENTS_TOPIC,
                DEFAULT_PARTITIONS,
                CompressionAlgorithm::None,
                None, // replication
                None, // topic_id
                IggyExpiry::NeverExpire,
                MaxTopicSize::Unlimited,
            )
            .await
        {
            Ok(_) => info!("Created topic: {}", LIFECYCLE_EVENTS_TOPIC),
            Err(e) => debug!("Topic may already exist: {}", e),
        }

        info!("FlowR Iggy initialization complete");
        Ok(())
    }
}

/// Context for a single task execution
///
/// Contains the task's input, scope variables, and access to shared services.
pub struct TaskContext {
    /// Task input (after input transformation)
    pub input: Value,

    /// Expression scope ($runtime, $workflow, $task, $context)
    pub scope: Scope,

    /// Shared flow context (services)
    pub flow_ctx: FlowContext,

    /// Current workflow instance ID
    pub instance_id: Uuid,

    /// Current task name
    pub task_name: String,

    /// Position in workflow (e.g., "do/step1")
    pub position: String,
}

impl TaskContext {
    /// Create a new task context
    pub fn new(
        input: Value,
        scope: Scope,
        flow_ctx: FlowContext,
        instance_id: Uuid,
        task_name: impl Into<String>,
        position: impl Into<String>,
    ) -> Self {
        Self {
            input,
            scope,
            flow_ctx,
            instance_id,
            task_name: task_name.into(),
            position: position.into(),
        }
    }

    /// Evaluate an expression against the task input
    pub fn eval(&self, expr: &str) -> Result<Value, flowr_expr::ExprError> {
        self.flow_ctx.expr.eval(&self.input, expr, &self.scope)
    }

    /// Evaluate a value recursively, processing any expressions
    pub fn eval_recursive(&self, value: &Value) -> Result<Value, flowr_expr::ExprError> {
        self.flow_ctx
            .expr
            .eval_recursive(value, &self.input, &self.scope)
    }

    /// Update the scope context (for Set tasks)
    pub fn update_context(&mut self, updates: Value) {
        if let (Value::Object(ctx), Value::Object(upd)) = (&mut self.scope.context, updates) {
            for (k, v) in upd {
                ctx.insert(k, v);
            }
        }
    }

    /// Get the current workflow context
    pub fn context(&self) -> &Value {
        &self.scope.context
    }

    /// Get a mutable reference to the workflow context
    pub fn context_mut(&mut self) -> &mut Value {
        &mut self.scope.context
    }
}
