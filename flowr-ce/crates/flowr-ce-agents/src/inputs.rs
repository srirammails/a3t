//! InputManager — manages pending human input requests
//!
//! When a workflow hits a `call:human` step, it creates a pending input request
//! and suspends. The InputManager holds the request and a oneshot channel.
//! When a response arrives via HTTP POST /inputs/:id, the workflow resumes.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::{RwLock, oneshot};
use tracing::{info, warn, debug};
use uuid::Uuid;

/// A pending human input request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InputRequest {
    /// Unique request ID
    pub id: String,
    /// What kind of input (approval, form, etc.)
    pub request_type: String,
    /// Channel (webhook, form, slack, etc.)
    pub channel: String,
    /// Message/prompt to display
    #[serde(default)]
    pub message: Option<String>,
    /// Title
    #[serde(default)]
    pub title: Option<String>,
    /// Choice options (for approvals)
    #[serde(default)]
    pub options: Option<Value>,
    /// Form fields (for form input)
    #[serde(default)]
    pub fields: Option<Value>,
    /// JSON Schema for validation
    #[serde(default)]
    pub schema: Option<Value>,
    /// Context data (shown to user)
    #[serde(default)]
    pub context: Option<Value>,
    /// Workflow info
    #[serde(default)]
    pub workflow: Option<Value>,
    /// Agent that requested this (if any)
    #[serde(default)]
    pub agent_id: Option<String>,
    /// When this was created
    pub created_at: String,
    /// When this expires
    pub expires_at: String,
    /// Current status
    pub status: InputStatus,
}

/// Input request status
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum InputStatus {
    Pending,
    Completed,
    Expired,
    Cancelled,
}

/// Response submitted by a human
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InputResponse {
    pub request_id: String,
    pub data: Value,
    pub responded_at: String,
    #[serde(default)]
    pub responded_by: Option<String>,
}

/// Internal handle — holds the request + the channel to resume the workflow
struct InputHandle {
    request: InputRequest,
    reply_tx: oneshot::Sender<InputResponse>,
    created: Instant,
    timeout: Duration,
}

/// Manages pending human input requests
pub struct InputManager {
    pending: RwLock<HashMap<String, InputHandle>>,
    /// Completed requests (for audit)
    completed: RwLock<Vec<InputRequest>>,
}

impl InputManager {
    pub fn new() -> Self {
        Self {
            pending: RwLock::new(HashMap::new()),
            completed: RwLock::new(Vec::new()),
        }
    }

    /// Create a pending input request. Returns a receiver that the workflow awaits.
    pub async fn create_request(
        &self,
        request: InputRequest,
        timeout: Duration,
    ) -> oneshot::Receiver<InputResponse> {
        let id = request.id.clone();
        let (tx, rx) = oneshot::channel();

        let handle = InputHandle {
            request,
            reply_tx: tx,
            created: Instant::now(),
            timeout,
        };

        self.pending.write().await.insert(id.clone(), handle);
        info!(request_id = %id, "Input request created, workflow suspended");

        rx
    }

    /// Submit a response to a pending request. Resumes the workflow.
    pub async fn submit_response(&self, response: InputResponse) -> anyhow::Result<()> {
        let id = response.request_id.clone();

        let handle = self.pending.write().await.remove(&id)
            .ok_or_else(|| anyhow::anyhow!("Input request '{}' not found or already completed", id))?;

        // Check if expired
        if handle.created.elapsed() > handle.timeout {
            let mut req = handle.request;
            req.status = InputStatus::Expired;
            self.completed.write().await.push(req);
            anyhow::bail!("Input request '{}' has expired", id);
        }

        // Update status
        let mut req = handle.request;
        req.status = InputStatus::Completed;
        self.completed.write().await.push(req);

        // Resume the workflow
        let _ = handle.reply_tx.send(response);
        info!(request_id = %id, "Input response submitted, workflow resumed");

        Ok(())
    }

    /// List all pending input requests
    pub async fn list_pending(&self) -> Vec<InputRequest> {
        let pending = self.pending.read().await;
        pending.values()
            .filter(|h| h.created.elapsed() <= h.timeout)
            .map(|h| h.request.clone())
            .collect()
    }

    /// Get a specific pending request
    pub async fn get_pending(&self, id: &str) -> Option<InputRequest> {
        let pending = self.pending.read().await;
        pending.get(id)
            .filter(|h| h.created.elapsed() <= h.timeout)
            .map(|h| h.request.clone())
    }

    /// List recently completed requests
    pub async fn list_completed(&self, limit: usize) -> Vec<InputRequest> {
        let completed = self.completed.read().await;
        let start = completed.len().saturating_sub(limit);
        completed[start..].to_vec()
    }

    /// Clean up expired requests
    pub async fn cleanup_expired(&self) {
        let mut pending = self.pending.write().await;
        let expired: Vec<String> = pending.iter()
            .filter(|(_, h)| h.created.elapsed() > h.timeout)
            .map(|(id, _)| id.clone())
            .collect();

        for id in expired {
            if let Some(handle) = pending.remove(&id) {
                let mut req = handle.request;
                req.status = InputStatus::Expired;
                warn!(request_id = %id, "Input request expired");
                self.completed.write().await.push(req);
                // Drop the sender — receiver gets an error, workflow handles timeout
            }
        }
    }
}

// Global singleton
static INPUT_MANAGER: std::sync::OnceLock<Arc<InputManager>> = std::sync::OnceLock::new();

pub fn input_manager() -> &'static Arc<InputManager> {
    INPUT_MANAGER.get_or_init(|| Arc::new(InputManager::new()))
}
