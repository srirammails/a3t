//! Retry policy for try/catch blocks

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryPolicy {
    #[serde(default)]
    pub delay: Option<Duration>,
    #[serde(default)]
    pub backoff: Option<BackoffPolicy>,
    #[serde(default)]
    pub limit: Option<RetryLimit>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Duration {
    #[serde(default)]
    pub seconds: Option<u64>,
    #[serde(default)]
    pub minutes: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackoffPolicy {
    #[serde(default)]
    pub linear: Option<Value>,
    #[serde(default)]
    pub exponential: Option<Value>,
    #[serde(default)]
    pub constant: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryLimit {
    #[serde(default)]
    pub attempt: Option<AttemptLimit>,
    #[serde(default)]
    pub duration: Option<Duration>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttemptLimit {
    pub count: u32,
}
