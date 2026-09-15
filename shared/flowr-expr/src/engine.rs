//! Expression Engine - jq-based evaluation
//!
//! Handles `${ ... }` expressions as per CNCF Serverless Workflow DSL.

use std::collections::HashMap;
use std::sync::RwLock;

use jaq_core::load::{Arena, File, Loader};
use jaq_core::{Compiler, Ctx, Filter, Native, RcIter};
use jaq_json::Val;
use serde_json::Value;
use tracing::{debug, trace};

use crate::error::{ExprError, ExprResult};
use crate::scope::Scope;

/// Expression engine for evaluating jq expressions
pub struct ExpressionEngine {
    /// Cached compiled filters
    cache: RwLock<HashMap<String, CompiledFilter>>,
}

/// A compiled jq filter ready for execution
struct CompiledFilter {
    filter: Filter<Native<Val>>,
}

// Safety: CompiledFilter is Send+Sync because Filter is designed for concurrent use
unsafe impl Send for CompiledFilter {}
unsafe impl Sync for CompiledFilter {}

impl Default for ExpressionEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl ExpressionEngine {
    /// Create a new expression engine
    pub fn new() -> Self {
        Self {
            cache: RwLock::new(HashMap::new()),
        }
    }

    /// Check if a string is an expression (contains `${ ... }`)
    pub fn is_expr(s: &str) -> bool {
        let trimmed = s.trim();
        trimmed.starts_with("${") && trimmed.ends_with('}')
    }

    /// Check if a string contains any expression patterns
    pub fn contains_expr(s: &str) -> bool {
        s.contains("${") && s.contains('}')
    }

    /// Extract the jq expression from `${ expr }` format
    pub fn extract_expr(s: &str) -> ExprResult<&str> {
        let trimmed = s.trim();
        if !trimmed.starts_with("${") {
            return Err(ExprError::InvalidFormat(
                "expression must start with '${'".to_string(),
            ));
        }
        if !trimmed.ends_with('}') {
            return Err(ExprError::InvalidFormat(
                "expression must end with '}'".to_string(),
            ));
        }

        // Extract content between ${ and }
        let inner = &trimmed[2..trimmed.len() - 1];
        Ok(inner.trim())
    }

    /// Evaluate an expression against input data with scope
    ///
    /// # Arguments
    /// * `input` - The input JSON value (accessible as `.`)
    /// * `expr` - The expression string (either raw jq or `${ ... }` format)
    /// * `scope` - Runtime scope with $runtime, $workflow, $task, $context
    ///
    /// # Returns
    /// The result of evaluating the expression
    pub fn eval(&self, input: &Value, expr: &str, scope: &Scope) -> ExprResult<Value> {
        // Extract jq expression if wrapped in ${ }
        let jq_expr = if Self::is_expr(expr) {
            Self::extract_expr(expr)?
        } else {
            expr.trim()
        };

        debug!(expr = jq_expr, "evaluating expression");

        // Compile or get from cache
        let filter = self.get_or_compile(jq_expr)?;

        // Execute the filter
        self.execute_filter(&filter, input, scope)
    }

    /// Evaluate a Value recursively, processing any expressions found
    ///
    /// This handles nested expressions in objects and arrays.
    pub fn eval_recursive(
        &self,
        value: &Value,
        input: &Value,
        scope: &Scope,
    ) -> ExprResult<Value> {
        match value {
            Value::String(s) => {
                if Self::is_expr(s) {
                    self.eval(input, s, scope)
                } else if Self::contains_expr(s) {
                    // Handle string interpolation (template strings)
                    self.eval_interpolated(s, input, scope)
                } else {
                    Ok(value.clone())
                }
            }
            Value::Array(arr) => {
                let results: ExprResult<Vec<Value>> = arr
                    .iter()
                    .map(|v| self.eval_recursive(v, input, scope))
                    .collect();
                Ok(Value::Array(results?))
            }
            Value::Object(obj) => {
                let mut result = serde_json::Map::new();
                for (key, val) in obj {
                    // Keys can also be expressions
                    let eval_key = if Self::is_expr(key) {
                        match self.eval(input, key, scope)? {
                            Value::String(s) => s,
                            other => other.to_string(),
                        }
                    } else {
                        key.clone()
                    };

                    let eval_val = self.eval_recursive(val, input, scope)?;
                    result.insert(eval_key, eval_val);
                }
                Ok(Value::Object(result))
            }
            // Primitives pass through unchanged
            _ => Ok(value.clone()),
        }
    }

    /// Evaluate interpolated string with multiple expressions
    /// e.g., "Hello ${ .name }, you have ${ .count } items"
    fn eval_interpolated(&self, s: &str, input: &Value, scope: &Scope) -> ExprResult<Value> {
        let mut result = String::new();
        let mut remaining = s;

        while let Some(start) = remaining.find("${") {
            // Add text before the expression
            result.push_str(&remaining[..start]);
            remaining = &remaining[start..];

            // Find the closing brace
            if let Some(end) = remaining.find('}') {
                let expr = &remaining[..=end];
                let value = self.eval(input, expr, scope)?;

                // Convert to string for interpolation
                match value {
                    Value::String(s) => result.push_str(&s),
                    Value::Null => result.push_str("null"),
                    other => result.push_str(&other.to_string()),
                }

                remaining = &remaining[end + 1..];
            } else {
                // No closing brace, treat as literal
                result.push_str(remaining);
                remaining = "";
            }
        }

        // Add any remaining text
        result.push_str(remaining);
        Ok(Value::String(result))
    }

    /// Get a compiled filter from cache or compile it
    fn get_or_compile(&self, expr: &str) -> ExprResult<CompiledFilter> {
        // Try read lock first
        {
            let cache = self.cache.read().unwrap();
            if let Some(filter) = cache.get(expr) {
                trace!(expr, "cache hit");
                return Ok(CompiledFilter {
                    filter: filter.filter.clone(),
                });
            }
        }

        // Compile and cache
        let filter = self.compile(expr)?;

        {
            let mut cache = self.cache.write().unwrap();
            cache.insert(expr.to_string(), CompiledFilter {
                filter: filter.filter.clone(),
            });
        }

        Ok(filter)
    }

    /// Compile a jq expression
    fn compile(&self, expr: &str) -> ExprResult<CompiledFilter> {
        trace!(expr, "compiling expression");

        // Create arena for parsing
        let arena = Arena::default();

        // Create loader with standard library definitions
        let loader = Loader::new(jaq_std::defs().chain(jaq_json::defs()));

        // Create the program file
        let program = File { code: expr, path: () };

        // Parse the expression
        let modules = loader.load(&arena, program).map_err(|errs| {
            let messages: Vec<String> = errs.into_iter()
                .map(|(path, err)| format!("{:?}: {:?}", path, err))
                .collect();
            ExprError::Parse(messages.join("; "))
        })?;

        // Compile with standard library functions and global scope variables
        // Note: Variables must be declared in reverse order from how they're passed to Ctx::new
        // $resource is a FlowR extension for accessing resolved resource config
        // $secrets provides access to workflow secrets per CNCF spec
        let filter = Compiler::default()
            .with_funs(jaq_std::funs().chain(jaq_json::funs()))
            .with_global_vars(["$secrets", "$resource", "$context", "$task", "$workflow", "$runtime"])
            .compile(modules)
            .map_err(|errs| {
                let messages: Vec<String> = errs.into_iter()
                    .map(|(path, err)| format!("{:?}: {:?}", path, err))
                    .collect();
                ExprError::Compile(messages.join("; "))
            })?;

        Ok(CompiledFilter { filter })
    }

    /// Execute a compiled filter against input with scope
    fn execute_filter(
        &self,
        compiled: &CompiledFilter,
        input: &Value,
        scope: &Scope,
    ) -> ExprResult<Value> {
        // Convert input to jaq Val
        let input_val: Val = Val::from(input.clone());

        // Build variable bindings from scope
        let vars: Vec<Val> = scope
            .to_vars()
            .into_iter()
            .map(|(_name, val)| Val::from(val))
            .collect();

        // Create context with variables
        let inputs = RcIter::new(std::iter::empty());
        let ctx = Ctx::new(vars, &inputs);

        // Run the filter
        let mut results: Vec<Value> = Vec::new();

        for output in compiled.filter.run((ctx.clone(), input_val.clone())) {
            match output {
                Ok(val) => {
                    let json_val: Value = val.into();
                    results.push(json_val);
                }
                Err(err) => {
                    return Err(ExprError::Eval {
                        path: ".".to_string(),
                        message: format!("{:?}", err),
                    });
                }
            }
        }

        // Return single value or array for multiple outputs
        match results.len() {
            0 => Ok(Value::Null),
            1 => Ok(results.remove(0)),
            _ => Ok(Value::Array(results)),
        }
    }

    /// Clear the filter cache
    pub fn clear_cache(&self) {
        let mut cache = self.cache.write().unwrap();
        cache.clear();
    }

    /// Get the number of cached filters
    pub fn cache_size(&self) -> usize {
        let cache = self.cache.read().unwrap();
        cache.len()
    }
}
