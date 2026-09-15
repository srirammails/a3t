//! FlowR Worker
//!
//! Consumes workflow execution requests and runs them using the Executor.
//!
//! Supports two storage modes:
//! - `dragonfly`: DragonflyDB + Iggy (distributed)
//! - `adb`: ClawDB/LanceDB (Agent Database, single-node)

use std::sync::Arc;
use std::time::Duration;

use clap::{Parser, ValueEnum};
use flowr_core::{
    FlowContext, Executor, FlowDurability, FlowError, FlowResult,
    TaskDefinition, TaskExecutor, WorkflowMetadata,
};
use flowr_types::ExecutionRequest;
use flowr_registry::WorkflowRecord;
use serde::{Deserialize, Serialize};
use tokio::sync::Semaphore;
use tracing::{debug, error, info, warn};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};
use uuid::Uuid;

#[cfg(feature = "dragonfly")]
use std::str::FromStr;
#[cfg(feature = "dragonfly")]
use flowr_hub::HubRegistry;
#[cfg(feature = "dragonfly")]
use flowr_registry::{FlowRegistry, ResourceResolver};
use flowr_registry::converter;
#[cfg(feature = "dragonfly")]
use iggy::client::{Client, ConsumerGroupClient, MessageClient, StreamClient, TopicClient, UserClient};
#[cfg(feature = "dragonfly")]
use iggy::clients::client::IggyClient;
#[cfg(feature = "dragonfly")]
use iggy::consumer::Consumer;
#[cfg(feature = "dragonfly")]
use iggy::identifier::Identifier;
#[cfg(feature = "dragonfly")]
use iggy::messages::poll_messages::PollingStrategy;

#[cfg(feature = "adb")]
use flowr_storage::StorageBackend;
#[cfg(feature = "adb")]
use flowr_ce_storage::LanceStorage;

use flowr_tasks::{
    // AI executors
    AgentExecutor, HumanExecutor, LlmExecutor,
    // I/O executors
    CallHttpConfig, CallHttpExecutor, EmitExecutor,
    InputFileExecutor, OutputFileExecutor,
    InputStreamExecutor, OutputStreamExecutor,
    StateGetExecutor, StateSetExecutor, StateDeleteExecutor,
    // Integration executors
    McpExecutor, QdrantExecutor, RedisExecutor, SqlExecutor,
    // Script executors
    InlineLanguage, InlineScriptExecutor, RunShellExecutor,
    // Flow control executors
    CatchBlock, DoExecutor, ErrorFilter, ForExecutor, ForkExecutor, ForkMode,
    RaiseExecutor, RetryConfig, SetExecutor, SwitchExecutor, TryExecutor,
};

/// Iggy stream for FlowR (Dragonfly mode)
#[cfg(feature = "dragonfly")]
const FLOWR_STREAM: &str = "flowr";

/// Topic for execution requests (Dragonfly mode)
#[cfg(feature = "dragonfly")]
const EXECUTION_REQUESTS_TOPIC: &str = "execution-requests";

/// Consumer group for workers (Dragonfly mode)
#[cfg(feature = "dragonfly")]
const WORKER_CONSUMER_GROUP: &str = "flowr-workers";

/// Default concurrency (number of parallel workflow executions)
const DEFAULT_CONCURRENCY: usize = 10;

// ─── ResourceResolver implementations for the converter ─────

/// HubRegistry-backed resource resolver (dragonfly mode)
#[cfg(feature = "dragonfly")]
struct HubResourceResolver {
    hub: Arc<HubRegistry>,
}

#[cfg(feature = "dragonfly")]
#[async_trait::async_trait]
impl ResourceResolver for HubResourceResolver {
    async fn resolve(&self, namespace: &str, name: &str) -> FlowResult<serde_json::Value> {
        let resource = self.hub.get_resource(namespace, name).await
            .map_err(|e| FlowError::Internal(format!("resource resolve failed: {}", e)))?;
        Ok(resource.config)
    }
}

/// LanceStorage-backed resource resolver (adb mode)
#[cfg(feature = "adb")]
struct LanceResourceResolver {
    storage: Arc<LanceStorage>,
}

#[cfg(feature = "adb")]
#[async_trait::async_trait]
impl ResourceResolver for LanceResourceResolver {
    async fn resolve(&self, namespace: &str, name: &str) -> FlowResult<serde_json::Value> {
        use flowr_storage::ResourceStore;
        let resource = self.storage.resources().get(namespace, name).await?;
        Ok(resource.config)
    }
}

/// Storage backend mode
#[derive(Debug, Clone, Copy, ValueEnum, Default)]
pub enum StorageMode {
    /// DragonflyDB + Iggy (distributed mode)
    #[default]
    Dragonfly,
    /// ClawDB/LanceDB Agent Database (single-node mode)
    Adb,
}

/// FlowR Worker
#[derive(Parser, Debug)]
#[command(name = "flowr-worker")]
#[command(about = "FlowR Worker - Executes workflows from the execution queue")]
struct Cli {
    /// Storage mode
    #[arg(short, long, env = "STORAGE_MODE", value_enum, default_value = "dragonfly")]
    storage: StorageMode,

    /// Worker concurrency (parallel executions)
    #[arg(short, long, env = "WORKER_CONCURRENCY", default_value_t = DEFAULT_CONCURRENCY)]
    concurrency: usize,

    /// Dragonfly URL (for dragonfly mode)
    #[arg(long, env = "DRAGONFLY_URL", default_value = "redis://127.0.0.1:6379")]
    dragonfly_url: String,

    /// Iggy URL (for dragonfly mode)
    #[arg(long, env = "IGGY_URL", default_value = "127.0.0.1:8090")]
    iggy_url: String,

    /// Iggy username (for dragonfly mode)
    #[arg(long, env = "IGGY_USERNAME", default_value = "iggy")]
    iggy_username: String,

    /// Iggy password (for dragonfly mode)
    #[arg(long, env = "IGGY_PASSWORD", default_value = "iggy")]
    iggy_password: String,

    /// ADB (ClawDB) data path (for adb mode)
    #[arg(long, env = "ADB_PATH", default_value = "./flowr_data")]
    adb_path: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize tracing
    tracing_subscriber::registry()
        .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| {
            EnvFilter::new("flowr_worker=debug,flowr_core=debug,info")
        }))
        .with(tracing_subscriber::fmt::layer())
        .init();

    let cli = Cli::parse();

    info!("Starting FlowR Worker");
    info!(mode = ?cli.storage, "Storage mode");

    match cli.storage {
        StorageMode::Dragonfly => {
            #[cfg(feature = "dragonfly")]
            {
                run_dragonfly_mode(cli).await?;
            }
            #[cfg(not(feature = "dragonfly"))]
            {
                anyhow::bail!("Dragonfly mode not enabled. Rebuild with --features dragonfly");
            }
        }
        StorageMode::Adb => {
            #[cfg(feature = "adb")]
            {
                run_adb_mode(cli).await?;
            }
            #[cfg(not(feature = "adb"))]
            {
                anyhow::bail!("ADB mode not enabled. Rebuild with --features adb");
            }
        }
    }

    info!("Worker shut down gracefully");
    Ok(())
}

// =============================================================================
// Dragonfly Mode
// =============================================================================

#[cfg(feature = "dragonfly")]
async fn run_dragonfly_mode(cli: Cli) -> anyhow::Result<()> {
    use iggy::compression::compression_algorithm::CompressionAlgorithm;
    use iggy::utils::expiry::IggyExpiry;
    use iggy::utils::topic_size::MaxTopicSize;

    // Connect to Dragonfly
    info!(url = %cli.dragonfly_url, "Connecting to Dragonfly");
    let redis_client = redis::Client::open(cli.dragonfly_url.as_str())?;
    let redis = redis_client.get_multiplexed_async_connection().await?;
    info!("Connected to Dragonfly");

    // Connect to Iggy
    info!(url = %cli.iggy_url, "Connecting to Iggy");
    let iggy = IggyClient::builder()
        .with_tcp()
        .with_server_address(cli.iggy_url.clone())
        .build()?;
    iggy.connect().await?;

    // Login to Iggy
    iggy.login_user(&cli.iggy_username, &cli.iggy_password).await?;
    info!("Connected and authenticated to Iggy");

    let iggy = Arc::new(iggy);

    // Create registries
    let registry = Arc::new(FlowRegistry::new(redis.clone()));
    let hub = Arc::new(HubRegistry::new(redis.clone()));

    // Ensure stream and topics exist
    ensure_infrastructure_dragonfly(&iggy).await?;

    // Create consumer group
    let stream_id = Identifier::from_str(FLOWR_STREAM)?;
    let topic_id = Identifier::from_str(EXECUTION_REQUESTS_TOPIC)?;

    // Try to create consumer group (ignore if exists)
    match iggy
        .create_consumer_group(&stream_id, &topic_id, WORKER_CONSUMER_GROUP, None)
        .await
    {
        Ok(_) => info!("Created consumer group: {}", WORKER_CONSUMER_GROUP),
        Err(e) => {
            if !e.to_string().contains("already exists") {
                return Err(e.into());
            }
            debug!("Consumer group already exists");
        }
    }

    // Join consumer group
    let group_id = Identifier::from_str(WORKER_CONSUMER_GROUP)?;
    iggy.join_consumer_group(&stream_id, &topic_id, &group_id).await?;
    info!("Joined consumer group: {}", WORKER_CONSUMER_GROUP);

    // Create semaphore for concurrency control
    let semaphore = Arc::new(Semaphore::new(cli.concurrency));

    info!(
        concurrency = cli.concurrency,
        "Worker ready, polling for execution requests (Dragonfly mode)"
    );

    // Main polling loop
    let consumer = Consumer::group(group_id.clone());

    loop {
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {
                info!("Received shutdown signal");
                break;
            }
            result = poll_and_execute_dragonfly(
                &iggy,
                &consumer,
                &stream_id,
                &topic_id,
                &registry,
                &hub,
                redis.clone(),
                semaphore.clone(),
            ) => {
                if let Err(e) = result {
                    error!(error = %e, "Error in poll loop");
                    tokio::time::sleep(Duration::from_secs(1)).await;
                }
            }
        }
    }

    Ok(())
}

#[cfg(feature = "dragonfly")]
async fn ensure_infrastructure_dragonfly(iggy: &IggyClient) -> anyhow::Result<()> {
    use iggy::compression::compression_algorithm::CompressionAlgorithm;
    use iggy::utils::expiry::IggyExpiry;
    use iggy::utils::topic_size::MaxTopicSize;

    let stream_id = Identifier::from_str(FLOWR_STREAM)?;

    // Create stream if not exists
    match iggy.create_stream(FLOWR_STREAM, None).await {
        Ok(_) => info!("Created stream: {}", FLOWR_STREAM),
        Err(e) => {
            if !e.to_string().contains("already exists") {
                return Err(e.into());
            }
        }
    }

    // Create execution-requests topic
    match iggy
        .create_topic(
            &stream_id,
            EXECUTION_REQUESTS_TOPIC,
            8,
            CompressionAlgorithm::None,
            None,
            None, // topic_id
            IggyExpiry::NeverExpire,
            MaxTopicSize::Unlimited,
        )
        .await
    {
        Ok(_) => info!("Created topic: {}", EXECUTION_REQUESTS_TOPIC),
        Err(e) => {
            if !e.to_string().contains("already exists") {
                return Err(e.into());
            }
        }
    }

    // Create lifecycle-events topic
    match iggy
        .create_topic(
            &stream_id,
            flowr_core::LIFECYCLE_EVENTS_TOPIC,
            8,
            CompressionAlgorithm::None,
            None,
            None, // topic_id
            IggyExpiry::NeverExpire,
            MaxTopicSize::Unlimited,
        )
        .await
    {
        Ok(_) => info!("Created topic: {}", flowr_core::LIFECYCLE_EVENTS_TOPIC),
        Err(e) => {
            if !e.to_string().contains("already exists") {
                return Err(e.into());
            }
        }
    }

    // Create checkpoints topic
    match iggy
        .create_topic(
            &stream_id,
            "checkpoints",
            8,
            CompressionAlgorithm::None,
            None,
            None, // topic_id
            IggyExpiry::NeverExpire,
            MaxTopicSize::Unlimited,
        )
        .await
    {
        Ok(_) => info!("Created topic: checkpoints"),
        Err(e) => {
            if !e.to_string().contains("already exists") {
                return Err(e.into());
            }
        }
    }

    Ok(())
}

#[cfg(feature = "dragonfly")]
async fn poll_and_execute_dragonfly(
    iggy: &Arc<IggyClient>,
    consumer: &Consumer,
    stream_id: &Identifier,
    topic_id: &Identifier,
    registry: &Arc<FlowRegistry>,
    hub: &Arc<HubRegistry>,
    redis: redis::aio::MultiplexedConnection,
    semaphore: Arc<Semaphore>,
) -> anyhow::Result<()> {
    // Poll for messages
    let polled = iggy
        .poll_messages(
            stream_id,
            topic_id,
            None,
            consumer,
            &PollingStrategy::next(),
            10,
            true,
        )
        .await?;

    if polled.messages.is_empty() {
        tokio::time::sleep(Duration::from_millis(100)).await;
        return Ok(());
    }

    info!(count = polled.messages.len(), "Received execution requests");

    for msg in polled.messages {
        let request: ExecutionRequest = match serde_json::from_slice(&msg.payload) {
            Ok(r) => r,
            Err(e) => {
                error!(error = %e, "Failed to deserialize execution request");
                continue;
            }
        };

        let permit = semaphore.clone().acquire_owned().await?;

        let iggy = iggy.clone();
        let registry = registry.clone();
        let hub = hub.clone();
        let redis = redis.clone();

        tokio::spawn(async move {
            let _permit = permit;

            if let Err(e) = execute_workflow_dragonfly(&iggy, &registry, &hub, redis, request.clone()).await {
                error!(
                    instance_id = %request.instance_id,
                    namespace = %request.namespace,
                    name = %request.name,
                    error = %e,
                    "Workflow execution failed"
                );
            }
        });
    }

    Ok(())
}

#[cfg(feature = "dragonfly")]
async fn execute_workflow_dragonfly(
    iggy: &Arc<IggyClient>,
    registry: &Arc<FlowRegistry>,
    hub: &Arc<HubRegistry>,
    redis: redis::aio::MultiplexedConnection,
    request: ExecutionRequest,
) -> anyhow::Result<()> {
    info!(
        instance_id = %request.instance_id,
        namespace = %request.namespace,
        name = %request.name,
        version = ?request.version,
        "Starting workflow execution (Dragonfly)"
    );

    // Get workflow record (CNCF format)
    let record = if let Some(version) = &request.version {
        registry.get_record_version(&request.namespace, &request.name, version).await?
    } else {
        registry.get_record(&request.namespace, &request.name).await?
    };

    // Parse CNCF workflow
    let swf = record.parse()
        .map_err(|e| anyhow::anyhow!("Failed to parse CNCF workflow: {}", e))?;

    let definition_meta = &record.metadata;

    // Determine durability mode
    let durability = match request.durability.as_deref() {
        Some("checkpoint") => FlowDurability::Checkpoint,
        _ => FlowDurability::Completion,
    };

    // Create HTTP client
    let http_client = reqwest::Client::new();

    // Create flow context
    let flow_ctx = FlowContext::new(iggy.clone(), redis.clone(), http_client);
    flow_ctx.init_iggy().await.map_err(|e| anyhow::anyhow!("init_iggy failed: {}", e))?;

    // Create executor
    let executor = Executor::new(flow_ctx);

    // Build task definitions from CNCF AST using typed converter
    let hub_resolver = Arc::new(HubResourceResolver { hub: hub.clone() });
    let wf_resolver: Arc<dyn flowr_tasks::WorkflowResolver> = Arc::new(
        converter::RegistryWorkflowResolver::new(registry.clone(), hub_resolver.clone(), &request.namespace)
    );
    let tasks = converter::convert_named_tasks_with_workflow_resolver(
        &swf.tasks, hub_resolver.as_ref(), &request.namespace, Some(wf_resolver)
    ).await?;

    // Create workflow metadata
    let metadata = WorkflowMetadata::new(
        &request.namespace,
        &request.name,
        &definition_meta.version,
    );

    let workflow_id = format!("{}/{}", request.namespace, request.name);

    // Store running status in Redis
    let status_key = format!("flowr:exec:{}", request.instance_id);
    let mut redis_status = redis.clone();
    let _ = redis::cmd("SET")
        .arg(&status_key)
        .arg(serde_json::json!({"status": "Running", "workflow": &workflow_id, "started_at": chrono::Utc::now().to_rfc3339()}).to_string())
        .arg("EX").arg(3600)
        .query_async::<_, ()>(&mut redis_status)
        .await;

    // Execute the workflow
    let result = executor
        .execute_with_metadata(&workflow_id, &tasks, request.input, durability, &metadata)
        .await;

    // Store completion status in Redis
    match &result {
        Ok(output) => {
            info!(instance_id = %request.instance_id, "Workflow completed successfully");
            let status_json = serde_json::json!({
                "status": "Completed",
                "workflow": &workflow_id,
                "output": output,
                "completed_at": chrono::Utc::now().to_rfc3339()
            });
            let _ = redis::cmd("SET")
                .arg(&status_key)
                .arg(status_json.to_string())
                .arg("EX").arg(3600)
                .query_async::<_, ()>(&mut redis_status)
                .await;
        }
        Err(e) => {
            error!(instance_id = %request.instance_id, error = %e, "Workflow execution error");
            let status_json = serde_json::json!({
                "status": "Failed",
                "workflow": &workflow_id,
                "error": e.to_string(),
                "failed_at": chrono::Utc::now().to_rfc3339()
            });
            let _ = redis::cmd("SET")
                .arg(&status_key)
                .arg(status_json.to_string())
                .arg("EX").arg(3600)
                .query_async::<_, ()>(&mut redis_status)
                .await;
        }
    }

    result.map(|_| ()).map_err(|e| anyhow::anyhow!("{}", e))
}

#[cfg(feature = "dragonfly")]
async fn build_tasks_dragonfly(
    workflow: &flowr_registry::WorkflowDefinition,
    hub: &Arc<HubRegistry>,
    default_namespace: &str,
) -> FlowResult<Vec<TaskDefinition>> {
    let mut tasks = Vec::with_capacity(workflow.steps.len());

    for step in &workflow.steps {
        let task = build_single_task_dragonfly(step, hub, default_namespace).await?;
        tasks.push(task);
    }

    Ok(tasks)
}

#[cfg(feature = "dragonfly")]
fn parse_nested_steps_dragonfly(value: &serde_json::Value) -> FlowResult<Vec<flowr_registry::StepDefinition>> {
    match value {
        serde_json::Value::Array(arr) => {
            let mut steps = Vec::with_capacity(arr.len());
            for item in arr {
                let step: flowr_registry::StepDefinition = serde_json::from_value(item.clone())
                    .map_err(|e| FlowError::task_failed("nested", format!("invalid nested step: {}", e)))?;
                steps.push(step);
            }
            Ok(steps)
        }
        serde_json::Value::Null => Ok(Vec::new()),
        _ => Err(FlowError::task_failed("nested", "expected array of steps")),
    }
}

#[cfg(feature = "dragonfly")]
#[async_recursion::async_recursion]
async fn build_single_task_dragonfly(
    step: &flowr_registry::StepDefinition,
    hub: &Arc<HubRegistry>,
    default_namespace: &str,
) -> FlowResult<TaskDefinition> {
    let executor: Arc<dyn TaskExecutor> = match step.step_type.as_str() {
        "do" => {
            let nested_steps = if let Some(do_steps) = step.config.get("do") {
                parse_nested_steps_dragonfly(do_steps)?
            } else if step.config.is_array() {
                parse_nested_steps_dragonfly(&step.config)?
            } else {
                Vec::new()
            };

            let mut subtasks = Vec::with_capacity(nested_steps.len());
            for nested_step in &nested_steps {
                let subtask = build_single_task_dragonfly(nested_step, hub, default_namespace).await?;
                subtasks.push(subtask);
            }

            Arc::new(DoExecutor::new(subtasks))
        }

        "for" => {
            let each = step.config.get("each")
                .and_then(|v| v.as_str())
                .unwrap_or("item")
                .to_string();

            let collection_expr = step.config.get("in")
                .and_then(|v| v.as_str())
                .ok_or_else(|| FlowError::task_failed(&step.name, "'in' expression required for 'for' task"))?
                .to_string();

            let do_steps = step.config.get("do")
                .ok_or_else(|| FlowError::task_failed(&step.name, "'do' block required for 'for' task"))?;
            let nested_steps = parse_nested_steps_dragonfly(do_steps)?;

            let mut subtasks = Vec::with_capacity(nested_steps.len());
            for nested_step in &nested_steps {
                let subtask = build_single_task_dragonfly(nested_step, hub, default_namespace).await?;
                subtasks.push(subtask);
            }

            let mut executor = ForExecutor::new(each, collection_expr, subtasks);

            if let Some(at) = step.config.get("at").and_then(|v| v.as_str()) {
                executor = executor.with_index(at);
            }

            if let Some(while_expr) = step.config.get("while").and_then(|v| v.as_str()) {
                executor = executor.with_while(while_expr);
            }

            Arc::new(executor)
        }

        "switch" => {
            let cases = step.config.as_array()
                .or_else(|| step.config.get("cases").and_then(|v| v.as_array()))
                .ok_or_else(|| FlowError::task_failed(&step.name, "switch requires array of cases"))?;

            let mut builder = SwitchExecutor::builder();

            for case in cases {
                if case.get("default").is_some() {
                    let then_steps = case.get("default")
                        .or_else(|| case.get("then"))
                        .ok_or_else(|| FlowError::task_failed(&step.name, "default case needs 'then' block"))?;
                    let nested_steps = parse_nested_steps_dragonfly(then_steps)?;

                    let mut subtasks = Vec::new();
                    for nested_step in &nested_steps {
                        subtasks.push(build_single_task_dragonfly(nested_step, hub, default_namespace).await?);
                    }
                    builder = builder.default(subtasks);
                } else if let Some(when_expr) = case.get("when").and_then(|v| v.as_str()) {
                    let then_steps = case.get("then")
                        .ok_or_else(|| FlowError::task_failed(&step.name, "switch case needs 'then' block"))?;
                    let nested_steps = parse_nested_steps_dragonfly(then_steps)?;

                    let mut subtasks = Vec::new();
                    for nested_step in &nested_steps {
                        subtasks.push(build_single_task_dragonfly(nested_step, hub, default_namespace).await?);
                    }
                    builder = builder.when(when_expr, subtasks);
                }
            }

            Arc::new(builder.build())
        }

        "fork" => {
            let branches = step.config.get("branches")
                .and_then(|v| v.as_array())
                .or_else(|| step.config.as_array())
                .ok_or_else(|| FlowError::task_failed(&step.name, "fork requires 'branches' array"))?;

            let mut fork_branches = Vec::with_capacity(branches.len());

            for branch in branches {
                let branch_name = branch.get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("branch")
                    .to_string();

                let do_steps = branch.get("do")
                    .or_else(|| branch.get("tasks"))
                    .ok_or_else(|| FlowError::task_failed(&step.name, "fork branch needs 'do' block"))?;
                let nested_steps = parse_nested_steps_dragonfly(do_steps)?;

                let mut subtasks = Vec::new();
                for nested_step in &nested_steps {
                    subtasks.push(build_single_task_dragonfly(nested_step, hub, default_namespace).await?);
                }

                fork_branches.push(flowr_tasks::ForkBranch {
                    name: branch_name,
                    tasks: subtasks,
                });
            }

            let mut executor = ForkExecutor::new(fork_branches);
            if step.config.get("compete").and_then(|v| v.as_bool()) == Some(true) {
                executor = executor.with_mode(ForkMode::Compete);
            }

            Arc::new(executor)
        }

        "try" => {
            let do_steps = step.config.get("do")
                .ok_or_else(|| FlowError::task_failed(&step.name, "try requires 'do' block"))?;
            let nested_steps = parse_nested_steps_dragonfly(do_steps)?;

            let mut do_tasks = Vec::new();
            for nested_step in &nested_steps {
                do_tasks.push(build_single_task_dragonfly(nested_step, hub, default_namespace).await?);
            }

            let mut executor = TryExecutor::new(do_tasks);

            if let Some(catches) = step.config.get("catch").and_then(|v| v.as_array()) {
                for catch in catches {
                    let error_types: Vec<String> = catch.get("errors")
                        .and_then(|v| v.as_str())
                        .map(|s| vec![s.to_string()])
                        .or_else(|| {
                            catch.get("errors")
                                .and_then(|v| v.as_array())
                                .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
                        })
                        .unwrap_or_else(|| vec!["*".to_string()]);

                    let catch_steps = catch.get("do").or_else(|| catch.get("then"));

                    if let Some(catch_steps) = catch_steps {
                        let nested_steps = parse_nested_steps_dragonfly(catch_steps)?;
                        let mut catch_tasks = Vec::new();
                        for nested_step in &nested_steps {
                            catch_tasks.push(build_single_task_dragonfly(nested_step, hub, default_namespace).await?);
                        }

                        let filter = ErrorFilter { types: error_types };
                        executor = executor.with_catch(CatchBlock::new(filter, catch_tasks));
                    }
                }
            }

            if let Some(retry) = step.config.get("retry") {
                let limit = retry.get("limit").and_then(|v| v.as_u64()).unwrap_or(3) as u32;
                let mut retry_config = RetryConfig::new(limit);
                if let Some(delay_ms) = retry.get("delay_ms").and_then(|v| v.as_u64()) {
                    retry_config.delay = std::time::Duration::from_millis(delay_ms);
                }
                executor = executor.with_retry(retry_config);
            }

            Arc::new(executor)
        }

        "wait" | "listen" => {
            return Err(FlowError::task_failed(
                &step.name,
                format!("task type '{}' not yet supported (requires suspend infrastructure)", step.step_type),
            ));
        }

        "set" => Arc::new(SetExecutor::new(step.config.clone())),
        "call" | "call:http" => {
            let config: CallHttpConfig = serde_json::from_value(step.config.clone())
                .map_err(|e| FlowError::task_failed(&step.name, format!("invalid call:http config: {}", e)))?;
            Arc::new(CallHttpExecutor::new(config))
        }
        "emit" => Arc::new(EmitExecutor::new(step.config.clone())),
        "raise" => Arc::new(RaiseExecutor::new(step.config.clone())),
        "run:shell" => Arc::new(RunShellExecutor::new(step.config.clone())),
        "inline:lua" | "script:lua" => Arc::new(InlineScriptExecutor::new(step.config.clone(), InlineLanguage::Lua)),
        "inline:js" | "inline:javascript" | "script:js" => Arc::new(InlineScriptExecutor::new(step.config.clone(), InlineLanguage::JavaScript)),
        "inline:py" | "inline:python" | "script:python" => Arc::new(InlineScriptExecutor::new(step.config.clone(), InlineLanguage::Python)),
        "call:sql" | "call:mysql" | "call:postgres" => Arc::new(SqlExecutor::new(step.config.clone())),
        "call:redis" | "call:dragonfly" => Arc::new(RedisExecutor::new(step.config.clone())),
        "call:qdrant" => Arc::new(QdrantExecutor::new(step.config.clone())),
        "call:mcp" => Arc::new(McpExecutor::new(step.config.clone())),
        "call:llm" => Arc::new(LlmExecutor::new(step.config.clone())),
        "call:agent" => Arc::new(AgentExecutor::new(step.config.clone())),
        "call:human" => Arc::new(HumanExecutor::new(step.config.clone())),
        "state:get" => Arc::new(StateGetExecutor::new(step.config.clone())),
        "state:set" => Arc::new(StateSetExecutor::new(step.config.clone())),
        "state:delete" => Arc::new(StateDeleteExecutor::new(step.config.clone())),
        "input:file" => Arc::new(InputFileExecutor::new(step.config.clone())),
        "output:file" => Arc::new(OutputFileExecutor::new(step.config.clone())),
        "input:stream" => Arc::new(InputStreamExecutor::new(step.config.clone())),
        "output:stream" => Arc::new(OutputStreamExecutor::new(step.config.clone())),
        other => {
            return Err(FlowError::task_failed(&step.name, format!("unsupported task type: {}", other)));
        }
    };

    let mut task = TaskDefinition::new(&step.name, executor);

    if let Some(ref input_from) = step.input_from {
        task = task.with_input_from(input_from);
    }
    if let Some(ref output_as) = step.output_as {
        task = task.with_output_as(output_as);
    }
    if let Some(ref export_as) = step.export_as {
        task = task.with_export_as(export_as);
    }
    if let Some(ref then) = step.then {
        task = task.with_then(then);
    }
    if step.checkpoint {
        task = task.as_checkpoint();
    }

    // Resolve resource if specified
    if let Some(ref resource_ref) = step.resource {
        let (namespace, name) = if resource_ref.contains('/') {
            let parts: Vec<&str> = resource_ref.splitn(2, '/').collect();
            (parts[0], parts[1])
        } else {
            (default_namespace, resource_ref.as_str())
        };

        match hub.get_resource(namespace, name).await {
            Ok(resource) => {
                debug!(step = %step.name, resource = %resource_ref, "Resolved resource for task");
                task = task.with_resource(resource.config);
            }
            Err(e) => {
                return Err(FlowError::task_failed(
                    &step.name,
                    format!("failed to resolve resource '{}': {}", resource_ref, e),
                ));
            }
        }
    }

    Ok(task)
}

// =============================================================================
// ADB Mode
// =============================================================================

#[cfg(feature = "adb")]
async fn run_adb_mode(cli: Cli) -> anyhow::Result<()> {
    // Create data directory if needed
    std::fs::create_dir_all(&cli.adb_path)?;

    let db_path = format!("{}/flowr.lance", cli.adb_path);
    info!(path = %db_path, "Opening ADB (ClawDB/LanceDB)");

    // Initialize LanceDB storage
    let storage = LanceStorage::new(&db_path)
        .await
        .map_err(|e| anyhow::anyhow!("{}", e))?;
    let storage = Arc::new(storage);

    // Initialize storage tables
    storage.init().await.map_err(|e| anyhow::anyhow!("{}", e))?;
    info!("ADB storage initialized");

    // Create semaphore for concurrency control
    let semaphore = Arc::new(Semaphore::new(cli.concurrency));

    info!(
        concurrency = cli.concurrency,
        "Worker ready, polling for execution requests (ADB mode)"
    );

    // Main polling loop
    loop {
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {
                info!("Received shutdown signal");
                break;
            }
            result = poll_and_execute_adb(&storage, semaphore.clone()) => {
                if let Err(e) = result {
                    error!(error = %e, "Error in poll loop");
                    tokio::time::sleep(Duration::from_secs(1)).await;
                }
            }
        }
    }

    Ok(())
}

#[cfg(feature = "adb")]
async fn poll_and_execute_adb(
    storage: &Arc<LanceStorage>,
    semaphore: Arc<Semaphore>,
) -> anyhow::Result<()> {
    use flowr_storage::ExecutionQueue;

    // Dequeue next execution request (with timeout)
    let request = storage.queue().dequeue(Duration::from_millis(100)).await
        .map_err(|e| anyhow::anyhow!("dequeue failed: {}", e))?;

    let request = match request {
        Some(r) => r,
        None => return Ok(()), // No work available
    };

    info!(
        instance_id = %request.instance_id,
        namespace = %request.namespace,
        name = %request.name,
        "Received execution request"
    );

    // Acquire semaphore permit
    let permit = semaphore.clone().acquire_owned().await?;

    let storage = storage.clone();

    // Spawn task to execute workflow
    tokio::spawn(async move {
        let _permit = permit;

        if let Err(e) = execute_workflow_adb(&storage, request.clone()).await {
            error!(
                instance_id = %request.instance_id,
                namespace = %request.namespace,
                name = %request.name,
                error = %e,
                "Workflow execution failed"
            );
        }
    });

    Ok(())
}

#[cfg(feature = "adb")]
async fn execute_workflow_adb(
    storage: &Arc<LanceStorage>,
    request: ExecutionRequest,
) -> anyhow::Result<()> {
    use flowr_storage::{WorkflowStore, ExecutionQueue};

    info!(
        instance_id = %request.instance_id,
        namespace = %request.namespace,
        name = %request.name,
        version = ?request.version,
        "Starting workflow execution (ADB)"
    );

    // Get workflow record using StorageBackend
    let record: WorkflowRecord = if let Some(version) = &request.version {
        storage.workflows().get_version(&request.namespace, &request.name, version).await
    } else {
        storage.workflows().get(&request.namespace, &request.name).await
    }.map_err(|e| anyhow::anyhow!("failed to get workflow: {}", e))?;

    // Parse CNCF workflow
    let swf = record.parse()
        .map_err(|e| anyhow::anyhow!("Failed to parse CNCF workflow: {}", e))?;

    let definition_meta = &record.metadata;

    // Determine durability mode
    let durability = match request.durability.as_deref() {
        Some("checkpoint") => FlowDurability::Checkpoint,
        _ => FlowDurability::Completion,
    };

    // Build task definitions from CNCF AST
    let lance_resolver = LanceResourceResolver { storage: storage.clone() };
    let tasks = converter::convert_named_tasks(&swf.tasks, &lance_resolver, &request.namespace).await?;

    // Create workflow metadata
    let metadata = WorkflowMetadata::new(
        &request.namespace,
        &request.name,
        &definition_meta.version,
    );

    let workflow_id = format!("{}/{}", request.namespace, request.name);

    // For ADB mode, we need a simplified executor that doesn't require Iggy
    // TODO: Create an ADB-compatible FlowContext
    // For now, log that execution would happen here
    info!(
        instance_id = %request.instance_id,
        workflow_id = %workflow_id,
        task_count = tasks.len(),
        "Would execute workflow (ADB executor not yet implemented)"
    );

    // Acknowledge the request
    storage.queue().ack(request.instance_id).await
        .map_err(|e| anyhow::anyhow!("failed to ack: {}", e))?;

    Ok(())
}

#[cfg(feature = "adb")]
async fn build_tasks_adb(
    workflow: &WorkflowDefinition,
    storage: &Arc<LanceStorage>,
    default_namespace: &str,
) -> FlowResult<Vec<TaskDefinition>> {
    let mut tasks = Vec::with_capacity(workflow.steps.len());

    for step in &workflow.steps {
        let task = build_single_task_adb(step, storage, default_namespace).await?;
        tasks.push(task);
    }

    Ok(tasks)
}

#[cfg(feature = "adb")]
fn parse_nested_steps_adb(value: &serde_json::Value) -> FlowResult<Vec<StepDefinition>> {
    match value {
        serde_json::Value::Array(arr) => {
            let mut steps = Vec::with_capacity(arr.len());
            for item in arr {
                let step: StepDefinition = serde_json::from_value(item.clone())
                    .map_err(|e| FlowError::task_failed("nested", format!("invalid nested step: {}", e)))?;
                steps.push(step);
            }
            Ok(steps)
        }
        serde_json::Value::Null => Ok(Vec::new()),
        _ => Err(FlowError::task_failed("nested", "expected array of steps")),
    }
}

#[cfg(feature = "adb")]
#[async_recursion::async_recursion]
async fn build_single_task_adb(
    step: &StepDefinition,
    storage: &Arc<LanceStorage>,
    default_namespace: &str,
) -> FlowResult<TaskDefinition> {
    use flowr_storage::ResourceStore;

    let executor: Arc<dyn TaskExecutor> = match step.step_type.as_str() {
        "do" => {
            let nested_steps = if let Some(do_steps) = step.config.get("do") {
                parse_nested_steps_adb(do_steps)?
            } else if step.config.is_array() {
                parse_nested_steps_adb(&step.config)?
            } else {
                Vec::new()
            };

            let mut subtasks = Vec::with_capacity(nested_steps.len());
            for nested_step in &nested_steps {
                let subtask = build_single_task_adb(nested_step, storage, default_namespace).await?;
                subtasks.push(subtask);
            }

            Arc::new(DoExecutor::new(subtasks))
        }

        "set" => Arc::new(SetExecutor::new(step.config.clone())),
        "call" | "call:http" => {
            let config: CallHttpConfig = serde_json::from_value(step.config.clone())
                .map_err(|e| FlowError::task_failed(&step.name, format!("invalid call:http config: {}", e)))?;
            Arc::new(CallHttpExecutor::new(config))
        }
        "emit" => Arc::new(EmitExecutor::new(step.config.clone())),
        "raise" => Arc::new(RaiseExecutor::new(step.config.clone())),
        "run:shell" => Arc::new(RunShellExecutor::new(step.config.clone())),
        "call:sql" | "call:mysql" | "call:postgres" => Arc::new(SqlExecutor::new(step.config.clone())),
        "call:redis" | "call:dragonfly" => Arc::new(RedisExecutor::new(step.config.clone())),
        "call:qdrant" => Arc::new(QdrantExecutor::new(step.config.clone())),
        "call:mcp" => Arc::new(McpExecutor::new(step.config.clone())),
        "call:llm" => Arc::new(LlmExecutor::new(step.config.clone())),
        "call:agent" => Arc::new(AgentExecutor::new(step.config.clone())),
        "call:human" => Arc::new(HumanExecutor::new(step.config.clone())),
        "state:get" => Arc::new(StateGetExecutor::new(step.config.clone())),
        "state:set" => Arc::new(StateSetExecutor::new(step.config.clone())),
        "state:delete" => Arc::new(StateDeleteExecutor::new(step.config.clone())),
        "input:file" => Arc::new(InputFileExecutor::new(step.config.clone())),
        "output:file" => Arc::new(OutputFileExecutor::new(step.config.clone())),
        "input:stream" => Arc::new(InputStreamExecutor::new(step.config.clone())),
        "output:stream" => Arc::new(OutputStreamExecutor::new(step.config.clone())),
        other => {
            return Err(FlowError::task_failed(&step.name, format!("unsupported task type: {}", other)));
        }
    };

    let mut task = TaskDefinition::new(&step.name, executor);

    if let Some(ref input_from) = step.input_from {
        task = task.with_input_from(input_from);
    }
    if let Some(ref output_as) = step.output_as {
        task = task.with_output_as(output_as);
    }
    if let Some(ref export_as) = step.export_as {
        task = task.with_export_as(export_as);
    }
    if let Some(ref then) = step.then {
        task = task.with_then(then);
    }
    if step.checkpoint {
        task = task.as_checkpoint();
    }

    // Resolve resource if specified
    if let Some(ref resource_ref) = step.resource {
        let (namespace, name) = if resource_ref.contains('/') {
            let parts: Vec<&str> = resource_ref.splitn(2, '/').collect();
            (parts[0], parts[1])
        } else {
            (default_namespace, resource_ref.as_str())
        };

        match storage.resources().get(namespace, name).await {
            Ok(resource) => {
                debug!(step = %step.name, resource = %resource_ref, "Resolved resource for task");
                task = task.with_resource(resource.config);
            }
            Err(e) => {
                return Err(FlowError::task_failed(
                    &step.name,
                    format!("failed to resolve resource '{}': {}", resource_ref, e),
                ));
            }
        }
    }

    Ok(task)
}
