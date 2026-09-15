//! Execution store implementation using LanceDB

use std::sync::Arc;

use arrow_array::{Array, RecordBatch, RecordBatchIterator, StringArray};
use arrow_schema::{DataType, Field, Schema};
use async_trait::async_trait;
use futures::TryStreamExt;
use lancedb::query::{ExecutableQuery, QueryBase};
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

        conn.create_table(TABLE_NAME, Box::new(batches))
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
            .add(Box::new(batches))
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
        let conn = self.conn.read().await;

        let table = conn
            .open_table(TABLE_NAME)
            .execute()
            .await
            .map_err(|e| FlowError::Storage(format!("Failed to open executions table: {}", e)))?;

        let batches: Vec<RecordBatch> = table
            .query()
            .limit(limit)
            .execute()
            .await
            .map_err(|e| FlowError::Storage(format!("Failed to query executions: {}", e)))?
            .try_collect()
            .await
            .map_err(|e| FlowError::Storage(format!("Failed to collect results: {}", e)))?;

        let mut states = Vec::new();

        for batch in batches {
            if let Some(data_col) = batch
                .column_by_name("state_data")
                .and_then(|c| c.as_any().downcast_ref::<StringArray>())
            {
                for i in 0..batch.num_rows() {
                    if let Ok(state) = serde_json::from_str(data_col.value(i)) {
                        states.push(state);
                    }
                }
            }
        }

        Ok(states)
    }

    async fn list_by_workflow(
        &self,
        namespace: &str,
        name: &str,
        limit: usize,
    ) -> FlowResult<Vec<ExecutionState>> {
        let conn = self.conn.read().await;

        let table = conn
            .open_table(TABLE_NAME)
            .execute()
            .await
            .map_err(|e| FlowError::Storage(format!("Failed to open executions table: {}", e)))?;

        let filter = format!("namespace = '{}' AND name = '{}'", namespace, name);

        let batches: Vec<RecordBatch> = table
            .query()
            .only_if(filter)
            .limit(limit)
            .execute()
            .await
            .map_err(|e| FlowError::Storage(format!("Failed to query executions: {}", e)))?
            .try_collect()
            .await
            .map_err(|e| FlowError::Storage(format!("Failed to collect results: {}", e)))?;

        let mut states = Vec::new();

        for batch in batches {
            if let Some(data_col) = batch
                .column_by_name("state_data")
                .and_then(|c| c.as_any().downcast_ref::<StringArray>())
            {
                for i in 0..batch.num_rows() {
                    if let Ok(state) = serde_json::from_str(data_col.value(i)) {
                        states.push(state);
                    }
                }
            }
        }

        Ok(states)
    }
}

impl LanceExecutionStore {
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
