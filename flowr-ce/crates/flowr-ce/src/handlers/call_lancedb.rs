//! call:lancedb handler — LanceDB table operations
//!
//! Operations: query, insert, delete, create_table, vector_search, describe, list_tables

use async_trait::async_trait;
use serde_json::{json, Value};
use tracing::{info, debug, warn};

use swf_parser::{NamedTask, Task};
use swf_runtime::context::ExecutionContext;
use swf_runtime::error::{RuntimeError, Result};
use swf_runtime::traits::{TaskHandler, TaskResult};

pub struct LanceDbHandler;

#[async_trait]
impl TaskHandler for LanceDbHandler {
    fn task_type(&self) -> &str {
        "call:lancedb"
    }

    async fn execute(
        &self,
        task: &NamedTask,
        _input: Value,
        ctx: &mut ExecutionContext,
    ) -> Result<TaskResult> {
        let with = match &task.task {
            Task::Call(c) => c.with.clone().unwrap_or(json!({})),
            _ => return Err(RuntimeError::task_failed(&task.name, "expected call task")),
        };

        let operation = with.get("operation").and_then(|v| v.as_str()).unwrap_or("query");
        let table_name = with.get("table").and_then(|v| v.as_str()).unwrap_or("");

        // Resolve resource for LanceDB path
        let resource_name = with.get("_resource").and_then(|v| v.as_str());
        let db_path = if let Some(rn) = resource_name {
            resolve_db_path(ctx, rn).await.unwrap_or_else(|| "/data/flowr/a3t-platform.lance".to_string())
        } else {
            "/data/flowr/a3t-platform.lance".to_string()
        };

        info!(task = %task.name, op = %operation, table = %table_name, db = %db_path, "call:lancedb");

        // Use lancedb directly
        let db = lancedb::connect(&db_path).execute().await
            .map_err(|e| RuntimeError::Communication(format!("LanceDB connect error: {}", e)))?;

        match operation {
            "list_tables" => {
                let tables = db.table_names().execute().await
                    .map_err(|e| RuntimeError::Communication(format!("list_tables error: {}", e)))?;
                Ok(TaskResult::new(json!(tables)))
            }

            "query" => {
                let filter = with.get("filter").and_then(|v| v.as_str());
                let limit = with.get("limit").and_then(|v| v.as_u64()).unwrap_or(100) as usize;
                let select: Option<Vec<String>> = with.get("select")
                    .and_then(|v| v.as_array())
                    .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect());

                let table = db.open_table(table_name).execute().await
                    .map_err(|e| RuntimeError::Communication(format!("open table '{}' error: {}", table_name, e)))?;

                use lancedb::query::{ExecutableQuery, QueryBase};
                use futures::TryStreamExt;
                use arrow_array::RecordBatch;

                let mut query = table.query();
                if let Some(f) = filter {
                    query = query.only_if(f);
                }
                query = query.limit(limit);

                let batches: Vec<RecordBatch> = query.execute().await
                    .map_err(|e| RuntimeError::Communication(format!("query error: {}", e)))?
                    .try_collect().await
                    .map_err(|e| RuntimeError::Communication(format!("collect error: {}", e)))?;

                let rows = batches_to_json(&batches);
                Ok(TaskResult::new(json!(rows)))
            }

            "insert" => {
                let records = with.get("records").and_then(|v| v.as_array());
                if records.is_none() || records.unwrap().is_empty() {
                    return Ok(TaskResult::new(json!({"inserted": 0})));
                }
                let records = records.unwrap();

                // Ensure table exists
                let table = match db.open_table(table_name).execute().await {
                    Ok(t) => t,
                    Err(_) => {
                        // Create table from first record schema
                        let schema = infer_schema(&records[0]);
                        let batch = records_to_batch(records, &schema);
                        let batches = arrow_array::RecordBatchIterator::new(
                            vec![Ok(batch.clone())], std::sync::Arc::new(schema)
                        );
                        db.create_table(table_name, Box::new(batches)).execute().await
                            .map_err(|e| RuntimeError::Communication(format!("create table error: {}", e)))?;
                        return Ok(TaskResult::new(json!({"inserted": records.len()})));
                    }
                };

                let schema = infer_schema(&records[0]);
                let batch = records_to_batch(records, &schema);
                let batches = arrow_array::RecordBatchIterator::new(
                    vec![Ok(batch)], std::sync::Arc::new(schema)
                );
                table.add(Box::new(batches)).execute().await
                    .map_err(|e| RuntimeError::Communication(format!("insert error: {}", e)))?;

                Ok(TaskResult::new(json!({"inserted": records.len()})))
            }

            "create_table" => {
                let mode = with.get("mode").and_then(|v| v.as_str()).unwrap_or("create_if_not_exists");
                let schema_def = with.get("schema").and_then(|v| v.as_object());

                let tables = db.table_names().execute().await
                    .map_err(|e| RuntimeError::Communication(format!("list tables error: {}", e)))?;

                if tables.contains(&table_name.to_string()) {
                    if mode == "create_if_not_exists" {
                        debug!(table = %table_name, "table already exists, skipping");
                        return Ok(TaskResult::new(json!({"created": false, "exists": true})));
                    }
                }

                let schema = if let Some(s) = schema_def {
                    let fields: Vec<arrow_schema::Field> = s.iter()
                        .map(|(k, _v)| arrow_schema::Field::new(k, arrow_schema::DataType::Utf8, true))
                        .collect();
                    arrow_schema::Schema::new(fields)
                } else {
                    arrow_schema::Schema::new(vec![
                        arrow_schema::Field::new("id", arrow_schema::DataType::Utf8, false),
                    ])
                };

                let batch = arrow_array::RecordBatch::new_empty(std::sync::Arc::new(schema.clone()));
                let batches = arrow_array::RecordBatchIterator::new(
                    vec![Ok(batch)], std::sync::Arc::new(schema)
                );
                db.create_table(table_name, Box::new(batches)).execute().await
                    .map_err(|e| RuntimeError::Communication(format!("create_table error: {}", e)))?;

                info!(table = %table_name, "created LanceDB table");
                Ok(TaskResult::new(json!({"created": true, "table": table_name})))
            }

            "update" => {
                let filter = with.get("filter").and_then(|v| v.as_str())
                    .ok_or_else(|| RuntimeError::task_failed(&task.name, "update requires 'filter'"))?;
                let values = with.get("values").and_then(|v| v.as_object())
                    .ok_or_else(|| RuntimeError::task_failed(&task.name, "update requires 'values' object"))?;

                let table = db.open_table(table_name).execute().await
                    .map_err(|e| RuntimeError::Communication(format!("open table error: {}", e)))?;

                let columns: Vec<(String, String)> = values.iter()
                    .map(|(k, v)| {
                        let val_str = match v {
                            Value::String(s) => format!("'{}'", s.replace('\'', "''")),
                            Value::Null => "NULL".to_string(),
                            other => other.to_string(),
                        };
                        (k.clone(), val_str)
                    })
                    .collect();

                let mut op = table.update().only_if(filter);
                for (col, val) in &columns {
                    op = op.column(col, val.as_str());
                }
                op.execute().await
                    .map_err(|e| RuntimeError::Communication(format!("update error: {}", e)))?;

                Ok(TaskResult::new(json!({"updated": true})))
            }

            "delete" => {
                let filter = with.get("filter").and_then(|v| v.as_str())
                    .ok_or_else(|| RuntimeError::task_failed(&task.name, "delete requires 'filter'"))?;

                let table = db.open_table(table_name).execute().await
                    .map_err(|e| RuntimeError::Communication(format!("open table error: {}", e)))?;
                table.delete(filter).await
                    .map_err(|e| RuntimeError::Communication(format!("delete error: {}", e)))?;

                Ok(TaskResult::new(json!({"deleted": true})))
            }

            other => {
                warn!(op = %other, "unsupported lancedb operation, returning empty");
                Ok(TaskResult::new(json!({})))
            }
        }
    }
}

async fn resolve_db_path(ctx: &ExecutionContext, resource_name: &str) -> Option<String> {
    let storage_any = ctx.storage.as_ref()?;
    let storage = storage_any.downcast_ref::<std::sync::Arc<dyn flowr_storage::StorageBackend>>()?;
    let (ns, name) = if resource_name.contains('/') {
        let parts: Vec<&str> = resource_name.splitn(2, '/').collect();
        (parts[0], parts[1])
    } else {
        ("a3t", resource_name)
    };
    let resource = storage.resources().get(ns, name).await.ok()?;
    resource.config.get("db_path").and_then(|v| v.as_str()).map(String::from)
}

pub(crate) fn batches_to_json(batches: &[arrow_array::RecordBatch]) -> Vec<Value> {
    use arrow_array::Array;
    let mut rows = Vec::new();
    for batch in batches {
        let schema = batch.schema();
        for row_idx in 0..batch.num_rows() {
            let mut obj = serde_json::Map::new();
            for (col_idx, field) in schema.fields().iter().enumerate() {
                let col = batch.column(col_idx);
                if let Some(str_arr) = col.as_any().downcast_ref::<arrow_array::StringArray>() {
                    if str_arr.is_valid(row_idx) {
                        obj.insert(field.name().clone(), json!(str_arr.value(row_idx)));
                    }
                } else if let Some(str_arr) = col.as_any().downcast_ref::<arrow_array::LargeStringArray>() {
                    if str_arr.is_valid(row_idx) {
                        obj.insert(field.name().clone(), json!(str_arr.value(row_idx)));
                    }
                }
            }
            if !obj.is_empty() {
                rows.push(Value::Object(obj));
            }
        }
    }
    rows
}

fn infer_schema(record: &Value) -> arrow_schema::Schema {
    use arrow_schema::{DataType, Field};
    let fields: Vec<Field> = if let Some(obj) = record.as_object() {
        obj.keys().map(|k| Field::new(k, DataType::Utf8, true)).collect()
    } else {
        vec![Field::new("value", DataType::Utf8, true)]
    };
    arrow_schema::Schema::new(fields)
}

fn records_to_batch(records: &[Value], schema: &arrow_schema::Schema) -> arrow_array::RecordBatch {
    use arrow_array::StringArray;
    let columns: Vec<std::sync::Arc<dyn arrow_array::Array>> = schema.fields().iter().map(|field| {
        let values: Vec<Option<String>> = records.iter().map(|r| {
            r.get(field.name()).map(|v| match v {
                Value::String(s) => s.clone(),
                other => other.to_string(),
            })
        }).collect();
        std::sync::Arc::new(StringArray::from(values)) as std::sync::Arc<dyn arrow_array::Array>
    }).collect();
    arrow_array::RecordBatch::try_new(std::sync::Arc::new(schema.clone()), columns).unwrap()
}

/// Read a table from the A3T platform database.
///
/// The registry lives in LanceDB (`a3t/a3t-platform-db`), and the `a3t-*` CRUD
/// flows are how the console reads it. The MCP agent tools used to scan a state
/// key instead, which nothing writes — so they reported "no agents" while the
/// console listed several. This is the one reader they now share.
pub(crate) async fn query_platform_table(
    storage: &dyn flowr_storage::StorageBackend,
    table: &str,
    filter: Option<String>,
    limit: usize,
) -> std::result::Result<Vec<Value>, String> {
    let db_path = storage.resources().get("a3t", "a3t-platform-db").await.ok()
        .and_then(|r| r.config.get("db_path").and_then(|v| v.as_str()).map(String::from))
        .unwrap_or_else(|| "/data/flowr/a3t-platform.lance".to_string());

    let db = lancedb::connect(&db_path).execute().await
        .map_err(|e| format!("LanceDB connect error: {}", e))?;

    // A table that has never been written is "no rows", not an error — the
    // console shows an empty list in that case rather than failing.
    let names = db.table_names().execute().await
        .map_err(|e| format!("list_tables error: {}", e))?;
    if !names.contains(&table.to_string()) { return Ok(Vec::new()); }

    let tbl = db.open_table(table).execute().await
        .map_err(|e| format!("open table '{}' error: {}", table, e))?;

    use lancedb::query::{ExecutableQuery, QueryBase};
    use futures::TryStreamExt;
    let mut q = tbl.query();
    if let Some(f) = filter.as_deref() { q = q.only_if(f); }
    let batches: Vec<arrow_array::RecordBatch> = q.limit(limit).execute().await
        .map_err(|e| format!("query error: {}", e))?
        .try_collect().await
        .map_err(|e| format!("collect error: {}", e))?;
    Ok(batches_to_json(&batches))
}

/// Quote a value for a LanceDB filter expression. Doubling the quote is what the
/// update path already does; without it a name containing an apostrophe would
/// break the expression (or worse, change which rows match).
pub(crate) fn sql_quote(v: &str) -> String {
    format!("'{}'", v.replace('\'', "''"))
}
