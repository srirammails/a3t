//! AgentManager - Manages agent lifecycles across all 4 modes
//!
//! Singleton that lives in the ClawdFlow process. Handles:
//! - Ephemeral: create → turn → drop (no manager involvement)
//! - Session: create → turn → keep alive with TTL → reuse on next call
//! - Persistent: create on resource creation → accept messages → live forever
//! - Reactive: like persistent but triggered by webhooks/cron/events

#[cfg(feature = "zeroclaw-agent")]
use std::collections::HashMap;
#[cfg(feature = "zeroclaw-agent")]
use std::sync::Arc;
#[cfg(feature = "zeroclaw-agent")]
use std::time::{Duration, Instant};
#[cfg(feature = "zeroclaw-agent")]
use tokio::sync::{Mutex, RwLock, mpsc, oneshot};
#[cfg(feature = "zeroclaw-agent")]
use serde_json::{json, Value};
#[cfg(feature = "zeroclaw-agent")]
use tracing::{info, warn, error, debug};
#[cfg(feature = "zeroclaw-agent")]
use uuid::Uuid;

#[cfg(feature = "zeroclaw-agent")]
use crate::types::*;

/// Handle to a live agent with its runtime state
#[cfg(feature = "zeroclaw-agent")]
pub struct AgentHandle {
    pub agent_id: String,
    pub mode: AgentMode,
    pub status: AgentStatus,
    pub agent: Arc<Mutex<zeroclaw::agent::Agent>>,
    pub config: AgentResourceConfig,
    pub inbox_tx: mpsc::Sender<InboxMessage>,
    pub created_at: Instant,
    pub last_active: Mutex<Instant>,
    pub history_len: Mutex<usize>,
    pub tools_count: usize,
    /// Conversation history (bounded by max_history)
    pub history: Mutex<Vec<AgentTurn>>,
    /// Running metrics
    pub metrics: Mutex<AgentMetrics>,
}

/// Internal inbox message with optional reply channel
#[cfg(feature = "zeroclaw-agent")]
pub struct InboxMessage {
    pub message: AgentMessage,
    pub reply_tx: Option<oneshot::Sender<AgentResponse>>,
}

/// The global agent manager
#[cfg(feature = "zeroclaw-agent")]
pub struct AgentManager {
    agents: RwLock<HashMap<String, Arc<AgentHandle>>>,
    /// Responses for async send:agent — keyed by message_id
    responses: RwLock<HashMap<String, AgentResponse>>,
    /// Internal event bus — listeners receive (event_type, payload)
    event_tx: tokio::sync::broadcast::Sender<(String, Value)>,
}

#[cfg(feature = "zeroclaw-agent")]
impl AgentManager {
    pub fn new() -> Self {
        let (event_tx, _) = tokio::sync::broadcast::channel(256);
        Self {
            agents: RwLock::new(HashMap::new()),
            responses: RwLock::new(HashMap::new()),
            event_tx,
        }
    }

    /// Spawn a persistent, reactive, or session agent. Starts inbox processing loop.
    pub async fn spawn_agent(
        &self,
        handle: AgentHandle,
    ) -> anyhow::Result<String> {
        let agent_id = handle.agent_id.clone();
        let mode = handle.mode.clone();

        if self.agents.read().await.contains_key(&agent_id) {
            anyhow::bail!("Agent '{}' already exists", agent_id);
        }

        let handle = Arc::new(handle);
        self.agents.write().await.insert(agent_id.clone(), handle.clone());

        // Start cron triggers for reactive agents
        if mode == AgentMode::Reactive {
            let handle_clone = handle.clone();
            tokio::spawn(async move {
                Self::cron_loop(handle_clone).await;
            });

            // Start event listener for reactive agents
            let handle_clone = handle.clone();
            let mut event_rx = self.event_tx.subscribe();
            tokio::spawn(async move {
                Self::event_loop(handle_clone, &mut event_rx).await;
            });
        }

        info!(agent_id = %agent_id, mode = ?mode, "Agent spawned");
        Ok(agent_id)
    }

    /// Send a message to a persistent/session agent (fire-and-forget)
    pub async fn send_message(
        &self,
        agent_id: &str,
        message: AgentMessage,
    ) -> anyhow::Result<String> {
        let agents = self.agents.read().await;
        let handle = agents.get(agent_id)
            .ok_or_else(|| anyhow::anyhow!("Agent '{}' not found", agent_id))?;

        let msg_id = message.id.clone();
        handle.inbox_tx.send(InboxMessage {
            message,
            reply_tx: None,
        }).await.map_err(|e| anyhow::anyhow!("Failed to send to agent inbox: {}", e))?;

        Ok(msg_id)
    }

    /// Send a message and wait for reply (request-reply)
    pub async fn ask_message(
        &self,
        agent_id: &str,
        message: AgentMessage,
        timeout: Duration,
    ) -> anyhow::Result<AgentResponse> {
        let agents = self.agents.read().await;
        let handle = agents.get(agent_id)
            .ok_or_else(|| anyhow::anyhow!("Agent '{}' not found", agent_id))?;

        let (reply_tx, reply_rx) = oneshot::channel();

        handle.inbox_tx.send(InboxMessage {
            message,
            reply_tx: Some(reply_tx),
        }).await.map_err(|e| anyhow::anyhow!("Failed to send to agent inbox: {}", e))?;

        tokio::time::timeout(timeout, reply_rx)
            .await
            .map_err(|_| anyhow::anyhow!("Agent response timed out after {:?}", timeout))?
            .map_err(|_| anyhow::anyhow!("Agent reply channel closed"))
    }

    /// Stop a persistent/reactive agent
    pub async fn stop_agent(&self, agent_id: &str) -> anyhow::Result<()> {
        let mut agents = self.agents.write().await;
        if agents.remove(agent_id).is_some() {
            info!(agent_id = %agent_id, "Agent stopped");
            Ok(())
        } else {
            anyhow::bail!("Agent '{}' not found", agent_id)
        }
    }

    /// Get agent info
    pub async fn get_agent_info(&self, agent_id: &str) -> Option<AgentInfo> {
        let agents = self.agents.read().await;
        let handle = agents.get(agent_id)?;
        let history_len = handle.history.lock().await.len();

        Some(AgentInfo {
            agent_id: handle.agent_id.clone(),
            mode: handle.mode.clone(),
            status: handle.status.clone(),
            tools_count: handle.tools_count,
            has_memory: handle.config.memory.is_some(),
            history_len,
            created_at: format!("{:?}", handle.created_at),
            last_active: format!("{:?}", *handle.last_active.lock().await),
            triggers: handle.config.triggers.clone(),
        })
    }

    /// List all live agents
    pub async fn list_agents(&self) -> Vec<AgentInfo> {
        let agents = self.agents.read().await;
        let mut infos = Vec::new();
        for handle in agents.values() {
            let history_len = handle.history.lock().await.len();
            infos.push(AgentInfo {
                agent_id: handle.agent_id.clone(),
                mode: handle.mode.clone(),
                status: handle.status.clone(),
                tools_count: handle.tools_count,
                has_memory: handle.config.memory.is_some(),
                history_len,
                created_at: format!("{:?}", handle.created_at),
                last_active: format!("{:?}", *handle.last_active.lock().await),
                triggers: handle.config.triggers.clone(),
            });
        }
        infos
    }

    /// Get agent conversation history
    pub async fn get_history(&self, agent_id: &str, limit: usize) -> Option<Vec<AgentTurn>> {
        let agents = self.agents.read().await;
        let handle = agents.get(agent_id)?;
        let history = handle.history.lock().await;
        let start = history.len().saturating_sub(limit);
        Some(history[start..].to_vec())
    }

    /// Get agent health status
    pub async fn get_health(&self, agent_id: &str) -> Option<AgentHealth> {
        let agents = self.agents.read().await;
        let handle = agents.get(agent_id)?;
        let metrics = handle.metrics.lock().await.clone();

        Some(AgentHealth {
            agent_id: handle.agent_id.clone(),
            status: handle.status.clone(),
            mode: handle.mode.clone(),
            uptime_secs: handle.created_at.elapsed().as_secs(),
            memory_connected: handle.config.memory.is_some(),
            tools_count: handle.tools_count,
            metrics,
        })
    }

    /// Get agent metrics
    pub async fn get_metrics(&self, agent_id: &str) -> Option<AgentMetrics> {
        let agents = self.agents.read().await;
        match agents.get(agent_id) {
            Some(handle) => {
                let m = handle.metrics.lock().await.clone();
                Some(m)
            }
            None => None,
        }
    }

    /// Record a turn in agent history and metrics
    pub async fn record_turn(&self, agent_id: &str, turn: AgentTurn) {
        let agents = self.agents.read().await;
        if let Some(handle) = agents.get(agent_id) {
            // Update metrics
            {
                let mut metrics = handle.metrics.lock().await;
                metrics.total_turns += 1;
                if turn.success {
                    metrics.successful_turns += 1;
                } else {
                    metrics.failed_turns += 1;
                }
                metrics.total_duration_ms += turn.duration_ms;
                if metrics.total_turns > 0 {
                    metrics.avg_duration_ms = metrics.total_duration_ms / metrics.total_turns;
                }
                metrics.last_turn_at = Some(turn.timestamp.clone());
            }

            // Append to history (bounded)
            {
                let max = handle.config.max_history;
                let mut history = handle.history.lock().await;
                history.push(turn);
                if history.len() > max {
                    let drain = history.len() - max;
                    history.drain(..drain);
                }
            }

            *handle.last_active.lock().await = Instant::now();
        }
    }

    /// Get a stored async response by message ID
    pub async fn get_response(&self, message_id: &str) -> Option<AgentResponse> {
        self.responses.read().await.get(message_id).cloned()
    }

    // =========================================================================
    // Webhook trigger
    // =========================================================================

    /// Deliver a webhook message to matching reactive agents
    pub async fn deliver_webhook(&self, path: &str, body: Value) -> Vec<String> {
        let agents = self.agents.read().await;
        let mut delivered = Vec::new();

        for (id, handle) in agents.iter() {
            if handle.mode != AgentMode::Reactive {
                continue;
            }
            for trigger in &handle.config.triggers {
                if let AgentTrigger::Webhook { path: trigger_path, message } = trigger {
                    if trigger_path == path {
                        let content = message.clone()
                            .unwrap_or_else(|| {
                                format!("Webhook received at {}:\n{}", path,
                                    serde_json::to_string_pretty(&body).unwrap_or_default())
                            });

                        let msg = AgentMessage {
                            id: Uuid::now_v7().to_string(),
                            content,
                            context: Some(body.clone()),
                            source: Some(format!("webhook:{}", path)),
                        };

                        info!(agent_id = %id, path = %path, "Delivering webhook to agent");

                        if handle.inbox_tx.send(InboxMessage {
                            message: msg,
                            reply_tx: None,
                        }).await.is_ok() {
                            delivered.push(id.clone());
                        }
                    }
                }
            }
        }

        delivered
    }

    // =========================================================================
    // Cron trigger
    // =========================================================================

    /// Background: run cron triggers for reactive agents
    async fn cron_loop(handle: Arc<AgentHandle>) {
        let cron_triggers: Vec<_> = handle.config.triggers.iter()
            .filter_map(|t| {
                if let AgentTrigger::Cron { expression, message, .. } = t {
                    Some((expression.clone(), message.clone()))
                } else {
                    None
                }
            })
            .collect();

        if cron_triggers.is_empty() {
            return;
        }

        info!(
            agent_id = %handle.agent_id,
            cron_count = cron_triggers.len(),
            "Cron scheduler started"
        );

        loop {
            // Sleep until the next minute boundary
            let now = chrono::Utc::now();
            let secs_until_next_min = 60 - now.second() as u64;
            tokio::time::sleep(Duration::from_secs(secs_until_next_min)).await;

            // Check if agent still exists
            if Arc::strong_count(&handle) <= 1 {
                info!(agent_id = %handle.agent_id, "Cron scheduler stopping");
                break;
            }

            let now = chrono::Utc::now();

            for (expression, message) in &cron_triggers {
                if cron_matches(expression, &now) {
                    let content = message.clone()
                        .unwrap_or_else(|| format!("Cron trigger fired: {}", expression));

                    let msg = AgentMessage {
                        id: Uuid::now_v7().to_string(),
                        content,
                        context: Some(json!({
                            "trigger": "cron",
                            "expression": expression,
                            "fired_at": now.to_rfc3339(),
                        })),
                        source: Some(format!("cron:{}", expression)),
                    };

                    info!(agent_id = %handle.agent_id, cron = %expression, "Cron trigger fired");

                    if let Err(e) = handle.inbox_tx.send(InboxMessage {
                        message: msg,
                        reply_tx: None,
                    }).await {
                        warn!(agent_id = %handle.agent_id, error = %e, "Failed to deliver cron message");
                    }
                }
            }
        }
    }

    // =========================================================================
    // Event trigger
    // =========================================================================

    /// Emit an event to the internal event bus
    pub fn emit_event(&self, event_type: &str, payload: Value) {
        let _ = self.event_tx.send((event_type.to_string(), payload));
    }

    /// Background: listen for events and deliver to matching reactive agents
    async fn event_loop(
        handle: Arc<AgentHandle>,
        event_rx: &mut tokio::sync::broadcast::Receiver<(String, Value)>,
    ) {
        let event_triggers: Vec<_> = handle.config.triggers.iter()
            .filter_map(|t| {
                if let AgentTrigger::Event { source, filter, message } = t {
                    Some((source.clone(), filter.clone(), message.clone()))
                } else {
                    None
                }
            })
            .collect();

        if event_triggers.is_empty() {
            return;
        }

        info!(
            agent_id = %handle.agent_id,
            event_count = event_triggers.len(),
            "Event listener started"
        );

        loop {
            tokio::select! {
                result = event_rx.recv() => {
                    match result {
                        Ok((event_type, payload)) => {
                            for (source, filter, message) in &event_triggers {
                                // Match source
                                if source != &event_type && source != "*" {
                                    continue;
                                }

                                // Optional filter (simple string match on payload)
                                if let Some(f) = filter {
                                    let payload_str = serde_json::to_string(&payload).unwrap_or_default();
                                    if !payload_str.contains(f.as_str()) {
                                        continue;
                                    }
                                }

                                let content = message.clone()
                                    .unwrap_or_else(|| {
                                        format!("Event '{}' received:\n{}",
                                            event_type,
                                            serde_json::to_string_pretty(&payload).unwrap_or_default())
                                    });

                                let msg = AgentMessage {
                                    id: Uuid::now_v7().to_string(),
                                    content,
                                    context: Some(payload.clone()),
                                    source: Some(format!("event:{}", event_type)),
                                };

                                info!(agent_id = %handle.agent_id, event = %event_type, "Event trigger fired");

                                let _ = handle.inbox_tx.send(InboxMessage {
                                    message: msg,
                                    reply_tx: None,
                                }).await;
                            }
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                            warn!(agent_id = %handle.agent_id, skipped = n, "Event listener lagged");
                        }
                        Err(_) => break,
                    }
                }
                _ = tokio::time::sleep(Duration::from_secs(1)) => {
                    if Arc::strong_count(&handle) <= 1 {
                        break;
                    }
                }
            }
        }

        info!(agent_id = %handle.agent_id, "Event listener stopped");
    }

    /// Clean up expired session agents
    pub async fn cleanup_expired(&self) {
        let mut agents = self.agents.write().await;
        let mut expired = Vec::new();

        for (id, handle) in agents.iter() {
            if handle.mode == AgentMode::Session {
                let last = *handle.last_active.lock().await;
                let ttl = Duration::from_secs(handle.config.ttl);
                if ttl.as_secs() > 0 && last.elapsed() >= ttl {
                    expired.push(id.clone());
                }
            }
        }

        for id in &expired {
            agents.remove(id);
            info!(agent_id = %id, "Expired session agent cleaned up");
        }
    }
}

// =============================================================================
// Simple cron expression matcher (5-field: min hour dom month dow)
// =============================================================================

#[cfg(feature = "zeroclaw-agent")]
use chrono::{Datelike, Timelike};

/// Match a 5-field cron expression against a datetime
/// Supports: *, */N, N, N-M, N,M,O
#[cfg(feature = "zeroclaw-agent")]
fn cron_matches(expression: &str, now: &chrono::DateTime<chrono::Utc>) -> bool {
    let fields: Vec<&str> = expression.split_whitespace().collect();
    if fields.len() != 5 {
        warn!(expression = %expression, "Invalid cron expression (need 5 fields)");
        return false;
    }

    let checks = [
        (fields[0], now.minute() as u32, 0, 59),       // minute
        (fields[1], now.hour() as u32, 0, 23),          // hour
        (fields[2], now.day() as u32, 1, 31),            // day of month
        (fields[3], now.month() as u32, 1, 12),          // month
        (fields[4], now.weekday().num_days_from_sunday(), 0, 6), // day of week (0=Sun)
    ];

    for (field, value, min, max) in checks {
        if !field_matches(field, value, min, max) {
            return false;
        }
    }

    true
}

/// Match a single cron field against a value
#[cfg(feature = "zeroclaw-agent")]
fn field_matches(field: &str, value: u32, _min: u32, _max: u32) -> bool {
    if field == "*" {
        return true;
    }

    // */N — every N
    if let Some(step) = field.strip_prefix("*/") {
        if let Ok(n) = step.parse::<u32>() {
            return n > 0 && value % n == 0;
        }
    }

    // Comma-separated values: 1,5,10
    for part in field.split(',') {
        // Range: 1-5
        if let Some((start, end)) = part.split_once('-') {
            if let (Ok(s), Ok(e)) = (start.parse::<u32>(), end.parse::<u32>()) {
                if value >= s && value <= e {
                    return true;
                }
                continue;
            }
        }

        // Exact value
        if let Ok(n) = part.parse::<u32>() {
            if value == n {
                return true;
            }
        }
    }

    false
}

/// Trait for async agent builder function
#[cfg(feature = "zeroclaw-agent")]
#[async_trait::async_trait]
pub trait AsyncAgentBuilder: Send {
    async fn build(self, agent_id: &str) -> anyhow::Result<AgentHandle>;
}

/// Global singleton
#[cfg(feature = "zeroclaw-agent")]
static AGENT_MANAGER: std::sync::OnceLock<AgentManager> = std::sync::OnceLock::new();

#[cfg(feature = "zeroclaw-agent")]
pub fn agent_manager() -> &'static AgentManager {
    AGENT_MANAGER.get_or_init(AgentManager::new)
}
