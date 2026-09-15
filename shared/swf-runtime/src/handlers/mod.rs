//! Built-in TaskHandler implementations
//!
//! These handlers implement the CNCF SWF task types that require
//! external interaction (HTTP, shell, scripts, etc.)
//! Flow-control tasks (set, do, for, switch, try, fork, raise, wait)
//! are handled directly by the WorkflowExecutor.

pub mod call_http;
pub mod run_shell;
pub mod run_script;

use std::sync::Arc;
use crate::executor::WorkflowExecutor;

/// Register all built-in handlers with an executor
pub fn register_builtin_handlers(executor: &mut WorkflowExecutor) {
    executor.register_handler(Arc::new(call_http::HttpHandler::new()));
    executor.register_handler(Arc::new(run_shell::ShellHandler));
    executor.register_handler(Arc::new(run_script::ScriptHandler));
}
