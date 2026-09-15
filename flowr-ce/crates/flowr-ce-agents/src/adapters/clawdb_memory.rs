//! ClawDB → ZeroClaw Memory adapter
//!
//! Implements ZeroClaw's `Memory` trait backed by ClawDB HTTP API,
//! translating store/recall/forget operations into AQL queries.

use async_trait::async_trait;
use serde_json::{json, Value};
use zeroclaw::memory::{Memory, MemoryEntry, MemoryCategory};

/// ClawDB-backed memory for ZeroClaw agents
pub struct ClawDbMemory {
    url: String,
    client: reqwest::Client,
    namespace: String,
}

impl ClawDbMemory {
    pub fn new(url: String, client: reqwest::Client, namespace: String) -> Self {
        Self { url, client, namespace }
    }

    async fn execute_aql(&self, query: &str) -> anyhow::Result<Value> {
        let resp = self.client
            .post(&format!("{}/query", self.url))
            .header("Content-Type", "application/json")
            .json(&json!({ "query": query }))
            .send()
            .await?;

        let body: Value = resp.json().await?;

        let success = body.get("success").and_then(|s| s.as_bool()).unwrap_or(false);
        if !success {
            let error = body.get("error").and_then(|e| e.as_str()).unwrap_or("unknown error");
            anyhow::bail!("ClawDB AQL error: {}", error);
        }

        Ok(body.get("data").cloned().unwrap_or(Value::Null))
    }
}

fn category_to_memory_type(category: &MemoryCategory) -> &'static str {
    match category {
        MemoryCategory::Core => "SEMANTIC",
        MemoryCategory::Daily => "EPISODIC",
        MemoryCategory::Conversation => "WORKING",
        MemoryCategory::Custom(name) => {
            if name.eq_ignore_ascii_case("procedural") {
                "PROCEDURAL"
            } else {
                "WORKING"
            }
        }
    }
}

#[async_trait]
impl Memory for ClawDbMemory {
    fn name(&self) -> &str {
        "clawdb"
    }

    async fn store(
        &self,
        key: &str,
        content: &str,
        category: MemoryCategory,
        _session_id: Option<&str>,
    ) -> anyhow::Result<()> {
        let memory_type = category_to_memory_type(&category);
        let escaped_key = key.replace('"', r#"\""#);
        let escaped_content = content.replace('"', r#"\""#);

        let query = format!(
            r#"STORE INTO {} (key = "{}", content = "{}") SCOPE private NAMESPACE "{}""#,
            memory_type, escaped_key, escaped_content, self.namespace
        );

        self.execute_aql(&query).await?;
        Ok(())
    }

    async fn recall(
        &self,
        query: &str,
        limit: usize,
        _session_id: Option<&str>,
    ) -> anyhow::Result<Vec<MemoryEntry>> {
        let escaped = query.replace('"', r#"\""#);
        let aql = format!(
            r#"RECALL FROM WORKING WHERE content = "{}" LIMIT {}"#,
            escaped, limit
        );

        let data = self.execute_aql(&aql).await?;
        parse_recall_results(&data)
    }

    async fn get(&self, key: &str) -> anyhow::Result<Option<MemoryEntry>> {
        let escaped = key.replace('"', r#"\""#);
        let aql = format!(
            r#"RECALL FROM WORKING WHERE key = "{}" LIMIT 1"#,
            escaped
        );

        let data = self.execute_aql(&aql).await?;
        let entries = parse_recall_results(&data)?;
        Ok(entries.into_iter().next())
    }

    async fn list(
        &self,
        category: Option<&MemoryCategory>,
        _session_id: Option<&str>,
    ) -> anyhow::Result<Vec<MemoryEntry>> {
        let memory_type = category.map(category_to_memory_type).unwrap_or("WORKING");
        let aql = format!("RECALL FROM {} LIMIT 50", memory_type);
        let data = self.execute_aql(&aql).await?;
        parse_recall_results(&data)
    }

    async fn forget(&self, key: &str) -> anyhow::Result<bool> {
        let escaped = key.replace('"', r#"\""#);
        let aql = format!(
            r#"FORGET FROM WORKING WHERE key = "{}""#,
            escaped
        );

        self.execute_aql(&aql).await?;
        Ok(true)
    }

    async fn count(&self) -> anyhow::Result<usize> {
        let data = self.client
            .get(&format!("{}/stats", self.url))
            .send()
            .await?
            .json::<Value>()
            .await?;

        let total = data.get("data")
            .map(|d| {
                d.as_object()
                    .map(|obj| obj.values().filter_map(|v| v.as_u64()).sum::<u64>())
                    .unwrap_or(0)
            })
            .unwrap_or(0);

        Ok(total as usize)
    }

    async fn health_check(&self) -> bool {
        let resp = self.client
            .get(&format!("{}/health", self.url))
            .send()
            .await;

        matches!(resp, Ok(r) if r.status().is_success())
    }
}

fn parse_recall_results(data: &Value) -> anyhow::Result<Vec<MemoryEntry>> {
    let mut entries = Vec::new();

    // Navigate nested response: data.data.records
    let records = data
        .get("data").and_then(|d| d.get("records")).and_then(|r| r.as_array())
        .or_else(|| data.get("records").and_then(|r| r.as_array()));

    if let Some(records) = records {
        for record in records {
            let id = record.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let record_data = record.get("data").cloned().unwrap_or(Value::Null);

            let key = record_data.get("key")
                .and_then(|v| v.as_str())
                .unwrap_or(&id)
                .to_string();

            let content = record_data.get("content")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
                .unwrap_or_else(|| serde_json::to_string(&record_data).unwrap_or_default());

            let memory_type = record.get("memory_type")
                .and_then(|v| v.as_str())
                .unwrap_or("WORKING");

            let category = match memory_type {
                "SEMANTIC" => MemoryCategory::Core,
                "EPISODIC" => MemoryCategory::Daily,
                "WORKING" => MemoryCategory::Conversation,
                "PROCEDURAL" => MemoryCategory::Custom("procedural".to_string()),
                other => MemoryCategory::Custom(other.to_lowercase()),
            };

            let timestamp = record.get("metadata")
                .and_then(|m| m.get("created_at"))
                .and_then(|t| t.as_str())
                .unwrap_or("")
                .to_string();

            let session_id = record_data.get("session_id")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());

            entries.push(MemoryEntry {
                id,
                key,
                content,
                category,
                timestamp,
                session_id,
                score: None,
            });
        }
    }

    Ok(entries)
}
