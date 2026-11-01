//! The step hook (`.ai/FLOWR_QUEUE_CHECKPOINT_SPEC.md` §5.5): every task is
//! reported with a stable key, and a replayed task is not executed again —
//! its recorded output, context and flow directive are used instead.

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use serde_json::{json, Value};
use swf_parser::{parse_yaml, NamedTask};
use swf_runtime::error::Result;
use swf_runtime::{
    ExecutionContext, FlowDirective, Principal, RuntimeInfo, StepHook, StepOutcome, StepReplay, TaskHandler, TaskResult,
    WorkflowExecutor,
};

struct Count(Arc<AtomicUsize>);

#[async_trait]
impl TaskHandler for Count {
    fn task_type(&self) -> &str {
        "call:count"
    }
    async fn execute(&self, _task: &NamedTask, _input: Value, _ctx: &mut ExecutionContext) -> Result<TaskResult> {
        let n = self.0.fetch_add(1, Ordering::SeqCst) + 1;
        Ok(TaskResult { output: json!({ "n": n }), directive: FlowDirective::Continue, exports: None })
    }
}

#[derive(Default)]
struct Journal {
    /// key → (output, context, directive)
    done: Mutex<HashMap<String, (Value, Value, String)>>,
    order: Mutex<Vec<String>>,
    replay: Mutex<HashSet<String>>,
    replayed: Mutex<Vec<String>>,
}

#[async_trait]
impl StepHook for Journal {
    async fn before_step(&self, key: &str, _task: &NamedTask, _input: &Value) -> Result<StepReplay> {
        if self.replay.lock().unwrap().contains(key) {
            if let Some((output, context, directive)) = self.done.lock().unwrap().get(key).cloned() {
                self.replayed.lock().unwrap().push(key.to_string());
                return Ok(StepReplay::Completed { output, context: Some(context), directive });
            }
        }
        Ok(StepReplay::Run)
    }

    async fn after_step(&self, key: &str, _task: &NamedTask, _input: &Value, outcome: StepOutcome<'_>, _us: u64) -> Result<()> {
        self.order.lock().unwrap().push(key.to_string());
        if let StepOutcome::Completed { output, context, directive } = outcome {
            let mut c = context.clone();
            c.as_object_mut().map(|o| o.remove("_principal"));
            self.done.lock().unwrap().insert(key.to_string(), (output.clone(), c, directive.to_string()));
        }
        Ok(())
    }
}

fn ctx() -> ExecutionContext {
    ExecutionContext {
        instance_id: "run-1".into(),
        namespace: "t".into(),
        name: "w".into(),
        version: "1.0.0".into(),
        context: Value::Null,
        secrets: Default::default(),
        runtime: RuntimeInfo::default(),
        storage: None,
        http_client: None,
        extensions: Vec::new(),
        eval_fn: None,
        principal: Principal::human("usr-ann", vec!["Viewer".into()]),
        template_meta: Default::default(),
    }
}

async fn run(yaml: &str, counter: Arc<AtomicUsize>, hook: Arc<Journal>) -> Value {
    let wf = parse_yaml(yaml).unwrap();
    let mut ex = WorkflowExecutor::new();
    ex.register_handler(Arc::new(Count(counter)));
    ex.set_eval_fn(|input, expr, context| {
        let engine = flowr_expr::ExpressionEngine::new();
        let scope = flowr_expr::Scope::builder().context(context.clone()).build();
        engine.eval(input, expr, &scope).map_err(|e| e.to_string())
    });
    ex.set_step_hook(hook);
    ex.execute(&wf, json!({}), &mut ctx()).await.unwrap()
}

const STEPS: &str = r#"
document: { dsl: '1.0.3', namespace: t, name: w, version: '1.0.0' }
do:
  - a:
      set: { x: 7 }
      export: { as: "{x: .x}" }
  - b:
      call: count
      export: { as: "$context + {n: .n}" }
  - c:
      set:
        x: "${ $context.x }"
        n: "${ $context.n }"
        who: "${ $context._principal.id }"
"#;

#[tokio::test]
async fn replayed_steps_do_not_run_again() {
    let counter = Arc::new(AtomicUsize::new(0));
    let first = Arc::new(Journal::default());
    let out1 = run(STEPS, counter.clone(), first.clone()).await;
    assert_eq!(out1, json!({ "x": 7, "n": 1, "who": "usr-ann" }));
    assert_eq!(*first.order.lock().unwrap(), vec!["t/w/a", "t/w/b", "t/w/c"], "every task is reported, in order");
    assert_eq!(counter.load(Ordering::SeqCst), 1);

    // A resume after `b` completed: a and b replay, c runs.
    let resume = Arc::new(Journal::default());
    *resume.done.lock().unwrap() = first.done.lock().unwrap().clone();
    *resume.replay.lock().unwrap() = ["t/w/a".to_string(), "t/w/b".to_string()].into();
    let out2 = run(STEPS, counter.clone(), resume.clone()).await;
    assert_eq!(counter.load(Ordering::SeqCst), 1, "the effect in b is not repeated");
    assert_eq!(*resume.replayed.lock().unwrap(), vec!["t/w/a", "t/w/b"]);
    assert_eq!(out2, out1, "c sees the replayed context, and the principal comes from the run, not the record");
}

const LOOP: &str = r#"
document: { dsl: '1.0.3', namespace: t, name: w, version: '1.0.0' }
do:
  - start:
      set: { i: 0 }
      export: { as: "{i: .i}" }
  - inc:
      call: count
      export: { as: "$context + {i: ($context.i + 1)}" }
  - check:
      switch:
        - more:
            when: "${ $context.i < 3 }"
            then: inc
        - done:
            then: continue
  - fin:
      set: { i: "${ $context.i }" }
"#;

#[tokio::test]
async fn revisits_are_their_own_steps() {
    let counter = Arc::new(AtomicUsize::new(0));
    let first = Arc::new(Journal::default());
    let out = run(LOOP, counter.clone(), first.clone()).await;
    assert_eq!(out["i"], 3);
    let order = first.order.lock().unwrap().clone();
    for k in ["t/w/inc", "t/w/inc#2", "t/w/inc#3", "t/w/check#3"] {
        assert!(order.contains(&k.to_string()), "{k} in {order:?}");
    }

    // Crash after the second visit: visits 1–2 replay, the third runs.
    let resume = Arc::new(Journal::default());
    *resume.done.lock().unwrap() = first.done.lock().unwrap().clone();
    *resume.replay.lock().unwrap() =
        ["t/w/start", "t/w/inc", "t/w/check", "t/w/inc#2", "t/w/check#2"].iter().map(|s| s.to_string()).collect();
    let before = counter.load(Ordering::SeqCst);
    let out2 = run(LOOP, counter.clone(), resume).await;
    assert_eq!(counter.load(Ordering::SeqCst) - before, 1, "only the third visit runs");
    assert_eq!(out2["i"], 3);
}
