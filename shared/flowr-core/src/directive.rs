//! Flow directives - control flow instructions

use serde::{Deserialize, Serialize};

/// Flow directive returned by task execution
///
/// Controls what happens after a task completes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum FlowDirective {
    /// Continue to the next task in sequence
    Continue,

    /// Exit the current scope (do block, for loop, etc.)
    Exit,

    /// End the workflow immediately with current output
    End,

    /// Jump to a named task
    Goto(String),

    /// Suspend the workflow (Wait or Listen task)
    /// The flow will be resumed by the SuspendManager
    Suspend,
}

impl Default for FlowDirective {
    fn default() -> Self {
        Self::Continue
    }
}

impl FlowDirective {
    /// Create a goto directive
    pub fn goto(target: impl Into<String>) -> Self {
        Self::Goto(target.into())
    }

    /// Check if this directive continues normal flow
    pub fn is_continue(&self) -> bool {
        matches!(self, Self::Continue)
    }

    /// Check if this directive ends the workflow
    pub fn is_terminal(&self) -> bool {
        matches!(self, Self::End)
    }

    /// Check if this directive exits current scope
    pub fn is_exit(&self) -> bool {
        matches!(self, Self::Exit)
    }

    /// Check if this directive suspends the workflow
    pub fn is_suspend(&self) -> bool {
        matches!(self, Self::Suspend)
    }

    /// Get the goto target if this is a Goto directive
    pub fn goto_target(&self) -> Option<&str> {
        match self {
            Self::Goto(target) => Some(target),
            _ => None,
        }
    }
}

impl From<&str> for FlowDirective {
    fn from(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "continue" => Self::Continue,
            "exit" => Self::Exit,
            "end" => Self::End,
            target => Self::Goto(target.to_string()),
        }
    }
}
