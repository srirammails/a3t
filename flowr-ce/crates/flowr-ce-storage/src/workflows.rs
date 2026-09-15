//! Workflow store implementation using LanceDB

use std::sync::Arc;

use arrow_array::{Array, RecordBatch, RecordBatchIterator, StringArray};
use arrow_schema::{DataType, Field, Schema};
use async_trait::async_trait;
use futures::TryStreamExt;
use lancedb::query::{ExecutableQuery, QueryBase};
use lancedb::Connection;
use tokio::sync::RwLock;

use flowr_storage::WorkflowStore;
use flowr_types::{FlowError, FlowResult, WorkflowRecord, WorkflowSummary};

const TABLE_NAME: &str = "workflows";

/// LanceDB-backed workflow store
pub struct LanceWorkflowStore {
    conn: Arc<RwLock<Connection>>,
}

impl LanceWorkflowStore {
    pub fn new(conn: Arc<RwLock<Connection>>) -> Self {
        Self { conn }
    }

    /// Initialize the workflows table
    pub async fn init(&self) -> FlowResult<()> {
        let conn = self.conn.read().await;

        // Check if table exists
        let tables = conn
            .table_names()
            .execute()
            .await
            .map_err(|e| FlowError::Storage(format!("Failed to list tables: {}", e)))?;

        if tables.contains(&TABLE_NAME.to_string()) {
            return Ok(());
        }

        // Create schema
        let schema = Arc::new(Schema::new(vec![
            Field::new("namespace", DataType::Utf8, false),
            Field::new("name", DataType::Utf8, false),
            Field::new("version", DataType::Utf8, false),
            Field::new("active_version", DataType::Utf8, true),
            Field::new("enabled", DataType::Utf8, false), // "true" or "false"
            Field::new("definition", DataType::Utf8, false), // JSON serialized
            Field::new("created_at", DataType::Utf8, false),
            Field::new("updated_at", DataType::Utf8, false),
        ]));

        // Create empty table
        let batch = RecordBatch::new_empty(schema.clone());
        let batches = RecordBatchIterator::new(vec![Ok(batch)], schema);

        conn.create_table(TABLE_NAME, Box::new(batches))
            .execute()
            .await
            .map_err(|e| FlowError::Storage(format!("Failed to create workflows table: {}", e)))?;

        Ok(())
    }
}

#[async_trait]
impl WorkflowStore for LanceWorkflowStore {
    async fn register(&self, record: WorkflowRecord) -> FlowResult<()> {
        let conn = self.conn.read().await;

        let table = conn
            .open_table(TABLE_NAME)
            .execute()
            .await
            .map_err(|e| FlowError::Storage(format!("Failed to open workflows table: {}", e)))?;

        let now = chrono::Utc::now().to_rfc3339();
        let def_json = serde_json::to_string(&record)
            .map_err(|e| FlowError::Storage(format!("Failed to serialize workflow: {}", e)))?;

        let schema = Arc::new(Schema::new(vec![
            Field::new("namespace", DataType::Utf8, false),
            Field::new("name", DataType::Utf8, false),
            Field::new("version", DataType::Utf8, false),
            Field::new("active_version", DataType::Utf8, true),
            Field::new("enabled", DataType::Utf8, false),
            Field::new("definition", DataType::Utf8, false),
            Field::new("created_at", DataType::Utf8, false),
            Field::new("updated_at", DataType::Utf8, false),
        ]));

        let batch = RecordBatch::try_new(
            schema.clone(),
            vec![
                Arc::new(StringArray::from(vec![record.metadata.namespace.as_str()])),
                Arc::new(StringArray::from(vec![record.metadata.name.as_str()])),
                Arc::new(StringArray::from(vec![record.metadata.version.as_str()])),
                Arc::new(StringArray::from(vec![Some(record.metadata.version.as_str())])),
                Arc::new(StringArray::from(vec!["true"])),
                Arc::new(StringArray::from(vec![def_json.as_str()])),
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
            .map_err(|e| FlowError::Storage(format!("Failed to add workflow: {}", e)))?;

        Ok(())
    }

    async fn get(&self, namespace: &str, name: &str) -> FlowResult<WorkflowRecord> {
        let conn = self.conn.read().await;

        let table = conn
            .open_table(TABLE_NAME)
            .execute()
            .await
            .map_err(|e| FlowError::Storage(format!("Failed to open workflows table: {}", e)))?;

        // Query for active version using only_if
        let filter = format!(
            "namespace = '{}' AND name = '{}' AND enabled = 'true'",
            namespace, name
        );

        let batches: Vec<RecordBatch> = table
            .query()
            .only_if(filter)
            .limit(1)
            .execute()
            .await
            .map_err(|e| FlowError::Storage(format!("Failed to query workflows: {}", e)))?
            .try_collect()
            .await
            .map_err(|e| FlowError::Storage(format!("Failed to collect results: {}", e)))?;

        if batches.is_empty() || batches[0].num_rows() == 0 {
            return Err(FlowError::NotFound(format!(
                "Workflow {}/{} not found",
                namespace, name
            )));
        }

        let def_col = batches[0]
            .column_by_name("definition")
            .ok_or_else(|| FlowError::Storage("Missing definition column".to_string()))?;

        let def_array = def_col
            .as_any()
            .downcast_ref::<StringArray>()
            .ok_or_else(|| FlowError::Storage("Invalid definition column type".to_string()))?;

        let def_json = def_array.value(0);

        serde_json::from_str(def_json)
            .map_err(|e| FlowError::Storage(format!("Failed to deserialize workflow: {}", e)))
    }

    async fn get_version(
        &self,
        namespace: &str,
        name: &str,
        version: &str,
    ) -> FlowResult<WorkflowRecord> {
        let conn = self.conn.read().await;

        let table = conn
            .open_table(TABLE_NAME)
            .execute()
            .await
            .map_err(|e| FlowError::Storage(format!("Failed to open workflows table: {}", e)))?;

        let filter = format!(
            "namespace = '{}' AND name = '{}' AND version = '{}'",
            namespace, name, version
        );

        let batches: Vec<RecordBatch> = table
            .query()
            .only_if(filter)
            .limit(1)
            .execute()
            .await
            .map_err(|e| FlowError::Storage(format!("Failed to query workflows: {}", e)))?
            .try_collect()
            .await
            .map_err(|e| FlowError::Storage(format!("Failed to collect results: {}", e)))?;

        if batches.is_empty() || batches[0].num_rows() == 0 {
            return Err(FlowError::NotFound(format!(
                "Workflow {}/{} version {} not found",
                namespace, name, version
            )));
        }

        let def_col = batches[0]
            .column_by_name("definition")
            .ok_or_else(|| FlowError::Storage("Missing definition column".to_string()))?;

        let def_array = def_col
            .as_any()
            .downcast_ref::<StringArray>()
            .ok_or_else(|| FlowError::Storage("Invalid definition column type".to_string()))?;

        let def_json = def_array.value(0);

        serde_json::from_str(def_json)
            .map_err(|e| FlowError::Storage(format!("Failed to deserialize workflow: {}", e)))
    }

    async fn list(&self, namespace: &str) -> FlowResult<Vec<WorkflowSummary>> {
        let conn = self.conn.read().await;

        let table = conn
            .open_table(TABLE_NAME)
            .execute()
            .await
            .map_err(|e| FlowError::Storage(format!("Failed to open workflows table: {}", e)))?;

        let filter = format!("namespace = '{}'", namespace);

        let batches: Vec<RecordBatch> = table
            .query()
            .only_if(filter)
            .execute()
            .await
            .map_err(|e| FlowError::Storage(format!("Failed to query workflows: {}", e)))?
            .try_collect()
            .await
            .map_err(|e| FlowError::Storage(format!("Failed to collect results: {}", e)))?;

        let mut summaries = Vec::new();

        for batch in batches {
            // Get definition column and deserialize for full summary
            if let Some(def_array) = batch
                .column_by_name("definition")
                .and_then(|c| c.as_any().downcast_ref::<StringArray>())
            {
                for i in 0..batch.num_rows() {
                    if let Ok(record) = serde_json::from_str::<WorkflowRecord>(def_array.value(i))
                    {
                        let task_count = record.parse().map(|wf| wf.tasks.len()).unwrap_or(0);
                        summaries.push(WorkflowSummary {
                            namespace: record.metadata.namespace.clone(),
                            name: record.metadata.name.clone(),
                            version: record.metadata.version.clone(),
                            enabled: record.metadata.enabled,
                            step_count: task_count,
                        });
                    }
                }
            }
        }

        Ok(summaries)
    }

    async fn delete(&self, namespace: &str, name: &str) -> FlowResult<()> {
        let conn = self.conn.read().await;

        let table = conn
            .open_table(TABLE_NAME)
            .execute()
            .await
            .map_err(|e| FlowError::Storage(format!("Failed to open workflows table: {}", e)))?;

        let filter = format!("namespace = '{}' AND name = '{}'", namespace, name);

        table
            .delete(&filter)
            .await
            .map_err(|e| FlowError::Storage(format!("Failed to delete workflow: {}", e)))?;

        Ok(())
    }

    async fn set_enabled(&self, namespace: &str, name: &str, enabled: bool) -> FlowResult<()> {
        // LanceDB doesn't support in-place updates well
        // We'd need to delete and re-insert, but for now just log
        tracing::warn!(
            "set_enabled not fully implemented for LanceDB: {}/{} -> {}",
            namespace,
            name,
            enabled
        );
        Ok(())
    }

    async fn set_active_version(
        &self,
        namespace: &str,
        name: &str,
        version: &str,
    ) -> FlowResult<()> {
        // Similar limitation
        tracing::warn!(
            "set_active_version not fully implemented for LanceDB: {}/{} -> {}",
            namespace,
            name,
            version
        );
        Ok(())
    }

    async fn exists(&self, namespace: &str, name: &str) -> FlowResult<bool> {
        match self.get(namespace, name).await {
            Ok(_) => Ok(true),
            Err(FlowError::NotFound(_)) => Ok(false),
            Err(e) => Err(e),
        }
    }
}
