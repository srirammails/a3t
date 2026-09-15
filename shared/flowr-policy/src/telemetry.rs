//! The `aesop.*` telemetry field- and span-name contract.
//!
//! CE emits these on every security-relevant span from day one — first-class
//! observability is a CE feature. For EE, the audit trail becomes a different
//! subscriber on events CE already emits, not new instrumentation. Namespacing
//! `aesop.*` now means zero renaming later.
//!
//! These names are mirrored from `aesop-tower::fields`/`spans` (the AESOP repo
//! owns the canonical list); EE CI asserts the two stay identical. CE does not
//! depend on the aesop crates, so the constants are duplicated here on purpose.

/// Field names attached to security-relevant spans.
pub mod fields {
    pub const PRINCIPAL_KIND: &str = "aesop.principal.kind";
    pub const PRINCIPAL_ID: &str = "aesop.principal.id";
    pub const ON_BEHALF_OF: &str = "aesop.on_behalf_of";
    pub const ACTION: &str = "aesop.action";
    pub const RESOURCE_TYPE: &str = "aesop.resource.type";
    pub const RESOURCE_ID: &str = "aesop.resource.id";
    pub const DECISION: &str = "aesop.decision";
    pub const FLOW_ID: &str = "aesop.flow_id";
    pub const RUN_ID: &str = "aesop.run_id";
    pub const STEP: &str = "aesop.step";

    /// Where the task implementation comes from: `"builtin"` today, `"wasm"` once
    /// tasks load from sandboxed WASM plugins. Set on every `tool_call` span so
    /// the audit trail — and EE's Cedar policy — can distinguish trusted builtins
    /// from capability-scoped plugins. See the WASM task-loading direction: the
    /// loading infra is CE, the who-may-use-which-plugin control is EE.
    pub const TASK_SOURCE: &str = "aesop.task.source";
}

/// Value for [`fields::TASK_SOURCE`]: a compiled-in, first-party handler.
pub const TASK_SOURCE_BUILTIN: &str = "builtin";
/// Value for [`fields::TASK_SOURCE`]: a sandboxed WASM plugin (future).
pub const TASK_SOURCE_WASM: &str = "wasm";

/// The fixed span hierarchy: flow_run → flow_step → tool_call.
pub mod spans {
    pub const FLOW_RUN: &str = "flow_run";
    pub const FLOW_STEP: &str = "flow_step";
    pub const TOOL_CALL: &str = "tool_call";
}
