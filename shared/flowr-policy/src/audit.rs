//! Decision audit trail (E2 #1).
//!
//! Every policy decision — allow *and* deny — is recorded here as it is made at
//! the choke point. This is the CE-side, queryable audit log (a bounded
//! in-memory ring buffer exposed via `GET /security/audit`). The E0.5 `aesop.*`
//! `tool_call` spans remain the canonical tracing/OTel event source; EE's
//! richer exporter (aesop-tower `AuditLayer`, retention + PII redaction) is a
//! second subscriber layered on top — this buffer is the always-on baseline.

use std::collections::VecDeque;
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;

use crate::{ExecutionRequest, PolicyDecision};

/// One authorization decision, flattened for the audit log.
#[derive(Debug, Clone, Serialize)]
pub struct AuditRecord {
    /// Unix milliseconds when the decision was recorded.
    pub ts_ms: u64,
    pub principal_kind: String,
    pub principal_id: String,
    pub on_behalf_of: Option<String>,
    pub action: String,
    pub resource_type: String,
    pub resource_id: String,
    /// `"allow"` or `"deny"`.
    pub decision: String,
    pub policy_ids: Vec<String>,
    pub reason: Option<String>,
}

/// Ring-buffer capacity (records; oldest evicted).
const CAPACITY: usize = 1000;

static BUFFER: OnceLock<Mutex<VecDeque<AuditRecord>>> = OnceLock::new();

fn buffer() -> &'static Mutex<VecDeque<AuditRecord>> {
    BUFFER.get_or_init(|| Mutex::new(VecDeque::with_capacity(CAPACITY)))
}

/// Record a decision. Called at the choke point right after `check()`.
pub fn record(req: &ExecutionRequest, decision: &PolicyDecision) {
    let ts_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    let (decision_s, policy_ids, reason) = match decision {
        PolicyDecision::Allow => ("allow".to_string(), Vec::new(), None),
        PolicyDecision::Deny { reason, policy_ids } => {
            ("deny".to_string(), policy_ids.clone(), Some(reason.clone()))
        }
        // An `Ask` is audited at the moment it is raised, separately from
        // whatever the human then answers — so the trail shows both that
        // approval was demanded and, later, what was granted.
        PolicyDecision::Ask { reason, policy_ids, .. } => {
            ("ask".to_string(), policy_ids.clone(), Some(reason.clone()))
        }
    };
    let rec = AuditRecord {
        ts_ms,
        principal_kind: req.principal.kind().to_string(),
        principal_id: req.principal.id().to_string(),
        on_behalf_of: req.principal.on_behalf_of().map(String::from),
        action: req.action.as_str().to_string(),
        resource_type: req.resource.type_name.clone(),
        resource_id: req.resource.id.clone(),
        decision: decision_s,
        policy_ids,
        reason,
    };
    if let Ok(mut b) = buffer().lock() {
        if b.len() >= CAPACITY {
            b.pop_front();
        }
        b.push_back(rec);
    }
}

/// The most recent `limit` decisions, newest first.
pub fn recent(limit: usize) -> Vec<AuditRecord> {
    match buffer().lock() {
        Ok(b) => b.iter().rev().take(limit).cloned().collect(),
        Err(_) => Vec::new(),
    }
}

/// Recent decisions filtered to denials only (the security-relevant subset).
pub fn recent_denials(limit: usize) -> Vec<AuditRecord> {
    match buffer().lock() {
        Ok(b) => b
            .iter()
            .rev()
            .filter(|r| r.decision == "deny")
            .take(limit)
            .cloned()
            .collect(),
        Err(_) => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{actions, AgentId, Principal, Resource};

    #[test]
    fn records_allow_and_deny() {
        let req = ExecutionRequest {
            principal: Principal::Agent {
                agent_id: AgentId("agt-1".into()),
                on_behalf_of: Some("user::sriram".into()),
                flow_id: None,
                roles: vec![],
                owner: None,
            },
            action: actions::tool_invoke(),
            resource: Resource::tool("call:rotate"),
            context: Default::default(),
            attributes: Default::default(),
            params: serde_json::json!({}),
        };
        record(&req, &PolicyDecision::Allow);
        record(
            &req,
            &PolicyDecision::Deny {
                reason: "destructive".into(),
                policy_ids: vec!["policy3".into()],
            },
        );
        let recent = recent(10);
        assert!(recent.len() >= 2);
        // newest first: the deny
        let deny = &recent[0];
        assert_eq!(deny.decision, "deny");
        assert_eq!(deny.principal_kind, "agent");
        assert_eq!(deny.on_behalf_of.as_deref(), Some("user::sriram"));
        assert_eq!(deny.resource_id, "call:rotate");
        assert_eq!(deny.policy_ids, vec!["policy3".to_string()]);
        assert!(recent_denials(10).iter().all(|r| r.decision == "deny"));
    }
}
