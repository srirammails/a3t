//! Referential integrity for deletes — the dependency tree.
//!
//! ```text
//!   Template ◄── Agent ◄── Canvas (agentRef) ◄── Deployment ──► Runtime
//!   Flow     ◄──────────── Canvas (flowRef)  ◄── Deployment ──► Runtime
//! ```
//!
//! A parent cannot be deleted while something depends on it: a template while
//! agents are built from it, an agent or flow while it is deployed, a canvas
//! while a deployment uses it or an existing agent is bound to it. The refusal
//! names the dependents so the user knows what to remove first.
//!
//! A runtime is the exception, by design: deleting it is terminating a container,
//! and what was deployed on it is rescheduled (see `ops::terminate_runtime`), so
//! it is never refused — it is previewed, warned about, then confirmed.
//!
//! Every delete route goes through here — the `call:lancedb` delete the CRUD
//! flows use (templates, agents), the registry REST delete (canvases, runtimes,
//! flows) and the MCP `delete_resource` / `delete_flow` tools — so the rules
//! cannot be bypassed by picking a different door.

use std::sync::Arc;

use serde_json::Value;

use flowr_storage::StorageBackend;

/// The rows a filter selects, from the platform database (embedded or ADB).
async fn ids_matching(db: &dyn crate::platform_db::PlatformDb, table: &str, filter: &str) -> Result<Vec<Value>, String> {
    let batches = db.query(table, Some(filter), 1000).await?;
    Ok(crate::handlers::call_lancedb::batches_to_json(&batches))
}

fn quote(s: &str) -> String {
    crate::handlers::call_lancedb::sql_quote(s)
}

fn name_of(row: &Value) -> String {
    row.get("name")
        .or_else(|| row.get("id"))
        .and_then(|v| v.as_str())
        .unwrap_or("?")
        .to_string()
}

fn list(names: &[String]) -> String {
    const MAX: usize = 8;
    if names.len() <= MAX {
        names.join(", ")
    } else {
        format!("{} and {} more", names[..MAX].join(", "), names.len() - MAX)
    }
}

/// Guard a `call:lancedb` delete. Only the platform tables in the tree are
/// checked; every other table deletes as before.
pub async fn check_table_delete(db: &dyn crate::platform_db::PlatformDb, table: &str, filter: &str) -> Result<(), String> {
    match table {
        // A template with agents built from it.
        "template_registry" => {
            let rows = ids_matching(db, table, filter).await?;
            for t in rows {
                let Some(id) = t.get("id").and_then(|v| v.as_str()) else { continue };
                let agents = ids_matching(db, "agent_configs", &format!("template_id = {}", quote(id)))
                    .await
                    .unwrap_or_default();
                if !agents.is_empty() {
                    let names: Vec<String> = agents.iter().map(name_of).collect();
                    return Err(format!(
                        "Template '{}' is used by {} agent(s): {}. Delete those agents first.",
                        name_of(&t),
                        names.len(),
                        list(&names)
                    ));
                }
            }
            Ok(())
        }
        // A deployed agent.
        "agent_configs" => {
            let rows = ids_matching(db, table, filter).await?;
            for a in rows {
                let Some(id) = a.get("id").and_then(|v| v.as_str()) else { continue };
                if let Some(d) = crate::ops::deployment_of_agent(id) {
                    return Err(format!(
                        "Agent '{}' is deployed on runtime '{}'. Undeploy it first (Runtimes → {}).",
                        name_of(&a),
                        d.runtime,
                        d.target
                    ));
                }
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

/// Does an agent with this id exist in the platform registry?
async fn agent_exists(storage: &dyn StorageBackend, id: &str) -> bool {
    crate::handlers::call_lancedb::query_platform_table(storage, "agent_configs", Some(format!("id = {}", quote(id))), 1)
        .await
        .map(|r| !r.is_empty())
        .unwrap_or(false)
}

/// Guard a registry resource delete. Runtimes are not handled here — their
/// delete is a reschedule, routed to `ops::terminate_runtime` by the caller.
pub async fn check_resource_delete(storage: &dyn StorageBackend, ns: &str, name: &str) -> Result<(), String> {
    let Ok(r) = storage.resources().get(ns, name).await else { return Ok(()) };
    if r.type_ref != "canvas" {
        return Ok(());
    }
    let canvas = format!("{ns}/{name}");
    let users = crate::ops::deployments_using_canvas(&canvas);
    if !users.is_empty() {
        let names: Vec<String> = users.iter().map(|d| format!("{} (on {})", d.target, d.runtime)).collect();
        return Err(format!(
            "Canvas '{canvas}' is used by {} deployment(s): {}. Undeploy them first.",
            names.len(),
            list(&names)
        ));
    }
    let agent_ref = r.config.get("agentRef").and_then(|v| v.as_str()).unwrap_or("");
    if !agent_ref.is_empty() {
        let id = agent_ref.rsplit('/').next().unwrap_or(agent_ref);
        if agent_exists(storage, id).await {
            return Err(format!(
                "Canvas '{canvas}' is bound to agent '{id}'. Delete the agent first."
            ));
        }
    }
    Ok(())
}

/// Guard a workflow delete.
pub fn check_flow_delete(ns: &str, name: &str) -> Result<(), String> {
    let flow = format!("{ns}/{name}");
    let users = crate::ops::deployments_of_flow(&flow);
    if users.is_empty() {
        return Ok(());
    }
    let names: Vec<String> = users.iter().map(|d| format!("{} (on {})", d.target, d.runtime)).collect();
    Err(format!(
        "Flow '{flow}' is deployed: {}. Undeploy it first.",
        list(&names)
    ))
}

/// The same rules, offered to the shared MCP tools (`delete_resource`,
/// `delete_flow`), which cannot see the deployment ledger themselves.
pub struct Guard {
    pub storage: Arc<dyn StorageBackend>,
}

#[async_trait::async_trait]
impl flowr_mcp_core::DeleteGuard for Guard {
    async fn check(&self, kind: &str, ns: &str, name: &str) -> Result<(), String> {
        match kind {
            "flow" => check_flow_delete(ns, name),
            "resource" => {
                if let Ok(r) = self.storage.resources().get(ns, name).await {
                    if r.type_ref == "runtime" {
                        let plan = crate::ops::terminate_runtime(self.storage.as_ref(), name, "", None).await?;
                        let deployed = plan["impact"]["plan"].as_array().map(|a| a.len()).unwrap_or(0);
                        if deployed > 0 {
                            return Err(format!(
                                "Runtime '{name}' has {deployed} deployment(s). Use terminate_runtime: it previews \
                                 the reschedule (shared ones move, isolated ones get a fresh runtime) and then \
                                 confirms."
                            ));
                        }
                    }
                }
                check_resource_delete(self.storage.as_ref(), ns, name).await
            }
            _ => Ok(()),
        }
    }
}
