//! The A3T action vocabulary — the single source of legal `domain:verb`
//! action names. This is the shared language between CE's runtime, the
//! `aesop.*` trace fields, and AESOP's Cedar schema. If it drifts per call
//! site, no policy can be written against it, so additions are reviewed
//! changes and EE CI asserts parity with `aesop/schemas/a3t.cedarschema`.

use crate::Action;

// Tool / MCP execution
pub const TOOL_INVOKE: &str = "tool:invoke";
pub const MCP_CALL: &str = "mcp:call";

// Flow lifecycle
pub const FLOW_START: &str = "flow:start";
pub const FLOW_CANCEL: &str = "flow:cancel";
/// Placing a flow into a runtime pool — the deployment decision.
///
/// Distinct from `flow:start`, which runs a flow that is already deployed.
/// Deploying chooses WHERE something runs (which runtime pool, which namespace,
/// shared or isolated), so it is gated on the target rather than the flow: the
/// resource is the `Runtime`, carrying its labels. That is what lets a policy say
/// "prod tier needs Admin" without enumerating flows.
pub const FLOW_DEPLOY: &str = "flow:deploy";
/// Operating a deployment: hard-pause it, resume it, or remove it. Gated on the
/// Runtime it lives in, like `flow:deploy`, so whoever may place work in a pool
/// may also stop it there — and prod stays Admin-only both ways.
pub const FLOW_PAUSE: &str = "flow:pause";
pub const FLOW_RESUME: &str = "flow:resume";
pub const FLOW_UNDEPLOY: &str = "flow:undeploy";

// Runtime lifecycle. A runtime is shared infrastructure hosting many teams'
// deployments, so changing ITS state is an operator decision, not a deployer's.
pub const RUNTIME_HIBERNATE: &str = "runtime:hibernate";
pub const RUNTIME_PAUSE: &str = "runtime:pause";
pub const RUNTIME_RESUME: &str = "runtime:resume";

// Agent lifecycle & delegation
pub const AGENT_SPAWN: &str = "agent:spawn";
pub const AGENT_STOP: &str = "agent:stop";

// Memory (ClawDB)
pub const MEMORY_READ: &str = "memory:read";
pub const MEMORY_WRITE: &str = "memory:write";

// LLM
pub const LLM_CALL: &str = "llm:call";

// Registry resource governance (E2 #2b). Writing a Cedar *policy* resource is
// itself a policy-gated action — this is the separation-of-duties seam: only
// InfoSec roles may create/update/delete `type_ref: cedar` resources.
pub const RESOURCE_CREATE: &str = "resource:create";
pub const RESOURCE_UPDATE: &str = "resource:update";
pub const RESOURCE_DELETE: &str = "resource:delete";

// Chat transcript access. A persisted conversation is owned by the principal that
// created it; reading or deleting one is policy-gated so "owners only, InfoSec by
// exception" lives in the policy pack rather than in an `if` in the endpoint.
pub const SESSION_READ: &str = "session:read";
pub const SESSION_DELETE: &str = "session:delete";

/// Every action the runtime may emit. EE CI checks this list against the
/// Cedar schema so no action is unenforceable and no policy is orphaned.
pub const ALL: &[&str] = &[
    TOOL_INVOKE,
    MCP_CALL,
    FLOW_START,
    FLOW_CANCEL,
    FLOW_DEPLOY,
    FLOW_PAUSE,
    FLOW_RESUME,
    FLOW_UNDEPLOY,
    RUNTIME_HIBERNATE,
    RUNTIME_PAUSE,
    RUNTIME_RESUME,
    AGENT_SPAWN,
    AGENT_STOP,
    MEMORY_READ,
    MEMORY_WRITE,
    LLM_CALL,
    RESOURCE_CREATE,
    RESOURCE_UPDATE,
    RESOURCE_DELETE,
    SESSION_READ,
    SESSION_DELETE,
];

/// Whether `s` is a registered action name.
pub fn is_registered(s: &str) -> bool {
    ALL.contains(&s)
}

// Typed constructors — the ergonomic, no-typo way to name an action at a
// call site.
pub fn tool_invoke() -> Action {
    Action::known(TOOL_INVOKE)
}
pub fn mcp_call() -> Action {
    Action::known(MCP_CALL)
}
pub fn flow_start() -> Action {
    Action::known(FLOW_START)
}
pub fn flow_cancel() -> Action {
    Action::known(FLOW_CANCEL)
}
pub fn flow_deploy() -> Action {
    Action::known(FLOW_DEPLOY)
}
pub fn flow_pause() -> Action {
    Action::known(FLOW_PAUSE)
}
pub fn flow_resume() -> Action {
    Action::known(FLOW_RESUME)
}
pub fn flow_undeploy() -> Action {
    Action::known(FLOW_UNDEPLOY)
}
pub fn runtime_hibernate() -> Action {
    Action::known(RUNTIME_HIBERNATE)
}
pub fn runtime_pause() -> Action {
    Action::known(RUNTIME_PAUSE)
}
pub fn runtime_resume() -> Action {
    Action::known(RUNTIME_RESUME)
}
pub fn agent_spawn() -> Action {
    Action::known(AGENT_SPAWN)
}
pub fn agent_stop() -> Action {
    Action::known(AGENT_STOP)
}
pub fn memory_read() -> Action {
    Action::known(MEMORY_READ)
}
pub fn memory_write() -> Action {
    Action::known(MEMORY_WRITE)
}
pub fn llm_call() -> Action {
    Action::known(LLM_CALL)
}
pub fn resource_create() -> Action {
    Action::known(RESOURCE_CREATE)
}
pub fn resource_update() -> Action {
    Action::known(RESOURCE_UPDATE)
}
pub fn resource_delete() -> Action {
    Action::known(RESOURCE_DELETE)
}
pub fn session_read() -> Action {
    Action::known(SESSION_READ)
}
pub fn session_delete() -> Action {
    Action::known(SESSION_DELETE)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_entries_are_valid_and_unique() {
        let mut seen = std::collections::HashSet::new();
        for a in ALL {
            assert!(Action::new(*a).is_ok(), "invalid registered action: {a}");
            assert!(seen.insert(*a), "duplicate registered action: {a}");
        }
    }

    #[test]
    fn constructors_match_registry() {
        assert_eq!(tool_invoke().as_str(), TOOL_INVOKE);
        assert!(is_registered(mcp_call().as_str()));
        assert!(!is_registered("bogus:action"));
    }
}
