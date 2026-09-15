//! Flow directives — control task sequencing

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum FlowDirective {
    Named(String),
}

impl FlowDirective {
    pub fn is_continue(&self) -> bool {
        matches!(self, FlowDirective::Named(s) if s == "continue")
    }
    pub fn is_end(&self) -> bool {
        matches!(self, FlowDirective::Named(s) if s == "end")
    }
    pub fn is_exit(&self) -> bool {
        matches!(self, FlowDirective::Named(s) if s == "exit")
    }
}
