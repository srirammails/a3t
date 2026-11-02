//! Settings store implementation using LanceDB
//!
//! Key-value settings for FlowR runtime configuration.
//! Settings are namespaced (e.g., "auth.mode", "rbac.provider", "cors.origins").

use std::sync::Arc;

use arrow_array::{Array, RecordBatch, RecordBatchIterator, StringArray};
use arrow_schema::{DataType, Field, Schema};
use async_trait::async_trait;
use futures::TryStreamExt;
use lancedb::query::{ExecutableQuery, QueryBase};
use lancedb::Connection;
use serde_json::Value;
use tokio::sync::RwLock;
use tracing::info;

use flowr_types::{FlowError, FlowResult};

const TABLE_NAME: &str = "settings";

/// LanceDB-backed settings store
pub struct LanceSettingsStore {
    conn: Arc<RwLock<Connection>>,
}

impl LanceSettingsStore {
    pub fn new(conn: Arc<RwLock<Connection>>) -> Self {
        Self { conn }
    }

    /// Initialize the settings table
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
            Field::new("key", DataType::Utf8, false),
            Field::new("value", DataType::Utf8, false),    // JSON string
            Field::new("source", DataType::Utf8, false),   // "default" | "config" | "api"
            Field::new("updated_at", DataType::Utf8, false),
        ]));

        let batch = RecordBatch::new_empty(schema.clone());
        let batches = RecordBatchIterator::new(vec![Ok(batch)], schema);

        conn.create_table(TABLE_NAME, Box::new(batches) as Box<dyn arrow_array::RecordBatchReader + Send>)
            .execute()
            .await
            .map_err(|e| FlowError::Storage(format!("Failed to create settings table: {}", e)))?;

        info!("Settings table created");
        Ok(())
    }

    /// Get a setting by key
    pub async fn get(&self, key: &str) -> FlowResult<Option<Value>> {
        let conn = self.conn.read().await;

        let table = conn
            .open_table(TABLE_NAME)
            .execute()
            .await
            .map_err(|e| FlowError::Storage(format!("Failed to open settings table: {}", e)))?;

        let filter = format!("key = '{}'", key);

        let batches: Vec<RecordBatch> = table
            .query()
            .only_if(filter)
            .limit(1)
            .execute()
            .await
            .map_err(|e| FlowError::Storage(format!("Failed to query settings: {}", e)))?
            .try_collect()
            .await
            .map_err(|e| FlowError::Storage(format!("Failed to collect: {}", e)))?;

        if batches.is_empty() || batches[0].num_rows() == 0 {
            return Ok(None);
        }

        let value_col = batches[0]
            .column_by_name("value")
            .and_then(|c| c.as_any().downcast_ref::<StringArray>())
            .ok_or_else(|| FlowError::Storage("Missing value column".to_string()))?;

        let json_str = value_col.value(0);
        let value: Value = serde_json::from_str(json_str)
            .unwrap_or_else(|_| Value::String(json_str.to_string()));

        Ok(Some(value))
    }

    /// Get a setting as string
    pub async fn get_string(&self, key: &str) -> FlowResult<Option<String>> {
        match self.get(key).await? {
            Some(Value::String(s)) => Ok(Some(s)),
            Some(v) => Ok(Some(v.to_string())),
            None => Ok(None),
        }
    }

    /// Set a setting (upsert)
    pub async fn set(&self, key: &str, value: &Value, source: &str) -> FlowResult<()> {
        let conn = self.conn.read().await;

        let table = conn
            .open_table(TABLE_NAME)
            .execute()
            .await
            .map_err(|e| FlowError::Storage(format!("Failed to open settings table: {}", e)))?;

        // Delete existing
        let filter = format!("key = '{}'", key);
        let _ = table.delete(&filter).await;

        // Insert new
        let now = chrono::Utc::now().to_rfc3339();
        let value_str = serde_json::to_string(value)
            .map_err(|e| FlowError::Storage(format!("Failed to serialize: {}", e)))?;

        let schema = Arc::new(Schema::new(vec![
            Field::new("key", DataType::Utf8, false),
            Field::new("value", DataType::Utf8, false),
            Field::new("source", DataType::Utf8, false),
            Field::new("updated_at", DataType::Utf8, false),
        ]));

        let batch = RecordBatch::try_new(
            schema.clone(),
            vec![
                Arc::new(StringArray::from(vec![key])),
                Arc::new(StringArray::from(vec![value_str.as_str()])),
                Arc::new(StringArray::from(vec![source])),
                Arc::new(StringArray::from(vec![now.as_str()])),
            ],
        )
        .map_err(|e| FlowError::Storage(format!("Failed to create batch: {}", e)))?;

        let batches = RecordBatchIterator::new(vec![Ok(batch)], schema);
        table
            .add(Box::new(batches) as Box<dyn arrow_array::RecordBatchReader + Send>)
            .execute()
            .await
            .map_err(|e| FlowError::Storage(format!("Failed to save setting: {}", e)))?;

        Ok(())
    }

    /// Get all settings as a map
    pub async fn get_all(&self) -> FlowResult<Vec<(String, Value)>> {
        let conn = self.conn.read().await;

        let table = conn
            .open_table(TABLE_NAME)
            .execute()
            .await
            .map_err(|e| FlowError::Storage(format!("Failed to open settings table: {}", e)))?;

        let batches: Vec<RecordBatch> = table
            .query()
            .execute()
            .await
            .map_err(|e| FlowError::Storage(format!("Failed to query: {}", e)))?
            .try_collect()
            .await
            .map_err(|e| FlowError::Storage(format!("Failed to collect: {}", e)))?;

        let mut result = Vec::new();
        for batch in &batches {
            let key_col = batch.column_by_name("key")
                .and_then(|c| c.as_any().downcast_ref::<StringArray>());
            let val_col = batch.column_by_name("value")
                .and_then(|c| c.as_any().downcast_ref::<StringArray>());

            if let (Some(keys), Some(vals)) = (key_col, val_col) {
                for i in 0..batch.num_rows() {
                    let k = keys.value(i).to_string();
                    let v: Value = serde_json::from_str(vals.value(i))
                        .unwrap_or_else(|_| Value::String(vals.value(i).to_string()));
                    result.push((k, v));
                }
            }
        }

        Ok(result)
    }

    /// Seed default settings (only if key doesn't exist)
    pub async fn seed_defaults(&self, defaults: &[(&str, Value, &str)]) -> FlowResult<usize> {
        let mut seeded = 0;
        for (key, value, source) in defaults {
            if self.get(key).await?.is_none() {
                self.set(key, value, source).await?;
                seeded += 1;
            }
        }
        Ok(seeded)
    }
}

// Expose the concrete store through the backend-agnostic trait so `StorageBackend`
// can hand out `&dyn SettingsStore` (forwards to the inherent methods above).
#[async_trait]
impl flowr_storage::SettingsStore for LanceSettingsStore {
    async fn get(&self, key: &str) -> FlowResult<Option<Value>> { LanceSettingsStore::get(self, key).await }
    async fn get_string(&self, key: &str) -> FlowResult<Option<String>> { LanceSettingsStore::get_string(self, key).await }
    async fn set(&self, key: &str, value: &Value, source: &str) -> FlowResult<()> { LanceSettingsStore::set(self, key, value, source).await }
    async fn get_all(&self) -> FlowResult<Vec<(String, Value)>> { LanceSettingsStore::get_all(self).await }
    async fn seed_defaults(&self, defaults: &[(&str, Value, &str)]) -> FlowResult<usize> { LanceSettingsStore::seed_defaults(self, defaults).await }
}
