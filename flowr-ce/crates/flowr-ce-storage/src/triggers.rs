//! Trigger store implementation backed by flowr-ce-agents TriggerManager
//!
//! Delegates CRUD to the global trigger_manager() singleton and persists
//! changes to the state store under the `_triggers` key.

use async_trait::async_trait;
use serde_json::{json, Value};
use tracing::{info, warn};

use flowr_storage::{StateStore, TriggerStore};
use flowr_types::FlowResult;

use crate::state::LanceStateStore;

/// TriggerStore backed by flowr-ce-agents TriggerManager + state persistence
pub struct LanceTriggerStore {
    state: LanceStateStore,
}

impl LanceTriggerStore {
    pub fn new(state: LanceStateStore) -> Self {
        Self { state }
    }

    /// Persist current trigger state to the state store
    async fn persist(&self) {
        let data = flowr_ce_agents::trigger_manager().export_all().await;
        if let Err(e) = self.state.set("_triggers", data, None).await {
            warn!(error = %e, "Failed to persist triggers");
        }
    }
}

#[async_trait]
impl TriggerStore for LanceTriggerStore {
    async fn list(&self, namespace: Option<&str>) -> FlowResult<Vec<Value>> {
        let triggers = flowr_ce_agents::trigger_manager().list().await;
        let filtered: Vec<Value> = triggers
            .into_iter()
            .filter(|t| namespace.map_or(true, |ns| t.namespace == ns))
            .map(|t| serde_json::to_value(t).unwrap_or(json!(null)))
            .collect();
        Ok(filtered)
    }

    async fn get(&self, id: &str) -> FlowResult<Option<Value>> {
        Ok(flowr_ce_agents::trigger_manager()
            .get(id)
            .await
            .map(|t| serde_json::to_value(t).unwrap_or(json!(null))))
    }

    async fn create(&self, trigger: Value) -> FlowResult<String> {
        let trigger: flowr_ce_agents::Trigger = serde_json::from_value(trigger)
            .map_err(|e| flowr_types::FlowError::task_failed("trigger", format!("Invalid trigger: {}", e)))?;
        let id = flowr_ce_agents::trigger_manager()
            .create(trigger)
            .await
            .map_err(|e| flowr_types::FlowError::task_failed("trigger", e.to_string()))?;
        self.persist().await;
        Ok(id)
    }

    async fn delete(&self, id: &str) -> FlowResult<()> {
        flowr_ce_agents::trigger_manager()
            .delete(id)
            .await
            .map_err(|e| flowr_types::FlowError::task_failed("trigger", e.to_string()))?;
        self.persist().await;
        Ok(())
    }

    async fn enable(&self, id: &str) -> FlowResult<()> {
        flowr_ce_agents::trigger_manager()
            .enable(id)
            .await
            .map_err(|e| flowr_types::FlowError::task_failed("trigger", e.to_string()))?;
        self.persist().await;
        Ok(())
    }

    async fn pause(&self, id: &str) -> FlowResult<()> {
        flowr_ce_agents::trigger_manager()
            .pause(id)
            .await
            .map_err(|e| flowr_types::FlowError::task_failed("trigger", e.to_string()))?;
        self.persist().await;
        Ok(())
    }

    async fn update(&self, id: &str, trigger: Value) -> FlowResult<()> {
        // Delete + recreate with same ID
        let mut new_trigger: flowr_ce_agents::Trigger = serde_json::from_value(trigger)
            .map_err(|e| flowr_types::FlowError::task_failed("trigger", format!("Invalid trigger: {}", e)))?;
        new_trigger.id = id.to_string();

        // Remove old, insert new
        let _ = flowr_ce_agents::trigger_manager().delete(id).await;
        flowr_ce_agents::trigger_manager()
            .create(new_trigger)
            .await
            .map_err(|e| flowr_types::FlowError::task_failed("trigger", e.to_string()))?;
        self.persist().await;
        Ok(())
    }

    async fn attach(&self, trigger_id: &str, target: Value) -> FlowResult<()> {
        let target: flowr_ce_agents::TriggerTarget = serde_json::from_value(target)
            .map_err(|e| flowr_types::FlowError::task_failed("trigger", format!("Invalid target: {}", e)))?;
        flowr_ce_agents::trigger_manager()
            .attach(trigger_id, target)
            .await
            .map_err(|e| flowr_types::FlowError::task_failed("trigger", e.to_string()))?;
        self.persist().await;
        Ok(())
    }

    async fn detach(&self, trigger_id: &str, target_id: &str) -> FlowResult<()> {
        flowr_ce_agents::trigger_manager()
            .detach(trigger_id, target_id)
            .await
            .map_err(|e| flowr_types::FlowError::task_failed("trigger", e.to_string()))?;
        self.persist().await;
        Ok(())
    }
}
