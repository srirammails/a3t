//! Flow control tasks: do, for, fork, switch, try

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::directive::FlowDirective;
use crate::retry::RetryPolicy;
use crate::task::NamedTask;

/// Do task — sequential subtask execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DoTask {
    pub tasks: Vec<NamedTask>,
}

/// For task — iterate over a collection
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForTask {
    /// Iteration variable name
    pub each: String,

    /// Expression evaluating to an iterable (array)
    #[serde(rename = "in")]
    pub collection: String,

    /// Optional index variable name
    #[serde(default)]
    pub at: Option<String>,

    /// Optional while condition — exit loop when false
    #[serde(rename = "while")]
    #[serde(default)]
    pub while_condition: Option<String>,

    /// Subtasks to execute per iteration
    #[serde(rename = "do")]
    pub tasks: Vec<NamedTask>,
}

/// Fork task — parallel branch execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForkTask {
    /// Branches to execute in parallel
    pub branches: Vec<NamedTask>,

    /// If true, first branch to complete wins (others cancelled)
    #[serde(default)]
    pub compete: bool,
}

/// Switch task — conditional branching
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwitchTask {
    pub cases: Vec<SwitchCase>,
}

/// A single case in a switch task
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwitchCase {
    /// Case name
    pub name: String,

    /// Condition expression (mutually exclusive with `default`)
    #[serde(default)]
    pub when: Option<String>,

    /// Flow directive on match
    #[serde(default)]
    pub then: Option<FlowDirective>,

    /// Is this the default case?
    #[serde(default)]
    pub default: bool,
}

/// Try task — error handling with optional retry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TryTask {
    /// Tasks to attempt
    #[serde(rename = "do")]
    pub tasks: Vec<NamedTask>,

    /// Catch block
    #[serde(default)]
    pub catch: Option<CatchBlock>,
}

/// Catch block in a try task
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CatchBlock {
    /// Error filter
    #[serde(default)]
    pub errors: Option<ErrorFilter>,

    /// Retry policy
    #[serde(default)]
    pub retry: Option<RetryPolicy>,

    /// Handler tasks
    #[serde(rename = "do")]
    #[serde(default)]
    pub tasks: Vec<NamedTask>,
}

/// Error filter for catch blocks
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorFilter {
    #[serde(default)]
    pub with: Option<ErrorMatch>,
}

/// Error matching criteria
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorMatch {
    #[serde(rename = "type")]
    #[serde(default)]
    pub error_type: Option<String>,
    #[serde(default)]
    pub status: Option<u16>,
}
