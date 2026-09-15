//! Event filtering and matching

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// CloudEvent filter criteria
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventFilter {
    /// Event type pattern (supports wildcards with *)
    #[serde(default)]
    pub event_type: Option<String>,

    /// Event source pattern
    #[serde(default)]
    pub source: Option<String>,

    /// Event subject pattern
    #[serde(default)]
    pub subject: Option<String>,

    /// Data content type
    #[serde(default)]
    pub data_content_type: Option<String>,

    /// Additional attribute filters (key-value pairs)
    #[serde(default)]
    pub attributes: Vec<AttributeFilter>,

    /// Data field filters using jq expressions
    #[serde(default)]
    pub data_filters: Vec<DataFilter>,

    /// Correlation key expression (jq path in event data)
    #[serde(default)]
    pub correlation_key: Option<String>,
}

/// Filter on CloudEvent attribute
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttributeFilter {
    /// Attribute name
    pub name: String,
    /// Expected value (exact match or pattern)
    pub value: String,
    /// Whether to use pattern matching
    #[serde(default)]
    pub pattern: bool,
}

/// Filter on CloudEvent data field
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataFilter {
    /// jq expression to extract value from data
    pub expression: String,
    /// Expected value to match
    pub value: Value,
}

impl EventFilter {
    /// Create a filter for a specific event type
    pub fn for_type(event_type: impl Into<String>) -> Self {
        Self {
            event_type: Some(event_type.into()),
            source: None,
            subject: None,
            data_content_type: None,
            attributes: Vec::new(),
            data_filters: Vec::new(),
            correlation_key: None,
        }
    }

    /// Add source filter
    pub fn with_source(mut self, source: impl Into<String>) -> Self {
        self.source = Some(source.into());
        self
    }

    /// Add subject filter
    pub fn with_subject(mut self, subject: impl Into<String>) -> Self {
        self.subject = Some(subject.into());
        self
    }

    /// Add correlation key
    pub fn with_correlation_key(mut self, key: impl Into<String>) -> Self {
        self.correlation_key = Some(key.into());
        self
    }
}

/// CloudEvent representation for matching
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloudEvent {
    /// Event ID
    pub id: String,

    /// Event type
    #[serde(rename = "type")]
    pub event_type: String,

    /// Event source
    pub source: String,

    /// Event subject
    #[serde(default)]
    pub subject: Option<String>,

    /// Spec version
    #[serde(default = "default_spec_version")]
    pub specversion: String,

    /// Data content type
    #[serde(default)]
    pub datacontenttype: Option<String>,

    /// Event timestamp
    #[serde(default)]
    pub time: Option<String>,

    /// Event data
    #[serde(default)]
    pub data: Value,

    /// Extension attributes
    #[serde(flatten)]
    pub extensions: std::collections::HashMap<String, Value>,
}

fn default_spec_version() -> String {
    "1.0".to_string()
}

/// Event matcher for filtering CloudEvents
pub struct EventMatcher;

impl EventMatcher {
    /// Check if a CloudEvent matches a filter
    pub fn matches(event: &CloudEvent, filter: &EventFilter) -> bool {
        // Match event type
        if let Some(ref pattern) = filter.event_type {
            if !Self::pattern_match(&event.event_type, pattern) {
                return false;
            }
        }

        // Match source
        if let Some(ref pattern) = filter.source {
            if !Self::pattern_match(&event.source, pattern) {
                return false;
            }
        }

        // Match subject
        if let Some(ref pattern) = filter.subject {
            match &event.subject {
                Some(subject) => {
                    if !Self::pattern_match(subject, pattern) {
                        return false;
                    }
                }
                None => return false,
            }
        }

        // Match data content type
        if let Some(ref expected) = filter.data_content_type {
            match &event.datacontenttype {
                Some(actual) => {
                    if actual != expected {
                        return false;
                    }
                }
                None => return false,
            }
        }

        // Match attributes
        for attr in &filter.attributes {
            let value = event.extensions.get(&attr.name);
            match value {
                Some(v) => {
                    let v_str = match v {
                        Value::String(s) => s.clone(),
                        _ => v.to_string(),
                    };
                    if attr.pattern {
                        if !Self::pattern_match(&v_str, &attr.value) {
                            return false;
                        }
                    } else if v_str != attr.value {
                        return false;
                    }
                }
                None => return false,
            }
        }

        // Match data filters (simplified - full implementation would use jq)
        for data_filter in &filter.data_filters {
            // Simple path-based matching for now
            // Full implementation would integrate with flowr-expr
            if let Some(actual) = Self::extract_path(&event.data, &data_filter.expression) {
                if actual != data_filter.value {
                    return false;
                }
            } else {
                return false;
            }
        }

        true
    }

    /// Pattern matching with wildcards
    fn pattern_match(value: &str, pattern: &str) -> bool {
        if pattern == "*" {
            return true;
        }

        if pattern.contains('*') {
            // Simple wildcard matching
            let parts: Vec<&str> = pattern.split('*').collect();
            if parts.len() == 2 {
                let prefix = parts[0];
                let suffix = parts[1];
                return value.starts_with(prefix) && value.ends_with(suffix);
            }
        }

        value == pattern
    }

    /// Extract value from JSON using simple dot notation
    fn extract_path(data: &Value, path: &str) -> Option<Value> {
        let path = path.trim_start_matches('.');
        let parts: Vec<&str> = path.split('.').collect();

        let mut current = data;
        for part in parts {
            if part.is_empty() {
                continue;
            }
            match current.get(part) {
                Some(v) => current = v,
                None => return None,
            }
        }

        Some(current.clone())
    }

    /// Find all filters that match an event
    pub fn find_matching_filters<'a>(
        event: &CloudEvent,
        filters: impl IntoIterator<Item = &'a EventFilter>,
    ) -> Vec<&'a EventFilter> {
        filters
            .into_iter()
            .filter(|f| Self::matches(event, f))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_type_match() {
        let event = CloudEvent {
            id: "1".to_string(),
            event_type: "order.created".to_string(),
            source: "orders-service".to_string(),
            subject: None,
            specversion: "1.0".to_string(),
            datacontenttype: None,
            time: None,
            data: Value::Null,
            extensions: Default::default(),
        };

        let filter = EventFilter::for_type("order.created");
        assert!(EventMatcher::matches(&event, &filter));

        let filter = EventFilter::for_type("order.*");
        assert!(EventMatcher::matches(&event, &filter));

        let filter = EventFilter::for_type("payment.created");
        assert!(!EventMatcher::matches(&event, &filter));
    }

    #[test]
    fn test_source_match() {
        let event = CloudEvent {
            id: "1".to_string(),
            event_type: "order.created".to_string(),
            source: "orders-service".to_string(),
            subject: None,
            specversion: "1.0".to_string(),
            datacontenttype: None,
            time: None,
            data: Value::Null,
            extensions: Default::default(),
        };

        let filter = EventFilter::for_type("order.created").with_source("orders-service");
        assert!(EventMatcher::matches(&event, &filter));

        let filter = EventFilter::for_type("order.created").with_source("other-service");
        assert!(!EventMatcher::matches(&event, &filter));
    }
}
