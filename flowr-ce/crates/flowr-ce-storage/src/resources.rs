//! Resource store implementation using LanceDB

use std::sync::Arc;

use arrow_array::{Array, RecordBatch, RecordBatchIterator, StringArray};
use arrow_schema::{DataType, Field, Schema};
use async_trait::async_trait;
use futures::TryStreamExt;
use lancedb::query::{ExecutableQuery, QueryBase};
use lancedb::Connection;
use serde_json::Value;
use tokio::sync::RwLock;

use flowr_storage::ResourceStore;
use flowr_types::{FlowError, FlowResult, Resource, ResourceType};

const TYPES_TABLE: &str = "resource_types";
const RESOURCES_TABLE: &str = "resources";

/// LanceDB-backed resource store
pub struct LanceResourceStore {
    conn: Arc<RwLock<Connection>>,
}

impl LanceResourceStore {
    pub fn new(conn: Arc<RwLock<Connection>>) -> Self {
        Self { conn }
    }

    /// Initialize the resource tables
    pub async fn init(&self) -> FlowResult<()> {
        let conn = self.conn.read().await;

        let tables = conn
            .table_names()
            .execute()
            .await
            .map_err(|e| FlowError::Storage(format!("Failed to list tables: {}", e)))?;

        // Create resource types table
        if !tables.contains(&TYPES_TABLE.to_string()) {
            let schema = Arc::new(Schema::new(vec![
                Field::new("name", DataType::Utf8, false),
                Field::new("definition", DataType::Utf8, false),
            ]));

            let batch = RecordBatch::new_empty(schema.clone());
            let batches = RecordBatchIterator::new(vec![Ok(batch)], schema);

            conn.create_table(TYPES_TABLE, Box::new(batches))
                .execute()
                .await
                .map_err(|e| {
                    FlowError::Storage(format!("Failed to create resource_types table: {}", e))
                })?;
        }

        // Create resources table
        if !tables.contains(&RESOURCES_TABLE.to_string()) {
            let schema = Arc::new(Schema::new(vec![
                Field::new("namespace", DataType::Utf8, false),
                Field::new("name", DataType::Utf8, false),
                Field::new("type_ref", DataType::Utf8, false),
                Field::new("config", DataType::Utf8, false),
                Field::new("created_at", DataType::Utf8, false),
                Field::new("updated_at", DataType::Utf8, false),
            ]));

            let batch = RecordBatch::new_empty(schema.clone());
            let batches = RecordBatchIterator::new(vec![Ok(batch)], schema);

            conn.create_table(RESOURCES_TABLE, Box::new(batches))
                .execute()
                .await
                .map_err(|e| {
                    FlowError::Storage(format!("Failed to create resources table: {}", e))
                })?;
        }

        Ok(())
    }
}

#[async_trait]
impl ResourceStore for LanceResourceStore {
    async fn register_type(&self, resource_type: ResourceType) -> FlowResult<()> {
        let conn = self.conn.read().await;

        let table = conn
            .open_table(TYPES_TABLE)
            .execute()
            .await
            .map_err(|e| FlowError::Storage(format!("Failed to open resource_types table: {}", e)))?;

        let def_json = serde_json::to_string(&resource_type)
            .map_err(|e| FlowError::Storage(format!("Failed to serialize resource type: {}", e)))?;

        let schema = Arc::new(Schema::new(vec![
            Field::new("name", DataType::Utf8, false),
            Field::new("definition", DataType::Utf8, false),
        ]));

        let batch = RecordBatch::try_new(
            schema.clone(),
            vec![
                Arc::new(StringArray::from(vec![resource_type.name.as_str()])),
                Arc::new(StringArray::from(vec![def_json.as_str()])),
            ],
        )
        .map_err(|e| FlowError::Storage(format!("Failed to create record batch: {}", e)))?;

        let batches = RecordBatchIterator::new(vec![Ok(batch)], schema);
        table
            .add(Box::new(batches))
            .execute()
            .await
            .map_err(|e| FlowError::Storage(format!("Failed to add resource type: {}", e)))?;

        Ok(())
    }

    async fn get_type(&self, name: &str) -> FlowResult<ResourceType> {
        let conn = self.conn.read().await;

        let table = conn
            .open_table(TYPES_TABLE)
            .execute()
            .await
            .map_err(|e| FlowError::Storage(format!("Failed to open resource_types table: {}", e)))?;

        let filter = format!("name = '{}'", name);

        let batches: Vec<RecordBatch> = table
            .query()
            .only_if(filter)
            .limit(1)
            .execute()
            .await
            .map_err(|e| FlowError::Storage(format!("Failed to query resource types: {}", e)))?
            .try_collect()
            .await
            .map_err(|e| FlowError::Storage(format!("Failed to collect results: {}", e)))?;

        if batches.is_empty() || batches[0].num_rows() == 0 {
            return Err(FlowError::NotFound(format!(
                "Resource type {} not found",
                name
            )));
        }

        let def_col = batches[0]
            .column_by_name("definition")
            .and_then(|c| c.as_any().downcast_ref::<StringArray>())
            .ok_or_else(|| FlowError::Storage("Missing definition column".to_string()))?;

        let def_json = def_col.value(0);

        serde_json::from_str(def_json)
            .map_err(|e| FlowError::Storage(format!("Failed to deserialize resource type: {}", e)))
    }

    async fn list_types(&self) -> FlowResult<Vec<ResourceType>> {
        let conn = self.conn.read().await;

        let table = conn
            .open_table(TYPES_TABLE)
            .execute()
            .await
            .map_err(|e| FlowError::Storage(format!("Failed to open resource_types table: {}", e)))?;

        let batches: Vec<RecordBatch> = table
            .query()
            .execute()
            .await
            .map_err(|e| FlowError::Storage(format!("Failed to query resource types: {}", e)))?
            .try_collect()
            .await
            .map_err(|e| FlowError::Storage(format!("Failed to collect results: {}", e)))?;

        let mut types = Vec::new();

        for batch in batches {
            if let Some(def_col) = batch
                .column_by_name("definition")
                .and_then(|c| c.as_any().downcast_ref::<StringArray>())
            {
                for i in 0..batch.num_rows() {
                    if let Ok(rt) = serde_json::from_str(def_col.value(i)) {
                        types.push(rt);
                    }
                }
            }
        }

        Ok(types)
    }

    async fn create(&self, resource: Resource) -> FlowResult<Resource> {
        let conn = self.conn.read().await;

        let table = conn
            .open_table(RESOURCES_TABLE)
            .execute()
            .await
            .map_err(|e| FlowError::Storage(format!("Failed to open resources table: {}", e)))?;

        let now = chrono::Utc::now().to_rfc3339();
        let config_json = serde_json::to_string(&resource.config)
            .map_err(|e| FlowError::Storage(format!("Failed to serialize config: {}", e)))?;

        let schema = Arc::new(Schema::new(vec![
            Field::new("namespace", DataType::Utf8, false),
            Field::new("name", DataType::Utf8, false),
            Field::new("type_ref", DataType::Utf8, false),
            Field::new("config", DataType::Utf8, false),
            Field::new("created_at", DataType::Utf8, false),
            Field::new("updated_at", DataType::Utf8, false),
        ]));

        let batch = RecordBatch::try_new(
            schema.clone(),
            vec![
                Arc::new(StringArray::from(vec![resource.namespace.as_str()])),
                Arc::new(StringArray::from(vec![resource.name.as_str()])),
                Arc::new(StringArray::from(vec![resource.type_ref.as_str()])),
                Arc::new(StringArray::from(vec![config_json.as_str()])),
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
            .map_err(|e| FlowError::Storage(format!("Failed to add resource: {}", e)))?;

        Ok(resource)
    }

    async fn get(&self, namespace: &str, name: &str) -> FlowResult<Resource> {
        let conn = self.conn.read().await;

        let table = conn
            .open_table(RESOURCES_TABLE)
            .execute()
            .await
            .map_err(|e| FlowError::Storage(format!("Failed to open resources table: {}", e)))?;

        let filter = format!("namespace = '{}' AND name = '{}'", namespace, name);

        let batches: Vec<RecordBatch> = table
            .query()
            .only_if(filter)
            .limit(1)
            .execute()
            .await
            .map_err(|e| FlowError::Storage(format!("Failed to query resources: {}", e)))?
            .try_collect()
            .await
            .map_err(|e| FlowError::Storage(format!("Failed to collect results: {}", e)))?;

        if batches.is_empty() || batches[0].num_rows() == 0 {
            return Err(FlowError::NotFound(format!(
                "Resource {}/{} not found",
                namespace, name
            )));
        }

        let batch = &batches[0];
        let type_ref = batch
            .column_by_name("type_ref")
            .and_then(|c| c.as_any().downcast_ref::<StringArray>())
            .map(|a| a.value(0).to_string())
            .ok_or_else(|| FlowError::Storage("Missing type_ref column".to_string()))?;

        let config_json = batch
            .column_by_name("config")
            .and_then(|c| c.as_any().downcast_ref::<StringArray>())
            .map(|a| a.value(0))
            .ok_or_else(|| FlowError::Storage("Missing config column".to_string()))?;

        let config: Value = serde_json::from_str(config_json)
            .map_err(|e| FlowError::Storage(format!("Failed to deserialize config: {}", e)))?;

        Ok(Resource::new(name, type_ref, config).with_namespace(namespace))
    }

    async fn namespaces(&self) -> FlowResult<Vec<String>> {
        crate::workflows::distinct_namespaces(&*self.conn.read().await, RESOURCES_TABLE).await
    }

    async fn list(&self, namespace: &str) -> FlowResult<Vec<Resource>> {
        let conn = self.conn.read().await;

        let table = conn
            .open_table(RESOURCES_TABLE)
            .execute()
            .await
            .map_err(|e| FlowError::Storage(format!("Failed to open resources table: {}", e)))?;

        let filter = format!("namespace = '{}'", namespace);

        let batches: Vec<RecordBatch> = table
            .query()
            .only_if(filter)
            .execute()
            .await
            .map_err(|e| FlowError::Storage(format!("Failed to query resources: {}", e)))?
            .try_collect()
            .await
            .map_err(|e| FlowError::Storage(format!("Failed to collect results: {}", e)))?;

        let mut resources = Vec::new();

        for batch in batches {
            let names = batch
                .column_by_name("name")
                .and_then(|c| c.as_any().downcast_ref::<StringArray>());
            let types = batch
                .column_by_name("type_ref")
                .and_then(|c| c.as_any().downcast_ref::<StringArray>());
            let configs = batch
                .column_by_name("config")
                .and_then(|c| c.as_any().downcast_ref::<StringArray>());

            if let (Some(names), Some(types), Some(configs)) = (names, types, configs) {
                for i in 0..batch.num_rows() {
                    if let Ok(config) = serde_json::from_str(configs.value(i)) {
                        resources.push(
                            Resource::new(names.value(i), types.value(i), config)
                                .with_namespace(namespace),
                        );
                    }
                }
            }
        }

        Ok(resources)
    }

    async fn update(&self, namespace: &str, name: &str, config: Value) -> FlowResult<Resource> {
        // Get existing resource
        let existing = self.get(namespace, name).await?;

        // Delete and re-create (LanceDB limitation)
        self.delete(namespace, name).await?;

        let updated = Resource::new(name, existing.type_ref, config).with_namespace(namespace);

        self.create(updated).await
    }

    async fn delete(&self, namespace: &str, name: &str) -> FlowResult<()> {
        let conn = self.conn.read().await;

        let table = conn
            .open_table(RESOURCES_TABLE)
            .execute()
            .await
            .map_err(|e| FlowError::Storage(format!("Failed to open resources table: {}", e)))?;

        let filter = format!("namespace = '{}' AND name = '{}'", namespace, name);

        table
            .delete(&filter)
            .await
            .map_err(|e| FlowError::Storage(format!("Failed to delete resource: {}", e)))?;

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
