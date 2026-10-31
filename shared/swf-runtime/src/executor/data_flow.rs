//! Data flow utilities — schema validation and context merging

use serde_json::Value;
use crate::error::{RuntimeError, Result};

/// Validate a value against a JSON Schema
pub fn validate_schema(value: &Value, schema: &Value, location: &str) -> Result<()> {
    // Use jsonschema if available, otherwise skip
    // For now, basic required-field checking
    if let Some(required) = schema.get("required").and_then(|v| v.as_array()) {
        if let Some(obj) = value.as_object() {
            for field in required {
                if let Some(field_name) = field.as_str() {
                    if !obj.contains_key(field_name) {
                        return Err(RuntimeError::Validation {
                            location: location.to_string(),
                            message: format!("'{}' is a required property", field_name),
                        });
                    }
                }
            }
        }
    }
    Ok(())
}

/// Merge context fields into input so tasks can access exported state via `.`
/// Context fields are added to the input without overwriting existing input fields.
pub fn merge_context_into_input(input: &Value, context: &Value) -> Value {
    match (input, context) {
        (Value::Object(inp), Value::Object(ctx)) => {
            let mut merged = ctx.clone();
            // Input fields take priority over context
            for (key, val) in inp {
                merged.insert(key.clone(), val.clone());
            }
            Value::Object(merged)
        }
        _ => input.clone(),
    }
}

/// Merge exported values into the workflow context
/// Always merges — never replaces context entirely
/// The `$context` key holding the run's principal; set once, never merged over.
pub const PRINCIPAL_KEY: &str = "_principal";

/// The principal as flows see it: `kind`, `id`, `roles`, and `user` — the human
/// the run acts for (the agent's `on_behalf_of`, or the human itself).
pub fn principal_value(p: &flowr_policy::Principal) -> Value {
    let user = match p {
        flowr_policy::Principal::Human { user_id, .. } => Value::String(user_id.clone()),
        flowr_policy::Principal::Agent { on_behalf_of, .. } => on_behalf_of.clone().map(Value::String).unwrap_or(Value::Null),
        flowr_policy::Principal::Service { .. } => Value::Null,
    };
    serde_json::json!({ "kind": p.kind(), "id": p.id(), "roles": p.roles(), "user": user })
}

pub fn merge_into_context(context: &mut Value, exports: Value) {
    match (&mut *context, exports) {
        (Value::Object(ctx), Value::Object(exp)) => {
            for (key, val) in exp {
                if key == PRINCIPAL_KEY && ctx.contains_key(PRINCIPAL_KEY) {
                    continue;
                }
                ctx.insert(key, val);
            }
        }
        (Value::Object(ctx), other) => {
            // Non-object export: store under "_export" key
            ctx.insert("_export".to_string(), other);
        }
        (ctx, Value::Object(exp)) => {
            // Context was not an object — initialize it
            let mut map = serde_json::Map::new();
            for (key, val) in exp {
                map.insert(key, val);
            }
            *ctx = Value::Object(map);
        }
        (ctx, exp) => {
            *ctx = exp;
        }
    }
}
