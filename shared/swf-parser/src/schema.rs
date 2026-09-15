//! Input, Output, Export definitions with transform + schema

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InputDefinition {
    #[serde(default)]
    pub from: Option<String>,
    #[serde(default)]
    pub schema: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutputDefinition {
    #[serde(rename = "as")]
    #[serde(default)]
    pub transform: Option<String>,
    #[serde(default)]
    pub schema: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportDefinition {
    #[serde(rename = "as")]
    #[serde(default)]
    pub transform: Option<String>,
    #[serde(default)]
    pub schema: Option<Value>,
}
