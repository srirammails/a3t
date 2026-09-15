//! State store implementation using LanceDB

use std::sync::Arc;
use std::time::Duration;

use arrow_array::{Array, RecordBatch, RecordBatchIterator, StringArray};
use arrow_schema::{DataType, Field, Schema};
use async_trait::async_trait;
use futures::TryStreamExt;
use lancedb::query::{ExecutableQuery, QueryBase};
use lancedb::Connection;
use serde_json::Value;
use tokio::sync::RwLock;

use flowr_storage::StateStore;
use flowr_types::{FlowError, FlowResult};

const TABLE_NAME: &str = "state";

/// LanceDB-backed key-value state store
pub struct LanceStateStore {
    conn: Arc<RwLock<Connection>>,
}

impl LanceStateStore {
    pub fn new(conn: Arc<RwLock<Connection>>) -> Self {
        Self { conn }
    }

    /// Initialize the state table
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
            Field::new("value", DataType::Utf8, false),
            Field::new("expires_at", DataType::Utf8, true),
            Field::new("created_at", DataType::Utf8, false),
        ]));

        let batch = RecordBatch::new_empty(schema.clone());
        let batches = RecordBatchIterator::new(vec![Ok(batch)], schema);

        conn.create_table(TABLE_NAME, Box::new(batches))
            .execute()
            .await
            .map_err(|e| FlowError::Storage(format!("Failed to create state table: {}", e)))?;

        Ok(())
    }
}

#[async_trait]
impl StateStore for LanceStateStore {
    async fn get(&self, key: &str) -> FlowResult<Option<Value>> {
        let conn = self.conn.read().await;

        let table = conn
            .open_table(TABLE_NAME)
            .execute()
            .await
            .map_err(|e| FlowError::Storage(format!("Failed to open state table: {}", e)))?;

        let filter = format!("key = '{}'", key);

        let batches: Vec<RecordBatch> = table
            .query()
            .only_if(filter)
            .limit(1)
            .execute()
            .await
            .map_err(|e| FlowError::Storage(format!("Failed to query state: {}", e)))?
            .try_collect()
            .await
            .map_err(|e| FlowError::Storage(format!("Failed to collect results: {}", e)))?;

        if batches.is_empty() || batches[0].num_rows() == 0 {
            return Ok(None);
        }

        let batch = &batches[0];

        // Check expiration
        if let Some(expires_col) = batch
            .column_by_name("expires_at")
            .and_then(|c| c.as_any().downcast_ref::<StringArray>())
        {
            if !expires_col.is_null(0) {
                let expires_str = expires_col.value(0);
                if let Ok(expires) = chrono::DateTime::parse_from_rfc3339(expires_str) {
                    if expires < chrono::Utc::now() {
                        // Expired - delete and return None
                        let _ = self.delete(key).await;
                        return Ok(None);
                    }
                }
            }
        }

        let value_col = batch
            .column_by_name("value")
            .and_then(|c| c.as_any().downcast_ref::<StringArray>())
            .ok_or_else(|| FlowError::Storage("Missing value column".to_string()))?;

        let value_json = value_col.value(0);

        let value: Value = serde_json::from_str(value_json)
            .map_err(|e| FlowError::Storage(format!("Failed to deserialize value: {}", e)))?;

        Ok(Some(value))
    }

    async fn set(&self, key: &str, value: Value, ttl: Option<Duration>) -> FlowResult<()> {
        // Delete existing key first
        let _ = self.delete(key).await;

        let conn = self.conn.read().await;

        let table = conn
            .open_table(TABLE_NAME)
            .execute()
            .await
            .map_err(|e| FlowError::Storage(format!("Failed to open state table: {}", e)))?;

        let now = chrono::Utc::now();
        let now_str = now.to_rfc3339();
        let expires_at = ttl.map(|d| (now + chrono::Duration::from_std(d).unwrap()).to_rfc3339());

        let value_json = serde_json::to_string(&value)
            .map_err(|e| FlowError::Storage(format!("Failed to serialize value: {}", e)))?;

        let schema = Arc::new(Schema::new(vec![
            Field::new("key", DataType::Utf8, false),
            Field::new("value", DataType::Utf8, false),
            Field::new("expires_at", DataType::Utf8, true),
            Field::new("created_at", DataType::Utf8, false),
        ]));

        let expires_array: StringArray = match &expires_at {
            Some(e) => StringArray::from(vec![Some(e.as_str())]),
            None => StringArray::from(vec![None::<&str>]),
        };

        let batch = RecordBatch::try_new(
            schema.clone(),
            vec![
                Arc::new(StringArray::from(vec![key])),
                Arc::new(StringArray::from(vec![value_json.as_str()])),
                Arc::new(expires_array),
                Arc::new(StringArray::from(vec![now_str.as_str()])),
            ],
        )
        .map_err(|e| FlowError::Storage(format!("Failed to create record batch: {}", e)))?;

        let batches = RecordBatchIterator::new(vec![Ok(batch)], schema);
        table
            .add(Box::new(batches))
            .execute()
            .await
            .map_err(|e| FlowError::Storage(format!("Failed to set state: {}", e)))?;

        Ok(())
    }

    async fn delete(&self, key: &str) -> FlowResult<bool> {
        let conn = self.conn.read().await;

        let table = conn
            .open_table(TABLE_NAME)
            .execute()
            .await
            .map_err(|e| FlowError::Storage(format!("Failed to open state table: {}", e)))?;

        let filter = format!("key = '{}'", key);

        table
            .delete(&filter)
            .await
            .map_err(|e| FlowError::Storage(format!("Failed to delete state: {}", e)))?;

        Ok(true)
    }

    async fn keys(&self, pattern: &str) -> FlowResult<Vec<String>> {
        let conn = self.conn.read().await;

        let table = conn
            .open_table(TABLE_NAME)
            .execute()
            .await
            .map_err(|e| FlowError::Storage(format!("Failed to open state table: {}", e)))?;

        // Convert glob pattern to SQL LIKE pattern
        let like_pattern = pattern.replace('*', "%").replace('?', "_");
        let filter = format!("key LIKE '{}'", like_pattern);

        let batches: Vec<RecordBatch> = table
            .query()
            .only_if(filter)
            .execute()
            .await
            .map_err(|e| FlowError::Storage(format!("Failed to query state: {}", e)))?
            .try_collect()
            .await
            .map_err(|e| FlowError::Storage(format!("Failed to collect results: {}", e)))?;

        let mut keys = Vec::new();

        for batch in batches {
            if let Some(key_col) = batch
                .column_by_name("key")
                .and_then(|c| c.as_any().downcast_ref::<StringArray>())
            {
                for i in 0..batch.num_rows() {
                    keys.push(key_col.value(i).to_string());
                }
            }
        }

        Ok(keys)
    }

    async fn exists(&self, key: &str) -> FlowResult<bool> {
        Ok(self.get(key).await?.is_some())
    }

    async fn incr(&self, key: &str, delta: i64) -> FlowResult<i64> {
        let current = self.get(key).await?;

        let new_value = match current {
            Some(Value::Number(n)) => {
                let current_i64 = n.as_i64().unwrap_or(0);
                current_i64 + delta
            }
            None => delta,
            _ => return Err(FlowError::Storage("Value is not a number".to_string())),
        };

        self.set(key, Value::Number(new_value.into()), None).await?;

        Ok(new_value)
    }
}
