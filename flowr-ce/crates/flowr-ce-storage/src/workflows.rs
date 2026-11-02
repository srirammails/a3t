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

        conn.create_table(TABLE_NAME, Box::new(batches) as Box<dyn arrow_array::RecordBatchReader + Send>)
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

        // Re-registering a version replaces it; other versions stay.
        let same = format!("namespace = '{}' AND name = '{}' AND version = '{}'",
            record.metadata.namespace, record.metadata.name, record.metadata.version);
        table.delete(&same).await.map_err(|e| FlowError::Storage(format!("Failed to replace version: {}", e)))?;
        let batches = RecordBatchIterator::new(vec![Ok(batch)], schema);
        table
            .add(Box::new(batches) as Box<dyn arrow_array::RecordBatchReader + Send>)
            .execute()
            .await
            .map_err(|e| FlowError::Storage(format!("Failed to add workflow: {}", e)))?;
        drop(table);
        drop(conn);
        // The new version is the active one.
        self.set_active_version(&record.metadata.namespace, &record.metadata.name, &record.metadata.version).await
    }

    async fn get(&self, namespace: &str, name: &str) -> FlowResult<WorkflowRecord> {
        let conn = self.conn.read().await;
        let rows = version_rows(&conn, &format!("namespace = '{}' AND name = '{}' AND enabled = 'true'", namespace, name)).await?;
        let row = active_of(&rows).ok_or_else(|| FlowError::NotFound(format!("Workflow {}/{} not found", namespace, name)))?;
        serde_json::from_str(&row.4).map_err(|e| FlowError::Storage(format!("Failed to deserialize workflow: {}", e)))
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
        let rows = version_rows(&conn, &format!("namespace = '{}'", namespace)).await?;
        // One entry per flow: its active version.
        let mut by_name: std::collections::BTreeMap<String, Vec<(String, String, String, String, String)>> = Default::default();
        for r in rows {
            let name = serde_json::from_str::<WorkflowRecord>(&r.4).map(|w| w.metadata.name).unwrap_or_default();
            by_name.entry(name).or_default().push(r);
        }
        let mut summaries = Vec::new();
        for versions in by_name.values() {
            let Some(row) = active_of(versions) else { continue };
            if let Ok(record) = serde_json::from_str::<WorkflowRecord>(&row.4) {
                summaries.push(WorkflowSummary {
                    step_count: record.parse().map(|wf| wf.tasks.len()).unwrap_or(0),
                    namespace: record.metadata.namespace.clone(),
                    name: record.metadata.name.clone(),
                    version: record.metadata.version.clone(),
                    enabled: record.metadata.enabled,
                });
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
        let conn = self.conn.read().await;
        let table = conn.open_table(TABLE_NAME).execute().await
            .map_err(|e| FlowError::Storage(format!("Failed to open workflows table: {}", e)))?;
        table.update()
            .only_if(format!("namespace = '{}' AND name = '{}'", namespace, name))
            .column("active_version", format!("'{}'", version.replace('\'', "''")))
            .execute().await
            .map_err(|e| FlowError::Storage(format!("Failed to set the active version: {}", e)))?;
        Ok(())
    }

    async fn exists(&self, namespace: &str, name: &str) -> FlowResult<bool> {
        match self.get(namespace, name).await {
            Ok(_) => Ok(true),
            Err(FlowError::NotFound(_)) => Ok(false),
            Err(e) => Err(e),
        }
    }

    async fn namespaces(&self) -> FlowResult<Vec<String>> {
        distinct_namespaces(&*self.conn.read().await, TABLE_NAME).await
    }

    async fn versions(&self, namespace: &str, name: &str) -> FlowResult<(Vec<String>, String)> {
        let conn = self.conn.read().await;
        let rows = version_rows(&conn, &format!("namespace = '{}' AND name = '{}'", namespace, name)).await?;
        let active = active_of(&rows).map(|r| r.0.clone()).unwrap_or_default();
        let mut v: Vec<String> = rows.into_iter().map(|r| r.0).collect();
        v.sort();
        v.dedup();
        Ok((v, active))
    }
}

/// The distinct values of a table's `namespace` column, sorted.
pub(crate) async fn distinct_namespaces(conn: &Connection, table: &str) -> FlowResult<Vec<String>> {
    let table = conn
        .open_table(table)
        .execute()
        .await
        .map_err(|e| FlowError::Storage(format!("Failed to open {table} table: {e}")))?;
    let batches: Vec<RecordBatch> = table
        .query()
        .select(lancedb::query::Select::columns(&["namespace"]))
        .execute()
        .await
        .map_err(|e| FlowError::Storage(format!("Failed to query namespaces: {e}")))?
        .try_collect()
        .await
        .map_err(|e| FlowError::Storage(format!("Failed to collect namespaces: {e}")))?;
    let mut out = std::collections::BTreeSet::new();
    for batch in &batches {
        if let Some(col) = batch.column_by_name("namespace").and_then(|c| c.as_any().downcast_ref::<StringArray>()) {
            out.extend((0..col.len()).filter(|&i| col.is_valid(i)).map(|i| col.value(i).to_string()));
        }
    }
    Ok(out.into_iter().collect())
}


/// A flow's version rows: (version, active_version, enabled, updated_at, definition).
async fn version_rows(conn: &Connection, filter: &str) -> FlowResult<Vec<(String, String, String, String, String)>> {
    let table = conn.open_table(TABLE_NAME).execute().await
        .map_err(|e| FlowError::Storage(format!("Failed to open workflows table: {}", e)))?;
    let batches: Vec<RecordBatch> = table.query().only_if(filter.to_string()).execute().await
        .map_err(|e| FlowError::Storage(format!("Failed to query workflows: {}", e)))?
        .try_collect().await
        .map_err(|e| FlowError::Storage(format!("Failed to collect results: {}", e)))?;
    let mut out = Vec::new();
    for b in &batches {
        let col = |n: &str| b.column_by_name(n).and_then(|c| c.as_any().downcast_ref::<StringArray>().cloned());
        let (Some(v), Some(a), Some(e), Some(u), Some(d)) = (col("version"), col("active_version"), col("enabled"), col("updated_at"), col("definition")) else { continue };
        for i in 0..b.num_rows() {
            let get = |c: &StringArray| if c.is_valid(i) { c.value(i).to_string() } else { String::new() };
            out.push((get(&v), get(&a), get(&e), get(&u), get(&d)));
        }
    }
    Ok(out)
}

/// Of a flow's version rows, the active one: its `active_version`, else the newest.
fn active_of(rows: &[(String, String, String, String, String)]) -> Option<&(String, String, String, String, String)> {
    let newest = rows.iter().max_by_key(|r| r.3.clone())?;
    rows.iter().find(|r| !newest.1.is_empty() && r.0 == newest.1).or(Some(newest))
}

#[cfg(test)]
mod tests {
    use flowr_storage::StorageBackend;
    use flowr_types::{Resource, WorkflowRecord};

    use crate::LanceStorage;

    fn flow(namespace: &str, name: &str) -> WorkflowRecord {
        WorkflowRecord::from_yaml(&format!(
            "document:\n  dsl: 1.0.3\n  namespace: {namespace}\n  name: {name}\n  version: 1.0.0\ndo:\n- a:\n    set:\n      x: 1\n"
        ))
        .unwrap()
    }

    fn flow_v(namespace: &str, name: &str, version: &str) -> WorkflowRecord {
        WorkflowRecord::from_yaml(&format!(
            "document:\n  dsl: 1.0.3\n  namespace: {namespace}\n  name: {name}\n  version: {version}\ndo:\n- a:\n    set:\n      v: '{version}'\n"
        ))
        .unwrap()
    }

    #[tokio::test]
    async fn versions_stay_side_by_side_and_one_is_active() {
        let storage = LanceStorage::in_memory().await.unwrap();
        storage.init().await.unwrap();
        let w = storage.workflows();
        w.register(flow_v("p", "f", "1.0.0")).await.unwrap();
        w.register(flow_v("p", "f", "1.1.0")).await.unwrap();
        assert_eq!(w.get("p", "f").await.unwrap().metadata.version, "1.1.0", "the newest release is active");
        assert_eq!(w.get_version("p", "f", "1.0.0").await.unwrap().metadata.version, "1.0.0", "the old one stays pinnable");
        assert_eq!(w.list("p").await.unwrap().len(), 1, "listed once");
        w.set_active_version("p", "f", "1.0.0").await.unwrap();
        assert_eq!(w.get("p", "f").await.unwrap().metadata.version, "1.0.0", "rolled back");
        w.register(flow_v("p", "f", "1.0.0")).await.unwrap();
        assert_eq!(w.list("p").await.unwrap()[0].version, "1.0.0", "re-registering a version replaces it, not duplicates");
    }

    #[tokio::test]
    async fn namespaces_are_distinct_and_sorted() {
        let storage = LanceStorage::in_memory().await.unwrap();
        storage.init().await.unwrap();
        for (ns, name) in [("orders", "a"), ("a3t", "b"), ("orders", "c")] {
            storage.workflows().register(flow(ns, name)).await.unwrap();
        }
        storage.resources().create(Resource::new("db", "lancedb", serde_json::json!({})).with_namespace("billing")).await.unwrap();

        assert_eq!(storage.workflows().namespaces().await.unwrap(), ["a3t", "orders"]);
        assert_eq!(storage.resources().namespaces().await.unwrap(), ["billing"]);
    }
}
