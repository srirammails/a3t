//! Session-scoped authorization decisions — what the user answered when asked.
//!
//! When a [`PolicyDecision::Ask`](crate::PolicyDecision::Ask) is put to a human
//! and answered, the answer is recorded here so the same question is not asked
//! twice in one session. Both answers are recorded, and they are not symmetric:
//!
//! - **Granted** — subsequent matching calls are allowed (or, for
//!   [`GrantScope::Once`], exactly one is).
//! - **Denied** — subsequent matching calls are denied *without re-asking*.
//!   A refusal that kept re-prompting would train people to click through.
//!
//! A denial is **sticky but not final**. It stays listed, with its reason, and
//! can be lifted later — by asking the agent ("what have I denied?") or from the
//! chat tools UI. Lifting is deliberate and separate from answering the original
//! prompt, so nothing silently re-opens.
//!
//! ## Why a ledger and not a per-session Cedar policy set
//!
//! `.ai/policy-implementation.md` §2.3 frames grants as Cedar `permit`s, which
//! holds for the *persisted* `Agent` scope (a registry resource, composed into
//! the policy set and hot-reloaded). Session scope deliberately does not work
//! that way: building a policy set per session would mean parsing and validating
//! Cedar on every session, on a path that runs for **every tool call**. For
//! "allow this action on this resource for this session" a ledger lookup is
//! exactly equivalent in meaning and orders of magnitude cheaper. The semantics
//! are the same; only the representation differs.
//!
//! State is in-memory and dies with the process — correct for session scope,
//! which must not outlive the conversation. Anything that needs to survive a
//! restart belongs in the persisted path.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::GrantScope;

/// How a human answered when asked.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Verdict {
    Granted,
    Denied,
}

/// One recorded answer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Decision {
    /// Stable identity of the question — see [`decision_key`].
    pub key: String,
    pub action: String,
    pub resource_type: String,
    /// The tool or task the question was about, as shown to the user.
    pub resource_id: String,
    pub verdict: Verdict,
    pub scope: GrantScope,
    /// Why it was asked (carried from the `Ask`), or why the user refused.
    pub reason: String,
    /// The principal that answered — always a human.
    pub decided_by: String,
    pub decided_at_ms: u64,
    /// Remaining uses for a [`GrantScope::Once`] grant. `None` = unlimited
    /// within the session.
    pub remaining_uses: Option<u32>,
}

/// The identity of a question, and therefore the unit a grant or denial covers.
///
/// Deliberately **excludes tool arguments**. If arguments were part of the key,
/// an agent could vary one character and re-raise a question the user already
/// refused — turning a sticky denial into a prompt loop, which is precisely the
/// click-through-fatigue failure this design avoids. It also matches the grain a
/// person reasons at: they allow "delete flows", not "delete this one flow".
pub fn decision_key(action: &str, resource_type: &str, resource_id: &str) -> String {
    format!("{action}|{resource_type}|{resource_id}")
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

struct Session {
    decisions: HashMap<String, Decision>,
    touched_ms: u64,
}

/// Sessions idle longer than this are swept. Generous: a conversation left open
/// over lunch should not silently forget what was allowed.
const IDLE_TTL_MS: u64 = 12 * 60 * 60 * 1000;

fn store() -> &'static Mutex<HashMap<String, Session>> {
    static STORE: OnceLock<Mutex<HashMap<String, Session>>> = OnceLock::new();
    STORE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn sweep(map: &mut HashMap<String, Session>) {
    let cutoff = now_ms().saturating_sub(IDLE_TTL_MS);
    map.retain(|_, s| s.touched_ms >= cutoff);
}

/// Record an answer. Replaces any previous answer to the same question, so
/// lifting a denial is just recording a grant over it.
pub fn record(session_id: &str, decision: Decision) {
    let Ok(mut map) = store().lock() else { return };
    sweep(&mut map);
    let entry = map.entry(session_id.to_string()).or_insert_with(|| Session {
        decisions: HashMap::new(),
        touched_ms: now_ms(),
    });
    entry.touched_ms = now_ms();
    entry.decisions.insert(decision.key.clone(), decision);
}

/// What was already answered for this question, if anything.
///
/// A [`GrantScope::Once`] grant is **consumed here**: the call that observes it
/// is the one it authorizes, and it is gone for the next. That makes consumption
/// atomic with the lookup, so two concurrent tool calls cannot both spend it.
pub fn consult(session_id: &str, key: &str) -> Option<Decision> {
    let Ok(mut map) = store().lock() else { return None };
    let session = map.get_mut(session_id)?;
    session.touched_ms = now_ms();
    let decision = session.decisions.get_mut(key)?;

    if decision.verdict == Verdict::Granted {
        if let Some(left) = decision.remaining_uses {
            if left == 0 {
                // Spent. Drop it so the question is OPEN again rather than
                // lingering as a permanent "no" — a one-time grant decaying into
                // a silent denial is a downgrade the user never chose.
                session.decisions.remove(key);
                return None;
            }
            decision.remaining_uses = Some(left - 1);
        }
    }
    Some(decision.clone())
}

/// Every answer given in this session, newest first. Backs "what have I denied?"
/// and the chat tools panel.
pub fn list(session_id: &str) -> Vec<Decision> {
    let Ok(map) = store().lock() else { return Vec::new() };
    let Some(session) = map.get(session_id) else { return Vec::new() };
    let mut out: Vec<Decision> = session.decisions.values().cloned().collect();
    out.sort_by(|a, b| b.decided_at_ms.cmp(&a.decided_at_ms));
    out
}

/// Only the refusals — the list a user asks for by name.
pub fn list_denied(session_id: &str) -> Vec<Decision> {
    list(session_id).into_iter().filter(|d| d.verdict == Verdict::Denied).collect()
}

/// Forget a recorded decision so the question can be asked again.
///
/// Applies to BOTH verdicts, and that symmetry is the point: a grant is as
/// revocable as a refusal is liftable. Someone who allowed a tool for the rest of
/// a conversation must be able to take it back without ending the conversation.
///
/// It re-opens the question rather than answering it the other way — the next
/// matching call prompts — so nothing is silently granted or silently blocked by
/// a click in a list. Returns whether there was anything to forget.
pub fn forget(session_id: &str, key: &str) -> bool {
    let Ok(mut map) = store().lock() else { return false };
    let Some(session) = map.get_mut(session_id) else { return false };
    session.touched_ms = now_ms();
    session.decisions.remove(key).is_some()
}

/// Forget a decision only if it is a refusal. Kept for the denial-specific path.
pub fn lift_denial(session_id: &str, key: &str) -> bool {
    let Ok(mut map) = store().lock() else { return false };
    let Some(session) = map.get_mut(session_id) else { return false };
    session.touched_ms = now_ms();
    match session.decisions.get(key) {
        Some(d) if d.verdict == Verdict::Denied => {
            session.decisions.remove(key);
            true
        }
        _ => false,
    }
}

/// Drop everything for a session — used when a conversation is deleted, so
/// permissions never outlive the transcript that justified them.
pub fn clear(session_id: &str) {
    if let Ok(mut map) = store().lock() {
        map.remove(session_id);
    }
}

/// Build a decision from an answered `Ask`.
pub fn answered(
    key: String,
    action: &str,
    resource_type: &str,
    resource_id: &str,
    verdict: Verdict,
    scope: GrantScope,
    reason: impl Into<String>,
    decided_by: impl Into<String>,
) -> Decision {
    Decision {
        key,
        action: action.to_string(),
        resource_type: resource_type.to_string(),
        resource_id: resource_id.to_string(),
        verdict,
        scope,
        reason: reason.into(),
        decided_by: decided_by.into(),
        decided_at_ms: now_ms(),
        remaining_uses: match (verdict, scope) {
            (Verdict::Granted, GrantScope::Once) => Some(1),
            _ => None,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sess(n: &str) -> String {
        // Unique per test: the store is process-wide.
        format!("session-{n}-{}", now_ms())
    }

    fn grant(scope: GrantScope) -> Decision {
        answered(
            decision_key("mcp:call", "Tool", "delete_flow"),
            "mcp:call", "Tool", "delete_flow",
            Verdict::Granted, scope, "approved in chat", "usr-sriram",
        )
    }

    fn denial() -> Decision {
        answered(
            decision_key("mcp:call", "Tool", "delete_flow"),
            "mcp:call", "Tool", "delete_flow",
            Verdict::Denied, GrantScope::Session, "user refused", "usr-sriram",
        )
    }

    #[test]
    fn a_denial_is_sticky_for_the_session() {
        let s = sess("sticky");
        let key = decision_key("mcp:call", "Tool", "delete_flow");
        record(&s, denial());
        // Every subsequent matching call sees the refusal, so nothing re-asks.
        for _ in 0..5 {
            let d = consult(&s, &key).expect("denial is remembered");
            assert_eq!(d.verdict, Verdict::Denied);
        }
    }

    #[test]
    fn a_denial_stays_visible_and_can_be_lifted() {
        let s = sess("visible");
        let key = decision_key("mcp:call", "Tool", "delete_flow");
        record(&s, denial());

        let denied = list_denied(&s);
        assert_eq!(denied.len(), 1, "a refusal must remain listable");
        assert_eq!(denied[0].resource_id, "delete_flow");
        assert_eq!(denied[0].reason, "user refused");

        assert!(lift_denial(&s, &key), "lifting reports what it did");
        // Lifting RE-OPENS the question rather than answering it: nothing is
        // remembered, so the next matching call prompts again.
        assert!(consult(&s, &key).is_none());
        assert!(list_denied(&s).is_empty());
        assert!(!lift_denial(&s, &key), "lifting a second time is a no-op");
    }

    #[test]
    fn a_session_grant_can_be_revoked() {
        // A grant must be as revocable as a refusal is liftable, or "for this
        // conversation" becomes irreversible short of deleting the conversation.
        let s = sess("revoke");
        let key = decision_key("mcp:call", "Tool", "delete_flow");
        record(&s, grant(GrantScope::Session));
        assert_eq!(consult(&s, &key).unwrap().verdict, Verdict::Granted);

        assert!(forget(&s, &key), "revoking reports what it did");
        // Revoking re-opens the question rather than flipping it to a refusal:
        // the next call asks, it is not silently blocked.
        assert!(consult(&s, &key).is_none());
        assert!(list(&s).is_empty());
        assert!(!forget(&s, &key), "revoking twice is a no-op");
    }

    #[test]
    fn granting_over_a_denial_replaces_it() {
        let s = sess("replace");
        let key = decision_key("mcp:call", "Tool", "delete_flow");
        record(&s, denial());
        record(&s, grant(GrantScope::Session));
        assert_eq!(consult(&s, &key).unwrap().verdict, Verdict::Granted);
        assert!(list_denied(&s).is_empty());
    }

    #[test]
    fn a_once_grant_authorizes_exactly_one_call() {
        let s = sess("once");
        let key = decision_key("mcp:call", "Tool", "delete_flow");
        record(&s, grant(GrantScope::Once));
        assert_eq!(consult(&s, &key).unwrap().verdict, Verdict::Granted);
        // Spent: the question is open again rather than silently becoming a no.
        assert!(consult(&s, &key).is_none(), "a one-time grant must not authorize twice");
    }

    #[test]
    fn a_once_grant_spent_at_approval_time_authorizes_nothing_further() {
        // The approval path returns Allow directly for the call that raised the
        // question, then spends the use. A second call must therefore find the
        // question open again rather than riding the same grant.
        let s = sess("once-spent");
        let key = decision_key("mcp:call", "Tool", "delete_flow");
        record(&s, grant(GrantScope::Once));
        // The approving call spends it.
        assert_eq!(consult(&s, &key).unwrap().verdict, Verdict::Granted);
        // Any later call asks again.
        assert!(consult(&s, &key).is_none(), "a one-time grant must not span two calls");
    }

    #[test]
    fn a_session_grant_keeps_authorizing() {
        let s = sess("session");
        let key = decision_key("mcp:call", "Tool", "delete_flow");
        record(&s, grant(GrantScope::Session));
        for _ in 0..3 {
            assert_eq!(consult(&s, &key).unwrap().verdict, Verdict::Granted);
        }
    }

    #[test]
    fn arguments_do_not_change_the_question() {
        // The key is (action, type, id) only. If args were included, an agent
        // could vary one and re-raise something the user already refused.
        assert_eq!(
            decision_key("mcp:call", "Tool", "delete_flow"),
            decision_key("mcp:call", "Tool", "delete_flow"),
        );
        assert_ne!(
            decision_key("mcp:call", "Tool", "delete_flow"),
            decision_key("mcp:call", "Tool", "create_flow"),
        );
    }

    #[test]
    fn listing_does_not_spend_a_one_time_grant() {
        // The tool list shows permission state. If listing consumed a Once
        // grant, opening the tools panel would silently burn a permission the
        // user had not used — hence the read-only peek in tool_permission.
        let s = sess("peek");
        let key = decision_key("mcp:call", "Tool", "delete_flow");
        record(&s, grant(GrantScope::Once));
        assert_eq!(list(&s).len(), 1);
        assert_eq!(list(&s).len(), 1, "listing twice must not consume it");
        // Still spendable exactly once.
        assert_eq!(consult(&s, &key).unwrap().verdict, Verdict::Granted);
        assert!(consult(&s, &key).is_none());
    }

    #[test]
    fn sessions_do_not_leak_into_each_other() {
        let a = sess("a");
        let b = sess("b");
        let key = decision_key("mcp:call", "Tool", "delete_flow");
        record(&a, grant(GrantScope::Session));
        assert!(consult(&a, &key).is_some());
        assert!(consult(&b, &key).is_none(), "one conversation's answer must not bind another");
    }

    #[test]
    fn clearing_a_session_forgets_its_permissions() {
        let s = sess("clear");
        let key = decision_key("mcp:call", "Tool", "delete_flow");
        record(&s, grant(GrantScope::Session));
        clear(&s);
        assert!(consult(&s, &key).is_none());
        assert!(list(&s).is_empty());
    }
}
