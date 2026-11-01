//! LanceDB storage backend implementation
//!
//! Single-process storage using LanceDB for persistence.
//! EventBus and ExecutionQueue use in-memory tokio channels.

use std::path::Path;
use std::sync::Arc;

use async_trait::async_trait;
use lancedb::Connection;
use tokio::sync::{broadcast, RwLock};

use flowr_storage::{
    CheckpointStore, EventBus, ExecutionQueue, ExecutionStore, ResourceStore, SettingsStore,
    StateStore, StorageBackend, TriggerStore, WorkflowStore,
};
use flowr_types::{FlowError, FlowResult};

use flowr_storage::engine::EngineStore;

use crate::checkpoints::LanceCheckpointStore;
use crate::engine_adapters::{EngineExecutions, EngineQueue};
use crate::events::InMemoryEventBus;
use crate::executions::LanceExecutionStore;
use crate::queue::InMemoryExecutionQueue;
use crate::resources::LanceResourceStore;
use crate::settings::LanceSettingsStore;
use crate::state::LanceStateStore;
use crate::triggers::LanceTriggerStore;
use crate::workflows::LanceWorkflowStore;

/// LanceDB storage backend for ClawdFlow
///
/// Provides persistent storage using LanceDB with in-memory channels
/// for event pub/sub and execution queue (single-process).
pub struct LanceStorage {
    /// LanceDB connection
    conn: Arc<RwLock<Connection>>,

    /// Workflow store
    workflows: LanceWorkflowStore,

    /// Resource store
    resources: LanceResourceStore,

    /// State store
    state: LanceStateStore,

    /// Checkpoint store
    checkpoints: LanceCheckpointStore,

    /// Event bus (in-memory)
    events: InMemoryEventBus,

    /// Execution queue (in-memory)
    queue: InMemoryExecutionQueue,

    /// Execution store
    executions: LanceExecutionStore,

    /// Settings store
    settings: LanceSettingsStore,

    /// Trigger store
    triggers: LanceTriggerStore,

    /// The durable engine store and today's queue/execution views over it.
    engine: Option<EngineParts>,
}

struct EngineParts {
    store: Arc<dyn EngineStore>,
    queue: EngineQueue,
    executions: EngineExecutions,
}

impl LanceStorage {
    /// Run flows on the durable engine store: enqueueing starts a run there,
    /// execution reads come from it (with older runs still read from LanceDB).
    pub fn with_engine(mut self, store: Arc<dyn EngineStore>) -> Self {
        self.engine = Some(EngineParts {
            queue: EngineQueue::new(store.clone()),
            executions: EngineExecutions::new(store.clone(), LanceExecutionStore::new(self.conn.clone())),
            store,
        });
        self
    }

    /// Get the underlying LanceDB connection
    pub fn connection(&self) -> Arc<RwLock<Connection>> {
        self.conn.clone()
    }

    /// Get the settings store
    pub fn settings(&self) -> &LanceSettingsStore {
        &self.settings
    }

    /// Create a new LanceStorage with the given database path
    pub async fn new(path: impl AsRef<Path>) -> Result<Self, String> {
        let path_str = path.as_ref().to_string_lossy().to_string();
        let conn = lancedb::connect(&path_str)
            .execute()
            .await
            .map_err(|e| format!("Failed to connect to LanceDB: {}", e))?;

        let conn = Arc::new(RwLock::new(conn));

        // Create event channel (broadcast for multiple subscribers)
        let (event_tx, _) = broadcast::channel(1024);

        let state = LanceStateStore::new(conn.clone());
        let triggers = LanceTriggerStore::new(LanceStateStore::new(conn.clone()));

        Ok(Self {
            workflows: LanceWorkflowStore::new(conn.clone()),
            resources: LanceResourceStore::new(conn.clone()),
            state,
            checkpoints: LanceCheckpointStore::new(conn.clone()),
            events: InMemoryEventBus::new(event_tx),
            queue: InMemoryExecutionQueue::new(),
            executions: LanceExecutionStore::new(conn.clone()),
            settings: LanceSettingsStore::new(conn.clone()),
            triggers,
            conn,
            engine: None,
        })
    }

    /// Create an in-memory LanceStorage (for testing)
    pub async fn in_memory() -> Result<Self, String> {
        let conn = lancedb::connect("memory://")
            .execute()
            .await
            .map_err(|e| format!("Failed to create in-memory LanceDB: {}", e))?;

        let conn = Arc::new(RwLock::new(conn));
        let (event_tx, _) = broadcast::channel(1024);

        let state = LanceStateStore::new(conn.clone());
        let triggers = LanceTriggerStore::new(LanceStateStore::new(conn.clone()));

        Ok(Self {
            workflows: LanceWorkflowStore::new(conn.clone()),
            resources: LanceResourceStore::new(conn.clone()),
            state,
            checkpoints: LanceCheckpointStore::new(conn.clone()),
            events: InMemoryEventBus::new(event_tx),
            queue: InMemoryExecutionQueue::new(),
            executions: LanceExecutionStore::new(conn.clone()),
            settings: LanceSettingsStore::new(conn.clone()),
            triggers,
            conn,
            engine: None,
        })
    }
}

#[async_trait]
impl StorageBackend for LanceStorage {
    fn workflows(&self) -> &dyn WorkflowStore {
        &self.workflows
    }

    fn resources(&self) -> &dyn ResourceStore {
        &self.resources
    }

    fn state(&self) -> &dyn StateStore {
        &self.state
    }

    fn checkpoints(&self) -> &dyn CheckpointStore {
        &self.checkpoints
    }

    fn events(&self) -> &dyn EventBus {
        &self.events
    }

    fn queue(&self) -> &dyn ExecutionQueue {
        match &self.engine {
            Some(e) => &e.queue,
            None => &self.queue,
        }
    }

    fn executions(&self) -> &dyn ExecutionStore {
        match &self.engine {
            Some(e) => &e.executions,
            None => &self.executions,
        }
    }

    fn engine(&self) -> Option<Arc<dyn EngineStore>> {
        self.engine.as_ref().map(|e| e.store.clone())
    }

    fn triggers(&self) -> &dyn TriggerStore {
        &self.triggers
    }

    fn settings(&self) -> &dyn SettingsStore {
        &self.settings
    }

    async fn init(&self) -> FlowResult<()> {
        // Initialize all tables
        self.workflows.init().await?;
        self.resources.init().await?;
        self.state.init().await?;
        self.checkpoints.init().await?;
        self.executions.init().await?;
        self.settings.init().await?;
        Ok(())
    }

    async fn close(&self) -> FlowResult<()> {
        // LanceDB handles cleanup automatically
        Ok(())
    }

    /// Compact the executions table and prune old versions.
    ///
    /// LanceDB never rewrites in place: every write appends a new data file, and
    /// an execution writes its row several times (queued, running, completed —
    /// each an update, i.e. delete + append). Uncompacted, the table had grown
    /// to 2,006 data files across 556 versions, so every read opened and scanned
    /// all of them: 0.5-0.7 s per access, and a page load touches this table 4-6
    /// times. A list that runs in 14 ms took 2-4 s.
    ///
    /// Only the executions table is compacted here, because only the worker
    /// writes it and the worker calls this with no execution in flight —
    /// compaction rewrites fragments, and a concurrent update to the same
    /// fragment would fail its commit. Other tables are written by request
    /// handlers at any moment and are small; they are left alone.
    ///
    async fn maintain(&self) -> FlowResult<String> {
        let conn = self.conn.read().await;
        let table = match conn.open_table("executions").execute().await {
            Ok(t) => t,
            // No table yet means nothing to compact.
            Err(_) => return Ok(String::new()),
        };
        let started = std::time::Instant::now();
        let compact = table
            .optimize(lancedb::table::OptimizeAction::Compact {
                options: Default::default(),
                remap_options: None,
            })
            .await
            .map_err(|e| FlowError::Storage(format!("executions compaction failed: {e}")))?;

        // Prune old versions AGGRESSIVELY, not LanceDB's default seven days.
        //
        // Retained versions were the second cost: LanceDB's auto-cleanup hook
        // fires every 20th commit and walks every retained version. With 558
        // kept, that walk stalled one status write in ~5 for ~2 s — the slow
        // outliers left after compaction. It also kept 120 MB of dead files.
        //
        // Ten minutes is safe here: this runs with no execution in flight (the
        // worker is the only writer, and it is blocked running this), and
        // readers always open the latest version, held for milliseconds.
        // `delete_unverified` is needed because LanceDB otherwise refuses to
        // delete anything newer than seven days in case a transaction is open —
        // which the in-flight check rules out.
        let prune = table
            .optimize(lancedb::table::OptimizeAction::Prune {
                older_than: Some(chrono::Duration::minutes(10)),
                delete_unverified: Some(true),
                error_if_tagged_old_versions: Some(false),
            })
            .await
            .map_err(|e| FlowError::Storage(format!("executions version pruning failed: {e}")))?;

        let mut out = format!("executions optimized in {:.1}s", started.elapsed().as_secs_f32());
        if let Some(c) = compact.compaction {
            out.push_str(&format!(", {} files -> {}", c.fragments_removed, c.fragments_added));
        }
        if let Some(p) = prune.prune {
            out.push_str(&format!(
                ", {} old versions pruned ({:.1} MB freed)",
                p.old_versions,
                p.bytes_removed as f64 / 1_048_576.0
            ));
        }
        Ok(out)
    }
}
