//! RuntimeRegistry — version-based provider selection
//!
//! Maps DSL version strings to RuntimeProvider implementations.
//! Enables pluggable runtime support for different SWF spec versions.

use std::collections::HashMap;
use std::sync::Arc;

use serde_json::Value;

use swf_parser::WorkflowDefinition;

use crate::context::ExecutionContext;
use crate::error::RuntimeError;
use crate::traits::RuntimeProvider;

/// Registry of runtime providers keyed by DSL version
pub struct RuntimeRegistry {
    providers: HashMap<String, Arc<dyn RuntimeProvider>>,
}

impl RuntimeRegistry {
    pub fn new() -> Self {
        Self {
            providers: HashMap::new(),
        }
    }

    /// Register a runtime provider for a specific DSL version
    pub fn register(&mut self, version: &str, provider: Arc<dyn RuntimeProvider>) {
        self.providers.insert(version.to_string(), provider);
    }

    /// Get a provider for a specific DSL version
    pub fn get(&self, version: &str) -> Option<&Arc<dyn RuntimeProvider>> {
        self.providers.get(version)
    }

    /// Execute a workflow, auto-selecting the runtime by DSL version
    pub async fn execute(
        &self,
        workflow: &WorkflowDefinition,
        input: Value,
        ctx: &mut ExecutionContext,
    ) -> Result<Value, RuntimeError> {
        let version = &workflow.document.dsl;

        let provider = self.providers.get(version).ok_or_else(|| {
            RuntimeError::UnsupportedVersion(format!(
                "No runtime registered for DSL version '{}'. Available: {:?}",
                version,
                self.providers.keys().collect::<Vec<_>>()
            ))
        })?;

        provider.execute(workflow, input, ctx).await
    }

    /// List registered DSL versions
    pub fn versions(&self) -> Vec<&str> {
        self.providers.keys().map(|s| s.as_str()).collect()
    }
}

impl Default for RuntimeRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;

    struct MockRuntime;

    #[async_trait]
    impl RuntimeProvider for MockRuntime {
        fn supported_versions(&self) -> &[&str] {
            &["1.0.3"]
        }

        async fn execute(
            &self,
            workflow: &WorkflowDefinition,
            _input: Value,
            _ctx: &mut ExecutionContext,
        ) -> Result<Value, RuntimeError> {
            Ok(serde_json::json!({
                "executed": true,
                "workflow": workflow.document.name,
            }))
        }
    }

    #[tokio::test]
    async fn test_registry_selects_by_version() {
        let mut registry = RuntimeRegistry::new();
        registry.register("1.0.3", Arc::new(MockRuntime));

        let yaml = r#"
document:
  dsl: '1.0.3'
  namespace: test
  name: mock-test
  version: '1.0.0'
do:
  - step:
      set:
        x: 1
"#;
        let workflow = swf_parser::parse_yaml(yaml).unwrap();
        let mut ctx = ExecutionContext {
            instance_id: "test-001".to_string(),
            namespace: "test".to_string(),
            name: "mock-test".to_string(),
            version: "1.0.0".to_string(),
            context: Value::Null,
            secrets: Default::default(),
            runtime: Default::default(),
            storage: None,
            http_client: None,
            extensions: Vec::new(),
            eval_fn: None,
            principal: crate::default_flow_principal(),
            template_meta: Default::default(),
        };

        let result = registry.execute(&workflow, serde_json::json!({}), &mut ctx).await.unwrap();
        assert_eq!(result["executed"], true);
        assert_eq!(result["workflow"], "mock-test");
    }

    #[tokio::test]
    async fn test_registry_rejects_unknown_version() {
        let registry = RuntimeRegistry::new();

        let yaml = r#"
document:
  dsl: '2.0.0'
  namespace: test
  name: future
  version: '1.0.0'
do:
  - step:
      set:
        x: 1
"#;
        let workflow = swf_parser::parse_yaml(yaml).unwrap();
        let mut ctx = ExecutionContext {
            instance_id: "test-002".to_string(),
            namespace: "test".to_string(),
            name: "future".to_string(),
            version: "1.0.0".to_string(),
            context: Value::Null,
            secrets: Default::default(),
            runtime: Default::default(),
            storage: None,
            http_client: None,
            extensions: Vec::new(),
            eval_fn: None,
            principal: crate::default_flow_principal(),
            template_meta: Default::default(),
        };

        let err = registry.execute(&workflow, serde_json::json!({}), &mut ctx).await.unwrap_err();
        assert!(err.to_string().contains("2.0.0"));
    }
}
