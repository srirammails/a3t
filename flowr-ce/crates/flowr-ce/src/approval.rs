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
//! **Durable on a journaled run** (limitation L1): the question is recorded in
//! the engine store (`ApprovalRequested`, a wait), the run lets go, and the
//! answer — through the same `POST /inputs/:id`, on any console process —
//! journals `ApprovalDecided` and wakes the run, which replays the agent's
//! recorded calls and continues. It survives restarts and works for runs on
//! runtime pods; it waits `FLOWR_APPROVAL_TIMEOUT_SECS` (default a day).
//!
//! A run without a journal (ephemeral, or `FLOWR_STORE=lance`) still waits
//! in memory through `InputManager` for five minutes, as before.

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

    let answer = match durable_answer(session_id, principal, &approver, server, tool, arguments, reason, &decision_id, &offered).await {
        Some(Durable::Answered(request_id, answer)) => Some((request_id, answer)),
        Some(Durable::Expired(request_id)) => return timed_out(session_id, tool, &request_id),
        Some(Durable::Waiting) => {
            // The run has let go; this result is never used.
            return PolicyDecision::Deny { reason: "waiting for approval".into(), policy_ids: vec!["approval-pending".into()] };
        }
        None => None,
    };
    let (request_id, answer) = match answer {
        Some(a) => a,
        None => match in_memory_answer(session_id, principal, &approver, server, tool, arguments, reason, &decision_id, &offered).await {
            Ok(a) => a,
            Err(request_id) => return timed_out(session_id, tool, &request_id),
        },
    };
    settle(session_id, &key, &decision_id, &approver, tool, &request_id, &answer)
}

/// The question as streamed to the chat and listed for the approver.
#[allow(clippy::too_many_arguments)]
fn question(
    request_id: &str,
    session_id: &str,
    principal: &Principal,
    approver: &str,
    server: Option<&str>,
    tool: &str,
    arguments: &Value,
    reason: &str,
    decision_id: &str,
    offered: &[GrantScope],
) -> Value {
    json!({
        "request_id": request_id,
        "request_type": "authz",
        "title": format!("Allow {tool}?"),
        "message": reason,
        "options": offered.iter().map(|s| s.as_str()).collect::<Vec<_>>(),
        "agent_id": principal.id(),
        "context": {
            "tool": tool,
            "server": server,
            "arguments": arguments,
            "decision_id": decision_id,
            "reason": reason,
            "approver": approver,
            "session_id": session_id,
        },
    })
}

fn publish_card(session_id: &str, request_id: &str, server: Option<&str>, tool: &str, arguments: &Value, reason: &str, offered: &[GrantScope]) {
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
}

enum Durable {
    Answered(String, Value),
    Expired(String),
    Waiting,
}

/// On a journaled run: the recorded answer, or record the question and stop
/// the run until it comes. `None` when the run has no journal.
#[allow(clippy::too_many_arguments)]
async fn durable_answer(
    session_id: &str,
    principal: &Principal,
    approver: &str,
    server: Option<&str>,
    tool: &str,
    arguments: &Value,
    reason: &str,
    decision_id: &str,
    offered: &[GrantScope],
) -> Option<Durable> {
    if !swf_runtime::effects::journaled() {
        return None;
    }
    let fresh = !swf_runtime::effects::asked("ask");
    let request_id = Uuid::now_v7().to_string();
    let request = question(&request_id, session_id, principal, approver, server, tool, arguments, reason, decision_id, offered);
    let secs: i64 = std::env::var("FLOWR_APPROVAL_TIMEOUT_SECS").ok().and_then(|v| v.parse().ok()).unwrap_or(86_400);
    let expires_at = chrono::Utc::now() + chrono::Duration::seconds(secs);
    if fresh {
        // Shown once, when first asked; `ask` records it with this id.
        publish_card(session_id, &request_id, server, tool, arguments, reason, offered);
        info!(session_id, tool, request_id, "approval requested; the run waits for it durably");
    }
    match swf_runtime::effects::ask("ask", &request, expires_at).await? {
        Ok(swf_runtime::effects::Answer::Given { request_id, decision, .. }) => Some(Durable::Answered(request_id, decision)),
        Ok(swf_runtime::effects::Answer::Expired { request_id }) => Some(Durable::Expired(request_id)),
        Err(_) => Some(Durable::Waiting),
    }
}

/// Without a journal: wait in this process (`InputManager`), five minutes.
#[allow(clippy::too_many_arguments)]
async fn in_memory_answer(
    session_id: &str,
    principal: &Principal,
    approver: &str,
    server: Option<&str>,
    tool: &str,
    arguments: &Value,
    reason: &str,
    decision_id: &str,
    offered: &[GrantScope],
) -> Result<(String, Value), String> {
    let request_id = Uuid::now_v7().to_string();
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
            // Who may answer. An approval is a security decision, so the
            // endpoint verifies the responder against this rather than trusting
            // whoever holds the request id.
            "approver": approver,
            "session_id": session_id,
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
    publish_card(session_id, &request_id, server, tool, arguments, reason, offered);
    info!(session_id, tool, request_id, "approval requested, agent suspended");

    match tokio::time::timeout(APPROVAL_TIMEOUT, rx).await {
        Ok(Ok(resp)) => Ok((request_id, resp.data)),
        // Timed out, or the channel dropped.
        _ => Err(request_id),
    }
}

/// Nobody answered in time. The question is deliberately NOT recorded as a
/// refusal: remembering a non-answer would silently harden into a permanent
/// "no" the user never chose. This call fails; the next one asks again.
fn timed_out(session_id: &str, tool: &str, request_id: &str) -> PolicyDecision {
    warn!(session_id, tool, request_id, "approval timed out unanswered");
    crate::token_stream::publish(
        session_id,
        "authz-resolved",
        json!({ "request_id": request_id, "allowed": false, "reason": "timed out" }).to_string(),
    );
    PolicyDecision::Deny {
        reason: format!("approval for {tool} was not answered in time"),
        policy_ids: vec!["approval-timeout".into()],
    }
}

/// Record the answer in the session ledger and turn it into a decision.
fn settle(session_id: &str, key: &str, decision_id: &str, approver: &str, tool: &str, request_id: &str, answer: &Value) -> PolicyDecision {
    let allowed = answer.get("allow").and_then(|v| v.as_bool()).unwrap_or(false);
    let scope = parse_scope(answer.get("scope").and_then(|v| v.as_str()));
    let note = answer
        .get("reason")
        .and_then(|v| v.as_str())
        .unwrap_or(if allowed { "approved in chat" } else { "declined in chat" });

    grants::record(
        session_id,
        grants::answered(
            key.to_string(),
            flowr_policy::actions::MCP_CALL,
            "Tool",
            decision_id,
            if allowed { Verdict::Granted } else { Verdict::Denied },
            scope,
            note,
            approver,
        ),
    );

    // Spend a one-time grant on the call that asked for it.
    //
    // This function returns `Allow` straight to the caller rather than going back
    // through `consult`, so without this the single use was still unspent and the
    // NEXT call consumed it — "just this once" quietly authorized two calls.
    if allowed && scope == GrantScope::Once {
        let _ = grants::consult(session_id, key);
    }

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
