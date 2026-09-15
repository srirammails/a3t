//! Putting a permission question to the human in the chat, and waiting.
//!
//! When policy returns [`PolicyDecision::Ask`] the agent stops mid-turn, the
//! question is streamed to the conversation as an `authz-request` chunk, and the
//! run suspends on a `oneshot` until the user answers through
//! `POST /inputs/:id`. The answer is recorded in the session ledger, so it binds
//! the rest of the conversation without being asked twice.
//!
//! This reuses the `call:human` suspend/resume machinery rather than inventing a
//! parallel one — an approval IS human input, and routing it through the same
//! `InputManager` means it shows up in the pending-inputs surfaces for free.
//!
//! **In-memory, and deliberately so at this stage.** `InputManager` holds
//! pending requests in-process, so a restart loses them and the suspended run
//! never resumes. That is acceptable while the only approver is a person sitting
//! in the chat that raised the question. It is NOT acceptable for a triggered run
//! waiting on an approver who may answer hours later — that needs durable
//! suspension, which `.ai/policy-implementation.md` §10 names as out of scope.

use std::time::Duration;

use flowr_policy::grants::{self, Verdict};
use flowr_policy::{GrantScope, PolicyDecision, Principal};
use serde_json::{json, Value};
use tracing::{info, warn};
use uuid::Uuid;

/// How long a question waits before giving up. Long enough for someone to read
/// it and think; short enough that an abandoned tab does not pin a run forever.
const APPROVAL_TIMEOUT: Duration = Duration::from_secs(300);

/// Scopes offered to the approver.
///
/// `Agent` is filtered out until persisted grants exist (Phase 4). Offering a
/// button that silently records something narrower than it promises is worse
/// than not offering it.
fn offerable(scopes: &[GrantScope]) -> Vec<GrantScope> {
    scopes
        .iter()
        .copied()
        .filter(|s| !matches!(s, GrantScope::Agent))
        .collect()
}

fn parse_scope(s: Option<&str>) -> GrantScope {
    match s {
        Some("once") => GrantScope::Once,
        Some("agent") => GrantScope::Agent,
        // The dev-time default: broad enough to be useful, gone when the
        // conversation ends.
        _ => GrantScope::Session,
    }
}

/// Ask the human, wait, record the answer, and return the resolved decision.
///
/// Returns `Allow` or `Deny` — never `Ask`, because by the time this returns the
/// question has been settled one way or another.
#[allow(clippy::too_many_arguments)]
pub async fn request_approval(
    session_id: &str,
    principal: &Principal,
    server: Option<&str>,
    tool: &str,
    arguments: &Value,
    reason: &str,
    scopes: &[GrantScope],
) -> PolicyDecision {
    let decision_id = flowr_mcp_core::mcp_decision_id(server, tool);
    let key = grants::decision_key(flowr_policy::actions::MCP_CALL, "Tool", &decision_id);
    let offered = offerable(scopes);
    let request_id = Uuid::now_v7().to_string();

    // The approver is the human the agent acts for; with nobody behind it there
    // is no one to ask. The hook should not have produced an Ask in that case,
    // so treat it as a denial rather than hanging.
    let Some(approver) = principal.on_behalf_of().map(str::to_string) else {
        warn!(tool, "approval requested for a principal with no human behind it");
        return PolicyDecision::Deny {
            reason: "approval required, but this run has no interactive approver".into(),
            policy_ids: vec!["no-approver".into()],
        };
    };

    let request = flowr_ce_agents::InputRequest {
        id: request_id.clone(),
        request_type: "authz".to_string(),
        channel: "chat".to_string(),
        title: Some(format!("Allow {tool}?")),
        message: Some(reason.to_string()),
        options: Some(json!(offered.iter().map(|s| s.as_str()).collect::<Vec<_>>())),
        fields: None,
        schema: None,
        // Everything the person needs to judge the request: which tool, on which
        // server, with what arguments. Withholding the arguments would make the
        // prompt unanswerable in any considered way.
        context: Some(json!({
            "tool": tool,
            "server": server,
            "arguments": arguments,
            "decision_id": decision_id,
            "reason": reason,
        })),
        workflow: None,
        agent_id: Some(principal.id().to_string()),
        created_at: chrono::Utc::now().to_rfc3339(),
        expires_at: (chrono::Utc::now() + chrono::Duration::seconds(APPROVAL_TIMEOUT.as_secs() as i64))
            .to_rfc3339(),
        status: flowr_ce_agents::InputStatus::Pending,
    };

    let rx = flowr_ce_agents::input_manager()
        .create_request(request, APPROVAL_TIMEOUT)
        .await;

    // Stream the card into the conversation that raised it.
    crate::token_stream::publish(
        session_id,
        "authz-request",
        json!({
            "request_id": request_id,
            "tool": tool,
            "server": server,
            "arguments": arguments,
            "reason": reason,
            "scopes": offered.iter().map(|s| s.as_str()).collect::<Vec<_>>(),
        })
        .to_string(),
    );
    info!(session_id, tool, request_id, "approval requested, agent suspended");

    let answer = match tokio::time::timeout(APPROVAL_TIMEOUT, rx).await {
        Ok(Ok(resp)) => resp.data,
        // Timed out, or the channel dropped. The question is deliberately NOT
        // recorded: nobody answered it, and remembering a non-answer as a
        // refusal would silently harden into a permanent "no" the user never
        // chose. This call fails; the next one asks again.
        _ => {
            warn!(session_id, tool, request_id, "approval timed out unanswered");
            crate::token_stream::publish(
                session_id,
                "authz-resolved",
                json!({ "request_id": request_id, "allowed": false, "reason": "timed out" })
                    .to_string(),
            );
            return PolicyDecision::Deny {
                reason: format!("approval for {tool} was not answered in time"),
                policy_ids: vec!["approval-timeout".into()],
            };
        }
    };

    let allowed = answer.get("allow").and_then(|v| v.as_bool()).unwrap_or(false);
    let scope = parse_scope(answer.get("scope").and_then(|v| v.as_str()));
    let note = answer
        .get("reason")
        .and_then(|v| v.as_str())
        .unwrap_or(if allowed { "approved in chat" } else { "declined in chat" });

    grants::record(
        session_id,
        grants::answered(
            key,
            flowr_policy::actions::MCP_CALL,
            "Tool",
            &decision_id,
            if allowed { Verdict::Granted } else { Verdict::Denied },
            scope,
            note,
            approver,
        ),
    );

    crate::token_stream::publish(
        session_id,
        "authz-resolved",
        json!({
            "request_id": request_id,
            "allowed": allowed,
            "scope": scope.as_str(),
            "tool": tool,
        })
        .to_string(),
    );
    info!(session_id, tool, allowed, scope = scope.as_str(), "approval answered");

    if allowed {
        PolicyDecision::Allow
    } else {
        PolicyDecision::Deny {
            reason: format!("you declined {tool} in this conversation"),
            policy_ids: vec!["session-denial".into()],
        }
    }
}
