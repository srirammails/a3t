//! FlowR's registry and agent state on a shared platform database (ADB MVP W2c).
//!
//! A console FlowR and the runtime pods it deploys must see the same flows,
//! resources, agent configs and chat sessions. [`SharedRegistryStorage`] wraps a
//! FlowR's local storage and moves those onto platform tables (ADB over Arrow
//! Flight in EE, see [`PlatformDb`]); executions, the queue, checkpoints, triggers
//! and settings stay local to each FlowR.
//!
//! | Store | Table | Columns (all UTF-8) |
//! |---|---|---|
//! | workflows | `flowr_workflows` | namespace, name, version, active_version, enabled, definition, created_at, updated_at |
//! | resource types | `flowr_resource_types` | name, definition |
//! | resources | `flowr_resources` | namespace, name, type_ref, config, created_at, updated_at |
//! | state keys `agent:*` (+ aliases) | `flowr_state` | key, value, expires_at, created_at |
//!
//! The columns are the local LanceDB tables', so [`seed`] copies them across as is.
//! Writes are not transactions: a `set` is a delete then an insert, and two FlowRs
//! writing the same key race (last write wins). Enough for one console plus
//! runtimes that each own their keys; ADB-side upsert is a follow-up.

use std::sync::Arc;
use std::time::Duration;

use arrow_array::RecordBatch;
use async_trait::async_trait;
use serde_json::{json, Map, Value};
use tracing::info;

use flowr_storage::{
    CheckpointStore, EventBus, ExecutionQueue, ExecutionStore, ResourceStore, SettingsStore, StateStore,
    StorageBackend, TriggerStore, WorkflowStore,
};
use flowr_types::{FlowError, FlowResult, Resource, ResourceType, WorkflowRecord, WorkflowSummary};

use crate::handlers::call_lancedb::{batches_to_json, sql_quote};
use crate::platform_db::{LanceDbPlatform, PlatformDb};

pub const WORKFLOWS: &str = "flowr_workflows";
pub const RESOURCE_TYPES: &str = "flowr_resource_types";
pub const RESOURCES: &str = "flowr_resources";
pub const STATE: &str = "flowr_state";
pub const SETTINGS: &str = "flowr_settings";

const WORKFLOW_COLUMNS: &[&str] =
    &["namespace", "name", "version", "active_version", "enabled", "definition", "created_at", "updated_at"];
const RESOURCE_TYPE_COLUMNS: &[&str] = &["name", "definition"];
const RESOURCE_COLUMNS: &[&str] = &["namespace", "name", "type_ref", "config", "created_at", "updated_at"];
const STATE_COLUMNS: &[&str] = &["key", "value", "expires_at", "created_at"];
const SETTINGS_COLUMNS: &[&str] = &["key", "value", "source", "updated_at"];

/// Shared-state marker: this deployment's console-local state and settings have
/// been copied to the shared tables once ([`migrate_local`]).
const MIGRATED_KEY: &str = "_cluster:local_state_migrated";

/// State keys that live in the shared table: agent configs, transcripts and the
/// per-user session index.
const SHARED_STATE_PREFIX: &str = "agent:";

/// Big enough for "all rows" (Flight `do_get` takes a limit).
const ALL: usize = 1 << 30;

fn storage_err(e: String) -> FlowError {
    FlowError::Storage(e)
}

fn now() -> String {
    chrono::Utc::now().to_rfc3339()
}

/// The string column `col` of every row.
fn column<'a>(rows: &'a [Value], col: &str) -> impl Iterator<Item = &'a str> + 'a {
    let col = col.to_string();
    rows.iter().map(move |r| r.get(&col).and_then(|v| v.as_str()).unwrap_or_default())
}

/// A LanceDB-SQL LIKE pattern from a state-store glob (`*`, `?`).
fn like(pattern: &str) -> String {
    sql_quote(&pattern.replace('*', "%").replace('?', "_"))
}

// ---------------------------------------------------------------- storage

/// A FlowR's storage with the registry and agent state on `db`.
pub struct SharedRegistryStorage {
    inner: Arc<dyn StorageBackend>,
    workflows: SharedWorkflows,
    resources: SharedResources,
    state: SplitState,
    triggers: SharedTriggers,
    /// Settings on `db` too (cluster mode, with every state key shared).
    settings: Option<SharedSettings>,
    db: Arc<dyn PlatformDb>,
}

impl SharedRegistryStorage {
    /// `aliases` maps local state keys to shared ones, e.g. a runtime's
    /// `_triggers` → `runtime:<canvas>:triggers`, so a restarted runtime pod
    /// finds its trigger (and the subscription it acked) again.
    pub fn new(inner: Arc<dyn StorageBackend>, db: Arc<dyn PlatformDb>, aliases: Vec<(String, String)>) -> Self {
        Self {
            workflows: SharedWorkflows { db: Arc::clone(&db) },
            resources: SharedResources { db: Arc::clone(&db) },
            state: SplitState { local: Arc::clone(&inner), shared: SharedState { db: Arc::clone(&db) }, aliases: aliases.clone(), all: false },
            triggers: SharedTriggers {
                state: SplitState { local: Arc::clone(&inner), shared: SharedState { db: Arc::clone(&db) }, aliases, all: false },
            },
            settings: None,
            inner,
            db,
        }
    }

    pub fn describe(&self) -> String {
        self.db.describe()
    }

    /// Every state key on `db`, not only `agent:*`, and the settings and
    /// triggers with them: CE on its embedded ADB (one process), and EE in a
    /// cluster, where every FlowR sharing the ADB must see the same state.
    /// Aliased keys (a runtime's own `_triggers`) still map to their alias.
    pub fn all_state(mut self) -> Self {
        self.state.all = true;
        self.triggers.state.all = true;
        self.settings = Some(SharedSettings { db: Arc::clone(&self.db) });
        self
    }

    /// Reload the trigger set from the shared state (another FlowR may have
    /// changed it). Returns whether anything changed.
    pub async fn reload_triggers(&self) -> FlowResult<bool> {
        self.triggers.reload().await
    }
}

#[async_trait]
impl StorageBackend for SharedRegistryStorage {
    fn settings(&self) -> &dyn SettingsStore {
        match &self.settings {
            Some(s) => s,
            None => self.inner.settings(),
        }
    }
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
        self.inner.checkpoints()
    }
    fn events(&self) -> &dyn EventBus {
        self.inner.events()
    }
    fn queue(&self) -> &dyn ExecutionQueue {
        self.inner.queue()
    }
    fn executions(&self) -> &dyn ExecutionStore {
        self.inner.executions()
    }
    fn triggers(&self) -> &dyn TriggerStore {
        &self.triggers
    }
    fn engine(&self) -> Option<std::sync::Arc<dyn flowr_storage::engine::EngineStore>> {
        self.inner.engine()
    }

    /// Creates the shared tables (if missing) after the local ones.
    async fn init(&self) -> FlowResult<()> {
        self.inner.init().await?;
        for (table, cols) in [
            (WORKFLOWS, WORKFLOW_COLUMNS),
            (RESOURCE_TYPES, RESOURCE_TYPE_COLUMNS),
            (RESOURCES, RESOURCE_COLUMNS),
            (STATE, STATE_COLUMNS),
            (SETTINGS, SETTINGS_COLUMNS),
        ] {
            let schema: Map<String, Value> = cols.iter().map(|c| (c.to_string(), json!("string"))).collect();
            self.db.create_table(table, Some(&schema), "create_if_not_exists").await.map_err(storage_err)?;
        }
        Ok(())
    }

    async fn close(&self) -> FlowResult<()> {
        self.inner.close().await
    }

    async fn maintain(&self) -> FlowResult<String> {
        self.inner.maintain().await
    }
}

/// First boot on a shared registry: if it has no workflows and no resources yet,
/// copy this FlowR's local ones (its LanceDB at `local_path`) across. Returns the
/// rows copied per table; empty when the shared registry already had data (a
/// runtime, or a console that seeded before).
pub async fn seed(db: &dyn PlatformDb, local_path: &str) -> Result<Vec<(String, usize)>, String> {
    seed_with(db, local_path, false).await
}

/// [`seed`]; `all_state`: every state key, not only `agent:*`.
pub async fn seed_with(db: &dyn PlatformDb, local_path: &str, all_state: bool) -> Result<Vec<(String, usize)>, String> {
    let has = |t: &'static str| async move { db.query(t, None, 1).await.map(|b| b.iter().any(|b| b.num_rows() > 0)) };
    if has(WORKFLOWS).await? || has(RESOURCES).await? {
        return Ok(Vec::new());
    }
    let local = LanceDbPlatform::new(local_path);
    let tables = local.list_tables().await?;
    let mut copied = Vec::new();
    for (from, to, cols, filter) in [
        ("workflows", WORKFLOWS, WORKFLOW_COLUMNS, None),
        ("resource_types", RESOURCE_TYPES, RESOURCE_TYPE_COLUMNS, None),
        ("resources", RESOURCES, RESOURCE_COLUMNS, None),
        ("state", STATE, STATE_COLUMNS, (!all_state).then(|| format!("key LIKE '{SHARED_STATE_PREFIX}%'"))),
    ] {
        if !tables.iter().any(|t| t == from) {
            continue;
        }
        let rows = batches_to_json(&local.query(from, filter.as_deref(), ALL).await?);
        let columns: Vec<String> = cols.iter().map(|c| c.to_string()).collect();
        db.replace_table(to, &columns, &rows).await.map_err(|e| format!("seeding {to}: {e}"))?;
        copied.push((to.to_string(), rows.len()));
    }
    info!(from = %local_path, to = %db.describe(), ?copied, "shared registry seeded from the local registry");
    Ok(copied)
}

/// Of a flow's version rows, the active one: the row whose version is the
/// `active_version` they record, else the most recently updated.
fn active_row(rows: &[Value]) -> Option<&Value> {
    let s = |r: &Value, k: &str| r.get(k).and_then(|v| v.as_str()).unwrap_or_default().to_string();
    let newest = rows.iter().max_by_key(|r| s(r, "updated_at"))?;
    let active = s(newest, "active_version");
    rows.iter().find(|r| !active.is_empty() && s(r, "version") == active).or(Some(newest))
}

async fn rows(db: &dyn PlatformDb, table: &str, filter: &str) -> FlowResult<Vec<Value>> {
    let batches: Vec<RecordBatch> = db.query(table, Some(filter), ALL).await.map_err(storage_err)?;
    Ok(batches_to_json(&batches))
}

/// A table's distinct namespaces, sorted.
async fn namespaces(db: &dyn PlatformDb, table: &str) -> FlowResult<Vec<String>> {
    let batches: Vec<RecordBatch> = db.query(table, None, ALL).await.map_err(storage_err)?;
    let rows = batches_to_json(&batches);
    let set: std::collections::BTreeSet<String> = column(&rows, "namespace").filter(|n| !n.is_empty()).map(String::from).collect();
    Ok(set.into_iter().collect())
}

async fn insert(db: &dyn PlatformDb, table: &str, row: Value) -> FlowResult<()> {
    db.insert(table, &[row]).await.map(|_| ()).map_err(storage_err)
}

// ---------------------------------------------------------------- workflows

struct SharedWorkflows {
    db: Arc<dyn PlatformDb>,
}

fn flow_filter(namespace: &str, name: &str) -> String {
    format!("namespace = {} AND name = {}", sql_quote(namespace), sql_quote(name))
}

fn parse_workflow(row: &Value) -> FlowResult<WorkflowRecord> {
    let def = row.get("definition").and_then(|d| d.as_str()).unwrap_or_default();
    serde_json::from_str(def).map_err(|e| FlowError::Storage(format!("Failed to deserialize workflow: {e}")))
}

#[async_trait]
impl WorkflowStore for SharedWorkflows {
    /// Replaces the same version if it was registered before.
    async fn register(&self, record: WorkflowRecord) -> FlowResult<()> {
        let (ns, name, version) = (&record.metadata.namespace, &record.metadata.name, &record.metadata.version);
        let def = serde_json::to_string(&record).map_err(|e| FlowError::Storage(format!("Failed to serialize workflow: {e}")))?;
        let same = format!("{} AND version = {}", flow_filter(ns, name), sql_quote(version));
        let created = rows(&*self.db, WORKFLOWS, &same).await?.first()
            .and_then(|r| r.get("created_at").and_then(|c| c.as_str()).map(String::from))
            .unwrap_or_else(now);
        self.db.delete(WORKFLOWS, &same).await.map_err(storage_err)?;
        insert(&*self.db, WORKFLOWS, json!({
            "namespace": ns, "name": name, "version": version, "active_version": version,
            "enabled": "true", "definition": def, "created_at": created, "updated_at": now(),
        })).await?;
        // Other versions stay (callers can pin them); the new one is active.
        self.set_active_version(ns, name, version).await
    }

    /// The active version (`active_version`), else the most recently registered.
    async fn get(&self, namespace: &str, name: &str) -> FlowResult<WorkflowRecord> {
        let rows = rows(&*self.db, WORKFLOWS, &format!("{} AND enabled = 'true'", flow_filter(namespace, name))).await?;
        match active_row(&rows) {
            Some(r) => parse_workflow(r),
            None => Err(FlowError::NotFound(format!("Workflow {namespace}/{name} not found"))),
        }
    }

    async fn get_version(&self, namespace: &str, name: &str, version: &str) -> FlowResult<WorkflowRecord> {
        let filter = format!("{} AND version = {}", flow_filter(namespace, name), sql_quote(version));
        match rows(&*self.db, WORKFLOWS, &filter).await?.first() {
            Some(r) => parse_workflow(r),
            None => Err(FlowError::NotFound(format!("Workflow {namespace}/{name} version {version} not found"))),
        }
    }

    async fn namespaces(&self) -> FlowResult<Vec<String>> {
        namespaces(&*self.db, WORKFLOWS).await
    }

    async fn versions(&self, namespace: &str, name: &str) -> FlowResult<(Vec<String>, String)> {
        let rows = rows(&*self.db, WORKFLOWS, &flow_filter(namespace, name)).await?;
        let active = active_row(&rows).and_then(|r| r.get("version").and_then(|v| v.as_str())).unwrap_or_default().to_string();
        let mut v: Vec<String> = rows.iter().filter_map(|r| r.get("version").and_then(|v| v.as_str()).map(String::from)).collect();
        v.sort();
        v.dedup();
        Ok((v, active))
    }

    async fn list(&self, namespace: &str) -> FlowResult<Vec<WorkflowSummary>> {
        let all = rows(&*self.db, WORKFLOWS, &format!("namespace = {}", sql_quote(namespace))).await?;
        // One entry per flow: its active version.
        let mut by_name: std::collections::BTreeMap<String, Vec<Value>> = Default::default();
        for r in all {
            by_name.entry(r.get("name").and_then(|n| n.as_str()).unwrap_or_default().to_string()).or_default().push(r);
        }
        let rows: Vec<Value> = by_name.values().filter_map(|v| active_row(v).cloned()).collect();
        Ok(rows.iter().filter_map(|r| parse_workflow(r).ok()).map(|record| WorkflowSummary {
            step_count: record.parse().map(|wf| wf.tasks.len()).unwrap_or(0),
            namespace: record.metadata.namespace.clone(),
            name: record.metadata.name.clone(),
            version: record.metadata.version.clone(),
            enabled: record.metadata.enabled,
        }).collect())
    }

    async fn delete(&self, namespace: &str, name: &str) -> FlowResult<()> {
        self.db.delete(WORKFLOWS, &flow_filter(namespace, name)).await.map_err(storage_err)
    }

    async fn set_enabled(&self, namespace: &str, name: &str, enabled: bool) -> FlowResult<()> {
        let mut values = Map::new();
        values.insert("enabled".into(), json!(enabled.to_string()));
        self.db.update(WORKFLOWS, &flow_filter(namespace, name), &values).await.map_err(storage_err)
    }

    async fn set_active_version(&self, namespace: &str, name: &str, version: &str) -> FlowResult<()> {
        let mut values = Map::new();
        values.insert("active_version".into(), json!(version));
        self.db.update(WORKFLOWS, &flow_filter(namespace, name), &values).await.map_err(storage_err)
    }

    async fn exists(&self, namespace: &str, name: &str) -> FlowResult<bool> {
        match self.get(namespace, name).await {
            Ok(_) => Ok(true),
            Err(FlowError::NotFound(_)) => Ok(false),
            Err(e) => Err(e),
        }
    }
}

// ---------------------------------------------------------------- resources

struct SharedResources {
    db: Arc<dyn PlatformDb>,
}

fn parse_resource(row: &Value) -> Option<Resource> {
    let config: Value = serde_json::from_str(row.get("config")?.as_str()?).ok()?;
    let ns = row.get("namespace")?.as_str()?;
    Some(Resource::new(row.get("name")?.as_str()?, row.get("type_ref")?.as_str()?, config).with_namespace(ns))
}

#[async_trait]
impl ResourceStore for SharedResources {
    async fn register_type(&self, resource_type: ResourceType) -> FlowResult<()> {
        let def = serde_json::to_string(&resource_type)
            .map_err(|e| FlowError::Storage(format!("Failed to serialize resource type: {e}")))?;
        let filter = format!("name = {}", sql_quote(&resource_type.name));
        self.db.delete(RESOURCE_TYPES, &filter).await.map_err(storage_err)?;
        insert(&*self.db, RESOURCE_TYPES, json!({ "name": resource_type.name, "definition": def })).await
    }

    async fn get_type(&self, name: &str) -> FlowResult<ResourceType> {
        let rows = rows(&*self.db, RESOURCE_TYPES, &format!("name = {}", sql_quote(name))).await?;
        let def = column(&rows, "definition").next()
            .ok_or_else(|| FlowError::NotFound(format!("Resource type {name} not found")))?;
        serde_json::from_str(def).map_err(|e| FlowError::Storage(format!("Failed to deserialize resource type: {e}")))
    }

    async fn list_types(&self) -> FlowResult<Vec<ResourceType>> {
        let batches = self.db.query(RESOURCE_TYPES, None, ALL).await.map_err(storage_err)?;
        let rows = batches_to_json(&batches);
        Ok(column(&rows, "definition").filter_map(|d| serde_json::from_str(d).ok()).collect())
    }

    async fn create(&self, resource: Resource) -> FlowResult<Resource> {
        let config = serde_json::to_string(&resource.config)
            .map_err(|e| FlowError::Storage(format!("Failed to serialize config: {e}")))?;
        let t = now();
        insert(&*self.db, RESOURCES, json!({
            "namespace": resource.namespace, "name": resource.name, "type_ref": resource.type_ref,
            "config": config, "created_at": t, "updated_at": t,
        })).await?;
        Ok(resource)
    }

    async fn get(&self, namespace: &str, name: &str) -> FlowResult<Resource> {
        let rows = rows(&*self.db, RESOURCES, &flow_filter(namespace, name)).await?;
        rows.first().and_then(parse_resource)
            .ok_or_else(|| FlowError::NotFound(format!("Resource {namespace}/{name} not found")))
    }

    async fn namespaces(&self) -> FlowResult<Vec<String>> {
        namespaces(&*self.db, RESOURCES).await
    }

    async fn list(&self, namespace: &str) -> FlowResult<Vec<Resource>> {
        let rows = rows(&*self.db, RESOURCES, &format!("namespace = {}", sql_quote(namespace))).await?;
        Ok(rows.iter().filter_map(parse_resource).collect())
    }

    /// In place: one update, so a concurrent reader never sees the resource missing.
    async fn update(&self, namespace: &str, name: &str, config: Value) -> FlowResult<Resource> {
        let existing = self.get(namespace, name).await?;
        let mut values = Map::new();
        values.insert("config".into(), json!(serde_json::to_string(&config).unwrap_or_default()));
        values.insert("updated_at".into(), json!(now()));
        self.db.update(RESOURCES, &flow_filter(namespace, name), &values).await.map_err(storage_err)?;
        Ok(Resource::new(name, existing.type_ref, config).with_namespace(namespace))
    }

    async fn delete(&self, namespace: &str, name: &str) -> FlowResult<()> {
        self.db.delete(RESOURCES, &flow_filter(namespace, name)).await.map_err(storage_err)
    }

    async fn exists(&self, namespace: &str, name: &str) -> FlowResult<bool> {
        match self.get(namespace, name).await {
            Ok(_) => Ok(true),
            Err(FlowError::NotFound(_)) => Ok(false),
            Err(e) => Err(e),
        }
    }
}

// ---------------------------------------------------------------- state

/// `agent:*` keys (and aliased ones) in the shared table, the rest local.
#[derive(Clone)]
struct SplitState {
    local: Arc<dyn StorageBackend>,
    shared: SharedState,
    aliases: Vec<(String, String)>,
    /// Every key is shared.
    all: bool,
}

impl SplitState {
    /// The shared key for `key`, or `None` when it stays local.
    fn shared_key(&self, key: &str) -> Option<String> {
        if let Some((_, to)) = self.aliases.iter().find(|(from, _)| from == key) {
            return Some(to.clone());
        }
        if self.all {
            return Some(key.to_string());
        }
        key.starts_with(SHARED_STATE_PREFIX).then(|| key.to_string())
    }
}

#[async_trait]
impl StateStore for SplitState {
    async fn get(&self, key: &str) -> FlowResult<Option<Value>> {
        match self.shared_key(key) {
            Some(k) => self.shared.get(&k).await,
            None => self.local.state().get(key).await,
        }
    }

    async fn set(&self, key: &str, value: Value, ttl: Option<Duration>) -> FlowResult<()> {
        match self.shared_key(key) {
            Some(k) => self.shared.set(&k, value, ttl).await,
            None => self.local.state().set(key, value, ttl).await,
        }
    }

    async fn delete(&self, key: &str) -> FlowResult<bool> {
        match self.shared_key(key) {
            Some(k) => self.shared.delete(&k).await,
            None => self.local.state().delete(key).await,
        }
    }

    /// Both sides; shared keys only when the pattern could match `agent:*`.
    async fn keys(&self, pattern: &str) -> FlowResult<Vec<String>> {
        if self.all {
            return self.shared.keys(pattern).await;
        }
        let mut keys = self.local.state().keys(pattern).await?;
        let lead: String = pattern.chars().take_while(|c| *c != '*' && *c != '?').collect();
        if lead.starts_with(SHARED_STATE_PREFIX) || SHARED_STATE_PREFIX.starts_with(&lead) {
            keys.extend(self.shared.keys(pattern).await?);
        }
        Ok(keys)
    }

    async fn exists(&self, key: &str) -> FlowResult<bool> {
        Ok(self.get(key).await?.is_some())
    }

    async fn incr(&self, key: &str, delta: i64) -> FlowResult<i64> {
        match self.shared_key(key) {
            Some(k) => {
                // Not atomic across FlowRs; no shared key is a counter today.
                let n = self.shared.get(&k).await?.and_then(|v| v.as_i64()).unwrap_or(0) + delta;
                self.shared.set(&k, json!(n), None).await?;
                Ok(n)
            }
            None => self.local.state().incr(key, delta).await,
        }
    }
}

#[derive(Clone)]
struct SharedState {
    db: Arc<dyn PlatformDb>,
}

impl SharedState {
    fn key_filter(key: &str) -> String {
        format!("key = {}", sql_quote(key))
    }

    async fn get(&self, key: &str) -> FlowResult<Option<Value>> {
        let rows = rows(&*self.db, STATE, &Self::key_filter(key)).await?;
        let Some(row) = rows.first() else { return Ok(None) };
        let expires = row.get("expires_at").and_then(|e| e.as_str()).unwrap_or_default();
        if let Ok(at) = chrono::DateTime::parse_from_rfc3339(expires) {
            if at < chrono::Utc::now() {
                let _ = self.delete(key).await;
                return Ok(None);
            }
        }
        let value = row.get("value").and_then(|v| v.as_str()).unwrap_or("null");
        serde_json::from_str(value).map(Some).map_err(|e| FlowError::Storage(format!("Failed to deserialize value: {e}")))
    }

    async fn set(&self, key: &str, value: Value, ttl: Option<Duration>) -> FlowResult<()> {
        let now_t = chrono::Utc::now();
        let expires = ttl
            .and_then(|d| chrono::Duration::from_std(d).ok())
            .map(|d| (now_t + d).to_rfc3339())
            .unwrap_or_default();
        let value = serde_json::to_string(&value).map_err(|e| FlowError::Storage(format!("Failed to serialize value: {e}")))?;
        self.db.delete(STATE, &Self::key_filter(key)).await.map_err(storage_err)?;
        insert(&*self.db, STATE, json!({ "key": key, "value": value, "expires_at": expires, "created_at": now_t.to_rfc3339() })).await
    }

    async fn delete(&self, key: &str) -> FlowResult<bool> {
        let existed = !rows(&*self.db, STATE, &Self::key_filter(key)).await?.is_empty();
        if existed {
            self.db.delete(STATE, &Self::key_filter(key)).await.map_err(storage_err)?;
        }
        Ok(existed)
    }

    async fn keys(&self, pattern: &str) -> FlowResult<Vec<String>> {
        let rows = rows(&*self.db, STATE, &format!("key LIKE {}", like(pattern))).await?;
        Ok(column(&rows, "key").map(String::from).collect())
    }
}

// ---------------------------------------------------------------- triggers

/// The trigger store over the shared state: the set is the `_triggers` key
/// (or the runtime's alias for it), and this process's trigger manager is a
/// cache of it, reloaded before every read or change. Two FlowRs changing
/// triggers at the same moment still race on the one key (last write wins;
/// limitation CA-1).
struct SharedTriggers {
    state: SplitState,
}

fn trigger_err(e: impl std::fmt::Display) -> FlowError {
    FlowError::task_failed("trigger", e.to_string())
}

impl SharedTriggers {
    async fn reload(&self) -> FlowResult<bool> {
        let data = self.state.get("_triggers").await?.unwrap_or_else(|| json!([]));
        flowr_ce_agents::trigger_manager().replace_all(&data).await.map_err(trigger_err)
    }

    async fn persist(&self) -> FlowResult<()> {
        let data = flowr_ce_agents::trigger_manager().export_all().await;
        self.state.set("_triggers", data, None).await
    }
}

#[async_trait]
impl TriggerStore for SharedTriggers {
    async fn list(&self, namespace: Option<&str>) -> FlowResult<Vec<Value>> {
        self.reload().await?;
        Ok(flowr_ce_agents::trigger_manager()
            .list()
            .await
            .into_iter()
            .filter(|t| namespace.map_or(true, |ns| t.namespace == ns))
            .map(|t| serde_json::to_value(t).unwrap_or(Value::Null))
            .collect())
    }

    async fn get(&self, id: &str) -> FlowResult<Option<Value>> {
        self.reload().await?;
        Ok(flowr_ce_agents::trigger_manager().get(id).await.map(|t| serde_json::to_value(t).unwrap_or(Value::Null)))
    }

    async fn create(&self, trigger: Value) -> FlowResult<String> {
        let trigger: flowr_ce_agents::Trigger =
            serde_json::from_value(trigger).map_err(|e| trigger_err(format!("Invalid trigger: {e}")))?;
        self.reload().await?;
        let id = flowr_ce_agents::trigger_manager().create(trigger).await.map_err(trigger_err)?;
        self.persist().await?;
        Ok(id)
    }

    async fn delete(&self, id: &str) -> FlowResult<()> {
        self.reload().await?;
        flowr_ce_agents::trigger_manager().delete(id).await.map_err(trigger_err)?;
        self.persist().await
    }

    async fn enable(&self, id: &str) -> FlowResult<()> {
        self.reload().await?;
        flowr_ce_agents::trigger_manager().enable(id).await.map_err(trigger_err)?;
        self.persist().await
    }

    async fn pause(&self, id: &str) -> FlowResult<()> {
        self.reload().await?;
        flowr_ce_agents::trigger_manager().pause(id).await.map_err(trigger_err)?;
        self.persist().await
    }

    async fn update(&self, id: &str, trigger: Value) -> FlowResult<()> {
        let mut next: flowr_ce_agents::Trigger =
            serde_json::from_value(trigger).map_err(|e| trigger_err(format!("Invalid trigger: {e}")))?;
        next.id = id.to_string();
        self.reload().await?;
        let manager = flowr_ce_agents::trigger_manager();
        let _ = manager.delete(id).await;
        manager.create(next).await.map_err(trigger_err)?;
        self.persist().await
    }

    async fn attach(&self, trigger_id: &str, target: Value) -> FlowResult<()> {
        let target: flowr_ce_agents::TriggerTarget =
            serde_json::from_value(target).map_err(|e| trigger_err(format!("Invalid target: {e}")))?;
        self.reload().await?;
        flowr_ce_agents::trigger_manager().attach(trigger_id, target).await.map_err(trigger_err)?;
        self.persist().await
    }

    async fn detach(&self, trigger_id: &str, target_id: &str) -> FlowResult<()> {
        self.reload().await?;
        flowr_ce_agents::trigger_manager().detach(trigger_id, target_id).await.map_err(trigger_err)?;
        self.persist().await
    }
}

// ---------------------------------------------------------------- settings

/// Settings on the shared table `flowr_settings` (cluster mode).
struct SharedSettings {
    db: Arc<dyn PlatformDb>,
}

impl SharedSettings {
    fn decode(row: &Value) -> Option<(String, Value)> {
        let key = row.get("key")?.as_str()?.to_string();
        let raw = row.get("value").and_then(|v| v.as_str()).unwrap_or("null");
        Some((key, serde_json::from_str(raw).unwrap_or(Value::String(raw.to_string()))))
    }
}

#[async_trait]
impl SettingsStore for SharedSettings {
    async fn get(&self, key: &str) -> FlowResult<Option<Value>> {
        let rows = rows(&*self.db, SETTINGS, &format!("key = {}", sql_quote(key))).await?;
        Ok(rows.first().and_then(Self::decode).map(|(_, v)| v))
    }

    async fn get_string(&self, key: &str) -> FlowResult<Option<String>> {
        Ok(self.get(key).await?.map(|v| match v {
            Value::String(s) => s,
            other => other.to_string(),
        }))
    }

    async fn set(&self, key: &str, value: &Value, source: &str) -> FlowResult<()> {
        let value = serde_json::to_string(value).map_err(|e| FlowError::Storage(format!("Failed to serialize setting: {e}")))?;
        self.db.delete(SETTINGS, &format!("key = {}", sql_quote(key))).await.map_err(storage_err)?;
        insert(&*self.db, SETTINGS, json!({ "key": key, "value": value, "source": source, "updated_at": now() })).await
    }

    async fn get_all(&self) -> FlowResult<Vec<(String, Value)>> {
        let batches: Vec<RecordBatch> = self.db.query(SETTINGS, None, ALL).await.map_err(storage_err)?;
        let mut all: Vec<(String, Value)> = batches_to_json(&batches).iter().filter_map(Self::decode).collect();
        all.sort_by(|a, b| a.0.cmp(&b.0));
        Ok(all)
    }

    async fn seed_defaults(&self, defaults: &[(&str, Value, &str)]) -> FlowResult<usize> {
        let mut n = 0;
        for (key, value, source) in defaults {
            if self.get(key).await?.is_none() {
                self.set(key, value, source).await?;
                n += 1;
            }
        }
        Ok(n)
    }
}

/// Cluster mode, first start: copy the state keys and settings this FlowR kept
/// on its own disk (LanceDB at `local_path`) to the shared tables, keys the
/// shared side lacks only. Done once per deployment (marker key), so a stale
/// disk on a later replica cannot bring back what was deleted since.
pub async fn migrate_local(db: &dyn PlatformDb, local_path: &str) -> Result<Vec<(String, usize)>, String> {
    let marker = batches_to_json(&db.query(STATE, Some(&format!("key = {}", sql_quote(MIGRATED_KEY))), 1).await?);
    if !marker.is_empty() {
        return Ok(Vec::new());
    }
    let local = LanceDbPlatform::new(local_path);
    let tables = local.list_tables().await?;
    let mut copied = Vec::new();
    for (from, to, cols) in [("state", STATE, STATE_COLUMNS), ("settings", SETTINGS, SETTINGS_COLUMNS)] {
        if !tables.iter().any(|t| t == from) {
            continue;
        }
        let have: std::collections::HashSet<String> =
            column(&batches_to_json(&db.query(to, None, ALL).await?), "key").map(String::from).collect();
        let rows: Vec<Value> = batches_to_json(&local.query(from, None, ALL).await?)
            .into_iter()
            .filter(|r| r.get("key").and_then(|k| k.as_str()).map_or(false, |k| !have.contains(k)))
            .map(|r| Value::Object(cols.iter().map(|c| (c.to_string(), r.get(*c).cloned().unwrap_or(json!("")))).collect()))
            .collect();
        if !rows.is_empty() {
            db.insert(to, &rows).await.map_err(|e| format!("migrating {from} to {to}: {e}"))?;
        }
        copied.push((to.to_string(), rows.len()));
    }
    db.insert(STATE, &[json!({ "key": MIGRATED_KEY, "value": "true", "expires_at": "", "created_at": now() })]).await?;
    info!(from = %local_path, to = %db.describe(), ?copied, "local state and settings copied to the shared tables");
    Ok(copied)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Both sides on embedded LanceDB: the semantics, without an ADB.
    async fn storage(dir: &std::path::Path) -> SharedRegistryStorage {
        let local = crate::init_storage(dir.join("local").to_str().unwrap()).await.unwrap();
        let db: Arc<dyn PlatformDb> = Arc::new(LanceDbPlatform::new(dir.join("shared.lance").to_string_lossy()));
        let s = SharedRegistryStorage::new(local, db, vec![("_triggers".into(), "runtime:cv-x:triggers".into())]);
        s.init().await.unwrap();
        s
    }

    #[tokio::test]
    async fn resources_and_state_split() {
        let dir = tempfile::tempdir().unwrap();
        let s = storage(dir.path()).await;
        s.resources().create(Resource::new("mem", "adb", json!({"url": "http://adb:3000"})).with_namespace("a3t")).await.unwrap();
        s.resources().update("a3t", "mem", json!({"url": "http://adb2:3000"})).await.unwrap();
        let r = s.resources().get("a3t", "mem").await.unwrap();
        assert_eq!(r.config["url"], "http://adb2:3000");
        assert_eq!(s.resources().list("a3t").await.unwrap().len(), 1);

        s.state().set("agent:session:a1", json!({"messages": [1, 2]}), None).await.unwrap();
        s.state().set("agent:session:a1", json!({"messages": [1, 2, 3]}), None).await.unwrap();
        s.state().set("_triggers", json!([{"id": "t"}]), None).await.unwrap();
        s.state().set("local:only", json!(1), None).await.unwrap();
        assert_eq!(s.state().get("agent:session:a1").await.unwrap().unwrap()["messages"], json!([1, 2, 3]));
        assert_eq!(s.state().get("_triggers").await.unwrap().unwrap(), json!([{"id": "t"}]));
        let mut keys = s.state().keys("agent:*").await.unwrap();
        keys.sort();
        assert_eq!(keys, vec!["agent:session:a1"]);
        // The shared side holds the agent key and the aliased one, not the local key.
        let shared = SharedState { db: Arc::clone(&s.db) };
        let mut all = shared.keys("*").await.unwrap();
        all.sort();
        assert_eq!(all, vec!["agent:session:a1", "runtime:cv-x:triggers"]);
        assert!(s.state().delete("agent:session:a1").await.unwrap());
        assert!(s.state().get("agent:session:a1").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn workflows_register_replace_and_seed() {
        let dir = tempfile::tempdir().unwrap();
        let yaml = "document:\n  dsl: 1.0.0\n  namespace: t\n  name: hello\n  version: 1.0.0\ndo:\n- a:\n    set:\n      x: 1\n";
        // A local registry with one flow, then a shared one seeded from it.
        // LanceDB only (no engine, so no registry on the embedded ADB either).
        let local = crate::init_storage_with(dir.path().join("local").to_str().unwrap(), false).await.unwrap();
        local.workflows().register(WorkflowRecord::from_yaml(yaml).unwrap()).await.unwrap();
        let db: Arc<dyn PlatformDb> = Arc::new(LanceDbPlatform::new(dir.path().join("shared.lance").to_string_lossy()));
        let s = SharedRegistryStorage::new(Arc::clone(&local), Arc::clone(&db), Vec::new());
        s.init().await.unwrap();
        let copied = seed(&*db, &dir.path().join("local/clawdflow.lance").to_string_lossy()).await.unwrap();
        assert!(copied.iter().any(|(t, n)| t == WORKFLOWS && *n == 1), "{copied:?}");
        assert_eq!(s.workflows().get("t", "hello").await.unwrap().metadata.name, "hello");
        // Seeding again is a no-op; registering the same version replaces it.
        assert!(seed(&*db, &dir.path().join("local/clawdflow.lance").to_string_lossy()).await.unwrap().is_empty());
        s.workflows().register(WorkflowRecord::from_yaml(yaml).unwrap()).await.unwrap();
        assert_eq!(s.workflows().list("t").await.unwrap().len(), 1);
        s.workflows().delete("t", "hello").await.unwrap();
        assert!(!s.workflows().exists("t", "hello").await.unwrap());
    }

    /// Cluster mode: two FlowRs on one shared store see the same settings,
    /// state and triggers; a runtime's alias still wins over "every key".
    #[tokio::test]
    async fn cluster_mode_shares_settings_state_and_triggers() {
        let dir = tempfile::tempdir().unwrap();
        let db: Arc<dyn PlatformDb> = Arc::new(LanceDbPlatform::new(dir.path().join("shared.lance").to_string_lossy()));
        let open = |name: &str, aliases: Vec<(String, String)>| {
            let db = Arc::clone(&db);
            let path = dir.path().join(name);
            async move {
                let local = crate::init_storage_with(path.to_str().unwrap(), false).await.unwrap();
                let s = SharedRegistryStorage::new(local, db, aliases).all_state();
                s.init().await.unwrap();
                s
            }
        };
        let a = open("a", Vec::new()).await;
        let b = open("b", Vec::new()).await;
        let rt = open("rt", vec![("_triggers".into(), "runtime:t/cv:triggers".into())]).await;

        a.settings().set("auth.mode", &json!("oidc"), "api").await.unwrap();
        assert_eq!(b.settings().get_string("auth.mode").await.unwrap().as_deref(), Some("oidc"));
        assert_eq!(b.settings().seed_defaults(&[("auth.mode", json!("none"), "default"), ("cors.origins", json!("*"), "default")]).await.unwrap(), 1);
        assert_eq!(a.settings().get_all().await.unwrap().len(), 2);

        a.state().set("pins:u1", json!(["x"]), None).await.unwrap();
        assert_eq!(b.state().get("pins:u1").await.unwrap(), Some(json!(["x"])));

        // A trigger created through A is listed through B (B reloads from the
        // shared state; the manager is one per process, so clear it first).
        let t = json!({"id": "t1", "name": "nightly", "namespace": "n", "enabled": true,
                       "config": {"type": "cron", "expression": "0 3 * * *", "message": "go"}, "targets": []});
        a.triggers().create(t).await.unwrap();
        flowr_ce_agents::trigger_manager().replace_all(&json!([])).await.unwrap();
        assert_eq!(b.triggers().list(Some("n")).await.unwrap().len(), 1);
        // The runtime's `_triggers` is its own key, not the console's.
        assert!(rt.triggers().list(None).await.unwrap().is_empty());
        b.triggers().delete("t1").await.unwrap();
        assert!(a.triggers().get("t1").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn local_state_is_copied_once() {
        let dir = tempfile::tempdir().unwrap();
        let local_dir = dir.path().join("local");
        let local = crate::init_storage_with(local_dir.to_str().unwrap(), false).await.unwrap();
        local.state().set("pins:u1", json!(["a"]), None).await.unwrap();
        local.settings().set("auth.mode", &json!("oidc"), "api").await.unwrap();
        let db: Arc<dyn PlatformDb> = Arc::new(LanceDbPlatform::new(dir.path().join("shared.lance").to_string_lossy()));
        let s = SharedRegistryStorage::new(Arc::clone(&local), Arc::clone(&db), Vec::new()).all_state();
        s.init().await.unwrap();
        let lance = local_dir.join("clawdflow.lance").to_string_lossy().to_string();
        let copied = migrate_local(&*db, &lance).await.unwrap();
        assert!(copied.iter().any(|(t, n)| t == STATE && *n == 1), "{copied:?}");
        assert_eq!(s.state().get("pins:u1").await.unwrap(), Some(json!(["a"])));
        assert_eq!(s.settings().get_string("auth.mode").await.unwrap().as_deref(), Some("oidc"));
        // Deleted afterwards: a second start does not bring it back.
        s.state().delete("pins:u1").await.unwrap();
        assert!(migrate_local(&*db, &lance).await.unwrap().is_empty());
        assert!(s.state().get("pins:u1").await.unwrap().is_none());
    }
}
