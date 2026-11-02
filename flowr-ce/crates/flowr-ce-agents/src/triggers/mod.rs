//! Trigger system — first-class resources that route events to agents or workflows
//!
//! Triggers are independent from agents. They can be created, attached to agents
//! or workflows, and reassigned dynamically.

pub mod types;
pub mod cron;

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use serde_json::{json, Value};
use tokio::sync::RwLock;
use tracing::{info, warn, debug, error};
use uuid::Uuid;

use types::*;

/// Manages trigger lifecycle and delivery
pub struct TriggerManager {
    triggers: RwLock<HashMap<String, Trigger>>,
    /// Event bus for internal events
    event_tx: tokio::sync::broadcast::Sender<(String, Value)>,
}

impl TriggerManager {
    pub fn new() -> Self {
        let (event_tx, _) = tokio::sync::broadcast::channel(256);
        Self {
            triggers: RwLock::new(HashMap::new()),
            event_tx,
        }
    }

    // =========================================================================
    // CRUD
    // =========================================================================

    /// Create a new trigger
    pub async fn create(&self, mut trigger: Trigger) -> anyhow::Result<String> {
        if trigger.id.is_empty() {
            trigger.id = Uuid::now_v7().to_string();
        }
        let id = trigger.id.clone();

        let mut triggers = self.triggers.write().await;
        if triggers.contains_key(&id) {
            anyhow::bail!("Trigger '{}' already exists", id);
        }

        info!(trigger_id = %id, name = %trigger.name, "Trigger created");
        triggers.insert(id.clone(), trigger);

        Ok(id)
    }

    /// Get a trigger by ID
    pub async fn get(&self, id: &str) -> Option<Trigger> {
        self.triggers.read().await.get(id).cloned()
    }

    /// List all triggers
    pub async fn list(&self) -> Vec<Trigger> {
        self.triggers.read().await.values().cloned().collect()
    }

    /// Delete a trigger
    pub async fn delete(&self, id: &str) -> anyhow::Result<()> {
        let mut triggers = self.triggers.write().await;
        if triggers.remove(id).is_some() {
            info!(trigger_id = %id, "Trigger deleted");
            Ok(())
        } else {
            anyhow::bail!("Trigger '{}' not found", id)
        }
    }

    /// Enable a trigger
    pub async fn enable(&self, id: &str) -> anyhow::Result<()> {
        let mut triggers = self.triggers.write().await;
        let trigger = triggers.get_mut(id)
            .ok_or_else(|| anyhow::anyhow!("Trigger '{}' not found", id))?;
        trigger.enabled = true;
        trigger.status = TriggerStatus::Active;
        info!(trigger_id = %id, "Trigger enabled");
        Ok(())
    }

    /// Pause a trigger
    pub async fn pause(&self, id: &str) -> anyhow::Result<()> {
        let mut triggers = self.triggers.write().await;
        let trigger = triggers.get_mut(id)
            .ok_or_else(|| anyhow::anyhow!("Trigger '{}' not found", id))?;
        trigger.enabled = false;
        trigger.status = TriggerStatus::Paused;
        info!(trigger_id = %id, "Trigger paused");
        Ok(())
    }

    // =========================================================================
    // Attach / Detach targets
    // =========================================================================

    /// Attach a target to a trigger
    pub async fn attach(&self, trigger_id: &str, target: TriggerTarget) -> anyhow::Result<()> {
        let mut triggers = self.triggers.write().await;
        let trigger = triggers.get_mut(trigger_id)
            .ok_or_else(|| anyhow::anyhow!("Trigger '{}' not found", trigger_id))?;

        let target_desc = match &target {
            TriggerTarget::Agent { agent_id } => format!("agent:{}", agent_id),
            TriggerTarget::Workflow { namespace, name, .. } => format!("workflow:{}/{}", namespace, name),
            TriggerTarget::Webhook { url, .. } => format!("webhook:{}", url),
        };

        trigger.targets.push(target);
        info!(trigger_id = %trigger_id, target = %target_desc, "Target attached");
        Ok(())
    }

    /// Detach a target from a trigger by matching agent_id, workflow name, or webhook url
    pub async fn detach(&self, trigger_id: &str, target_id: &str) -> anyhow::Result<()> {
        let mut triggers = self.triggers.write().await;
        let trigger = triggers.get_mut(trigger_id)
            .ok_or_else(|| anyhow::anyhow!("Trigger '{}' not found", trigger_id))?;

        let before = trigger.targets.len();
        trigger.targets.retain(|t| {
            match t {
                TriggerTarget::Agent { agent_id } => agent_id != target_id,
                TriggerTarget::Workflow { name, .. } => name != target_id,
                TriggerTarget::Webhook { url, .. } => url != target_id,
            }
        });

        let removed = before - trigger.targets.len();
        if removed > 0 {
            info!(trigger_id = %trigger_id, target = %target_id, removed = removed, "Target detached");
            Ok(())
        } else {
            anyhow::bail!("No target '{}' found on trigger '{}'", target_id, trigger_id)
        }
    }

    // =========================================================================
    // Delivery
    // =========================================================================

    /// Deliver a webhook to matching triggers
    pub async fn deliver_webhook(
        &self,
        path: &str,
        body: Value,
        agent_sender: &dyn AgentMessageSender,
        workflow_executor: &dyn WorkflowExecutor,
    ) -> Vec<TriggerDeliveryResult> {
        let triggers = self.triggers.read().await;
        let mut results = Vec::new();

        for trigger in triggers.values() {
            if !trigger.enabled {
                continue;
            }

            if let TriggerConfig::Webhook { path: trigger_path, message_template, .. } = &trigger.config {
                if trigger_path != path {
                    continue;
                }

                let content = message_template.clone()
                    .unwrap_or_else(|| {
                        format!("Webhook received at {}:\n{}", path,
                            serde_json::to_string_pretty(&body).unwrap_or_default())
                    });

                let result = self.deliver_to_targets(
                    &trigger.id,
                    &trigger.name,
                    &trigger.targets,
                    &content,
                    Some(body.clone()),
                    &format!("webhook:{}", path),
                    agent_sender,
                    workflow_executor,
                ).await;

                results.push(result);
            }
        }

        results
    }

    /// Deliver one ADB topic message (`message` = the published fields) to an
    /// `adb_topic` trigger's targets. `None` if the trigger is gone or disabled.
    pub async fn deliver_topic_message(
        &self,
        trigger_id: &str,
        offset: u64,
        message: Value,
        agent_sender: &dyn AgentMessageSender,
        workflow_executor: &dyn WorkflowExecutor,
    ) -> Option<TriggerDeliveryResult> {
        let trigger = self.triggers.read().await.get(trigger_id).cloned()?;
        if !trigger.enabled {
            return None;
        }
        let TriggerConfig::AdbTopic { topic, message_template, .. } = &trigger.config else { return None };
        let content = message_template.clone().unwrap_or_else(|| {
            format!("Message on topic {}:\n{}", topic, serde_json::to_string_pretty(&message).unwrap_or_default())
        });
        // Workflow targets get the message's fields, plus where it came from.
        let mut input = message;
        if let Some(m) = input.as_object_mut() {
            m.insert("_topic".into(), json!(topic));
            m.insert("_offset".into(), json!(offset));
        }
        Some(
            self.deliver_to_targets(
                &trigger.id,
                &trigger.name,
                &trigger.targets,
                &content,
                Some(input),
                &format!("adb_topic:{}", topic),
                agent_sender,
                workflow_executor,
            )
            .await,
        )
    }

    /// Record (or clear) the ADB subscription an `adb_topic` trigger reads.
    pub async fn set_topic_subscription(&self, trigger_id: &str, subscription: Option<String>) -> bool {
        let mut triggers = self.triggers.write().await;
        match triggers.get_mut(trigger_id).map(|t| &mut t.config) {
            Some(TriggerConfig::AdbTopic { subscription: s, .. }) => {
                *s = subscription;
                true
            }
            _ => false,
        }
    }

    /// Deliver an internal event to matching triggers
    pub async fn deliver_event(
        &self,
        event_type: &str,
        payload: Value,
        agent_sender: &dyn AgentMessageSender,
        workflow_executor: &dyn WorkflowExecutor,
    ) -> Vec<TriggerDeliveryResult> {
        let triggers = self.triggers.read().await;
        let mut results = Vec::new();

        for trigger in triggers.values() {
            if !trigger.enabled {
                continue;
            }

            if let TriggerConfig::Event { source, filter, message } = &trigger.config {
                if source != event_type && source != "*" {
                    continue;
                }

                // Optional filter
                if let Some(f) = filter {
                    let payload_str = serde_json::to_string(&payload).unwrap_or_default();
                    if !payload_str.contains(f.as_str()) {
                        continue;
                    }
                }

                let content = message.clone()
                    .unwrap_or_else(|| {
                        format!("Event '{}' received:\n{}", event_type,
                            serde_json::to_string_pretty(&payload).unwrap_or_default())
                    });

                let result = self.deliver_to_targets(
                    &trigger.id,
                    &trigger.name,
                    &trigger.targets,
                    &content,
                    Some(payload.clone()),
                    &format!("event:{}", event_type),
                    agent_sender,
                    workflow_executor,
                ).await;

                results.push(result);
            }
        }

        results
    }

    /// Deliver a CloudEvent sent in `namespace` (by `emit`, `send_event` or
    /// `POST /events`) to that namespace's event triggers whose `source` is its
    /// type, or `*`. A workflow target gets the event as its input.
    pub async fn deliver_cloud_event(
        &self,
        namespace: &str,
        event: &Value,
        agent_sender: &dyn AgentMessageSender,
        workflow_executor: &dyn WorkflowExecutor,
    ) -> Vec<TriggerDeliveryResult> {
        let event_type = event.get("type").and_then(|t| t.as_str()).unwrap_or_default();
        let matching: Vec<Trigger> = {
            let triggers = self.triggers.read().await;
            triggers
                .values()
                .filter(|t| t.enabled && t.namespace == namespace)
                .filter(|t| match &t.config {
                    TriggerConfig::Event { source, filter, .. } => {
                        (source == event_type || source == "*")
                            && filter.as_ref().is_none_or(|f| serde_json::to_string(event).unwrap_or_default().contains(f.as_str()))
                    }
                    _ => false,
                })
                .cloned()
                .collect()
        };
        let mut results = Vec::new();
        for trigger in matching {
            let TriggerConfig::Event { message, .. } = &trigger.config else { continue };
            let content = message.clone().unwrap_or_else(|| {
                format!("Event '{}' received:\n{}", event_type, serde_json::to_string_pretty(event).unwrap_or_default())
            });
            results.push(
                self.deliver_to_targets(
                    &trigger.id,
                    &trigger.name,
                    &trigger.targets,
                    &content,
                    Some(event.clone()),
                    &format!("event:{event_type}"),
                    agent_sender,
                    workflow_executor,
                )
                .await,
            );
        }
        results
    }

    /// Emit an event to the internal event bus
    pub fn emit_event(&self, event_type: &str, payload: Value) {
        let _ = self.event_tx.send((event_type.to_string(), payload));
    }

    /// Get matching cron triggers that should fire now
    pub async fn get_cron_triggers_due(
        &self,
        now: &chrono::DateTime<chrono::Utc>,
    ) -> Vec<(Trigger, String)> {
        let triggers = self.triggers.read().await;
        let mut due = Vec::new();

        for trigger in triggers.values() {
            if !trigger.enabled {
                continue;
            }
            if let TriggerConfig::Cron { expression, message, .. } = &trigger.config {
                if cron::cron_matches(expression, now) {
                    due.push((trigger.clone(), message.clone()));
                }
            }
        }

        due
    }

    /// Deliver to all targets of a trigger
    async fn deliver_to_targets(
        &self,
        trigger_id: &str,
        trigger_name: &str,
        targets: &[TriggerTarget],
        content: &str,
        context: Option<Value>,
        source: &str,
        agent_sender: &dyn AgentMessageSender,
        workflow_executor: &dyn WorkflowExecutor,
    ) -> TriggerDeliveryResult {
        let mut delivered = Vec::new();
        let mut failed = Vec::new();

        for target in targets {
            match target {
                TriggerTarget::Agent { agent_id } => {
                    match agent_sender.send_to_agent(agent_id, content, context.clone(), source).await {
                        Ok(_) => {
                            info!(trigger = %trigger_id, agent = %agent_id, "Delivered to agent");
                            delivered.push(format!("agent:{}", agent_id));
                        }
                        Err(e) => {
                            warn!(trigger = %trigger_id, agent = %agent_id, error = %e, "Failed to deliver to agent");
                            failed.push(format!("agent:{} ({})", agent_id, e));
                        }
                    }
                }
                TriggerTarget::Workflow { namespace, name, input_mapping } => {
                    let input = context.clone().unwrap_or(json!({ "message": content }));
                    match workflow_executor.execute_workflow(namespace, name, input).await {
                        Ok(instance_id) => {
                            info!(trigger = %trigger_id, workflow = %name, instance = %instance_id, "Delivered to workflow");
                            delivered.push(format!("workflow:{}/{} ({})", namespace, name, instance_id));
                        }
                        Err(e) => {
                            warn!(trigger = %trigger_id, workflow = %name, error = %e, "Failed to execute workflow");
                            failed.push(format!("workflow:{}/{} ({})", namespace, name, e));
                        }
                    }
                }
                TriggerTarget::Webhook { url, headers } => {
                    let client = reqwest::Client::new();
                    let mut req = client.post(url)
                        .header("Content-Type", "application/json")
                        .json(&json!({
                            "trigger_id": trigger_id,
                            "trigger_name": trigger_name,
                            "source": source,
                            "content": content,
                            "context": context,
                        }));

                    if let Some(hdrs) = headers {
                        if let Some(obj) = hdrs.as_object() {
                            for (k, v) in obj {
                                if let Some(val) = v.as_str() {
                                    req = req.header(k.as_str(), val);
                                }
                            }
                        }
                    }

                    match req.send().await {
                        Ok(resp) if resp.status().is_success() => {
                            info!(trigger = %trigger_id, url = %url, "Delivered to webhook");
                            delivered.push(format!("webhook:{}", url));
                        }
                        Ok(resp) => {
                            let status = resp.status();
                            warn!(trigger = %trigger_id, url = %url, status = %status, "Webhook returned error");
                            failed.push(format!("webhook:{} ({})", url, status));
                        }
                        Err(e) => {
                            warn!(trigger = %trigger_id, url = %url, error = %e, "Webhook request failed");
                            failed.push(format!("webhook:{} ({})", url, e));
                        }
                    }
                }
            }
        }

        TriggerDeliveryResult {
            trigger_id: trigger_id.to_string(),
            trigger_name: trigger_name.to_string(),
            delivered_to: delivered,
            failed,
        }
    }

    /// Start the cron scheduler background task
    pub fn start_cron_scheduler(
        self: &Arc<Self>,
        agent_sender: Arc<dyn AgentMessageSender>,
        workflow_executor: Arc<dyn WorkflowExecutor>,
    ) {
        let manager = self.clone();
        tokio::spawn(async move {
            info!("Trigger cron scheduler started");
            loop {
                // Sleep until next minute boundary
                let now = chrono::Utc::now();
                let secs_until = 60 - now.second() as u64;
                tokio::time::sleep(Duration::from_secs(secs_until)).await;

                let now = chrono::Utc::now();
                let due = manager.get_cron_triggers_due(&now).await;

                for (trigger, message) in due {
                    info!(trigger_id = %trigger.id, name = %trigger.name, "Cron trigger fired");
                    // `minute` names the tick, so a second delivery of the same
                    // tick (e.g. around a restart) can be recognised and dropped.
                    let context = json!({
                        "trigger": "cron",
                        "trigger_id": trigger.id,
                        "fired_at": now.to_rfc3339(),
                        "minute": now.format("%Y-%m-%dT%H:%MZ").to_string(),
                    });

                    manager.deliver_to_targets(
                        &trigger.id,
                        &trigger.name,
                        &trigger.targets,
                        &message,
                        Some(context),
                        &format!("cron:{}", trigger.id),
                        agent_sender.as_ref(),
                        workflow_executor.as_ref(),
                    ).await;
                }
            }
        });
    }

    /// Start the event listener background task
    pub fn start_event_listener(
        self: &Arc<Self>,
        agent_sender: Arc<dyn AgentMessageSender>,
        workflow_executor: Arc<dyn WorkflowExecutor>,
    ) {
        let manager = self.clone();
        let mut event_rx = self.event_tx.subscribe();
        tokio::spawn(async move {
            info!("Trigger event listener started");
            loop {
                match event_rx.recv().await {
                    Ok((event_type, payload)) => {
                        manager.deliver_event(
                            &event_type,
                            payload,
                            agent_sender.as_ref(),
                            workflow_executor.as_ref(),
                        ).await;
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                        warn!(skipped = n, "Event listener lagged");
                    }
                    Err(_) => break,
                }
            }
        });
    }

    // =========================================================================
    // Persistence — save/load triggers to survive restarts
    // =========================================================================

    /// Export all triggers as JSON (for persistence)
    pub async fn export_all(&self) -> Value {
        let triggers = self.triggers.read().await;
        let list: Vec<&Trigger> = triggers.values().collect();
        json!(list)
    }

    /// Import triggers from JSON (for restore on startup)
    pub async fn import_all(&self, data: &Value) -> anyhow::Result<usize> {
        let triggers_list: Vec<Trigger> = serde_json::from_value(data.clone())
            .map_err(|e| anyhow::anyhow!("Invalid trigger data: {}", e))?;

        let count = triggers_list.len();
        let mut triggers = self.triggers.write().await;

        for trigger in triggers_list {
            let id = if trigger.id.is_empty() {
                Uuid::now_v7().to_string()
            } else {
                trigger.id.clone()
            };
            info!(trigger_id = %id, name = %trigger.name, "Trigger restored");
            triggers.insert(id, trigger);
        }

        info!(count = count, "Triggers imported");
        Ok(count)
    }
}

// =============================================================================
// Traits for decoupling TriggerManager from AgentManager/WorkflowExecutor
// =============================================================================

use chrono::Timelike;

/// Sends messages to agents (implemented by AgentManager)
#[async_trait::async_trait]
pub trait AgentMessageSender: Send + Sync {
    async fn send_to_agent(
        &self,
        agent_id: &str,
        content: &str,
        context: Option<Value>,
        source: &str,
    ) -> anyhow::Result<String>;
}

/// Executes workflows (implemented by the workflow engine)
#[async_trait::async_trait]
pub trait WorkflowExecutor: Send + Sync {
    async fn execute_workflow(
        &self,
        namespace: &str,
        name: &str,
        input: Value,
    ) -> anyhow::Result<String>;
}

// =============================================================================
// Global singleton
// =============================================================================

static TRIGGER_MANAGER: std::sync::OnceLock<Arc<TriggerManager>> = std::sync::OnceLock::new();

pub fn trigger_manager() -> &'static Arc<TriggerManager> {
    TRIGGER_MANAGER.get_or_init(|| Arc::new(TriggerManager::new()))
}
