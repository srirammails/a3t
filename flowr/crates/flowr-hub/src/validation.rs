//! Config Validation - JSON Schema validation for resource configs

use jsonschema::Validator;
use serde_json::Value;

use crate::error::{HubError, HubResult};

/// Validate resource config against a JSON Schema
pub fn validate_resource_config(config: &Value, schema: &Value) -> HubResult<()> {
    let validator = Validator::new(schema).map_err(|e| {
        HubError::ValidationFailed(format!("Invalid schema: {}", e))
    })?;

    let result = validator.validate(config);

    if let Err(error) = result {
        // Single error, format it nicely
        let path = error.instance_path.to_string();
        let message = if path.is_empty() {
            error.to_string()
        } else {
            format!("{}: {}", path, error)
        };
        return Err(HubError::ValidationFailed(message));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_valid_config() {
        let schema = json!({
            "type": "object",
            "properties": {
                "host": { "type": "string" },
                "port": { "type": "integer" }
            },
            "required": ["host", "port"]
        });

        let config = json!({
            "host": "localhost",
            "port": 3306
        });

        assert!(validate_resource_config(&config, &schema).is_ok());
    }

    #[test]
    fn test_missing_required() {
        let schema = json!({
            "type": "object",
            "properties": {
                "host": { "type": "string" },
                "port": { "type": "integer" }
            },
            "required": ["host", "port"]
        });

        let config = json!({
            "host": "localhost"
            // missing port
        });

        let result = validate_resource_config(&config, &schema);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("port"));
    }

    #[test]
    fn test_wrong_type() {
        let schema = json!({
            "type": "object",
            "properties": {
                "port": { "type": "integer" }
            }
        });

        let config = json!({
            "port": "not a number"
        });

        let result = validate_resource_config(&config, &schema);
        assert!(result.is_err());
    }
}
