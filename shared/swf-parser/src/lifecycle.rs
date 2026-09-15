//! Lifecycle events (CloudEvents format)

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LifecycleEvent {
    pub r#type: String,
    pub source: String,
    pub id: String,
    #[serde(default)]
    pub data: Option<serde_json::Value>,
}
