//! Workflow definition types
//!
//! Represents CNCF Serverless Workflow definitions for storage and retrieval.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Workflow metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowMetadata {
    /// DSL version (e.g., "1.0.3")
    #[serde(default)]
    pub dsl: Option<String>,

    /// Workflow namespace
    pub namespace: String,

    /// Workflow name
    pub name: String,

    /// Workflow version (semver)
    pub version: String,

    /// Human-readable title
    #[serde(default)]
    pub title: Option<String>,

    /// Description
    #[serde(default)]
    pub description: Option<String>,

    /// Tags for categorization
    #[serde(default)]
    pub tags: Vec<String>,

    /// Arbitrary metadata map
    #[serde(default)]
    pub metadata: Option<Value>,

    /// Whether the workflow is enabled
    #[serde(default = "default_enabled")]
    pub enabled: bool,

    /// Creation timestamp
    #[serde(default = "Utc::now")]
    pub created_at: DateTime<Utc>,

    /// Last update timestamp
    #[serde(default = "Utc::now")]
    pub updated_at: DateTime<Utc>,
}

fn default_enabled() -> bool {
    true
}

impl WorkflowMetadata {
    /// Create new metadata
    pub fn new(namespace: impl Into<String>, name: impl Into<String>, version: impl Into<String>) -> Self {
        let now = Utc::now();
        Self {
            dsl: None,
            namespace: namespace.into(),
            name: name.into(),
            version: version.into(),
            title: None,
            description: None,
            tags: Vec::new(),
            metadata: None,
            enabled: true,
            created_at: now,
            updated_at: now,
        }
    }

    /// Get the unique key for this workflow
    pub fn key(&self) -> String {
        format!("{}/{}", self.namespace, self.name)
    }

    /// Get the versioned key
    pub fn versioned_key(&self) -> String {
        format!("{}/{}:{}", self.namespace, self.name, self.version)
    }
}

/// Step definition within a workflow
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepDefinition {
    /// Step name (unique within workflow)
    pub name: String,

    /// Step type (set, call, emit, raise, do, switch, for, fork, try, wait, listen, run)
    #[serde(rename = "type")]
    pub step_type: String,

    /// Step configuration (varies by type)
    #[serde(default)]
    pub config: Value,

    /// Condition expression — skip task if evaluates to false (CNCF spec `if`)
    #[serde(rename = "if", default)]
    pub if_condition: Option<String>,

    /// Input transformation expression
    #[serde(rename = "input.from")]
    #[serde(default)]
    pub input_from: Option<String>,

    /// Output transformation expression
    #[serde(rename = "output.as")]
    #[serde(default)]
    pub output_as: Option<String>,

    /// Export transformation expression
    #[serde(rename = "export.as")]
    #[serde(default)]
    pub export_as: Option<String>,

    /// Flow directive (continue, exit, end, or task name for goto)
    #[serde(default)]
    pub then: Option<String>,

    /// Task-level timeout (ISO 8601 duration, e.g., "PT30S", "PT2M")
    #[serde(default)]
    pub timeout: Option<String>,

    /// Whether this step is a checkpoint
    #[serde(default)]
    pub checkpoint: bool,

    /// Whether this step is enabled
    #[serde(default = "default_enabled")]
    pub enabled: bool,

    /// Resource reference (e.g., "prod-mysql", "default/stripe-api")
    /// Resolved at runtime and made available in task context as $resource
    #[serde(default)]
    pub resource: Option<String>,

    /// Arbitrary metadata map
    #[serde(default)]
    pub metadata: Option<Value>,
}

/// Extension definition within a workflow's `use` block
///
/// Extensions act as before/after interceptors on task types per CNCF spec.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtensionDefinition {
    /// Which task types this extension applies to ("all" or specific type like "call", "set")
    pub extend: String,

    /// Optional condition expression — extension only fires when this evaluates to true
    #[serde(default)]
    pub when: Option<String>,

    /// Tasks to run before the target task
    #[serde(default)]
    pub before: Vec<StepDefinition>,

    /// Tasks to run after the target task
    #[serde(default)]
    pub after: Vec<StepDefinition>,
}

/// Named extension entry (name -> definition)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NamedExtension {
    /// Extension name (e.g., "logging")
    pub name: String,

    /// Extension definition
    #[serde(flatten)]
    pub definition: ExtensionDefinition,
}

/// The `use` block for workflow-level declarations per CNCF spec
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UseBlock {
    /// Named extensions (before/after interceptors)
    #[serde(default)]
    pub extensions: Vec<NamedExtension>,

    /// Secret names available to the workflow
    #[serde(default)]
    pub secrets: Option<Vec<String>>,

    /// Reusable error definitions
    #[serde(default)]
    pub errors: Option<Value>,

    /// Reusable retry policy definitions
    #[serde(default)]
    pub retries: Option<Value>,

    /// Reusable timeout definitions
    #[serde(default)]
    pub timeouts: Option<Value>,
}

/// Complete workflow definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowDefinition {
    /// Workflow metadata
    #[serde(flatten)]
    pub metadata: WorkflowMetadata,

    /// Use block with extensions, secrets, errors, retries, timeouts
    #[serde(rename = "use", default)]
    pub use_block: Option<UseBlock>,

    /// Input schema (JSON Schema)
    #[serde(default)]
    pub input_schema: Option<Value>,

    /// Output schema (JSON Schema)
    #[serde(default)]
    pub output_schema: Option<Value>,

    /// Default durability mode (completion or checkpoint)
    #[serde(default = "default_durability")]
    pub durability: String,

    /// Workflow steps
    pub steps: Vec<StepDefinition>,

    /// Raw definition (original YAML/JSON)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub raw: Option<String>,
}

fn default_durability() -> String {
    "completion".to_string()
}

impl WorkflowDefinition {
    /// Create a new workflow definition
    pub fn new(
        namespace: impl Into<String>,
        name: impl Into<String>,
        version: impl Into<String>,
        steps: Vec<StepDefinition>,
    ) -> Self {
        Self {
            metadata: WorkflowMetadata::new(namespace, name, version),
            use_block: None,
            input_schema: None,
            output_schema: None,
            durability: default_durability(),
            steps,
            raw: None,
        }
    }

    /// Parse from YAML string
    pub fn from_yaml(yaml: &str) -> Result<Self, serde_yaml::Error> {
        let mut def: Self = serde_yaml::from_str(yaml)?;
        def.raw = Some(yaml.to_string());
        Ok(def)
    }

    /// Parse from JSON string
    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        let mut def: Self = serde_json::from_str(json)?;
        def.raw = Some(json.to_string());
        Ok(def)
    }

    /// Serialize to JSON
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }

    /// Serialize to pretty JSON
    pub fn to_json_pretty(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }

    /// Get the workflow key
    pub fn key(&self) -> String {
        self.metadata.key()
    }

    /// Get the versioned key
    pub fn versioned_key(&self) -> String {
        self.metadata.versioned_key()
    }

    /// Check if the workflow is enabled
    pub fn is_enabled(&self) -> bool {
        self.metadata.enabled
    }

    /// Get step by name
    pub fn get_step(&self, name: &str) -> Option<&StepDefinition> {
        self.steps.iter().find(|s| s.name == name)
    }

    /// Try to parse from a JSON Value, handling both CNCF Serverless Workflow format
    /// (document: + do:) and the internal format (flat metadata + steps:).
    pub fn from_value(value: Value) -> Result<Self, String> {
        // Try CNCF format first
        if let Some(converted) = cncf::convert_cncf_to_internal(&value) {
            serde_json::from_value(converted)
                .map_err(|e| format!("Failed to parse converted CNCF workflow: {}", e))
        } else {
            // Try internal format directly
            serde_json::from_value(value)
                .map_err(|e| format!("Failed to parse workflow: {}", e))
        }
    }

    /// Try to parse from a YAML string, handling both CNCF and internal formats.
    pub fn from_yaml_auto(yaml: &str) -> Result<Self, String> {
        let value: Value = serde_yaml::from_str(yaml)
            .map_err(|e| format!("Invalid YAML: {}", e))?;
        Self::from_value(value)
    }
}

/// CNCF Serverless Workflow format converter.
///
/// Converts from the CNCF DSL format (document: + do:) to the internal
/// WorkflowDefinition format (flat metadata + steps:).
pub mod cncf {
    use serde_json::{Map, Value};

    /// Detect CNCF format and convert to internal format.
    /// Returns None if the input is not CNCF format.
    pub fn convert_cncf_to_internal(value: &Value) -> Option<Value> {
        let doc = value.get("document")?;
        let do_block = value.get("do")?;

        let mut result = Map::new();

        // Flatten document metadata into root
        if let Value::Object(doc_map) = doc {
            for (k, v) in doc_map {
                result.insert(k.clone(), v.clone());
            }
        }

        // Convert do: array to steps: array
        let steps = convert_do_to_steps(do_block);
        result.insert("steps".to_string(), steps);

        // Convert input.schema to input_schema
        if let Some(input) = value.get("input") {
            if let Some(schema) = input.get("schema") {
                result.insert("input_schema".to_string(), schema.clone());
            }
        }

        // Preserve use block
        if let Some(use_block) = value.get("use") {
            result.insert("use".to_string(), use_block.clone());
        }

        Some(Value::Object(result))
    }

    /// Convert a CNCF `do:` array into internal `steps:` array.
    fn convert_do_to_steps(do_block: &Value) -> Value {
        let arr = match do_block.as_array() {
            Some(a) => a,
            None => return Value::Array(vec![]),
        };

        let steps: Vec<Value> = arr
            .iter()
            .filter_map(|entry| {
                let obj = entry.as_object()?;
                let (step_name, step_def) = obj.iter().next()?;
                let step_obj = step_def.as_object()?;
                convert_single_step(step_name, step_obj)
            })
            .collect();

        Value::Array(steps)
    }

    /// Convert a single CNCF step entry into an internal StepDefinition JSON.
    fn convert_single_step(name: &str, def: &Map<String, Value>) -> Option<Value> {
        let mut step = Map::new();
        step.insert("name".to_string(), Value::String(name.to_string()));

        // Determine step type and config from the action key
        let (step_type, config, resource) = determine_type_and_config(def)?;
        step.insert("type".to_string(), Value::String(step_type));
        step.insert("config".to_string(), config);

        // Extract resource
        if let Some(res) = resource {
            step.insert("resource".to_string(), Value::String(res));
        }

        // Extract common fields
        if let Some(if_cond) = def.get("if") {
            step.insert("if".to_string(), if_cond.clone());
        }
        if let Some(export) = def.get("export") {
            if let Some(as_expr) = export.get("as") {
                step.insert("export.as".to_string(), as_expr.clone());
            }
        }
        if let Some(output) = def.get("output") {
            if let Some(as_expr) = output.get("as") {
                step.insert("output.as".to_string(), as_expr.clone());
            }
        }
        if let Some(input) = def.get("input") {
            if let Some(from_expr) = input.get("from") {
                step.insert("input.from".to_string(), from_expr.clone());
            }
        }
        if let Some(then) = def.get("then") {
            step.insert("then".to_string(), then.clone());
        }
        if let Some(timeout) = def.get("timeout") {
            // Store timeout as JSON string for the Option<String> field
            if timeout.is_string() {
                step.insert("timeout".to_string(), timeout.clone());
            } else {
                step.insert(
                    "timeout".to_string(),
                    Value::String(timeout.to_string()),
                );
            }
        }

        Some(Value::Object(step))
    }

    /// Determine the internal step type and config from a CNCF step definition.
    /// Returns (type, config, optional_resource).
    fn determine_type_and_config(
        def: &Map<String, Value>,
    ) -> Option<(String, Value, Option<String>)> {
        // set:
        if let Some(set_val) = def.get("set") {
            return Some(("set".to_string(), set_val.clone(), None));
        }

        // call: <target> with: <config>
        if let Some(call_target) = def.get("call") {
            let target = call_target.as_str().unwrap_or("unknown");

            let mut config = def
                .get("with")
                .cloned()
                .unwrap_or(Value::Object(Map::new()));

            // Extract _resource from with: block
            let resource = config
                .as_object_mut()
                .and_then(|m| m.remove("_resource"))
                .and_then(|v| v.as_str().map(String::from));

            // Map call targets to internal type names
            let step_type = match target {
                // state/io operations drop the "call:" prefix
                "state:get" | "state:set" | "state:delete" | "input:file" | "output:file"
                | "input:stream" | "output:stream" => target.to_string(),
                // http just becomes call:http
                "http" => "call:http".to_string(),
                // everything else: call:<target>
                other => format!("call:{}", other),
            };

            return Some((step_type, config, resource));
        }

        // run: { shell: | script: | workflow: }
        if let Some(run_val) = def.get("run") {
            if let Some(run_obj) = run_val.as_object() {
                if let Some(shell_config) = run_obj.get("shell") {
                    return Some(("run:shell".to_string(), shell_config.clone(), None));
                }
                if let Some(script_config) = run_obj.get("script") {
                    // Determine language for type
                    let lang = script_config
                        .get("language")
                        .and_then(|v| v.as_str())
                        .unwrap_or("python");
                    let step_type = match lang {
                        "lua" => "script:lua",
                        "js" | "javascript" => "script:js",
                        "python" | "py" => "script:python",
                        _ => "run:script",
                    };
                    return Some((step_type.to_string(), script_config.clone(), None));
                }
                if let Some(workflow_config) = run_obj.get("workflow") {
                    return Some((
                        "run:workflow".to_string(),
                        workflow_config.clone(),
                        None,
                    ));
                }
            }
        }

        // for:
        if let Some(for_val) = def.get("for") {
            let mut for_config = for_val.clone();
            // Convert nested do: steps recursively
            if let Some(for_obj) = for_config.as_object_mut() {
                if let Some(do_block) = for_obj.get("do").cloned() {
                    let converted = convert_do_to_steps(&do_block);
                    for_obj.insert("do".to_string(), converted);
                }
            }
            return Some(("for".to_string(), for_config, None));
        }

        // switch:
        if let Some(switch_val) = def.get("switch") {
            let cases = convert_switch_cases(switch_val);
            let mut config = Map::new();
            config.insert("cases".to_string(), cases);
            return Some(("switch".to_string(), Value::Object(config), None));
        }

        // do: (nested block)
        if let Some(do_val) = def.get("do") {
            let converted = convert_do_to_steps(do_val);
            let mut config = Map::new();
            config.insert("do".to_string(), converted);
            return Some(("do".to_string(), Value::Object(config), None));
        }

        // try:
        if let Some(try_val) = def.get("try") {
            let config = convert_try_block(try_val);
            return Some(("try".to_string(), config, None));
        }

        // fork:
        if let Some(fork_val) = def.get("fork") {
            return Some(("fork".to_string(), fork_val.clone(), None));
        }

        // raise:
        if let Some(raise_val) = def.get("raise") {
            return Some(("raise".to_string(), raise_val.clone(), None));
        }

        // wait:
        if let Some(wait_val) = def.get("wait") {
            return Some(("wait".to_string(), wait_val.clone(), None));
        }

        None
    }

    /// Convert CNCF switch cases from named entries to an internal cases array.
    fn convert_switch_cases(switch_val: &Value) -> Value {
        let arr = match switch_val.as_array() {
            Some(a) => a,
            None => return Value::Array(vec![]),
        };

        let cases: Vec<Value> = arr
            .iter()
            .filter_map(|entry| {
                let obj = entry.as_object()?;
                let (_, case_def) = obj.iter().next()?;
                let case_obj = case_def.as_object()?;

                let mut case_map = Map::new();
                if let Some(when) = case_obj.get("when") {
                    case_map.insert("when".to_string(), when.clone());
                }
                if let Some(then) = case_obj.get("then") {
                    case_map.insert("then".to_string(), then.clone());
                }

                Some(Value::Object(case_map))
            })
            .collect();

        Value::Array(cases)
    }

    /// Convert a CNCF try block, recursively converting nested do/catch steps.
    fn convert_try_block(try_val: &Value) -> Value {
        let mut config = match try_val.as_object() {
            Some(obj) => obj.clone(),
            None => return try_val.clone(),
        };

        // Convert try's do: block
        if let Some(do_block) = config.get("do").cloned() {
            let converted = convert_do_to_steps(&do_block);
            config.insert("do".to_string(), converted);
        }

        // Convert catch block — worker expects catch as an array
        if let Some(catch_val) = config.get("catch").cloned() {
            if catch_val.is_object() {
                // Single catch block — convert nested do: and wrap in array
                let mut catch_obj = catch_val.as_object().unwrap().clone();
                if let Some(catch_do) = catch_obj.get("do").cloned() {
                    let converted = convert_do_to_steps(&catch_do);
                    catch_obj.insert("do".to_string(), converted);
                }
                config.insert(
                    "catch".to_string(),
                    Value::Array(vec![Value::Object(catch_obj)]),
                );
            } else if catch_val.is_array() {
                // Multiple catch blocks — convert each
                let catches: Vec<Value> = catch_val
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|c| {
                        if let Some(catch_obj) = c.as_object() {
                            let mut co = catch_obj.clone();
                            if let Some(catch_do) = co.get("do").cloned() {
                                let converted = convert_do_to_steps(&catch_do);
                                co.insert("do".to_string(), converted);
                            }
                            Value::Object(co)
                        } else {
                            c.clone()
                        }
                    })
                    .collect();
                config.insert("catch".to_string(), Value::Array(catches));
            }
        }

        Value::Object(config)
    }
}

/// Workflow summary (for listing)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowSummary {
    /// Namespace
    pub namespace: String,
    /// Name
    pub name: String,
    /// Active version
    pub version: String,
    /// Title
    pub title: Option<String>,
    /// Whether enabled
    pub enabled: bool,
    /// Number of steps
    pub step_count: usize,
    /// Available versions
    pub versions: Vec<String>,
    /// Last updated
    pub updated_at: DateTime<Utc>,
}

impl From<&WorkflowDefinition> for WorkflowSummary {
    fn from(def: &WorkflowDefinition) -> Self {
        Self {
            namespace: def.metadata.namespace.clone(),
            name: def.metadata.name.clone(),
            version: def.metadata.version.clone(),
            title: def.metadata.title.clone(),
            enabled: def.metadata.enabled,
            step_count: def.steps.len(),
            versions: vec![def.metadata.version.clone()],
            updated_at: def.metadata.updated_at,
        }
    }
}
