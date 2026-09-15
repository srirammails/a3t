//! Workflow scheduling

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Schedule {
    #[serde(default)]
    pub every: Option<Value>,
    #[serde(default)]
    pub cron: Option<String>,
    #[serde(default)]
    pub after: Option<Value>,
    #[serde(default)]
    pub on: Option<Value>,
}
