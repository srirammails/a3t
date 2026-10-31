//! Who may do what in which namespace — the project rules, enforced.
//!
//! The A3T project flows (a3t/backend/flows/projects) own the data: `projects`,
//! `org_members` and `orgs` in the platform DB. This guard reads them (a snapshot
//! refreshed every few seconds) and answers the same question the `a3t-projects`
//! flow answers for listing — what the caller may do in a namespace — so the
//! server refuses what the dash hides. FlowR EE installs it when policy is
//! enforcing (`install`); CE keeps allow-all.
//!
//! - A personal project: its owner; members by role; superadmin reads it unless
//!   it is private.
//! - An org project: members by role; the org's admins write, its security
//!   admins read; every org member reads it unless it is private; superadmin.
//! - A namespace from before projects (`default`, …): org `default`'s.
//! - The platform (`a3t`, `system`): everyone reads and runs it; superadmin
//!   writes. Users' work saved there before projects counts as `default`'s.
//! - Runtimes: an `a3t.io/org`-labelled runtime is that org's (its admins
//!   control it); every other runtime is shared and only superadmin controls it.

use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use serde_json::Value;

use flowr_policy::namespace::{acting_user, is_superadmin, ItemKind, NamespaceGuard, NsAccess};
use flowr_policy::Principal;
use flowr_storage::StorageBackend;

use crate::handlers::call_lancedb::{batches_to_json, DEFAULT_PLATFORM_DB_PATH, PLATFORM_DB_RESOURCE};

const PLATFORM: &[&str] = &["a3t", "system"];
/// The org namespaces from before projects belong to.
const LEGACY_ORG: &str = "org-default";
/// The label that makes a runtime an org's own.
pub const ORG_LABEL: &str = "a3t.io/org";
/// The label that makes a runtime a single project's own (isolated personal projects).
pub const PROJECT_LABEL: &str = "a3t.io/project";
const REFRESH: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, Default)]
struct Project {
    owner_kind: String,
    /// The owner's handle (user or org).
    owner: String,
    owner_id: String,
    private: bool,
    members: Vec<(String, String)>,
    /// `isolated` | `shared` | empty (the org's setting).
    runtime: String,
}

/// The rows the decision needs, as of `loaded`.
#[derive(Debug, Default)]
pub struct Snapshot {
    projects: HashMap<String, Project>,
    /// (org id, user id) → member | admin | securityadmin
    org_roles: HashMap<(String, String), String>,
    /// org handle → org id
    orgs: HashMap<String, String>,
    /// org handle → runtime policy (`isolated` | `shared`)
    org_runtime: HashMap<String, String>,
}

impl Snapshot {
    fn from_rows(projects: &[Value], members: &[Value], orgs: &[Value]) -> Self {
        let s = |v: &Value, k: &str| v.get(k).and_then(|x| x.as_str()).unwrap_or_default().to_string();
        let mut snap = Snapshot::default();
        for p in projects {
            let members = serde_json::from_str::<Vec<Value>>(&s(p, "members")).unwrap_or_default()
                .iter().map(|m| (s(m, "id"), s(m, "role"))).filter(|(id, _)| !id.is_empty()).collect();
            let runtime = serde_json::from_str::<Value>(&s(p, "policy")).ok()
                .and_then(|v| v.get("runtime").and_then(|r| r.as_str()).map(String::from)).unwrap_or_default();
            snap.projects.insert(s(p, "namespace"), Project {
                owner_kind: s(p, "owner_kind"), owner: s(p, "owner"), owner_id: s(p, "owner_id"),
                private: s(p, "private") == "true", members, runtime,
            });
        }
        for m in members {
            snap.org_roles.insert((s(m, "org_id"), s(m, "user_id")), s(m, "role"));
        }
        for o in orgs {
            snap.orgs.entry(s(o, "slug")).or_insert_with(|| s(o, "id"));
            let runtime = serde_json::from_str::<Value>(&s(o, "settings")).ok()
                .and_then(|v| v.get("runtime").and_then(|r| r.as_str()).map(String::from)).unwrap_or_default();
            if !runtime.is_empty() {
                snap.org_runtime.insert(s(o, "slug"), runtime);
            }
        }
        snap
    }

    fn org_role(&self, org_id: &str, user: &str) -> Option<&str> {
        self.org_roles.get(&(org_id.to_string(), user.to_string())).map(String::as_str)
    }

    /// Whether a flow of `namespace` may run on a runtime with `labels`.
    /// - Nothing runs on another org's runtime (`a3t.io/org`) or another
    ///   project's (`a3t.io/project`).
    /// - Isolated — the project's `runtime` policy, else its org's — runs only on
    ///   its own: an org project on `a3t.io/org=<org>`, a personal project on
    ///   `a3t.io/project=<namespace>`.
    /// - Shared runs on the shared runtimes (no owner label) or its org's own.
    /// - The platform's flows run on shared runtimes only. A namespace from before
    ///   projects is org `default`'s.
    pub fn runtime_allowed(&self, namespace: &str, labels: &BTreeMap<String, String>) -> bool {
        let rt_org = labels.get(ORG_LABEL).map(String::as_str);
        let rt_project = labels.get(PROJECT_LABEL).map(String::as_str);
        if PLATFORM.contains(&namespace) {
            return rt_org.is_none() && rt_project.is_none();
        }
        let (org, policy) = match self.projects.get(namespace) {
            Some(p) if p.owner_kind == "org" => {
                let policy = if p.runtime.is_empty() { self.org_runtime.get(&p.owner).cloned().unwrap_or_default() } else { p.runtime.clone() };
                (Some(p.owner.as_str()), policy)
            }
            Some(p) => (None, p.runtime.clone()),
            None => (Some("default"), self.org_runtime.get("default").cloned().unwrap_or_default()),
        };
        if rt_project.is_some_and(|p| p != namespace) || rt_org.is_some_and(|o| Some(o) != org) {
            return false;
        }
        if policy == "isolated" {
            return match org {
                Some(o) => rt_org == Some(o),
                None => rt_project == Some(namespace),
            };
        }
        true
    }

    /// The decision, for a known user (`super_` = holds SuperAdmin).
    pub fn decide(&self, user: &str, super_: bool, namespace: &str) -> NsAccess {
        if PLATFORM.contains(&namespace) {
            return if super_ { NsAccess::Write } else { NsAccess::Run };
        }
        let by_role = |role: &str| match role {
            "owner" | "developer" => NsAccess::Write,
            "operator" => NsAccess::Run,
            "viewer" => NsAccess::Read,
            _ => NsAccess::None,
        };
        let Some(p) = self.projects.get(namespace) else {
            // From before projects: org default's.
            return match (super_, self.org_role(LEGACY_ORG, user)) {
                (true, _) | (_, Some("admin" | "member")) => NsAccess::Write,
                (_, Some("securityadmin")) => NsAccess::Read,
                _ => NsAccess::None,
            };
        };
        let member = p.members.iter().find(|(id, _)| id == user).map(|(_, r)| by_role(r));
        if p.owner_kind == "user" {
            if p.owner_id == user {
                return NsAccess::Write;
            }
            if p.private {
                return NsAccess::None;
            }
            return member.unwrap_or(if super_ { NsAccess::Read } else { NsAccess::None });
        }
        let org = match self.org_role(&p.owner_id, user) {
            Some("admin") => NsAccess::Write,
            Some("securityadmin") => NsAccess::Read,
            Some(_) if !p.private => NsAccess::Read,
            _ => NsAccess::None,
        };
        let sup = if super_ { NsAccess::Write } else { NsAccess::None };
        member.unwrap_or(NsAccess::None).max(org).max(sup)
    }
}

pub struct ProjectGuard {
    storage: Arc<dyn StorageBackend>,
    snapshot: RwLock<(Option<Instant>, Arc<Snapshot>)>,
}

impl ProjectGuard {
    pub fn new(storage: Arc<dyn StorageBackend>) -> Self {
        ProjectGuard { storage, snapshot: RwLock::new((None, Arc::new(Snapshot::default()))) }
    }

    async fn snapshot(&self) -> Arc<Snapshot> {
        {
            let (at, snap) = &*self.snapshot.read().unwrap();
            if at.is_some_and(|t| t.elapsed() < REFRESH) {
                return snap.clone();
            }
        }
        let db = crate::platform_db::open_platform_db(self.storage.as_ref(), Some(PLATFORM_DB_RESOURCE), DEFAULT_PLATFORM_DB_PATH).await;
        let rows = |t: &'static str| {
            let db = db.clone();
            async move { db.query(t, None, 100_000).await.map(|b| batches_to_json(&b)) }
        };
        let (p, m, o) = (rows("projects").await, rows("org_members").await, rows("orgs").await);
        let snap = match (p, m, o) {
            (Ok(p), Ok(m), Ok(o)) => Arc::new(Snapshot::from_rows(&p, &m, &o)),
            // Tables not installed yet, or the DB is unreachable: keep the last
            // good snapshot rather than open everything up or lock everyone out
            // on a blip; with none, the empty one (legacy rules only).
            (p, m, o) => {
                tracing::warn!(projects = ?p.err(), members = ?m.err(), orgs = ?o.err(), "project tables unreadable; using the last snapshot");
                self.snapshot.read().unwrap().1.clone()
            }
        };
        *self.snapshot.write().unwrap() = (Some(Instant::now()), snap.clone());
        snap
    }
}

#[async_trait]
impl NamespaceGuard for ProjectGuard {
    async fn access(&self, principal: &Principal, namespace: &str) -> NsAccess {
        if let Principal::Service { .. } = principal {
            return NsAccess::Write; // internal machinery
        }
        let Some(user) = acting_user(principal) else {
            return NsAccess::None; // an agent acting for no one
        };
        self.snapshot().await.decide(user, is_superadmin(principal), namespace)
    }

    fn is_platform(&self, namespace: &str) -> bool {
        PLATFORM.contains(&namespace)
    }

    fn effective_namespace(&self, kind: ItemKind, namespace: &str, name: &str, type_ref: &str, created_by: &str) -> String {
        effective_namespace(kind, namespace, name, type_ref, created_by)
    }

    async fn can_control_runtime(&self, principal: &Principal, labels: &BTreeMap<String, String>) -> bool {
        if matches!(principal, Principal::Service { .. }) || is_superadmin(principal) {
            return true;
        }
        let (Some(org), Some(user)) = (labels.get(ORG_LABEL), acting_user(principal)) else {
            return false; // a shared runtime: superadmin only
        };
        let snap = self.snapshot().await;
        snap.orgs.get(org).is_some_and(|id| snap.org_role(id, user) == Some("admin"))
    }

    async fn can_see_runtime(&self, principal: &Principal, labels: &BTreeMap<String, String>) -> bool {
        let Some(org) = labels.get(ORG_LABEL) else { return true };
        if matches!(principal, Principal::Service { .. }) || is_superadmin(principal) {
            return true;
        }
        let Some(user) = acting_user(principal) else { return false };
        let snap = self.snapshot().await;
        snap.orgs.get(org).is_some_and(|id| snap.org_role(id, user).is_some())
    }

    async fn runtime_allowed(&self, namespace: &str, labels: &BTreeMap<String, String>) -> bool {
        self.snapshot().await.runtime_allowed(namespace, labels)
    }

    fn enforcing(&self) -> bool {
        true
    }
}

/// The runtimes a flow of `namespace` may be placed on (the runtime policy).
pub async fn eligible_runtimes(namespace: &str, flow: &str, runtimes: Vec<flowr_types::canvas::RuntimeSpec>) -> Vec<flowr_types::canvas::RuntimeSpec> {
    let g = flowr_policy::namespace::namespace_guard();
    if !g.enforcing() {
        return runtimes;
    }
    let ns = effective_namespace(ItemKind::Flow, namespace, flow, "", "");
    let mut out = Vec::new();
    for r in runtimes {
        if g.runtime_allowed(&ns, &r.labels).await {
            out.push(r);
        }
    }
    out
}

/// Where an item in a platform namespace really belongs. Before projects the dash
/// saved users' work in `a3t`; it is org `default`'s (the `default` namespace).
/// What the platform ships stays platform: built-in agents (`created_by:
/// system`), `a3t-*`/`adb-*`/`system` flows, infrastructure resources, `preset-*`
/// canvases and `a3t-*` triggers. The dash applies the same rule
/// (`itemProject` in stores/context.svelte.ts).
pub fn effective_namespace(kind: ItemKind, namespace: &str, name: &str, type_ref: &str, created_by: &str) -> String {
    if !PLATFORM.contains(&namespace) {
        return namespace.to_string();
    }
    let platform_flow = namespace == "system" || name.starts_with("a3t-") || name.starts_with("adb-");
    let stays = match kind {
        ItemKind::Agent => created_by == "system",
        ItemKind::Flow => platform_flow,
        ItemKind::Resource => type_ref != "canvas" || name.starts_with("preset-"),
        ItemKind::Trigger => name.starts_with("a3t-"),
    };
    if stays { namespace.to_string() } else { "default".to_string() }
}

/// Install the guard (FlowR EE, policy enforcing).
pub fn install(storage: Arc<dyn StorageBackend>) {
    flowr_policy::namespace::set_namespace_guard(Arc::new(ProjectGuard::new(storage)));
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn snap() -> Snapshot {
        Snapshot::from_rows(
            &[
                json!({ "namespace": "ann-orders", "owner_kind": "user", "owner_id": "ann", "private": "true", "members": "[]" }),
                json!({ "namespace": "ann-blog", "owner_kind": "user", "owner_id": "ann", "private": "false", "members": "[{\"id\":\"bob\",\"role\":\"viewer\"}]" }),
                json!({ "namespace": "acme-web", "owner_kind": "org", "owner_id": "org-acme", "private": "true", "members": "[{\"id\":\"carl\",\"role\":\"operator\"}]" }),
                json!({ "namespace": "acme-docs", "owner_kind": "org", "owner_id": "org-acme", "private": "false", "members": "[]" }),
                json!({ "namespace": "globex-x", "owner_kind": "org", "owner_id": "org-globex", "private": "false", "members": "[]" }),
            ],
            &[
                json!({ "org_id": "org-acme", "user_id": "bob", "role": "admin" }),
                json!({ "org_id": "org-acme", "user_id": "carl", "role": "member" }),
                json!({ "org_id": "org-acme", "user_id": "sec", "role": "securityadmin" }),
                json!({ "org_id": "org-default", "user_id": "dora", "role": "member" }),
            ],
            &[json!({ "id": "org-acme", "slug": "acme" })],
        )
    }

    #[test]
    fn an_org_admin_sees_only_their_org() {
        let s = snap();
        assert_eq!(s.decide("bob", false, "acme-web"), NsAccess::Write);
        assert_eq!(s.decide("bob", false, "acme-docs"), NsAccess::Write);
        assert_eq!(s.decide("bob", false, "globex-x"), NsAccess::None, "another org's project");
        assert_eq!(s.decide("bob", false, "ann-orders"), NsAccess::None, "a private personal project");
        assert_eq!(s.decide("bob", false, "ann-blog"), NsAccess::Read, "a member, as viewer");
        assert_eq!(s.decide("bob", false, "default"), NsAccess::None, "not in org default");
    }

    #[test]
    fn org_members_security_admins_and_private_projects() {
        let s = snap();
        assert_eq!(s.decide("carl", false, "acme-docs"), NsAccess::Read);
        assert_eq!(s.decide("carl", false, "acme-web"), NsAccess::Run, "private, but a member as operator");
        assert_eq!(s.decide("sec", false, "acme-web"), NsAccess::Read);
        assert_eq!(s.decide("zed", false, "acme-docs"), NsAccess::None);
    }

    #[test]
    fn superadmin_platform_and_legacy() {
        let s = snap();
        assert_eq!(s.decide("root", true, "globex-x"), NsAccess::Write);
        assert_eq!(s.decide("root", true, "ann-orders"), NsAccess::None, "private personal: owner only");
        assert_eq!(s.decide("root", true, "ann-blog"), NsAccess::Read);
        assert_eq!(s.decide("zed", false, "a3t"), NsAccess::Run);
        assert_eq!(s.decide("root", true, "a3t"), NsAccess::Write);
        assert_eq!(s.decide("dora", false, "default"), NsAccess::Write);
        assert_eq!(s.decide("ann", false, "ann-orders"), NsAccess::Write);
    }

    fn labels(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
    }

    #[test]
    fn runtime_policy_keeps_isolated_work_on_its_own_runtimes() {
        let s = Snapshot::from_rows(
            &[
                json!({ "namespace": "acme-web", "owner_kind": "org", "owner": "acme", "owner_id": "org-acme", "private": "true", "members": "[]", "policy": "{}" }),
                json!({ "namespace": "acme-lab", "owner_kind": "org", "owner": "acme", "owner_id": "org-acme", "private": "false", "members": "[]", "policy": "{\"runtime\":\"shared\"}" }),
                json!({ "namespace": "globex-x", "owner_kind": "org", "owner": "globex", "owner_id": "org-globex", "private": "false", "members": "[]", "policy": "{}" }),
                json!({ "namespace": "ann-secret", "owner_kind": "user", "owner": "ann", "owner_id": "ann", "private": "true", "members": "[]", "policy": "{\"runtime\":\"isolated\"}" }),
                json!({ "namespace": "ann-blog", "owner_kind": "user", "owner": "ann", "owner_id": "ann", "private": "false", "members": "[]", "policy": "{}" }),
            ],
            &[],
            &[json!({ "id": "org-acme", "slug": "acme", "settings": "{\"runtime\":\"isolated\"}" }),
              json!({ "id": "org-globex", "slug": "globex", "settings": "{}" })],
        );
        let shared = labels(&[]);
        let acme = labels(&[(ORG_LABEL, "acme")]);
        let globex = labels(&[(ORG_LABEL, "globex")]);
        let ann = labels(&[(PROJECT_LABEL, "ann-secret")]);
        // acme is isolated: only its own runtime, never shared or globex's.
        assert!(s.runtime_allowed("acme-web", &acme));
        assert!(!s.runtime_allowed("acme-web", &shared));
        assert!(!s.runtime_allowed("acme-web", &globex));
        // ...unless the project chose shared.
        assert!(s.runtime_allowed("acme-lab", &shared));
        assert!(!s.runtime_allowed("acme-lab", &globex));
        // globex is shared: shared runtimes or its own, never acme's.
        assert!(s.runtime_allowed("globex-x", &shared));
        assert!(s.runtime_allowed("globex-x", &globex));
        assert!(!s.runtime_allowed("globex-x", &acme));
        // An isolated personal project: its own runtime only.
        assert!(s.runtime_allowed("ann-secret", &ann));
        assert!(!s.runtime_allowed("ann-secret", &shared));
        assert!(!s.runtime_allowed("ann-blog", &ann), "another project's runtime");
        assert!(s.runtime_allowed("ann-blog", &shared));
        // The platform's flows: shared runtimes only.
        assert!(s.runtime_allowed("a3t", &shared));
        assert!(!s.runtime_allowed("a3t", &acme));
    }

    #[test]
    fn users_work_in_the_platform_namespace_is_defaults() {
        assert_eq!(effective_namespace(ItemKind::Flow, "a3t", "ops-ping", "", ""), "default");
        assert_eq!(effective_namespace(ItemKind::Flow, "a3t", "a3t-agents", "", ""), "a3t");
        assert_eq!(effective_namespace(ItemKind::Resource, "a3t", "Deepseek", "openai", ""), "a3t");
        assert_eq!(effective_namespace(ItemKind::Resource, "a3t", "ops-ping-canvas", "canvas", ""), "default");
        assert_eq!(effective_namespace(ItemKind::Trigger, "a3t", "agent-agt-1-chat", "", ""), "default");
        assert_eq!(effective_namespace(ItemKind::Agent, "a3t", "x", "", "usr-1"), "default");
        assert_eq!(effective_namespace(ItemKind::Agent, "a3t", "x", "", "system"), "a3t");
        assert_eq!(effective_namespace(ItemKind::Flow, "acme-web", "a3t-x", "", ""), "acme-web");
    }
}

// ── Enforcement in FlowR CE's own tools and routes ──

use flowr_mcp_core::CallToolResult;

fn arg<'a>(args: &'a Value, k: &str) -> &'a str {
    args.get(k).and_then(|v| v.as_str()).unwrap_or_default()
}

fn guard() -> Arc<dyn NamespaceGuard> {
    flowr_policy::namespace::namespace_guard()
}

/// The namespace a deployment's work belongs to.
fn deployment_namespace(d: &crate::ops::Deployment) -> String {
    let ns = d.flow_ref.split_once('/').map(|(n, _)| n.to_string()).unwrap_or_else(|| d.namespace.clone());
    let name = d.flow_ref.split_once('/').map(|(_, n)| n).unwrap_or(&d.target);
    effective_namespace(ItemKind::Flow, &ns, name, "", "")
}

async fn allowed(principal: &Principal, ns: &str, needed: NsAccess) -> Result<(), CallToolResult> {
    let has = guard().access(principal, ns).await;
    if has >= needed {
        return Ok(());
    }
    Err(CallToolResult::error(if has == NsAccess::None {
        format!("Not found: no project '{ns}' that you can see.")
    } else {
        format!("Forbidden: this needs more access to project '{ns}' than you have.")
    }))
}

/// Gate FlowR CE's own tools (agents, canvases, runtimes, deployments, the DB
/// console). `Some` = refused.
pub async fn gate_ce_tool(storage: &Arc<dyn StorageBackend>, principal: &Principal, name: &str, args: &Value) -> Option<CallToolResult> {
    if !guard().enforcing() {
        return None;
    }
    let ns_or = |d: &str| { let n = arg(args, "namespace"); if n.is_empty() { d.to_string() } else { n.to_string() } };
    let canvas = |level| {
        let ns = ns_or("a3t");
        (effective_namespace(ItemKind::Resource, &ns, arg(args, "name"), "canvas", ""), level)
    };
    let need: Option<(String, NsAccess)> = match name {
        "get_canvas" | "preview_placement" => Some(canvas(NsAccess::Read)),
        "update_canvas" | "bind_canvas" => Some(canvas(NsAccess::Write)),
        "clone_canvas" => Some((ns_or("default"), NsAccess::Write)),
        "deploy_flow" => {
            let ns = ns_or("a3t");
            Some((effective_namespace(ItemKind::Flow, &ns, arg(args, "name"), "", ""), NsAccess::Write))
        }
        "create_agent" => Some((ns_or("default"), NsAccess::Write)),
        "get_agent" | "delete_agent" | "send_message" => {
            let id = arg(args, "agent_id");
            let row = crate::handlers::call_lancedb::query_platform_table(
                storage.as_ref(), "agent_configs",
                Some(format!("id = {}", crate::handlers::call_lancedb::sql_quote(id))), 1).await
                .ok().and_then(|r| r.into_iter().next()).unwrap_or(Value::Null);
            let ns = effective_namespace(ItemKind::Agent, row.get("namespace").and_then(|v| v.as_str()).unwrap_or("a3t"),
                id, "", row.get("created_by").and_then(|v| v.as_str()).unwrap_or_default());
            let level = match name { "get_agent" => NsAccess::Read, "send_message" => NsAccess::Run, _ => NsAccess::Write };
            Some((ns, level))
        }
        "query_resource" => {
            let ns = ns_or("default");
            let r = arg(args, "resource");
            let type_ref = storage.resources().get(&ns, r).await.map(|x| x.type_ref).unwrap_or_default();
            Some((effective_namespace(ItemKind::Resource, &ns, r, &type_ref, ""), NsAccess::Read))
        }
        "probe_deployment" | "pause_deployment" | "resume_deployment" | "restart_deployment" | "undeploy" | "terminate_deployment" => {
            let Some(d) = crate::ops::deployment_by_id(storage.as_ref(), arg(args, "deployment")).await else { return None };
            let level = if name == "probe_deployment" { NsAccess::Read } else { NsAccess::Write };
            Some((deployment_namespace(&d), level))
        }
        "terminate_runtime" | "set_runtime_state" | "set_paused_response" => {
            let labels = crate::ops::runtime_by_name(storage.as_ref(), arg(args, "runtime")).await
                .map(|r| r.labels).unwrap_or_default();
            if !guard().can_see_runtime(principal, &labels).await {
                return Some(CallToolResult::error("Not found: no such runtime.".to_string()));
            }
            if !guard().can_control_runtime(principal, &labels).await {
                return Some(CallToolResult::error(
                    "Forbidden: a shared runtime is controlled by superadmin only; an org's own runtime by its admins.".to_string()));
            }
            None
        }
        "create_runtime" => {
            let labels: BTreeMap<String, String> = args.get("labels").and_then(|l| l.as_object())
                .map(|o| o.iter().map(|(k, v)| (k.clone(), v.as_str().unwrap_or_default().to_string())).collect())
                .unwrap_or_default();
            if !guard().can_control_runtime(principal, &labels).await {
                return Some(CallToolResult::error(
                    "Forbidden: shared runtimes are created by superadmin; an org's own runtime (label a3t.io/org) by its admins.".to_string()));
            }
            None
        }
        _ => None,
    };
    let (ns, level) = need?;
    allowed(principal, &ns, level).await.err()
}

/// The runtime overview (`list_deployments`, `GET /ops/overview`) as `principal`
/// may see it: runtimes it may see (shared ones, its orgs' own), and on each only
/// the deployments of projects it can read.
pub async fn filter_overview(principal: &Principal, mut overview: Value) -> Value {
    let g = guard();
    if !g.enforcing() {
        return overview;
    }
    async fn visible_deployment(p: &Principal, d: &Value) -> bool {
        let Ok(dep) = serde_json::from_value::<crate::ops::Deployment>(d.get("deployment").cloned().unwrap_or(Value::Null)) else { return false };
        guard().access(p, &deployment_namespace(&dep)).await >= NsAccess::Read
    }
    if let Some(rts) = overview.get_mut("runtimes").and_then(|v| v.as_array_mut()) {
        let mut keep = Vec::new();
        for mut rt in std::mem::take(rts) {
            let labels: BTreeMap<String, String> = rt.get("runtime").and_then(|r| r.get("labels"))
                .and_then(|l| serde_json::from_value(l.clone()).ok()).unwrap_or_default();
            if !g.can_see_runtime(principal, &labels).await {
                continue;
            }
            let control = g.can_control_runtime(principal, &labels).await;
            if let Some(deps) = rt.get_mut("deployments").and_then(|v| v.as_array_mut()) {
                let mut shown = Vec::new();
                for d in std::mem::take(deps) {
                    if visible_deployment(principal, &d).await { shown.push(d); }
                }
                *deps = shown;
            }
            rt["canControl"] = Value::Bool(control);
            keep.push(rt);
        }
        *rts = keep;
    }
    if let Some(orphans) = overview.get_mut("unplaced").and_then(|v| v.as_array_mut()) {
        let mut shown = Vec::new();
        for d in std::mem::take(orphans) {
            if visible_deployment(principal, &d).await { shown.push(d); }
        }
        *orphans = shown;
    }
    overview
}

/// Runtime pools `principal` may see.
pub async fn filter_runtimes(principal: &Principal, runtimes: Vec<flowr_types::canvas::RuntimeSpec>) -> Vec<flowr_types::canvas::RuntimeSpec> {
    let g = guard();
    let mut out = Vec::new();
    for r in runtimes {
        if !g.enforcing() || g.can_see_runtime(principal, &r.labels).await {
            out.push(r);
        }
    }
    out
}

/// Rows the caller may see, for flows (`call: registry` operation `visible`):
/// each item's `namespace` (and `name`, `type`, `created_by`) decide.
pub async fn filter_items(principal: &Principal, kind: ItemKind, items: Vec<Value>) -> Vec<Value> {
    let g = guard();
    if !g.enforcing() {
        return items;
    }
    let mut out = Vec::new();
    for it in items {
        let s = |k: &str| it.get(k).and_then(|v| v.as_str()).unwrap_or_default().to_string();
        let ns = if s("namespace").is_empty() { "default".to_string() } else { s("namespace") };
        let ns = effective_namespace(kind, &ns, &s("name"), &s("type"), &s("created_by"));
        if g.access(principal, &ns).await >= NsAccess::Read {
            out.push(it);
        }
    }
    out
}

/// The text of a tool result (for an HTTP error body).
pub fn result_text(r: &CallToolResult) -> String {
    r.content.iter().map(|c| match c { flowr_mcp_core::protocol::ToolContent::Text { text } => text.clone(), #[allow(unreachable_patterns)] _ => String::new() })
        .collect::<Vec<_>>().join(" ")
}

/// Whether `principal` may see an item (REST routes).
pub async fn can_read_item(principal: &Principal, kind: ItemKind, ns: &str, name: &str, type_ref: &str) -> bool {
    let g = guard();
    !g.enforcing() || g.access(principal, &effective_namespace(kind, ns, name, type_ref, "")).await >= NsAccess::Read
}

/// A REST write into `ns`: `Err` with the response when refused.
pub async fn rest_write(principal: &Principal, ns: &str) -> Result<(), (axum::http::StatusCode, axum::Json<Value>)> {
    let g = guard();
    if !g.enforcing() {
        return Ok(());
    }
    match g.access(principal, ns).await {
        NsAccess::Write => Ok(()),
        NsAccess::None => Err((axum::http::StatusCode::NOT_FOUND, axum::Json(serde_json::json!({ "error": format!("no project '{ns}' that you can see") })))),
        _ => Err((axum::http::StatusCode::FORBIDDEN, axum::Json(serde_json::json!({ "error": format!("changing project '{ns}' needs write access") })))),
    }
}
