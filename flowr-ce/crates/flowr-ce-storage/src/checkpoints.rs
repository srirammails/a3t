//! Checkpoint store implementation using LanceDB

use std::sync::Arc;

use arrow_array::{Array, RecordBatch, RecordBatchIterator, StringArray};
use arrow_schema::{DataType, Field, Schema};
use async_trait::async_trait;
use futures::TryStreamExt;
use lancedb::query::{ExecutableQuery, QueryBase};
use lancedb::Connection;
use tokio::sync::RwLock;
use uuid::Uuid;

use flowr_storage::CheckpointStore;
use flowr_types::{Checkpoint, FlowError, FlowResult};

const TABLE_NAME: &str = "checkpoints";

/// LanceDB-backed checkpoint store
pub struct LanceCheckpointStore {
    conn: Arc<RwLock<Connection>>,
}

impl LanceCheckpointStore {
    pub fn new(conn: Arc<RwLock<Connection>>) -> Self {
        Self { conn }
    }

    /// Initialize the checkpoints table
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
            Field::new("id", DataType::Utf8, false),
            Field::new("instance_id", DataType::Utf8, false),
            Field::new("checkpoint_data", DataType::Utf8, false),
            Field::new("created_at", DataType::Utf8, false),
        ]));

        let batch = RecordBatch::new_empty(schema.clone());
        let batches = RecordBatchIterator::new(vec![Ok(batch)], schema);

        conn.create_table(TABLE_NAME, Box::new(batches))
            .execute()
            .await
            .map_err(|e| FlowError::Storage(format!("Failed to create checkpoints table: {}", e)))?;

        Ok(())
    }
}

#[async_trait]
impl CheckpointStore for LanceCheckpointStore {
    async fn save(&self, checkpoint: Checkpoint) -> FlowResult<()> {
        let conn = self.conn.read().await;

        let table = conn
            .open_table(TABLE_NAME)
            .execute()
            .await
            .map_err(|e| FlowError::Storage(format!("Failed to open checkpoints table: {}", e)))?;

        let now = chrono::Utc::now().to_rfc3339();
        let checkpoint_json = serde_json::to_string(&checkpoint)
            .map_err(|e| FlowError::Storage(format!("Failed to serialize checkpoint: {}", e)))?;

        let id = Uuid::now_v7().to_string();
        let instance_id = checkpoint.instance_id.to_string();

        let schema = Arc::new(Schema::new(vec![
            Field::new("id", DataType::Utf8, false),
            Field::new("instance_id", DataType::Utf8, false),
            Field::new("checkpoint_data", DataType::Utf8, false),
            Field::new("created_at", DataType::Utf8, false),
        ]));

        let batch = RecordBatch::try_new(
            schema.clone(),
            vec![
                Arc::new(StringArray::from(vec![id.as_str()])),
                Arc::new(StringArray::from(vec![instance_id.as_str()])),
                Arc::new(StringArray::from(vec![checkpoint_json.as_str()])),
                Arc::new(StringArray::from(vec![now.as_str()])),
            ],
        )
        .map_err(|e| FlowError::Storage(format!("Failed to create record batch: {}", e)))?;

        let batches = RecordBatchIterator::new(vec![Ok(batch)], schema);
        table
            .add(Box::new(batches))
            .execute()
            .await
            .map_err(|e| FlowError::Storage(format!("Failed to save checkpoint: {}", e)))?;

        Ok(())
    }

    async fn load(&self, instance_id: Uuid) -> FlowResult<Option<Checkpoint>> {
        let checkpoints = self.list(instance_id).await?;
        Ok(checkpoints.into_iter().last())
    }

    async fn delete(&self, instance_id: Uuid) -> FlowResult<()> {
        let conn = self.conn.read().await;

        let table = conn
            .open_table(TABLE_NAME)
            .execute()
            .await
            .map_err(|e| FlowError::Storage(format!("Failed to open checkpoints table: {}", e)))?;

        let filter = format!("instance_id = '{}'", instance_id);

        table
            .delete(&filter)
            .await
            .map_err(|e| FlowError::Storage(format!("Failed to delete checkpoints: {}", e)))?;

        Ok(())
    }

    async fn list(&self, instance_id: Uuid) -> FlowResult<Vec<Checkpoint>> {
        let conn = self.conn.read().await;

        let table = conn
            .open_table(TABLE_NAME)
            .execute()
            .await
            .map_err(|e| FlowError::Storage(format!("Failed to open checkpoints table: {}", e)))?;

        let filter = format!("instance_id = '{}'", instance_id);

        let batches: Vec<RecordBatch> = table
            .query()
            .only_if(filter)
            .execute()
            .await
            .map_err(|e| FlowError::Storage(format!("Failed to query checkpoints: {}", e)))?
            .try_collect()
            .await
            .map_err(|e| FlowError::Storage(format!("Failed to collect results: {}", e)))?;

        let mut checkpoints = Vec::new();

        for batch in batches {
            if let Some(data_col) = batch
                .column_by_name("checkpoint_data")
                .and_then(|c| c.as_any().downcast_ref::<StringArray>())
            {
                for i in 0..batch.num_rows() {
                    if let Ok(checkpoint) = serde_json::from_str(data_col.value(i)) {
                        checkpoints.push(checkpoint);
                    }
                }
            }
        }

        // Sort by created_at (implicit in checkpoint data)
        Ok(checkpoints)
    }
}
