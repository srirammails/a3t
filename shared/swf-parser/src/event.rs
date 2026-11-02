//! Emit and Listen tasks — CloudEvents

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmitTask {
    pub event: EventDefinition,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventDefinition {
    pub with: EventProperties,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventProperties {
    #[serde(rename = "type")]
    pub event_type: String,
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default)]
    pub data: Option<Value>,
    /// Other CloudEvent attributes (`subject`, extensions…).
    #[serde(flatten)]
    pub extra: serde_json::Map<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListenTask {
    pub to: ListenTarget,
    /// What the task outputs for each event: `data` (the default) or
    /// `envelope` (the whole CloudEvent); `raw` reads as `data`.
    #[serde(default)]
    pub read: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListenTarget {
    #[serde(default)]
    pub one: Option<EventFilter>,
    #[serde(default)]
    pub all: Option<Vec<EventFilter>>,
    #[serde(default)]
    pub any: Option<Vec<EventFilter>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventFilter {
    pub with: EventFilterMatch,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventFilterMatch {
    #[serde(rename = "type")]
    pub event_type: String,
    #[serde(default)]
    pub source: Option<String>,
    /// Other attributes the event must carry (`subject`, `data`, extensions…);
    /// an object value matches as a subset.
    #[serde(flatten)]
    pub extra: serde_json::Map<String, Value>,
}
