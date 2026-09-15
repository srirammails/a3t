//! Use block — reusable components

use std::collections::HashMap;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::auth::Authentication;
use crate::catalog::Catalog;
use crate::extension::Extension;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UseBlock {
    #[serde(default)]
    pub extensions: Vec<Value>,  // Parsed separately into Extension structs
    #[serde(default)]
    pub functions: Option<HashMap<String, Value>>,
    #[serde(default)]
    pub secrets: Vec<String>,
    #[serde(default)]
    pub authentications: Option<HashMap<String, Authentication>>,
    #[serde(default)]
    pub catalogs: Option<HashMap<String, Catalog>>,
    #[serde(default)]
    pub errors: Option<HashMap<String, Value>>,
    #[serde(default)]
    pub retries: Option<HashMap<String, Value>>,
    #[serde(default)]
    pub timeouts: Option<HashMap<String, Value>>,
}
