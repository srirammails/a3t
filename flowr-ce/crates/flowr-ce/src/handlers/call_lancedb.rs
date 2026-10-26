//! call:lancedb handler — A3T platform table operations
//!
//! Operations: query, insert, delete, create_table, update, list_tables.
//!
//! The tables live where the `_resource` (default `a3t/a3t-platform-db`) points:
//! the embedded LanceDB (`config.db_path`, CE and EE by default), or — FlowR EE,
//! `config.backend: "adb"` — ADB platform tables over Arrow Flight. Same operations,
//! same answers either way (`crate::platform_db`); the flows don't change.

use async_trait::async_trait;
use serde_json::{json, Value};
use tracing::{info, warn};

use swf_parser::{NamedTask, Task};
use swf_runtime::context::ExecutionContext;
use swf_runtime::error::{RuntimeError, Result};
use swf_runtime::traits::{TaskHandler, TaskResult};

use crate::platform_db::{open_platform_db, PlatformDb};

/// Where A3T's platform database lives when nothing says otherwise.
pub const DEFAULT_PLATFORM_DB_PATH: &str = "/data/flowr/a3t-platform.lance";
/// The resource that names it.
pub const PLATFORM_DB_RESOURCE: &str = "a3t/a3t-platform-db";

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
        let db = platform_db_for(ctx, with.get("_resource").and_then(|v| v.as_str())).await;
        info!(task = %task.name, op = %operation, table = %table_name, db = %db.describe(), "call:lancedb");
        let comm = RuntimeError::Communication;

        match operation {
            "list_tables" => Ok(TaskResult::new(json!(db.list_tables().await.map_err(comm)?))),

            "query" => {
                let filter = with.get("filter").and_then(|v| v.as_str());
                let limit = with.get("limit").and_then(|v| v.as_u64()).unwrap_or(100) as usize;
                let batches = db.query(table_name, filter, limit).await.map_err(comm)?;
                Ok(TaskResult::new(json!(batches_to_json(&batches))))
            }

            "insert" => {
                let records = with.get("records").and_then(|v| v.as_array()).cloned().unwrap_or_default();
                let n = db.insert(table_name, &records).await.map_err(comm)?;
                Ok(TaskResult::new(json!({"inserted": n})))
            }

            "create_table" => {
                let mode = with.get("mode").and_then(|v| v.as_str()).unwrap_or("create_if_not_exists");
                let schema = with.get("schema").and_then(|v| v.as_object());
                Ok(TaskResult::new(db.create_table(table_name, schema, mode).await.map_err(comm)?))
            }

            "update" => {
                let filter = with.get("filter").and_then(|v| v.as_str())
                    .ok_or_else(|| RuntimeError::task_failed(&task.name, "update requires 'filter'"))?;
                let values = with.get("values").and_then(|v| v.as_object())
                    .ok_or_else(|| RuntimeError::task_failed(&task.name, "update requires 'values' object"))?;
                db.update(table_name, filter, values).await.map_err(comm)?;
                Ok(TaskResult::new(json!({"updated": true})))
            }

            "delete" => {
                let filter = with.get("filter").and_then(|v| v.as_str())
                    .ok_or_else(|| RuntimeError::task_failed(&task.name, "delete requires 'filter'"))?;
                // The dependency tree: a template with agents, a deployed agent.
                if let Err(why) = crate::integrity::check_table_delete(db.as_ref(), table_name, filter).await {
                    return Err(RuntimeError::task_failed(&task.name, why));
                }
                db.delete(table_name, filter).await.map_err(comm)?;
                Ok(TaskResult::new(json!({"deleted": true})))
            }

            other => {
                warn!(op = %other, "unsupported lancedb operation, returning empty");
                Ok(TaskResult::new(json!({})))
            }
        }
    }
}

/// The platform database a task's `_resource` names (default `a3t/a3t-platform-db`).
async fn platform_db_for(ctx: &ExecutionContext, resource: Option<&str>) -> std::sync::Arc<dyn PlatformDb> {
    let resource = resource.unwrap_or(PLATFORM_DB_RESOURCE);
    match ctx.storage.as_ref().and_then(|s| s.downcast_ref::<std::sync::Arc<dyn flowr_storage::StorageBackend>>()) {
        Some(storage) => open_platform_db(storage.as_ref(), Some(resource), DEFAULT_PLATFORM_DB_PATH).await,
        None => std::sync::Arc::new(crate::platform_db::LanceDbPlatform::new(DEFAULT_PLATFORM_DB_PATH)),
    }
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

/// Read a table from the A3T platform database.
///
/// The registry lives in the platform database (`a3t/a3t-platform-db`), and the
/// `a3t-*` CRUD flows are how the console reads it. The MCP agent tools used to scan
/// a state key instead, which nothing writes — so they reported "no agents" while the
/// console listed several. This is the one reader they now share.
pub(crate) async fn query_platform_table(
    storage: &dyn flowr_storage::StorageBackend,
    table: &str,
    filter: Option<String>,
    limit: usize,
) -> std::result::Result<Vec<Value>, String> {
    let db = open_platform_db(storage, Some(PLATFORM_DB_RESOURCE), DEFAULT_PLATFORM_DB_PATH).await;
    // A table that has never been written is "no rows", not an error — the
    // console shows an empty list in that case rather than failing.
    if !db.list_tables().await?.contains(&table.to_string()) {
        return Ok(Vec::new());
    }
    Ok(batches_to_json(&db.query(table, filter.as_deref(), limit).await?))
}

/// Quote a value for a platform filter expression. Doubling the quote is what the
/// update path already does; without it a name containing an apostrophe would
/// break the expression (or worse, change which rows match).
pub(crate) fn sql_quote(v: &str) -> String {
    format!("'{}'", v.replace('\'', "''"))
}
