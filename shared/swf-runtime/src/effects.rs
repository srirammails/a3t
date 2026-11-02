//! Effects inside a step (`.ai/FLOWR_QUEUE_CHECKPOINT_SPEC.md` §5.12, fine mode).
//!
//! A handler that makes several outside calls in one step — an agent's model
//! calls and tool calls — runs each through [`effect`]. On a journaled run the
//! step hook records each result under `<step key>/fx/<name>`; when the step
//! runs again after its worker died, recorded effects return their result
//! without being called, so the agent continues from the first unrecorded one.
//!
//! The executor sets the scope around every task (empty when the run has no
//! journal), so a nested flow's steps never record under an outer step.

use std::future::Future;
use std::sync::Arc;

use serde_json::Value;

use crate::error::Result;
use crate::traits::StepHook;

/// How an effect is treated when it started but its result was not recorded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EffectKind {
    /// Safe to call again (a model call): no write-ahead record.
    Read,
    /// Has side effects (a tool call): recorded before it runs; if it never
    /// finished, it is not repeated — the caller is told it is in doubt.
    Effect,
}

/// What the journal knows about an effect.
#[derive(Debug, Clone, PartialEq)]
pub enum EffectReplay {
    /// Not recorded: call it.
    Run,
    /// Recorded: its result.
    Done(std::result::Result<Value, String>),
    /// Started in an earlier attempt, never finished.
    InDoubt,
}

/// The answer to a question put to a person (an approval), as the journal has it.
#[derive(Debug, Clone, PartialEq)]
pub enum Answer {
    /// Answered: the decision (`{allow, scope, reason}`) and who gave it.
    Given { request_id: String, decision: Value, by: String },
    /// Nobody answered before it expired.
    Expired { request_id: String },
}

#[derive(Clone)]
pub struct EffectScope {
    hook: Arc<dyn StepHook>,
    key: String,
}

tokio::task_local! {
    static SCOPE: Option<EffectScope>;
}

/// Run `fut` (one task's body) with its effects recorded under `key`, or not
/// recorded when `hook` is `None`.
pub async fn scoped<F: Future>(hook: Option<Arc<dyn StepHook>>, key: &str, fut: F) -> F::Output {
    SCOPE.scope(hook.map(|hook| EffectScope { hook, key: key.to_string() }), fut).await
}

fn current() -> Option<EffectScope> {
    SCOPE.try_with(|s| s.clone()).ok().flatten()
}

/// Whether the current step is journaled (effects are recorded, waits durable).
pub fn journaled() -> bool {
    current().is_some()
}

/// The outcome of an effect: `Ok(result)` where `result` is what the call
/// returned (or its error, as data), replayed from the journal when recorded.
/// `Err` only when the run must stop (its journal could not be written).
pub async fn effect<F, Fut>(name: &str, kind: EffectKind, call: F) -> Result<(std::result::Result<Value, String>, bool)>
where
    F: FnOnce() -> Fut,
    Fut: Future<Output = std::result::Result<Value, String>>,
{
    let Some(scope) = current() else {
        return Ok((call().await, false));
    };
    match scope.hook.effect_replay(&scope.key, name).await? {
        EffectReplay::Done(result) => return Ok((result, true)),
        EffectReplay::InDoubt if kind == EffectKind::Effect => {
            let msg = format!(
                "the call {name} was interrupted before its result was recorded; it may or may not have taken effect — check before repeating it"
            );
            scope.hook.effect_done(&scope.key, name, kind, &Err(msg.clone()), 0).await?;
            return Ok((Err(msg), false));
        }
        _ => {}
    }
    if kind == EffectKind::Effect {
        scope.hook.effect_started(&scope.key, name).await?;
    }
    let started = std::time::Instant::now();
    // A question put to a person inside this effect is keyed under it.
    let inner = EffectScope { hook: scope.hook.clone(), key: format!("{}/fx/{name}", scope.key) };
    let result = SCOPE.scope(Some(inner), call()).await;
    scope.hook.effect_done(&scope.key, name, kind, &result, started.elapsed().as_micros() as u64).await?;
    Ok((result, false))
}

/// Whether question `name` was already put (so it is not shown twice).
pub fn asked(name: &str) -> bool {
    current().is_some_and(|s| s.hook.asked(&s.key, name))
}

/// Ask a person and wait durably: `None` when this step is not journaled (the
/// caller waits in memory instead). The first call records the question
/// (`request.request_id` names it) and stops the run until it is answered or
/// `expires_at` passes; when the step runs again, the answer is returned.
pub async fn ask(name: &str, request: &Value, expires_at: chrono::DateTime<chrono::Utc>) -> Option<Result<Answer>> {
    let scope = current()?;
    Some(scope.hook.ask(&scope.key, name, request, expires_at).await)
}
