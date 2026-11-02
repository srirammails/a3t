//! Execution store implementation using LanceDB

use std::collections::HashMap;
use std::sync::Arc;

use arrow_array::{Array, RecordBatch, RecordBatchIterator, StringArray};
use arrow_schema::{DataType, Field, Schema};
use async_trait::async_trait;
use futures::TryStreamExt;
use lancedb::query::{ExecutableQuery, QueryBase, Select};
use lancedb::Connection;
use tokio::sync::RwLock;
use uuid::Uuid;

use flowr_storage::ExecutionStore;
use flowr_types::{ExecutionState, FlowError, FlowResult};

const TABLE_NAME: &str = "executions";

/// LanceDB-backed execution store
pub struct LanceExecutionStore {
    conn: Arc<RwLock<Connection>>,
}

impl LanceExecutionStore {
    pub fn new(conn: Arc<RwLock<Connection>>) -> Self {
        Self { conn }
    }

    /// Initialize the executions table
    pub async fn init(&self) -> FlowResult<()> {
        let conn = self.conn.read().await;

        let tables = conn
            .table_names()
            .execute()
            .await
            .map_err(|e| FlowError::Storage(format!("Failed to list tables: {}", e)))?;

        if tables.contains(&TABLE_NAME.to_string()) {
            return Ok(());
        }

        let schema = Arc::new(Schema::new(vec![
            Field::new("instance_id", DataType::Utf8, false),
            Field::new("namespace", DataType::Utf8, false),
            Field::new("name", DataType::Utf8, false),
            Field::new("state_data", DataType::Utf8, false),
            Field::new("status", DataType::Utf8, false),
            Field::new("created_at", DataType::Utf8, false),
            Field::new("updated_at", DataType::Utf8, false),
        ]));

        let batch = RecordBatch::new_empty(schema.clone());
        let batches = RecordBatchIterator::new(vec![Ok(batch)], schema);

        conn.create_table(TABLE_NAME, Box::new(batches) as Box<dyn arrow_array::RecordBatchReader + Send>)
            .execute()
            .await
            .map_err(|e| FlowError::Storage(format!("Failed to create executions table: {}", e)))?;

        Ok(())
    }
}

#[async_trait]
impl ExecutionStore for LanceExecutionStore {
    async fn save(&self, state: ExecutionState) -> FlowResult<()> {
        // Delete existing if present
        let _ = self.delete_by_id(state.instance_id).await;

        let conn = self.conn.read().await;

        let table = conn
            .open_table(TABLE_NAME)
            .execute()
            .await
            .map_err(|e| FlowError::Storage(format!("Failed to open executions table: {}", e)))?;

        let now = chrono::Utc::now().to_rfc3339();
        let state_json = serde_json::to_string(&state)
            .map_err(|e| FlowError::Storage(format!("Failed to serialize execution state: {}", e)))?;

        let status = format!("{:?}", state.status);
        let instance_id = state.instance_id.to_string();

        let schema = Arc::new(Schema::new(vec![
            Field::new("instance_id", DataType::Utf8, false),
            Field::new("namespace", DataType::Utf8, false),
            Field::new("name", DataType::Utf8, false),
            Field::new("state_data", DataType::Utf8, false),
            Field::new("status", DataType::Utf8, false),
            Field::new("created_at", DataType::Utf8, false),
            Field::new("updated_at", DataType::Utf8, false),
        ]));

        let batch = RecordBatch::try_new(
            schema.clone(),
            vec![
                Arc::new(StringArray::from(vec![instance_id.as_str()])),
                Arc::new(StringArray::from(vec![state.namespace.as_str()])),
                Arc::new(StringArray::from(vec![state.name.as_str()])),
                Arc::new(StringArray::from(vec![state_json.as_str()])),
                Arc::new(StringArray::from(vec![status.as_str()])),
                Arc::new(StringArray::from(vec![now.as_str()])),
                Arc::new(StringArray::from(vec![now.as_str()])),
            ],
        )
        .map_err(|e| FlowError::Storage(format!("Failed to create record batch: {}", e)))?;

        let batches = RecordBatchIterator::new(vec![Ok(batch)], schema);
        table
            .add(Box::new(batches) as Box<dyn arrow_array::RecordBatchReader + Send>)
            .execute()
            .await
            .map_err(|e| FlowError::Storage(format!("Failed to save execution state: {}", e)))?;

        Ok(())
    }

    async fn get(&self, instance_id: Uuid) -> FlowResult<Option<ExecutionState>> {
        let conn = self.conn.read().await;

        let table = conn
            .open_table(TABLE_NAME)
            .execute()
            .await
            .map_err(|e| FlowError::Storage(format!("Failed to open executions table: {}", e)))?;

        let filter = format!("instance_id = '{}'", instance_id);

        let batches: Vec<RecordBatch> = table
            .query()
            .only_if(filter)
            .limit(1)
            .execute()
            .await
            .map_err(|e| FlowError::Storage(format!("Failed to query executions: {}", e)))?
            .try_collect()
            .await
            .map_err(|e| FlowError::Storage(format!("Failed to collect results: {}", e)))?;

        if batches.is_empty() || batches[0].num_rows() == 0 {
            return Ok(None);
        }

        let data_col = batches[0]
            .column_by_name("state_data")
            .and_then(|c| c.as_any().downcast_ref::<StringArray>())
            .ok_or_else(|| FlowError::Storage("Missing state_data column".to_string()))?;

        let state_json = data_col.value(0);

        let state: ExecutionState = serde_json::from_str(state_json)
            .map_err(|e| FlowError::Storage(format!("Failed to deserialize execution state: {}", e)))?;

        Ok(Some(state))
    }

    async fn list_recent(&self, limit: usize) -> FlowResult<Vec<ExecutionState>> {
        self.newest(None, limit).await
    }

    async fn list_by_workflow(
        &self,
        namespace: &str,
        name: &str,
        limit: usize,
    ) -> FlowResult<Vec<ExecutionState>> {
        let filter = format!("namespace = {} AND name = {}", quote(namespace), quote(name));
        self.newest(Some(filter), limit).await
    }

    async fn list_open(&self) -> FlowResult<Vec<ExecutionState>> {
        let table = self.table().await?;
        let batches: Vec<RecordBatch> = table
            .query()
            .only_if("status IN ('Queued', 'Running', 'Suspended')")
            .execute()
            .await
            .map_err(|e| FlowError::Storage(format!("Failed to query executions: {}", e)))?
            .try_collect()
            .await
            .map_err(|e| FlowError::Storage(format!("Failed to collect results: {}", e)))?;
        Ok(states_of(&batches).into_iter().map(|(_, s)| s).collect())
    }
}

/// A string literal for a Lance filter.
fn quote(v: &str) -> String {
    format!("'{}'", v.replace('\'', "''"))
}

/// The states in `batches`, with their instance ids.
fn states_of(batches: &[RecordBatch]) -> Vec<(String, ExecutionState)> {
    let mut out = Vec::new();
    for batch in batches {
        let ids = batch.column_by_name("instance_id").and_then(|c| c.as_any().downcast_ref::<StringArray>());
        let data = batch.column_by_name("state_data").and_then(|c| c.as_any().downcast_ref::<StringArray>());
        if let (Some(ids), Some(data)) = (ids, data) {
            for i in 0..batch.num_rows() {
                if let Ok(state) = serde_json::from_str(data.value(i)) {
                    out.push((ids.value(i).to_string(), state));
                }
            }
        }
    }
    out
}

impl LanceExecutionStore {
    async fn table(&self) -> FlowResult<lancedb::Table> {
        let conn = self.conn.read().await;
        conn.open_table(TABLE_NAME)
            .execute()
            .await
            .map_err(|e| FlowError::Storage(format!("Failed to open executions table: {}", e)))
    }

    /// The newest `limit` executions matching `filter`, newest first.
    ///
    /// A Lance query applies its limit before any ordering, so a plain
    /// `.limit(n)` returns whichever rows come first on disk — the oldest — and
    /// recent runs never showed. Read only id and update time for every match,
    /// sort, then load the states of the top `limit`.
    async fn newest(&self, filter: Option<String>, limit: usize) -> FlowResult<Vec<ExecutionState>> {
        if limit == 0 {
            return Ok(Vec::new());
        }
        let table = self.table().await?;
        let mut q = table.query().select(Select::columns(&["instance_id", "updated_at"]));
        if let Some(f) = &filter {
            q = q.only_if(f.clone());
        }
        let batches: Vec<RecordBatch> = q
            .execute()
            .await
            .map_err(|e| FlowError::Storage(format!("Failed to query executions: {}", e)))?
            .try_collect()
            .await
            .map_err(|e| FlowError::Storage(format!("Failed to collect results: {}", e)))?;

        let mut seen: Vec<(i64, String)> = Vec::new();
        for batch in &batches {
            let ids = batch.column_by_name("instance_id").and_then(|c| c.as_any().downcast_ref::<StringArray>());
            let at = batch.column_by_name("updated_at").and_then(|c| c.as_any().downcast_ref::<StringArray>());
            if let (Some(ids), Some(at)) = (ids, at) {
                for i in 0..batch.num_rows() {
                    let ts = chrono::DateTime::parse_from_rfc3339(at.value(i))
                        .map(|t| t.timestamp_nanos_opt().unwrap_or(i64::MAX))
                        .unwrap_or(0);
                    seen.push((ts, ids.value(i).to_string()));
                }
            }
        }
        seen.sort_by(|a, b| b.0.cmp(&a.0));
        let mut top: Vec<String> = Vec::new();
        for (_, id) in seen {
            if !top.contains(&id) {
                top.push(id);
                if top.len() == limit {
                    break;
                }
            }
        }

        let mut by_id: HashMap<String, ExecutionState> = HashMap::new();
        for chunk in top.chunks(200) {
            let list = chunk.iter().map(|id| quote(id)).collect::<Vec<_>>().join(", ");
            let batches: Vec<RecordBatch> = table
                .query()
                .only_if(format!("instance_id IN ({list})"))
                .execute()
                .await
                .map_err(|e| FlowError::Storage(format!("Failed to query executions: {}", e)))?
                .try_collect()
                .await
                .map_err(|e| FlowError::Storage(format!("Failed to collect results: {}", e)))?;
            by_id.extend(states_of(&batches));
        }
        Ok(top.into_iter().filter_map(|id| by_id.remove(&id)).collect())
    }

    async fn delete_by_id(&self, instance_id: Uuid) -> FlowResult<()> {
        let conn = self.conn.read().await;

        let table = conn
            .open_table(TABLE_NAME)
            .execute()
            .await
            .map_err(|e| FlowError::Storage(format!("Failed to open executions table: {}", e)))?;

        let filter = format!("instance_id = '{}'", instance_id);

        table
            .delete(&filter)
            .await
            .map_err(|e| FlowError::Storage(format!("Failed to delete execution: {}", e)))?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::LanceStorage;
    use flowr_storage::StorageBackend;
    use flowr_types::{ExecutionRequest, ExecutionState};

    fn run(ns: &str, name: &str) -> ExecutionState {
        ExecutionState::new(&ExecutionRequest::new(ns, name), "1.0.0")
    }

    #[tokio::test]
    async fn lists_are_newest_first_past_the_limit() {
        let storage = LanceStorage::in_memory().await.unwrap();
        storage.init().await.unwrap();
        let ex = storage.executions();
        let mut saved = Vec::new();
        for i in 0..30 {
            let s = run("p", if i % 2 == 0 { "even" } else { "odd" }).complete(serde_json::json!(i), 1);
            saved.push(s.instance_id);
            ex.save(s).await.unwrap();
        }
        let recent: Vec<_> = ex.list_recent(5).await.unwrap().into_iter().map(|s| s.instance_id).collect();
        let want: Vec<_> = saved.iter().rev().take(5).cloned().collect();
        assert_eq!(recent, want, "the five most recent, newest first");

        let odd = ex.list_by_workflow("p", "odd", 3).await.unwrap();
        assert_eq!(odd.len(), 3);
        assert!(odd.iter().all(|s| s.name == "odd"));
        assert_eq!(odd[0].instance_id, saved[29], "newest odd run first");
    }

    #[tokio::test]
    async fn a_resaved_run_moves_to_the_top_once() {
        let storage = LanceStorage::in_memory().await.unwrap();
        storage.init().await.unwrap();
        let ex = storage.executions();
        let first = run("p", "f");
        ex.save(first.clone()).await.unwrap();
        ex.save(run("p", "f")).await.unwrap();
        ex.save(first.clone().complete(serde_json::json!(1), 1)).await.unwrap();
        let recent = ex.list_recent(10).await.unwrap();
        assert_eq!(recent.len(), 2, "no duplicate rows");
        assert_eq!(recent[0].instance_id, first.instance_id);
    }

    #[tokio::test]
    async fn open_runs_are_the_unfinished_ones() {
        let storage = LanceStorage::in_memory().await.unwrap();
        storage.init().await.unwrap();
        let ex = storage.executions();
        let running = run("p", "a");
        ex.save(running.clone()).await.unwrap();
        ex.save(run("p", "b").complete(serde_json::json!(null), 1)).await.unwrap();
        ex.save(run("p", "c").fail("boom")).await.unwrap();
        let open = ex.list_open().await.unwrap();
        assert_eq!(open.len(), 1);
        assert_eq!(open[0].instance_id, running.instance_id);
    }
}
