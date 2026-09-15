//! Inline AESOP policy declared on a flow step (`metadata.a3t.policy`).
//!
//! By default every `TaskHandler` is gated by Cedar as an undifferentiated
//! `Tool{task_type}` / `tool:invoke` (see
//! `swf_runtime::traits::TaskHandler::policy_resource`) — so a read and a delete
//! through the same handler look identical to a policy.
//!
//! Rather than each handler hand-writing a `policy_resource` override in Rust,
//! the step declares what it is doing in the SWF YAML and the executor choke
//! point turns that into Cedar attributes. This works for *any* dispatched task
//! — `call:*` and `run:*` today — with no per-handler code, so a new backend is
//! policy-gated the moment a flow annotates it.
//!
//! `metadata` is a CNCF SWF `taskBase` property (`TaskMetadata`,
//! `additionalProperties: true`), so this is spec-compliant on every task type;
//! a non-AESOP runtime parses the flow and ignores the annotation.
//!
//! ```yaml
//! - delete-template:
//!     call: lancedb
//!     with: { operation: delete, table: template_registry }
//!     metadata:
//!       a3t:
//!         policy:
//!           op: delete
//!           resource:
//!             type: Template
//!             id: ${ .input.id }
//!             attrs: { table: template_registry }
//! ```

use crate::Resource;

/// Coarse, Cedar-facing operation bucket. Policies gate on this bucket, never
/// on the raw per-backend verb string (which varies per handler/backend).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Op {
    Read,
    Write,
    Delete,
    Execute,
}

impl Op {
    pub fn as_str(self) -> &'static str {
        match self {
            Op::Read => "read",
            Op::Write => "write",
            Op::Delete => "delete",
            Op::Execute => "execute",
        }
    }

    pub fn parse(s: &str) -> Option<Op> {
        match s {
            "read" => Some(Op::Read),
            "execute" => Some(Op::Execute),
            "write" => Some(Op::Write),
            "delete" => Some(Op::Delete),
            _ => None,
        }
    }

    /// Severity ranking used by the attenuate-only merge. A YAML annotation may
    /// only *raise* a step's bucket, never lower it — so a flow author cannot
    /// label a delete as a read and hand the flow to a Viewer.
    pub fn rank(self) -> u8 {
        match self {
            Op::Read => 0,
            Op::Execute => 1,
            Op::Write => 2,
            Op::Delete => 3,
        }
    }
}

/// Attributes the *handler* owns. A YAML annotation can never set or overwrite
/// these — otherwise a flow author could write `destructive: false` and walk
/// straight through the destructive-tool guardrail, or relabel an MCP tool's
/// `name` to impersonate a permitted one.
pub const RESERVED_ATTRS: &[&str] =
    &["destructive", "name", "operation", "task_type", "type"];

/// The `metadata.a3t.policy` annotation on a single flow step — inline AESOP
/// policy declared in the SWF YAML instead of in per-handler Rust:
///
/// ```yaml
/// - delete-template:
///     call: lancedb
///     with: { operation: delete, table: template_registry }
///     metadata:
///       a3t:
///         policy:
///           op: delete
///           resource:
///             type: Template
///             id: ${ .input.id }
///             attrs: { table: template_registry }
/// ```
///
/// The annotation is **attenuate-only**, matching the existing `metadata.a3t`
/// capability-composition rule (`exposedTools`/`enabledTools` INTERSECT): it may
/// raise a step's operation bucket and add identity/attributes for policy to
/// read, but it can never lower the bucket, rename the entity, or overwrite a
/// [`RESERVED_ATTRS`] key the handler set. So an annotation can only ever make a
/// step *more* restricted than the compiled handler already made it.
#[derive(Debug, Clone, Default)]
pub struct StepPolicy {
    pub op: Option<Op>,
    pub object_type: Option<String>,
    pub object_id: Option<String>,
    pub attrs: serde_json::Map<String, serde_json::Value>,
}

impl StepPolicy {
    /// Parse an already-expression-evaluated `metadata.a3t.policy` value.
    /// Returns `None` when the step carries no annotation.
    pub fn parse(policy: &serde_json::Value) -> Option<StepPolicy> {
        let obj = policy.as_object()?;
        let resource = obj.get("resource").and_then(|r| r.as_object());
        let attrs = resource
            .and_then(|r| r.get("attrs"))
            .and_then(|a| a.as_object())
            .cloned()
            .unwrap_or_default();
        // An `op` that is present but not a recognized bucket is an authoring
        // error (`op: delet`, `op: writes`). It must fail *closed* to the most
        // restrictive bucket, never silently vanish — a missing `operation`
        // attribute matches no operation-gated policy, which would quietly
        // disable the gate on exactly the step the author meant to protect.
        // An absent `op` is different and stays absent: an annotation may
        // legitimately carry only resource identity.
        let op = match obj.get("op") {
            Some(v) => Some(v.as_str().and_then(Op::parse).unwrap_or(Op::Delete)),
            None => None,
        };
        Some(StepPolicy {
            op,
            object_type: resource
                .and_then(|r| r.get("type"))
                .and_then(|v| v.as_str())
                .map(str::to_string),
            object_id: resource
                .and_then(|r| r.get("id"))
                .and_then(|v| v.as_str())
                .map(str::to_string),
            attrs,
        })
    }

    /// Read the annotation straight off a task's `metadata` value
    /// (`metadata.a3t.policy`). The value must already have had its `${ … }`
    /// expressions evaluated by the caller.
    pub fn from_metadata(metadata: Option<&serde_json::Value>) -> Option<StepPolicy> {
        StepPolicy::parse(metadata?.get("a3t")?.get("policy")?)
    }

    /// Overlay this annotation onto the handler-produced resource, attenuate-only.
    pub fn apply(&self, mut base: Resource) -> Resource {
        // Operation: take the more severe of handler-classified and declared.
        // A handler that classified nothing simply inherits the declaration —
        // which only ever adds a restriction, since a resource with no
        // `operation` attribute matches no operation-gated policy at all.
        let declared = self.op;
        let current = base
            .attributes
            .get("operation")
            .and_then(|v| v.as_str())
            .and_then(Op::parse);
        let effective = match (current, declared) {
            (Some(c), Some(d)) => Some(if d.rank() > c.rank() { d } else { c }),
            (c, d) => c.or(d),
        };
        if let Some(op) = effective {
            base = base.with_attr("operation", op.as_str());
        }

        // Identity rides as *attributes*, never as the entity id — overriding the
        // id would let an annotation rename `Tool{delete_flow}` to a benign id and
        // slip past a policy that names it.
        if let Some(t) = &self.object_type {
            base = base.with_attr("object_type", t.clone());
        }
        if let Some(id) = &self.object_id {
            base = base.with_attr("object_id", id.clone());
        }
        for (k, v) in &self.attrs {
            if !RESERVED_ATTRS.contains(&k.as_str()) {
                base = base.with_attr(k.clone(), v.clone());
            }
        }
        base
    }
}

/// Task types whose steps reach the executor's policy choke point, and on which
/// a `metadata.a3t.policy` annotation is therefore enforced. Everything else in
/// the CNCF task set (`set`, `switch`, `do`, `for`, `try`, `fork`, `raise`,
/// `wait`) is pure control-flow or data shaping with no external effect, and is
/// executed inline without a policy check — so an annotation there would be
/// silently ignored. [`validate_annotation_placement`] rejects that at
/// registration time rather than letting it fail open.
pub const ENFORCED_TASK_KEYS: &[&str] = &["call", "run"];

/// Walk a parsed workflow document and reject any `metadata.a3t.policy`
/// annotation on a task that will never be policy-checked. Returns the offending
/// task paths. Recurses into the nested task lists of container tasks, so an
/// annotation on a `for` loop is caught while the `call` steps inside it are not.
pub fn validate_annotation_placement(workflow: &serde_json::Value) -> Vec<String> {
    fn has_policy(task: &serde_json::Value) -> bool {
        task.get("metadata")
            .and_then(|m| m.get("a3t"))
            .and_then(|a| a.get("policy"))
            .is_some()
    }
    fn walk(tasks: &serde_json::Value, path: &str, bad: &mut Vec<String>) {
        let Some(list) = tasks.as_array() else { return };
        for entry in list {
            let Some(obj) = entry.as_object() else { continue };
            for (name, task) in obj {
                let here = if path.is_empty() {
                    name.clone()
                } else {
                    format!("{path}/{name}")
                };
                let enforced = ENFORCED_TASK_KEYS.iter().any(|k| task.get(*k).is_some());
                if has_policy(task) && !enforced {
                    bad.push(here.clone());
                }
                // A malformed `op` still fails closed at runtime, but the author
                // should hear about it at registration rather than discover it
                // as an unexpected denial.
                if let Some(op) = task
                    .get("metadata")
                    .and_then(|m| m.get("a3t"))
                    .and_then(|a| a.get("policy"))
                    .and_then(|p| p.get("op"))
                {
                    if op.as_str().and_then(Op::parse).is_none() {
                        bad.push(format!("{here} (invalid op: {op})"));
                    }
                }
                // Recurse into container tasks that hold nested task lists.
                for key in ["do", "fork", "try", "catch"] {
                    if let Some(inner) = task.get(key) {
                        walk(inner, &here, bad);
                        if let Some(branches) = inner.get("branches") {
                            walk(branches, &here, bad);
                        }
                        if let Some(inner_do) = inner.get("do") {
                            walk(inner_do, &here, bad);
                        }
                    }
                }
                if let Some(inner) = task.get("for").and_then(|f| f.get("do")) {
                    walk(inner, &here, bad);
                }
            }
        }
    }
    let mut bad = Vec::new();
    if let Some(d) = workflow.get("do") {
        walk(d, "", &mut bad);
    }
    bad
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── metadata.a3t.policy annotation ──

    fn ann(json: serde_json::Value) -> StepPolicy {
        StepPolicy::parse(&json).expect("annotation parses")
    }

    #[test]
    fn annotation_supplies_bucket_when_handler_classified_nothing() {
        // The generic case: any `call:` step, no handler-side Rust at all.
        let base = Resource::tool("call:http");
        let r = ann(serde_json::json!({"op": "write"})).apply(base);
        assert_eq!(r.attributes.get("operation").and_then(|v| v.as_str()), Some("write"));
    }

    #[test]
    fn annotation_may_raise_but_never_lower_the_bucket() {
        // Handler says delete; a flow author labelling it "read" must not win.
        let handler_said_delete =
            Resource::tool("call:lancedb").with_attr("operation", "delete");
        let r = ann(serde_json::json!({"op": "read"})).apply(handler_said_delete);
        assert_eq!(
            r.attributes.get("operation").and_then(|v| v.as_str()),
            Some("delete"),
            "annotation must not downgrade the handler's classification"
        );

        // Raising is allowed: handler says read, annotation says delete.
        let handler_said_read = Resource::tool("call:http").with_attr("operation", "read");
        let r = ann(serde_json::json!({"op": "delete"})).apply(handler_said_read);
        assert_eq!(r.attributes.get("operation").and_then(|v| v.as_str()), Some("delete"));
    }

    #[test]
    fn annotation_carries_identity_as_attributes_not_entity_id() {
        let r = ann(serde_json::json!({
            "op": "delete",
            "resource": {"type": "Template", "id": "tpl-42", "attrs": {"table": "template_registry"}}
        }))
        .apply(Resource::tool("call:lancedb"));
        assert_eq!(r.id, "call:lancedb", "entity id stays the handler's trusted value");
        assert_eq!(r.attributes.get("object_type").and_then(|v| v.as_str()), Some("Template"));
        assert_eq!(r.attributes.get("object_id").and_then(|v| v.as_str()), Some("tpl-42"));
        assert_eq!(r.attributes.get("table").and_then(|v| v.as_str()), Some("template_registry"));
    }

    #[test]
    fn annotation_cannot_overwrite_handler_owned_attributes() {
        // The escalation attempt: disguise a destructive tool as safe, and
        // rename it to impersonate a permitted one.
        let base = Resource::tool("call:rotate"); // is_destructive() => true
        assert_eq!(base.attributes.get("destructive").and_then(|v| v.as_bool()), Some(true));
        let r = ann(serde_json::json!({
            "resource": {"attrs": {"destructive": false, "name": "list_flows", "task_type": "call:http"}}
        }))
        .apply(base);
        assert_eq!(
            r.attributes.get("destructive").and_then(|v| v.as_bool()),
            Some(true),
            "annotation must not clear the destructive flag"
        );
        assert_eq!(r.attributes.get("name").and_then(|v| v.as_str()), Some("call:rotate"));
    }

    #[test]
    fn no_annotation_leaves_the_resource_untouched() {
        assert!(StepPolicy::from_metadata(None).is_none());
        assert!(StepPolicy::from_metadata(Some(&serde_json::json!({"other": 1}))).is_none());
        let md = serde_json::json!({"a3t": {"policy": {"op": "write"}}});
        assert_eq!(StepPolicy::from_metadata(Some(&md)).unwrap().op, Some(Op::Write));
    }

    #[test]
    fn annotation_placement_rejects_uncheckable_tasks() {
        let wf: serde_json::Value = serde_yaml::from_str(r#"
do:
  - ok-call:
      call: lancedb
      metadata: { a3t: { policy: { op: delete } } }
  - ok-run:
      run:
        script: { code: "x" }
      metadata: { a3t: { policy: { op: execute } } }
  - bad-set:
      set: { x: 1 }
      metadata: { a3t: { policy: { op: delete } } }
  - loop:
      for:
        each: item
        do:
          - bad-nested-switch:
              switch: []
              metadata: { a3t: { policy: { op: write } } }
          - ok-nested-call:
              call: http
              metadata: { a3t: { policy: { op: write } } }
"#).unwrap();
        let bad = validate_annotation_placement(&wf);
        assert!(bad.contains(&"bad-set".to_string()), "got {bad:?}");
        assert!(bad.contains(&"loop/bad-nested-switch".to_string()), "got {bad:?}");
        assert_eq!(bad.len(), 2, "call/run annotations must be accepted: {bad:?}");
    }

    #[test]
    fn malformed_op_fails_closed_but_absent_op_does_not() {
        // Present-but-invalid → most restrictive.
        let r = ann(serde_json::json!({"op": "delet"})).apply(Resource::tool("call:http"));
        assert_eq!(r.attributes.get("operation").and_then(|v| v.as_str()), Some("delete"));

        // Absent → no operation attribute; identity-only annotations stay valid.
        let r = ann(serde_json::json!({"resource": {"type": "Template"}}))
            .apply(Resource::tool("call:http"));
        assert!(r.attributes.get("operation").is_none());
        assert_eq!(r.attributes.get("object_type").and_then(|v| v.as_str()), Some("Template"));
    }
}
