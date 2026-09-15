//! Run task — execute processes (container, shell, script, workflow)

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Run task — one of container, shell, script, or workflow
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RunTask {
    Container(ContainerProcess),
    Shell(ShellProcess),
    Script(ScriptProcess),
    Workflow(WorkflowProcess),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContainerProcess {
    pub image: String,
    #[serde(default)]
    pub command: Option<String>,
    #[serde(default)]
    pub ports: Option<Value>,
    #[serde(default)]
    pub volumes: Option<Value>,
    #[serde(default)]
    pub environment: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShellProcess {
    pub command: String,
    #[serde(default)]
    pub arguments: Option<Value>,
    #[serde(default)]
    pub environment: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScriptProcess {
    pub language: String,
    #[serde(default)]
    pub code: Option<String>,
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default)]
    pub arguments: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowProcess {
    pub namespace: String,
    pub name: String,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub input: Option<Value>,
}
