//! The A3T project, org and profile flows (a3t/backend/flows/projects), run by
//! the real executor with the real `call:lancedb` and `call:registry` handlers
//! against a temporary platform DB — as different users, to check who sees and
//! may change what. The flows authorize on `$context._principal`, so the
//! caller is set on the request exactly as the HTTP edge does.

use std::sync::Arc;

use flowr_storage::StorageBackend;
use flowr_types::{ExecutionRequest, ExecutionStatus, Resource, WorkflowRecord};
use serde_json::{json, Value};

const FLOWS: &[&str] = &["a3t_projects_install.yaml", "a3t_profile.yaml", "a3t_orgs.yaml", "a3t_projects.yaml"];

struct World {
    storage: Arc<dyn StorageBackend>,
    http: reqwest::Client,
    _dir: tempfile::TempDir,
}

const SUPER: &[&str] = &["SuperAdmin", "SecurityAdmin", "Admin", "Operator", "Viewer"];
const DEV: &[&str] = &["Admin", "Operator", "Viewer"];

impl World {
    async fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let storage = flowr_ce::init_storage(dir.path().to_str().unwrap()).await.unwrap();
        let db = dir.path().join("platform.lance");
        storage.resources().create(
            Resource::new("a3t-platform-db", "lancedb", json!({ "db_path": db.to_str().unwrap() })).with_namespace("a3t"),
        ).await.unwrap();
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../a3t/backend/flows/projects");
        for f in FLOWS {
            let yaml = std::fs::read_to_string(root.join(f)).unwrap();
            storage.workflows().register(WorkflowRecord::from_yaml(&yaml).unwrap()).await.unwrap();
        }
        World { storage, http: reqwest::Client::new(), _dir: dir }
    }

    async fn run(&self, user: &str, roles: &[&str], flow: &str, input: Value) -> Value {
        let principal = flowr_policy::Principal::human(user, roles.iter().map(|r| r.to_string()).collect());
        let req = ExecutionRequest::new("a3t", flow).with_input(input).with_principal(principal);
        let state = flowr_ce::execute_workflow(self.storage.clone(), &self.http, req).await.unwrap();
        assert_eq!(state.status, ExecutionStatus::Completed, "{flow} at {:?}: {:?}", state.current_step, state.error);
        state.output.unwrap_or(Value::Null)
    }

    async fn ok(&self, user: &str, roles: &[&str], flow: &str, input: Value) -> Value {
        let out = self.run(user, roles, flow, input.clone()).await;
        assert_eq!(out["ok"], true, "{user} {flow} {input}: {out}");
        out
    }

    async fn refused(&self, user: &str, roles: &[&str], flow: &str, input: Value, why: &str) {
        let out = self.run(user, roles, flow, input.clone()).await;
        assert_eq!(out["ok"], false, "{user} {flow} {input} should be refused: {out}");
        let err = out["error"].as_str().unwrap_or_default();
        assert!(err.contains(why), "{user} {flow}: expected “{why}”, got “{err}”");
    }

    async fn visible(&self, user: &str, roles: &[&str]) -> Vec<(String, String)> {
        let out = self.ok(user, roles, "a3t-projects", json!({ "action": "list" })).await;
        let mut v: Vec<(String, String)> = out["projects"].as_array().unwrap().iter()
            .map(|p| (p["namespace"].as_str().unwrap().to_string(), p["role"].as_str().unwrap().to_string()))
            .collect();
        v.sort();
        v
    }
}

fn pairs(v: &[(&str, &str)]) -> Vec<(String, String)> {
    v.iter().map(|(a, b)| (a.to_string(), b.to_string())).collect()
}

#[tokio::test(flavor = "multi_thread")]
async fn users_orgs_and_projects() {
    let w = World::new().await;
    w.refused("usr-ann", DEV, "a3t-projects-install", json!({}), "SuperAdmin").await;
    w.ok("usr-root", SUPER, "a3t-projects-install", json!({})).await;

    // Profiles: one handle each, shared space with orgs.
    w.ok("usr-ann", DEV, "a3t-profile", json!({ "action": "claim", "handle": "ann", "email": "ann@x.io" })).await;
    w.refused("usr-ann", DEV, "a3t-profile", json!({ "action": "claim", "handle": "ann2" }), "already have").await;
    w.refused("usr-bob", DEV, "a3t-profile", json!({ "action": "claim", "handle": "ann" }), "taken").await;
    w.refused("usr-bob", DEV, "a3t-profile", json!({ "action": "claim", "handle": "Bob!" }), "lowercase").await;
    w.refused("usr-bob", DEV, "a3t-profile", json!({ "action": "claim", "handle": "system" }), "reserved").await;
    w.ok("usr-bob", DEV, "a3t-profile", json!({ "action": "claim", "handle": "bob" })).await;

    // Orgs: superadmin only — and a forged principal in the input changes nothing.
    w.refused("usr-ann", DEV, "a3t-orgs", json!({ "action": "create", "org": "evil",
        "_principal": { "user": "usr-root", "roles": ["SuperAdmin"] } }), "superadmin").await;
    w.ok("usr-root", SUPER, "a3t-orgs", json!({ "action": "create", "org": "acme", "display_name": "Acme" })).await;
    w.refused("usr-root", SUPER, "a3t-orgs", json!({ "action": "create", "org": "bob" }), "taken").await;
    w.ok("usr-root", SUPER, "a3t-orgs", json!({ "action": "set_member", "org": "acme", "user_id": "usr-bob", "role": "admin" })).await;
    // The org's admin manages members; a non-member can't.
    w.ok("usr-bob", DEV, "a3t-orgs", json!({ "action": "set_member", "org": "acme", "user_id": "usr-carl", "role": "member" })).await;
    w.refused("usr-ann", DEV, "a3t-orgs", json!({ "action": "set_member", "org": "acme", "user_id": "usr-ann", "role": "admin" }), "admin role").await;
    let orgs = w.ok("usr-ann", DEV, "a3t-orgs", json!({ "action": "list" })).await;
    assert_eq!(orgs["orgs"], json!([]), "ann is in no org");

    // A private personal project: its owner only — not even superadmin.
    w.ok("usr-ann", DEV, "a3t-projects", json!({ "action": "create", "owner_kind": "user", "owner": "ann", "name": "orders", "private": true })).await;
    w.refused("usr-ann", DEV, "a3t-projects", json!({ "action": "create", "owner_kind": "user", "owner": "bob", "name": "x" }), "your own handle").await;
    w.refused("usr-ann", DEV, "a3t-projects", json!({ "action": "create", "owner_kind": "user", "owner": "ann", "name": "orders" }), "already a project").await;
    w.refused("usr-ann", DEV, "a3t-projects", json!({ "action": "create", "owner_kind": "org", "owner": "acme", "name": "x" }), "aren't a member").await;
    assert_eq!(w.visible("usr-ann", DEV).await, pairs(&[("ann-orders", "owner")]));
    assert_eq!(w.visible("usr-bob", DEV).await, pairs(&[]));
    assert_eq!(w.visible("usr-root", SUPER).await, pairs(&[]));
    let listed = w.ok("usr-bob", DEV, "a3t-projects", json!({ "action": "list" })).await;
    assert_eq!(listed["project_namespaces"], json!(["ann-orders"]), "names only, so the dash can hide it");
    assert_eq!(listed["profile"]["handle"], "bob");

    // A private org project: members and the org's admins; a plain org member doesn't see it.
    w.ok("usr-carl", DEV, "a3t-projects", json!({ "action": "create", "owner_kind": "org", "owner": "acme", "name": "web", "private": true })).await;
    w.ok("usr-carl", DEV, "a3t-projects", json!({ "action": "create", "owner_kind": "org", "owner": "acme", "name": "docs" })).await;
    assert_eq!(w.visible("usr-bob", DEV).await, pairs(&[("acme-docs", "org-admin"), ("acme-web", "org-admin")]));
    assert_eq!(w.visible("usr-carl", DEV).await, pairs(&[("acme-docs", "org-member")]));
    assert_eq!(w.visible("usr-root", SUPER).await, pairs(&[("acme-docs", "superadmin"), ("acme-web", "superadmin")]));
    // Adding carl to the private project lets him see it.
    w.ok("usr-bob", DEV, "a3t-projects", json!({ "action": "update", "namespace": "acme-web",
        "members": [{ "kind": "user", "id": "usr-carl", "role": "developer" }] })).await;
    assert_eq!(w.visible("usr-carl", DEV).await, pairs(&[("acme-docs", "org-member"), ("acme-web", "developer")]));
    w.refused("usr-carl", DEV, "a3t-projects", json!({ "action": "update", "namespace": "acme-web", "private": false }), "owner").await;
    w.refused("usr-ann", DEV, "a3t-projects", json!({ "action": "update", "namespace": "acme-web" }), "No such project").await;

    // Moving ann-orders into acme needs ann to be an acme admin; references move with it.
    w.storage.resources().create(Resource::new("db", "postgres", json!({ "url": "postgres://x" })).with_namespace("ann-orders")).await.unwrap();
    w.storage.workflows().register(WorkflowRecord::from_yaml(
        "document:\n  dsl: 1.0.3\n  namespace: ann-orders\n  name: load\n  version: 1.0.0\ndo:\n- q:\n    call: sql\n    with:\n      _resource: ann-orders/db\n",
    ).unwrap()).await.unwrap();
    w.refused("usr-ann", DEV, "a3t-projects", json!({ "action": "move", "namespace": "ann-orders", "to_org": "acme" }), "admin role").await;
    w.ok("usr-root", SUPER, "a3t-orgs", json!({ "action": "set_member", "org": "acme", "user_id": "usr-ann", "role": "admin" })).await;
    let preview = w.ok("usr-ann", DEV, "a3t-projects", json!({ "action": "move", "namespace": "ann-orders", "to_org": "acme" })).await;
    assert_eq!(preview["report"]["flows"], json!(["load"]), "{preview}");
    assert!(w.storage.workflows().get("ann-orders", "load").await.is_ok(), "a dry run moves nothing");
    w.ok("usr-ann", DEV, "a3t-projects", json!({ "action": "move", "namespace": "ann-orders", "to_org": "acme", "dry_run": false })).await;
    let moved = w.storage.workflows().get("acme-orders", "load").await.unwrap();
    assert!(moved.raw_yaml.contains("_resource: acme-orders/db") && moved.raw_yaml.contains("namespace: acme-orders"), "{}", moved.raw_yaml);
    assert_eq!(w.storage.resources().get("acme-orders", "db").await.unwrap().config["url"], "postgres://x", "secrets copied as stored");
    assert!(w.storage.workflows().get("ann-orders", "load").await.is_err());
    assert!(w.visible("usr-bob", DEV).await.contains(&("acme-orders".to_string(), "org-admin".to_string())));

    // A project that still holds things can't be deleted; an empty one can.
    w.refused("usr-bob", DEV, "a3t-projects", json!({ "action": "delete", "namespace": "acme-orders" }), "Move or delete").await;
    w.ok("usr-bob", DEV, "a3t-projects", json!({ "action": "delete", "namespace": "acme-docs" })).await;
    w.refused("usr-root", SUPER, "a3t-orgs", json!({ "action": "delete", "org": "acme" }), "still owns projects").await;

    // Anonymous runs get nothing.
    let anon = flowr_ce::execute_workflow(w.storage.clone(), &w.http, ExecutionRequest::new("a3t", "a3t-projects").with_input(json!({ "action": "list" }))).await.unwrap();
    assert_eq!(anon.output.unwrap()["ok"], false);
}
