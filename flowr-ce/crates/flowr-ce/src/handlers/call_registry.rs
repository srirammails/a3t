//! call:registry — registry operations a flow can't do through other tasks.
//!
//! `visible` keeps the items the caller may see. `usage` counts what a namespace holds. `move_namespace` moves every flow, resource and trigger
//! from one namespace to another and rewrites the references inside them
//! (`<from>/name` → `<to>/name`, and `document.namespace`). The A3T project
//! flows use it to move a project from a user to an org; *who* may move what is
//! decided there, from `$context._principal` — this handler is only the
//! mechanism. It runs in-process because copying a resource through the API would
//! return its secrets masked.
//!
//! ```yaml
//! call: registry
//! with:
//!   operation: move_namespace
//!   from: sri-orders
//!   to: acme-orders
//!   dry_run: true      # report what would move and who else refers to it
//! ```
//!
//! The target must be empty. Execution history stays under the old name.

use async_trait::async_trait;
use serde_json::{json, Value};
use std::sync::Arc;
use tracing::info;

use flowr_storage::StorageBackend;
use flowr_types::{Resource, WorkflowRecord};
use swf_parser::{NamedTask, Task};
use swf_runtime::context::ExecutionContext;
use swf_runtime::error::{Result, RuntimeError};
use swf_runtime::traits::{TaskHandler, TaskResult};

pub struct RegistryHandler;

fn storage(ctx: &ExecutionContext) -> Result<Arc<dyn StorageBackend>> {
    ctx.storage
        .as_ref()
        .and_then(|s| s.downcast_ref::<Arc<dyn StorageBackend>>().cloned())
        .ok_or_else(|| RuntimeError::Internal("Storage not available".to_string()))
}

fn is_slug_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.'
}

/// `text` with every `<from>/` reference that starts a name (not the tail of a
/// longer namespace like `my-<from>/`) and every `namespace: <from>` YAML value
/// pointed at `to`. Returns how many it changed.
pub fn rewrite_refs(text: &str, from: &str, to: &str) -> (String, usize) {
    let needle = format!("{from}/");
    let mut out = String::with_capacity(text.len());
    let mut count = 0;
    let mut rest = text;
    while let Some(i) = rest.find(&needle) {
        // The character before the match, in the original text.
        let prev = if i > 0 { rest[..i].chars().last() } else { out.chars().last() };
        out.push_str(&rest[..i]);
        if prev.is_some_and(is_slug_char) {
            out.push_str(&needle);
        } else {
            out.push_str(to);
            out.push('/');
            count += 1;
        }
        rest = &rest[i + needle.len()..];
    }
    out.push_str(rest);

    let mut lines = Vec::new();
    for line in out.lines() {
        let trimmed = line.trim_start();
        let value = trimmed.strip_prefix("namespace:").map(|v| v.trim().trim_matches(|c| c == '"' || c == '\''));
        if value == Some(from) {
            let indent = &line[..line.len() - trimmed.len()];
            lines.push(format!("{indent}namespace: {to}"));
            count += 1;
        } else {
            lines.push(line.to_string());
        }
    }
    let mut joined = lines.join("\n");
    if out.ends_with('\n') {
        joined.push('\n');
    }
    (joined, count)
}

fn rewrite_json(v: &Value, from: &str, to: &str) -> Value {
    match v {
        Value::String(s) if s == from => Value::String(to.to_string()),
        Value::String(s) => Value::String(rewrite_refs(s, from, to).0),
        Value::Array(a) => Value::Array(a.iter().map(|x| rewrite_json(x, from, to)).collect()),
        Value::Object(o) => Value::Object(o.iter().map(|(k, x)| (k.clone(), rewrite_json(x, from, to))).collect()),
        other => other.clone(),
    }
}

fn mentions(text: &str, from: &str) -> bool {
    rewrite_refs(text, from, "\u{0}").1 > 0
}

async fn move_namespace(storage: &dyn StorageBackend, from: &str, to: &str, dry_run: bool) -> std::result::Result<Value, String> {
    if from.is_empty() || to.is_empty() || from == to {
        return Err("`from` and `to` must be two different namespaces".into());
    }
    let flows = storage.workflows().list(from).await.map_err(|e| e.to_string())?;
    let resources = storage.resources().list(from).await.map_err(|e| e.to_string())?;
    let triggers = storage.triggers().list(Some(from)).await.map_err(|e| e.to_string())?;

    if !storage.workflows().list(to).await.map_err(|e| e.to_string())?.is_empty()
        || !storage.resources().list(to).await.map_err(|e| e.to_string())?.is_empty()
        || !storage.triggers().list(Some(to)).await.map_err(|e| e.to_string())?.is_empty()
    {
        return Err(format!("namespace '{to}' is not empty"));
    }
    if let Some(d) = resources.iter().find(|r| r.type_ref == "deployment") {
        return Err(format!("'{from}' has a live deployment ({}); undeploy it first", d.name));
    }

    // Who outside the project refers to it — their references would break.
    let mut external = Vec::new();
    for ns in storage.workflows().namespaces().await.unwrap_or_default() {
        if ns == from { continue; }
        for f in storage.workflows().list(&ns).await.unwrap_or_default() {
            if let Ok(rec) = storage.workflows().get(&ns, &f.name).await {
                if mentions(&rec.raw_yaml, from) { external.push(format!("flow {ns}/{}", f.name)); }
            }
        }
    }
    for ns in storage.resources().namespaces().await.unwrap_or_default() {
        if ns == from { continue; }
        for r in storage.resources().list(&ns).await.unwrap_or_default() {
            if mentions(&r.config.to_string(), from) { external.push(format!("resource {ns}/{}", r.name)); }
        }
    }

    let report = json!({
        "from": from, "to": to, "dry_run": dry_run,
        "flows": flows.iter().map(|f| f.name.clone()).collect::<Vec<_>>(),
        "resources": resources.iter().map(|r| r.name.clone()).collect::<Vec<_>>(),
        "triggers": triggers.iter().filter_map(|t| t.get("name").and_then(|v| v.as_str()).map(String::from)).collect::<Vec<_>>(),
        "external_references": external,
    });
    if dry_run {
        return Ok(report);
    }

    // Copy everything first, then remove the originals: a failure part-way
    // leaves a duplicate to clean up, never a loss.
    let mut records: Vec<WorkflowRecord> = Vec::new();
    for f in &flows {
        let rec = storage.workflows().get(from, &f.name).await.map_err(|e| e.to_string())?;
        let (yaml, _) = rewrite_refs(&rec.raw_yaml, from, to);
        let mut moved = WorkflowRecord::from_yaml_owned(&yaml, rec.metadata.owner.clone()).map_err(|e| format!("flow {}: {e}", f.name))?;
        moved.metadata.enabled = rec.metadata.enabled;
        records.push(moved);
    }
    let mut copies: Vec<Resource> = Vec::new();
    for r in &resources {
        let full = storage.resources().get(from, &r.name).await.map_err(|e| e.to_string())?;
        let mut copy = Resource::new(full.name.clone(), full.type_ref.clone(), rewrite_json(&full.config, from, to)).with_namespace(to);
        copy.tags = full.tags.clone();
        copy.description = full.description.clone();
        copies.push(copy);
    }
    for rec in records { storage.workflows().register(rec).await.map_err(|e| e.to_string())?; }
    for copy in copies { storage.resources().create(copy).await.map_err(|e| e.to_string())?; }
    for t in &triggers {
        let id = t.get("id").and_then(|v| v.as_str()).unwrap_or_default().to_string();
        let moved = rewrite_json(t, from, to);
        storage.triggers().delete(&id).await.map_err(|e| e.to_string())?;
        if let Err(e) = storage.triggers().create(moved).await {
            let _ = storage.triggers().create(t.clone()).await; // put it back
            return Err(format!("trigger {id}: {e}"));
        }
    }
    for f in &flows { storage.workflows().delete(from, &f.name).await.map_err(|e| e.to_string())?; }
    for r in &resources { storage.resources().delete(from, &r.name).await.map_err(|e| e.to_string())?; }

    info!(%from, %to, flows = flows.len(), resources = resources.len(), triggers = triggers.len(), "namespace moved");
    Ok(report)
}

#[async_trait]
impl TaskHandler for RegistryHandler {
    fn task_type(&self) -> &str {
        "call:registry"
    }

    async fn execute(&self, task: &NamedTask, _input: Value, ctx: &mut ExecutionContext) -> Result<TaskResult> {
        let with = match &task.task {
            Task::Call(c) => c.with.clone().unwrap_or(json!({})),
            _ => return Err(RuntimeError::task_failed(&task.name, "expected call task")),
        };
        let s = |k: &str| with.get(k).and_then(|v| v.as_str()).unwrap_or_default().to_string();
        match s("operation").as_str() {
            "move_namespace" => {
                let dry_run = with.get("dry_run").and_then(|v| v.as_bool()).unwrap_or(true);
                let store = storage(ctx)?;
                move_namespace(store.as_ref(), &s("from"), &s("to"), dry_run)
                    .await
                    .map(|v| TaskResult::new(v))
                    .map_err(|e| RuntimeError::task_failed(&task.name, e))
            }
            // The rows the caller may see: `items` (each with `namespace`, and
            // `name` / `type` / `created_by`), `kind` = agent | flow | resource |
            // trigger. Flows that list platform tables (a3t-agents) filter with it.
            "visible" => {
                use flowr_policy::namespace::ItemKind;
                let kind = match s("kind").as_str() {
                    "flow" => ItemKind::Flow,
                    "resource" => ItemKind::Resource,
                    "trigger" => ItemKind::Trigger,
                    _ => ItemKind::Agent,
                };
                let items = with.get("items").and_then(|v| v.as_array()).cloned().unwrap_or_default();
                Ok(TaskResult::new(Value::Array(crate::access::filter_items(&ctx.principal, kind, items).await)))
            }
            // How much a namespace holds; the delete-project flow refuses a non-empty one.
            "usage" => {
                let store = storage(ctx)?;
                let ns = s("namespace");
                let flows = store.workflows().list(&ns).await.map(|v| v.len()).unwrap_or(0);
                let resources = store.resources().list(&ns).await.map(|v| v.len()).unwrap_or(0);
                let triggers = store.triggers().list(Some(&ns)).await.map(|v| v.len()).unwrap_or(0);
                Ok(TaskResult::new(json!({ "namespace": ns, "flows": flows, "resources": resources, "triggers": triggers,
                                           "empty": flows + resources + triggers == 0 })))
            }
            other => Err(RuntimeError::task_failed(&task.name, format!("unsupported registry operation: {other}"))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::rewrite_refs;

    #[test]
    fn references_to_the_namespace_move_and_others_stay() {
        let yaml = "document:\n  namespace: sri-orders\n  name: x\ndo:\n- a:\n    with:\n      _resource: sri-orders/db\n      other: my-sri-orders/db\n      llm: a3t/qwen\n";
        let (out, n) = rewrite_refs(yaml, "sri-orders", "acme-orders");
        assert_eq!(n, 2);
        assert!(out.contains("  namespace: acme-orders\n"));
        assert!(out.contains("_resource: acme-orders/db"));
        assert!(out.contains("other: my-sri-orders/db"), "{out}");
        assert!(out.contains("llm: a3t/qwen"));
    }

    #[test]
    fn a_reference_at_the_start_of_a_string_moves() {
        assert_eq!(rewrite_refs("sri-orders/db", "sri-orders", "acme-orders"), ("acme-orders/db".to_string(), 1));
    }
}
