//! A3T's platform database behind one interface (ADB stage G, G1–G3).
//!
//! The registry tables (`template_registry`, `users`, `agent_configs`, …) are read
//! and written through four doors — `call:lancedb`, the MCP agent tools
//! (`query_platform_table`), the delete integrity checks and EE login / signup — and
//! all of them go through [`PlatformDb`], so they always see the same data.
//!
//! - [`LanceDbPlatform`]: the embedded LanceDB at a path (CE, and EE by default).
//! - `AdbPlatform` (feature `adb-platform`, FlowR EE): ADB platform tables over Arrow
//!   Flight — an optional upgrade, chosen by the `a3t/a3t-platform-db` resource
//!   (`config.backend: "adb"`, `config.url`, optional `config.flight_url`).
//!
//! Choosing ADB is configuration, not failover (G3): when ADB is configured and down,
//! platform calls fail; they never fall back to a local copy.

use std::sync::Arc;

use arrow_array::RecordBatch;
use async_trait::async_trait;
use serde_json::{json, Map, Value};
use tracing::{debug, info, warn};

/// The six `call:lancedb` operations, with its semantics: UTF-8 columns, Lance SQL
/// filters, insert creating a missing table from the first record.
#[async_trait]
pub trait PlatformDb: Send + Sync {
    /// For logs: where the tables live.
    fn describe(&self) -> String;
    async fn list_tables(&self) -> Result<Vec<String>, String>;
    async fn query(&self, table: &str, filter: Option<&str>, limit: usize) -> Result<Vec<RecordBatch>, String>;
    /// Returns how many rows went in.
    async fn insert(&self, table: &str, records: &[Value]) -> Result<usize, String>;
    /// `{created, exists, table}`; `mode` is `call:lancedb`'s (`create_if_not_exists`).
    async fn create_table(&self, table: &str, schema: Option<&Map<String, Value>>, mode: &str) -> Result<Value, String>;
    async fn update(&self, table: &str, filter: &str, values: &Map<String, Value>) -> Result<(), String>;
    async fn delete(&self, table: &str, filter: &str) -> Result<(), String>;
    /// Make `rows` (fields → strings) the whole table, with `columns` — one step,
    /// creating the table if needed (copying across backends, [`switch`]).
    async fn replace_table(&self, table: &str, columns: &[String], rows: &[Value]) -> Result<(), String>;
}

/// The resource that names A3T's platform database, and its default path.
use crate::handlers::call_lancedb::{DEFAULT_PLATFORM_DB_PATH, PLATFORM_DB_RESOURCE};

/// Where a long-lived component (EE login) finds the platform database: looked up in
/// the registry on every use, so a [`switch`] takes effect without a restart.
#[derive(Clone)]
pub enum PlatformSource {
    Fixed(Arc<dyn PlatformDb>),
    Registry { storage: Arc<dyn flowr_storage::StorageBackend>, default_path: String },
}

impl PlatformSource {
    pub async fn get(&self) -> Arc<dyn PlatformDb> {
        match self {
            PlatformSource::Fixed(db) => Arc::clone(db),
            PlatformSource::Registry { storage, default_path } => {
                open_platform_db(storage.as_ref(), Some(PLATFORM_DB_RESOURCE), default_path).await
            }
        }
    }
}

/// Whether this build can use ADB platform tables (FlowR EE).
pub const ADB_PLATFORM_SUPPORTED: bool = cfg!(feature = "adb-platform");

/// The platform database now: `{backend, describe, adb_supported, config}`.
pub async fn status(storage: &dyn flowr_storage::StorageBackend) -> Value {
    let (ns, name) = PLATFORM_DB_RESOURCE.split_once('/').unwrap();
    let config = storage.resources().get(ns, name).await.ok().map(|r| r.config).unwrap_or(Value::Null);
    let db = open_platform_db(storage, Some(PLATFORM_DB_RESOURCE), DEFAULT_PLATFORM_DB_PATH).await;
    let backend = if db.describe().starts_with("adb:") { "adb" } else { "lancedb" };
    json!({ "backend": backend, "describe": db.describe(), "adb_supported": ADB_PLATFORM_SUPPORTED, "config": config })
}

/// Move A3T's platform database to `target` (`{"backend": "adb", "url", "flight_url"?}`
/// or `{"backend": "lancedb", "db_path"?}`): copy every table from the current
/// backend (replacing the target's copy, so nothing written since an earlier switch
/// is lost), then point `a3t/a3t-platform-db` at it. Writes made while the copy runs
/// may be missed: switch when the console is quiet. Returns `{from, to, tables, rows}`.
pub async fn switch(storage: &dyn flowr_storage::StorageBackend, target: &Value) -> Result<Value, String> {
    let (ns, name) = PLATFORM_DB_RESOURCE.split_once('/').unwrap();
    let mut config = storage.resources().get(ns, name).await.ok().map(|r| r.config).unwrap_or_else(|| json!({}));
    if !config.is_object() {
        config = json!({});
    }
    let mut next = config.clone();
    let m = next.as_object_mut().unwrap();
    match target.get("backend").and_then(|b| b.as_str()) {
        Some("adb") => {
            if !ADB_PLATFORM_SUPPORTED {
                return Err("ADB platform tables are a FlowR EE feature".into());
            }
            let url = target.get("url").and_then(|u| u.as_str()).or_else(|| config.get("url").and_then(|u| u.as_str()));
            let Some(url) = url else { return Err("switching to ADB needs its url".into()) };
            m.insert("backend".into(), json!("adb"));
            m.insert("url".into(), json!(url));
            match target.get("flight_url").and_then(|f| f.as_str()) {
                Some(f) => { m.insert("flight_url".into(), json!(f)); }
                None => { m.remove("flight_url"); }
            }
        }
        Some("lancedb") => {
            m.insert("backend".into(), json!("lancedb"));
            if let Some(p) = target.get("db_path").and_then(|p| p.as_str()) {
                m.insert("db_path".into(), json!(p));
            }
        }
        other => return Err(format!("unknown backend {other:?} (adb or lancedb)")),
    }
    let from = open_platform_db_config(&config, DEFAULT_PLATFORM_DB_PATH);
    let to = open_platform_db_config(&next, DEFAULT_PLATFORM_DB_PATH);
    if from.describe() == to.describe() {
        return Err(format!("the platform database is already {}", to.describe()));
    }
    // Copy first: a failure leaves the resource (and A3T) where it was.
    let mut tables = 0;
    let mut rows = 0;
    for table in from.list_tables().await? {
        let batches = from.query(&table, None, usize::MAX >> 1).await?;
        let columns: Vec<String> = match batches.first() {
            Some(b) => b.schema().fields().iter().map(|f| f.name().clone()).collect(),
            None => Vec::new(),
        };
        let data = crate::handlers::call_lancedb::batches_to_json(&batches);
        to.replace_table(&table, &columns, &data).await.map_err(|e| format!("copying {table}: {e}"))?;
        tables += 1;
        rows += data.len();
    }
    match storage.resources().get(ns, name).await {
        Ok(_) => storage.resources().update(ns, name, next).await.map(|_| ()).map_err(|e| e.to_string())?,
        Err(_) => {
            let r = flowr_types::Resource::new(name.to_string(), "lancedb".to_string(), next).with_namespace(ns.to_string());
            storage.resources().create(r).await.map(|_| ()).map_err(|e| e.to_string())?
        }
    }
    info!(from = %from.describe(), to = %to.describe(), tables, rows, "platform database switched");
    Ok(json!({ "from": from.describe(), "to": to.describe(), "tables": tables, "rows": rows }))
}

/// [`open_platform_db`] from a resource config already in hand.
pub fn open_platform_db_config(config: &Value, default_path: &str) -> Arc<dyn PlatformDb> {
    if config.get("backend").and_then(|b| b.as_str()) == Some("adb") {
        #[cfg(feature = "adb-platform")]
        if let Some(url) = config.get("url").or_else(|| config.get("endpoint")).and_then(|u| u.as_str()) {
            let ep = crate::handlers::call_adb::adb_endpoint(url, config.get("flight_url").and_then(|f| f.as_str()));
            return Arc::new(AdbPlatform { ep });
        }
    }
    let path = config.get("db_path").and_then(|p| p.as_str()).unwrap_or(default_path).to_string();
    Arc::new(LanceDbPlatform { path })
}

/// The platform database a resource names (`namespace/name`, or `name` in `a3t`),
/// or the embedded LanceDB at `default_path` when there is none or it isn't ADB.
pub async fn open_platform_db(
    storage: &dyn flowr_storage::StorageBackend,
    resource: Option<&str>,
    default_path: &str,
) -> Arc<dyn PlatformDb> {
    let config = match resource {
        Some(rn) => {
            let (ns, name) = rn.split_once('/').unwrap_or(("a3t", rn));
            storage.resources().get(ns, name).await.ok().map(|r| r.config)
        }
        None => None,
    };
    let config = config.unwrap_or(Value::Null);
    if config.get("backend").and_then(|b| b.as_str()) == Some("adb") {
        if !ADB_PLATFORM_SUPPORTED {
            warn!(resource = ?resource, "platform db: ADB platform tables are a FlowR EE feature; this build uses the embedded LanceDB");
        } else if config.get("url").or_else(|| config.get("endpoint")).is_none() {
            warn!(resource = ?resource, "platform db: backend adb without config.url; using the embedded LanceDB");
        }
    }
    open_platform_db_config(&config, default_path)
}

/// `call:lancedb`'s SQL literal for an update value: strings quoted ('' escapes),
/// null as NULL, anything else its JSON text.
pub fn sql_literal(v: &Value) -> String {
    match v {
        Value::String(s) => format!("'{}'", s.replace('\'', "''")),
        Value::Null => "NULL".to_string(),
        other => other.to_string(),
    }
}

/// All-UTF-8 schema from the first record's fields (`call:lancedb`'s `infer_schema`).
pub fn infer_schema(record: &Value) -> arrow_schema::Schema {
    use arrow_schema::{DataType, Field};
    let fields: Vec<Field> = match record.as_object() {
        Some(obj) => obj.keys().map(|k| Field::new(k, DataType::Utf8, true)).collect(),
        None => vec![Field::new("value", DataType::Utf8, true)],
    };
    arrow_schema::Schema::new(fields)
}

/// Records as a batch of `schema` (non-strings become their JSON text).
pub fn records_to_batch(records: &[Value], schema: &arrow_schema::Schema) -> RecordBatch {
    use arrow_array::StringArray;
    let columns: Vec<Arc<dyn arrow_array::Array>> = schema
        .fields()
        .iter()
        .map(|field| {
            let values: Vec<Option<String>> = records
                .iter()
                .map(|r| r.get(field.name()).map(|v| match v {
                    Value::String(s) => s.clone(),
                    other => other.to_string(),
                }))
                .collect();
            Arc::new(StringArray::from(values)) as Arc<dyn arrow_array::Array>
        })
        .collect();
    RecordBatch::try_new(Arc::new(schema.clone()), columns).expect("columns match the schema")
}

// ---------------------------------------------------------------- embedded LanceDB

/// The embedded LanceDB A3T has always used.
pub struct LanceDbPlatform {
    path: String,
}

impl LanceDbPlatform {
    pub fn new(path: impl Into<String>) -> Self {
        Self { path: path.into() }
    }

    async fn db(&self) -> Result<lancedb::Connection, String> {
        lancedb::connect(&self.path).execute().await.map_err(|e| format!("LanceDB connect error: {e}"))
    }
}

#[async_trait]
impl PlatformDb for LanceDbPlatform {
    fn describe(&self) -> String {
        format!("lancedb:{}", self.path)
    }

    async fn list_tables(&self) -> Result<Vec<String>, String> {
        self.db().await?.table_names().execute().await.map_err(|e| format!("list_tables error: {e}"))
    }

    async fn query(&self, table: &str, filter: Option<&str>, limit: usize) -> Result<Vec<RecordBatch>, String> {
        use futures::TryStreamExt;
        use lancedb::query::{ExecutableQuery, QueryBase};
        let t = self.db().await?.open_table(table).execute().await.map_err(|e| format!("open table '{table}' error: {e}"))?;
        let mut q = t.query();
        if let Some(f) = filter {
            q = q.only_if(flowr_storage::filter::compat(f));
        }
        q.limit(limit)
            .execute()
            .await
            .map_err(|e| format!("query error: {e}"))?
            .try_collect()
            .await
            .map_err(|e| format!("collect error: {e}"))
    }

    async fn insert(&self, table: &str, records: &[Value]) -> Result<usize, String> {
        if records.is_empty() {
            return Ok(0);
        }
        let db = self.db().await?;
        let schema = infer_schema(&records[0]);
        let batch = records_to_batch(records, &schema);
        let batches = arrow_array::RecordBatchIterator::new(vec![Ok(batch)], Arc::new(schema));
        match db.open_table(table).execute().await {
            Ok(t) => t.add(Box::new(batches) as Box<dyn arrow_array::RecordBatchReader + Send>).execute().await.map(|_| ()).map_err(|e| format!("insert error: {e}"))?,
            // Create the table from the first record's fields.
            Err(_) => db.create_table(table, Box::new(batches) as Box<dyn arrow_array::RecordBatchReader + Send>).execute().await.map(|_| ()).map_err(|e| format!("create table error: {e}"))?,
        }
        Ok(records.len())
    }

    async fn create_table(&self, table: &str, schema: Option<&Map<String, Value>>, mode: &str) -> Result<Value, String> {
        let db = self.db().await?;
        let tables = db.table_names().execute().await.map_err(|e| format!("list tables error: {e}"))?;
        if tables.contains(&table.to_string()) && mode == "create_if_not_exists" {
            debug!(table = %table, "table already exists, skipping");
            return Ok(json!({"created": false, "exists": true}));
        }
        let schema = match schema {
            Some(s) => arrow_schema::Schema::new(s.keys().map(|k| arrow_schema::Field::new(k, arrow_schema::DataType::Utf8, true)).collect::<Vec<_>>()),
            None => arrow_schema::Schema::new(vec![arrow_schema::Field::new("id", arrow_schema::DataType::Utf8, false)]),
        };
        let batch = RecordBatch::new_empty(Arc::new(schema.clone()));
        let batches = arrow_array::RecordBatchIterator::new(vec![Ok(batch)], Arc::new(schema));
        db.create_table(table, Box::new(batches) as Box<dyn arrow_array::RecordBatchReader + Send>).execute().await.map_err(|e| format!("create_table error: {e}"))?;
        info!(table = %table, "created LanceDB table");
        Ok(json!({"created": true, "table": table}))
    }

    async fn update(&self, table: &str, filter: &str, values: &Map<String, Value>) -> Result<(), String> {
        let t = self.db().await?.open_table(table).execute().await.map_err(|e| format!("open table error: {e}"))?;
        let columns: Vec<(String, String)> = values.iter().map(|(k, v)| (k.clone(), sql_literal(v))).collect();
        let mut op = t.update().only_if(flowr_storage::filter::compat(filter));
        for (col, val) in &columns {
            op = op.column(col, val.as_str());
        }
        op.execute().await.map(|_| ()).map_err(|e| format!("update error: {e}"))
    }

    async fn delete(&self, table: &str, filter: &str) -> Result<(), String> {
        let t = self.db().await?.open_table(table).execute().await.map_err(|e| format!("open table error: {e}"))?;
        t.delete(&flowr_storage::filter::compat(filter)).await.map(|_| ()).map_err(|e| format!("delete error: {e}"))
    }

    async fn replace_table(&self, table: &str, columns: &[String], rows: &[Value]) -> Result<(), String> {
        let cols: Vec<String> = if columns.is_empty() { vec!["id".into()] } else { columns.to_vec() };
        let schema = arrow_schema::Schema::new(cols.iter().map(|c| arrow_schema::Field::new(c, arrow_schema::DataType::Utf8, true)).collect::<Vec<_>>());
        let batch = if rows.is_empty() { RecordBatch::new_empty(Arc::new(schema.clone())) } else { records_to_batch(rows, &schema) };
        let batches = arrow_array::RecordBatchIterator::new(vec![Ok(batch)], Arc::new(schema));
        self.db()
            .await?
            .create_table(table, Box::new(batches) as Box<dyn arrow_array::RecordBatchReader + Send>)
            .mode(lancedb::database::CreateTableMode::Overwrite)
            .execute()
            .await
            .map(|_| ())
            .map_err(|e| format!("replace {table}: {e}"))
    }
}

// ---------------------------------------------------------------- ADB (EE)

/// ADB platform tables over Arrow Flight (FlowR EE).
#[cfg(feature = "adb-platform")]
pub struct AdbPlatform {
    ep: crate::handlers::call_adb::AdbEndpoint,
}

#[cfg(feature = "adb-platform")]
#[async_trait]
impl PlatformDb for AdbPlatform {
    fn describe(&self) -> String {
        format!("adb:{}", self.ep.flight)
    }

    async fn list_tables(&self) -> Result<Vec<String>, String> {
        let v = crate::handlers::call_adb::flight_action(&self.ep, "list_tables", &json!({})).await?;
        serde_json::from_value(v).map_err(|e| format!("list_tables: {e}"))
    }

    async fn query(&self, table: &str, filter: Option<&str>, limit: usize) -> Result<Vec<RecordBatch>, String> {
        let filter = filter.map(flowr_storage::filter::compat);
        crate::handlers::call_adb::flight_get(&self.ep, table, filter.as_deref(), limit).await
    }

    async fn insert(&self, table: &str, records: &[Value]) -> Result<usize, String> {
        if records.is_empty() {
            return Ok(0);
        }
        let schema = infer_schema(&records[0]);
        let batch = records_to_batch(records, &schema);
        let r = crate::handlers::call_adb::flight_put(&self.ep, table, batch, false).await?;
        Ok(r.get("inserted").and_then(|n| n.as_u64()).unwrap_or(records.len() as u64) as usize)
    }

    async fn create_table(&self, table: &str, schema: Option<&Map<String, Value>>, _mode: &str) -> Result<Value, String> {
        let body = json!({ "table": table, "schema": schema });
        let r = crate::handlers::call_adb::flight_action(&self.ep, "create_table", &body).await?;
        // Same answer as the LanceDB path.
        Ok(match r.get("exists").and_then(|e| e.as_bool()) {
            Some(true) => json!({"created": false, "exists": true}),
            _ => json!({"created": true, "table": table}),
        })
    }

    async fn update(&self, table: &str, filter: &str, values: &Map<String, Value>) -> Result<(), String> {
        crate::handlers::call_adb::flight_action(&self.ep, "update", &json!({ "table": table, "filter": flowr_storage::filter::compat(filter), "values": values }))
            .await
            .map(|_| ())
    }

    async fn delete(&self, table: &str, filter: &str) -> Result<(), String> {
        crate::handlers::call_adb::flight_action(&self.ep, "delete", &json!({ "table": table, "filter": flowr_storage::filter::compat(filter) }))
            .await
            .map(|_| ())
    }

    async fn replace_table(&self, table: &str, columns: &[String], rows: &[Value]) -> Result<(), String> {
        if rows.is_empty() {
            return crate::handlers::call_adb::flight_action(&self.ep, "replace_table", &json!({ "table": table, "columns": columns }))
                .await
                .map(|_| ());
        }
        let cols: Vec<String> = if columns.is_empty() { vec!["id".into()] } else { columns.to_vec() };
        let schema = arrow_schema::Schema::new(cols.iter().map(|c| arrow_schema::Field::new(c, arrow_schema::DataType::Utf8, true)).collect::<Vec<_>>());
        let batch = records_to_batch(rows, &schema);
        crate::handlers::call_adb::flight_put(&self.ep, table, batch, true).await.map(|_| ())
    }
}
