//! How an answered permission question binds the rest of the session.
//!
//! Rules under test (.ai/policy-implementation.md §7, refined):
//!   - a refusal is STICKY: matching calls are denied without re-asking;
//!   - a refusal is VISIBLE and LIFTABLE, so sticky is not final;
//!   - a grant short-circuits the prompt;
//!   - none of this can overturn a policy `Deny` — session answers resolve an
//!     `Ask`, they are not a way to out-vote the org policy.

use flowr_mcp_core::apply_session_decisions;
use flowr_policy::grants::{self, decision_key, Verdict};
use flowr_policy::{GrantScope, PolicyDecision};

fn ask() -> PolicyDecision {
    PolicyDecision::Ask {
        reason: "outside this agent's grants".into(),
        policy_ids: vec!["baseline".into()],
        scopes: vec![GrantScope::Once, GrantScope::Session],
    }
}

fn resolve(session: &str, d: PolicyDecision) -> PolicyDecision {
    apply_session_decisions(session, d, "mcp:call", "Tool", "delete_flow")
}

fn key() -> String {
    decision_key("mcp:call", "Tool", "delete_flow")
}

fn sid(tag: &str) -> String {
    format!("sess-{tag}-{}", std::process::id())
}

#[test]
fn an_unanswered_question_still_prompts() {
    let s = sid("fresh");
    assert!(resolve(&s, ask()).is_ask(), "nothing recorded yet, so it must ask");
}

#[test]
fn a_refusal_denies_without_re_asking() {
    let s = sid("refused");
    grants::record(&s, grants::answered(
        key(), "mcp:call", "Tool", "delete_flow",
        Verdict::Denied, GrantScope::Session, "not right now", "usr-sriram",
    ));

    for _ in 0..3 {
        match resolve(&s, ask()) {
            PolicyDecision::Deny { reason, policy_ids } => {
                assert!(reason.contains("refused this earlier"), "{reason}");
                // The reason must point the user at the way back.
                assert!(reason.contains("denied"), "should mention reviewing denials: {reason}");
                assert_eq!(policy_ids, vec!["session-denial"]);
            }
            other => panic!("a refusal must stick, not re-ask — got {other:?}"),
        }
    }
    grants::clear(&s);
}

#[test]
fn a_refusal_stays_visible_and_lifting_reopens_the_question() {
    let s = sid("liftable");
    grants::record(&s, grants::answered(
        key(), "mcp:call", "Tool", "delete_flow",
        Verdict::Denied, GrantScope::Session, "not right now", "usr-sriram",
    ));
    let denied = grants::list_denied(&s);
    assert_eq!(denied.len(), 1, "the user must be able to see what they refused");
    assert_eq!(denied[0].resource_id, "delete_flow");

    assert!(grants::lift_denial(&s, &key()));
    assert!(resolve(&s, ask()).is_ask(), "lifting reopens the question rather than granting it");
    grants::clear(&s);
}

#[test]
fn a_grant_short_circuits_the_prompt() {
    let s = sid("granted");
    grants::record(&s, grants::answered(
        key(), "mcp:call", "Tool", "delete_flow",
        Verdict::Granted, GrantScope::Session, "approved", "usr-sriram",
    ));
    assert!(resolve(&s, ask()).is_allow());
    grants::clear(&s);
}

#[test]
fn a_session_grant_cannot_overturn_a_policy_denial() {
    // The containment rule. Session answers resolve an Ask; they are not a
    // channel for out-voting the organization's policy. A grant recorded against
    // a tool the policy flatly denies must leave that denial untouched.
    let s = sid("contained");
    grants::record(&s, grants::answered(
        key(), "mcp:call", "Tool", "delete_flow",
        Verdict::Granted, GrantScope::Session, "approved", "usr-sriram",
    ));
    let hard_deny = PolicyDecision::Deny {
        reason: "cedar denied".into(),
        policy_ids: vec!["baseline".into()],
    };
    match resolve(&s, hard_deny) {
        PolicyDecision::Deny { policy_ids, .. } => assert_eq!(policy_ids, vec!["baseline"]),
        other => panic!("a session grant must not overturn a policy Deny — got {other:?}"),
    }
    grants::clear(&s);
}

#[test]
fn an_allow_is_left_alone() {
    let s = sid("allowed");
    assert!(resolve(&s, PolicyDecision::Allow).is_allow());
}
