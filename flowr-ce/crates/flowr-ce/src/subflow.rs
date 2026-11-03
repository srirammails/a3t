//! Subflows journaled under their calling step (L2): a `run: workflow` step,
//! or an agent's tool flow, gives the nested flow's executor the run's own
//! step hook with every key moved under the calling step
//! (`<step>/sub/<child step>`). When the run resumes after a crash, the
//! calling step runs again and the child's finished steps replay from the
//! journal, so the child continues where it stopped instead of starting over
//! (or the parent faulting in doubt); a durable wait inside the child parks
//! the whole run.

use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde_json::Value;
use swf_parser::NamedTask;
use swf_runtime::effects::{Answer, EffectKind, EffectReplay};
use swf_runtime::error::Result;
use swf_runtime::traits::{StepHook, StepOutcome, StepReplay};

/// The run's hook, seen from inside a nested flow.
pub struct SubflowHook {
    inner: Arc<dyn StepHook>,
    /// The calling step's path (as the outer executor names it).
    parent: String,
    /// The nested flow's own path prefix (`namespace/name`).
    child: String,
}

impl SubflowHook {
    fn map(&self, path: &str) -> String {
        let rest = path.strip_prefix(self.child.as_str()).unwrap_or(path);
        let rest = if rest.starts_with('/') { rest.to_string() } else { format!("/{rest}") };
        format!("{}/sub{rest}", self.parent)
    }
}

/// Journal the nested flow `namespace/name` under the current step, when the
/// run is journaled; otherwise it runs unrecorded, as before.
pub fn attach(executor: &mut swf_runtime::WorkflowExecutor, namespace: &str, name: &str) -> bool {
    match swf_runtime::effects::current_step() {
        Some((inner, parent)) => {
            executor.set_step_hook(Arc::new(SubflowHook { inner, parent, child: format!("{namespace}/{name}") }));
            true
        }
        None => false,
    }
}

#[async_trait]
impl StepHook for SubflowHook {
    async fn before_step(&self, key: &str, task: &NamedTask, input: &Value) -> Result<StepReplay> {
        self.inner.before_step(&self.map(key), task, input).await
    }

    async fn after_step(&self, key: &str, task: &NamedTask, input: &Value, outcome: StepOutcome<'_>, duration_us: u64) -> Result<()> {
        self.inner.after_step(&self.map(key), task, input, outcome, duration_us).await
    }

    fn idempotency_key(&self, key: &str) -> Option<String> {
        self.inner.idempotency_key(&self.map(key))
    }

    async fn effect_replay(&self, key: &str, name: &str) -> Result<EffectReplay> {
        self.inner.effect_replay(&self.map(key), name).await
    }

    async fn effect_started(&self, key: &str, name: &str) -> Result<()> {
        self.inner.effect_started(&self.map(key), name).await
    }

    async fn effect_done(&self, key: &str, name: &str, kind: EffectKind, result: &std::result::Result<Value, String>, duration_us: u64) -> Result<()> {
        self.inner.effect_done(&self.map(key), name, kind, result, duration_us).await
    }

    fn asked(&self, key: &str, name: &str) -> bool {
        self.inner.asked(&self.map(key), name)
    }

    async fn ask(&self, key: &str, name: &str, request: &Value, expires_at: DateTime<Utc>) -> Result<Answer> {
        self.inner.ask(&self.map(key), name, request, expires_at).await
    }
}
