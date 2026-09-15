//! Catalog — external function registries

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Catalog {
    pub endpoint: CatalogEndpoint,
    #[serde(default)]
    pub authentication: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CatalogEndpoint {
    pub uri: String,
    #[serde(default)]
    pub authentication: Option<serde_json::Value>,
}
