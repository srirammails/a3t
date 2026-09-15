//! External resource definitions

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExternalResource {
    pub name: String,
    pub uri: String,
    #[serde(default)]
    pub authentication: Option<String>,
    #[serde(default)]
    pub metadata: Option<Value>,
}
