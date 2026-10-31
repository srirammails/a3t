//! Project isolation, end to end: two orgs set up through the A3T project flows,
//! the namespace guard installed as FlowR EE installs it, and each org's admin
//! calling the real MCP tools — they must reach their own org's flows, runs,
//! resources and triggers and nothing of the other's.

use std::sync::Arc;

use flowr_storage::StorageBackend;
use flowr_types::{ExecutionRequest, ExecutionStatus, Resource, WorkflowRecord};
use serde_json::{json, Value};

const SUPER: &[&str] = &["SuperAdmin", "SecurityAdmin", "Admin", "Operator", "Viewer"];
const DEV: &[&str] = &["Admin", "Operator", "Viewer"];

fn who(user: &str, roles: &[&str]) -> flowr_policy::Principal {
    flowr_policy::Principal::human(user, roles.iter().map(|r| r.to_string()).collect())
}

async fn flow(storage: &Arc<dyn StorageBackend>, user: &str, roles: &[&str], name: &str, input: Value) -> Value {
    let req = ExecutionRequest::new(if name == "a3t-projects-install" { "system" } else { "a3t" }, name).with_input(input).with_principal(who(user, roles));
    let st = flowr_ce::execute_workflow(storage.clone(), &reqwest::Client::new(), req).await.unwrap();
    assert_eq!(st.status, ExecutionStatus::Completed, "{name}: {:?}", st.error);
    let out = st.output.unwrap_or(Value::Null);
    assert_eq!(out["ok"], true, "{user} {name}: {out}");
    out
}

async fn tool(storage: &Arc<dyn StorageBackend>, user: &str, roles: &[&str], name: &str, args: Value) -> (bool, String) {
    let r = flowr_mcp_core::handle_tool_call(storage.as_ref(), &who(user, roles), name, Some(args)).await;
    let text = r.content.iter().map(|c| match c { flowr_mcp_core::protocol::ToolContent::Text { text } => text.clone(), #[allow(unreachable_patterns)] _ => String::new() }).collect::<String>();
    (r.is_error != Some(true), text)
}

fn tiny(ns: &str, name: &str) -> WorkflowRecord {
    WorkflowRecord::from_yaml(&format!(
        "document:\n  dsl: 1.0.3\n  namespace: {ns}\n  name: {name}\n  version: 1.0.0\ndo:\n- a:\n    set:\n      done: true\n"
    )).unwrap()
}

#[tokio::test(flavor = "multi_thread")]
async fn one_org_admin_cannot_see_another_org() {
    let dir = tempfile::tempdir().unwrap();
    let storage = flowr_ce::init_storage(dir.path().to_str().unwrap()).await.unwrap();
    let db = dir.path().join("platform.lance");
    storage.resources().create(Resource::new("a3t-platform-db", "lancedb", json!({ "db_path": db.to_str().unwrap() })).with_namespace("a3t")).await.unwrap();
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../a3t/backend/flows");
    for f in ["system/a3t_projects_install.yaml", "a3t/a3t_profile.yaml", "a3t/a3t_orgs.yaml", "a3t/a3t_projects.yaml", "a3t/a3t_share.yaml"] {
        storage.workflows().register(WorkflowRecord::from_yaml(&std::fs::read_to_string(root.join(f)).unwrap()).unwrap()).await.unwrap();
    }

    // Two orgs, each with an admin and a project.
    flow(&storage, "root", SUPER, "a3t-projects-install", json!({})).await;
    for org in ["acme", "globex"] {
        flow(&storage, "root", SUPER, "a3t-orgs", json!({ "action": "create", "org": org })).await;
    }
    flow(&storage, "root", SUPER, "a3t-orgs", json!({ "action": "set_member", "org": "acme", "user_id": "bob", "role": "admin" })).await;
    flow(&storage, "root", SUPER, "a3t-orgs", json!({ "action": "set_member", "org": "globex", "user_id": "gina", "role": "admin" })).await;
    flow(&storage, "bob", DEV, "a3t-projects", json!({ "action": "create", "owner_kind": "org", "owner": "acme", "name": "web" })).await;
    flow(&storage, "gina", DEV, "a3t-projects", json!({ "action": "create", "owner_kind": "org", "owner": "globex", "name": "crm" })).await;
    for (ns, f) in [("acme-web", "acme-job"), ("globex-crm", "globex-job")] {
        storage.workflows().register(tiny(ns, f)).await.unwrap();
        storage.resources().create(Resource::new("db", "postgres", json!({ "url": format!("postgres://{ns}") })).with_namespace(ns)).await.unwrap();
        storage.triggers().create(json!({ "name": format!("{ns}-hook"), "namespace": ns, "config": { "type": "webhook", "path": format!("/{ns}") },
            "targets": [{ "type": "workflow", "namespace": ns, "name": f }], "enabled": true })).await.unwrap();
    }

    // From here on, enforced — as FlowR EE runs with policy on.
    flowr_ce::access::install(storage.clone());

    // bob runs his flow; gina runs hers (as the worker runs a queued request).
    for (user, ns, f) in [("bob", "acme-web", "acme-job"), ("gina", "globex-crm", "globex-job")] {
        let req = ExecutionRequest::new(ns, f).with_principal(who(user, DEV));
        let st = flowr_ce::execute_workflow(storage.clone(), &reqwest::Client::new(), req).await.unwrap();
        assert_eq!(st.status, ExecutionStatus::Completed);
    }

    // bob: his org's everything; globex's nothing.
    assert!(tool(&storage, "bob", DEV, "list_flows", json!({ "namespace": "acme-web" })).await.1.contains("acme-job"));
    let (ok, text) = tool(&storage, "bob", DEV, "list_flows", json!({ "namespace": "globex-crm" })).await;
    assert!(!ok && text.contains("Not found"), "{text}");
    assert!(!tool(&storage, "bob", DEV, "get_flow", json!({ "namespace": "globex-crm", "name": "globex-job" })).await.0);
    assert!(!tool(&storage, "bob", DEV, "execute_flow", json!({ "namespace": "globex-crm", "name": "globex-job" })).await.0);
    assert!(!tool(&storage, "bob", DEV, "list_resources", json!({ "namespace": "globex-crm" })).await.0);
    assert!(!tool(&storage, "bob", DEV, "get_resource", json!({ "namespace": "globex-crm", "name": "db" })).await.0);
    assert!(!tool(&storage, "bob", DEV, "create_flow", json!({ "yaml": "document:\n  dsl: 1.0.3\n  namespace: globex-crm\n  name: evil\n  version: 1.0.0\ndo:\n- a:\n    set:\n      x: 1\n" })).await.0);
    assert!(!tool(&storage, "bob", DEV, "delete_flow", json!({ "namespace": "globex-crm", "name": "globex-job" })).await.0);

    let (_, triggers) = tool(&storage, "bob", DEV, "list_triggers", json!({})).await;
    assert!(triggers.contains("acme-web-hook") && !triggers.contains("globex-crm-hook"), "{triggers}");

    let (_, runs) = tool(&storage, "bob", DEV, "list_executions", json!({ "limit": 100 })).await;
    assert!(runs.contains("acme-job") && !runs.contains("globex-job"), "{runs}");
    // Not even platform runs of other people (gina's project-flow calls).
    assert!(!runs.contains("\"gina\""), "{runs}");

    let (_, projects) = tool(&storage, "bob", DEV, "list_projects", json!({})).await;
    assert!(projects.contains("acme-web") && !projects.contains("globex-crm"), "{projects}");

    // gina mirrors it; superadmin sees both orgs.
    let (_, runs) = tool(&storage, "gina", DEV, "list_executions", json!({ "limit": 100 })).await;
    assert!(runs.contains("globex-job") && !runs.contains("acme-job"), "{runs}");
    let (_, projects) = tool(&storage, "root", SUPER, "list_projects", json!({})).await;
    assert!(projects.contains("acme-web") && projects.contains("globex-crm"), "{projects}");

    // Someone in no org sees neither, nor the legacy `default` namespace.
    assert!(!tool(&storage, "zed", DEV, "list_flows", json!({ "namespace": "acme-web" })).await.0);
    assert!(!tool(&storage, "zed", DEV, "list_flows", json!({ "namespace": "default" })).await.0);
    // The platform stays readable and runnable.
    assert!(tool(&storage, "zed", DEV, "list_flows", json!({ "namespace": "a3t" })).await.0);

    // ── Exposure: acme publishes into a community project every org can use ──
    flow(&storage, "bob", DEV, "a3t-projects", json!({ "action": "create", "owner_kind": "org", "owner": "acme", "name": "community" })).await;
    let refused = run_flow(&storage, "bob", "a3t-projects", json!({ "action": "update", "namespace": "acme-community",
        "policy": { "exposure": { "audience": "public", "kind": "training" } } })).await;
    assert_eq!(refused["ok"], false, "only the platform orgs label an exposure training: {refused}");
    flow(&storage, "bob", DEV, "a3t-projects", json!({ "action": "update", "namespace": "acme-community",
        "policy": { "exposure": { "audience": "public" } } })).await;
    flow(&storage, "bob", DEV, "a3t-share", json!({ "kind": "flow", "from": "acme-web", "name": "acme-job", "to": "acme-community" })).await;
    tokio::time::sleep(std::time::Duration::from_secs(6)).await; // the guard's snapshot refresh

    // gina (globex) sees it as community, reads and runs it — never changes it.
    let listed = flow(&storage, "gina", DEV, "a3t-projects", json!({ "action": "list" })).await;
    let community: Vec<&Value> = listed["projects"].as_array().unwrap().iter().filter(|p| p["namespace"] == "acme-community").collect();
    assert_eq!(community.len(), 1, "{listed}");
    assert_eq!(community[0]["role"], "community");
    assert!(tool(&storage, "gina", DEV, "get_flow", json!({ "namespace": "acme-community", "name": "acme-job" })).await.0);
    assert!(!tool(&storage, "gina", DEV, "delete_flow", json!({ "namespace": "acme-community", "name": "acme-job" })).await.0);
    assert!(!tool(&storage, "gina", DEV, "get_flow", json!({ "namespace": "acme-web", "name": "acme-job" })).await.0, "the working copy stays private");
    // She clones it into her project; she can't publish into acme's.
    flow(&storage, "gina", DEV, "a3t-share", json!({ "kind": "flow", "from": "acme-community", "name": "acme-job", "to": "globex-crm" })).await;
    assert!(storage.workflows().get("globex-crm", "acme-job").await.is_ok());
    let publish = run_flow(&storage, "gina", "a3t-share", json!({ "kind": "flow", "from": "globex-crm", "name": "globex-job", "to": "acme-community" })).await;
    assert!(publish.is_null(), "refused: no write on acme-community");

    // Wiring: a globex flow may run acme's community flow, not acme's private one.
    for (n, target) in [("wire-ok", "acme-community"), ("wire-private", "acme-web")] {
        storage.workflows().register(WorkflowRecord::from_yaml(&format!(
            "document:\n  dsl: 1.0.3\n  namespace: globex-crm\n  name: {n}\n  version: 1.0.0\ndo:\n- call:\n    run:\n      workflow:\n        namespace: {target}\n        name: acme-job\n        input: {{}}\n"
        )).unwrap()).await.unwrap();
    }
    let run = |n: &'static str| { let s = storage.clone(); async move {
        let req = ExecutionRequest::new("globex-crm", n).with_principal(who("gina", DEV));
        flowr_ce::execute_workflow(s, &reqwest::Client::new(), req).await.unwrap()
    }};
    assert_eq!(run("wire-ok").await.status, ExecutionStatus::Completed);
    let blocked = run("wire-private").await;
    assert_eq!(blocked.status, ExecutionStatus::Failed, "{:?}", blocked.error);
}

/// Run an a3t flow as a user and give back its output — Null when the run failed.
async fn run_flow(storage: &Arc<dyn StorageBackend>, user: &str, name: &str, input: Value) -> Value {
    let req = ExecutionRequest::new("a3t", name).with_input(input).with_principal(who(user, DEV));
    let st = flowr_ce::execute_workflow(storage.clone(), &reqwest::Client::new(), req).await.unwrap();
    st.output.unwrap_or(Value::Null)
}

/// Platform resources move with their secrets intact and references repointed;
/// nobody reads a secret's value back, and a reference isn't mistaken for one.
#[tokio::test(flavor = "multi_thread")]
async fn moving_platform_resources_keeps_secrets_server_side() {
    let dir = tempfile::tempdir().unwrap();
    let storage = flowr_ce::init_storage(dir.path().to_str().unwrap()).await.unwrap();
    let flows = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../a3t/backend/flows");
    storage.workflows().register(WorkflowRecord::from_yaml(&std::fs::read_to_string(flows.join("system/platform_move_resources.yaml")).unwrap()).unwrap()).await.unwrap();
    storage.resources().create(Resource::new("router-key", "secret", json!({ "value": "sk-or-v1-0123456789abcdef" })).with_namespace("default")).await.unwrap();
    storage.resources().create(Resource::new("glm", "openai", json!({ "model": "glm", "api_key_ref": "default/router-key" })).with_namespace("default")).await.unwrap();
    storage.resources().create(Resource::new("deepseek", "openai", json!({ "model": "ds", "api_key_ref": "default/router-key" })).with_namespace("a3t")).await.unwrap();

    // Not superadmin: refused.
    let req = ExecutionRequest::new("system", "platform-move-resources").with_principal(who("bob", DEV))
        .with_input(json!({ "from": "default", "to": "a3t", "names": ["router-key", "glm"], "dry_run": false }));
    let st = flowr_ce::execute_workflow(storage.clone(), &reqwest::Client::new(), req).await.unwrap();
    assert_eq!(st.output.unwrap()["ok"], false);

    let run = |dry: bool| ExecutionRequest::new("system", "platform-move-resources").with_principal(who("root", SUPER))
        .with_input(json!({ "from": "default", "to": "a3t", "names": ["router-key", "glm"], "dry_run": dry }));
    let st = flowr_ce::execute_workflow(storage.clone(), &reqwest::Client::new(), run(true)).await.unwrap();
    let report = st.output.unwrap();
    assert_eq!(report["report"]["repointed"], json!(["a3t/deepseek", "a3t/glm"]), "{report}");
    assert!(storage.resources().get("default", "router-key").await.is_ok(), "a dry run moves nothing");
    flowr_ce::execute_workflow(storage.clone(), &reqwest::Client::new(), run(false)).await.unwrap();

    assert_eq!(storage.resources().get("a3t", "router-key").await.unwrap().config["value"], "sk-or-v1-0123456789abcdef");
    assert!(storage.resources().get("default", "router-key").await.is_err());
    assert_eq!(storage.resources().get("a3t", "glm").await.unwrap().config["api_key_ref"], "a3t/router-key");
    assert_eq!(storage.resources().get("a3t", "deepseek").await.unwrap().config["api_key_ref"], "a3t/router-key");

    // Through the tools: the secret's value is hidden, the reference readable.
    let (_, secret) = tool(&storage, "root", SUPER, "get_resource", json!({ "namespace": "a3t", "name": "router-key" })).await;
    assert!(!secret.contains("0123456789") && secret.contains("****"), "{secret}");
    let (_, llm) = tool(&storage, "root", SUPER, "get_resource", json!({ "namespace": "a3t", "name": "glm" })).await;
    assert!(llm.contains("a3t/router-key"), "{llm}");
}
