//! Timeout configuration

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Timeout {
    #[serde(default)]
    pub after: Option<TimeoutDuration>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimeoutDuration {
    #[serde(default)]
    pub seconds: Option<u64>,
    #[serde(default)]
    pub minutes: Option<u64>,
    #[serde(default)]
    pub hours: Option<u64>,
}
