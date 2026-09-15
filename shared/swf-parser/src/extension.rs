//! CNCF SWF Extensions — before/after lifecycle hooks

use serde::{Deserialize, Serialize};
use crate::task::NamedTask;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Extension {
    pub name: String,
    pub extend: String,
    #[serde(default)]
    pub when: Option<String>,
    #[serde(default)]
    pub before: Vec<NamedTask>,
    #[serde(default)]
    pub after: Vec<NamedTask>,
}
