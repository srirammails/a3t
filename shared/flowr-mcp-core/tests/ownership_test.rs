//! Ownership stamping at flow creation (policy-implementation.md, Phase 2).
//!
//! The owner is the default authority over what a flow's agent may be granted,
//! so getting it wrong is not cosmetic — a wrong owner silently changes whose
//! permission bounds every future grant. Two rules are load-bearing:
//!
//!   1. The owner comes from the AUTHENTICATED CALLER, never from the payload.
//!      A flow that could name its own owner could name whose authority bounds
//!      its grants.
//!   2. The owner SURVIVES AN EDIT. `create_flow` is also the save path, so if
//!      re-saving re-stamped the owner, anyone able to save could take over a
//!      flow — and inherit the ability to grant against their own authority.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use flowr_mcp_core::handle_tool_call;
use flowr_storage::{
    CheckpointStore, EventBus, ExecutionQueue, ExecutionStore, ResourceStore, SettingsStore,
    StateStore, StorageBackend, TriggerStore, WorkflowStore,
};
use flowr_types::error::{FlowError, FlowResult};
use flowr_types::{WorkflowRecord, WorkflowSummary};
use serde_json::json;

#[derive(Default)]
struct MemWorkflows {
    rows: Mutex<HashMap<String, WorkflowRecord>>,
}

#[async_trait::async_trait]
impl WorkflowStore for MemWorkflows {
    async fn register(&self, record: WorkflowRecord) -> FlowResult<()> {
        let key = format!("{}/{}", record.namespace(), record.name());
        self.rows.lock().unwrap().insert(key, record);
        Ok(())
    }
    async fn get(&self, namespace: &str, name: &str) -> FlowResult<WorkflowRecord> {
        self.rows
            .lock()
            .unwrap()
            .get(&format!("{namespace}/{name}"))
            .cloned()
            .ok_or_else(|| FlowError::NotFound(format!("{namespace}/{name}")))
    }
    async fn get_version(&self, ns: &str, name: &str, _v: &str) -> FlowResult<WorkflowRecord> {
        self.get(ns, name).await
    }
    async fn list(&self, _namespace: &str) -> FlowResult<Vec<WorkflowSummary>> {
        Ok(vec![])
    }
    async fn delete(&self, namespace: &str, name: &str) -> FlowResult<()> {
        self.rows.lock().unwrap().remove(&format!("{namespace}/{name}"));
        Ok(())
    }
    async fn set_enabled(&self, _n: &str, _m: &str, _e: bool) -> FlowResult<()> {
        Ok(())
    }
    async fn set_active_version(&self, _n: &str, _m: &str, _v: &str) -> FlowResult<()> {
        Ok(())
    }
    async fn exists(&self, namespace: &str, name: &str) -> FlowResult<bool> {
        Ok(self.rows.lock().unwrap().contains_key(&format!("{namespace}/{name}")))
    }
}

/// Only the workflow store is real; every other accessor diverges, so a test
/// that strays outside the creation path fails loudly rather than quietly.
struct TestStorage {
    workflows: MemWorkflows,
}

#[async_trait::async_trait]
impl StorageBackend for TestStorage {
    fn workflows(&self) -> &dyn WorkflowStore {
        &self.workflows
    }
    fn settings(&self) -> &dyn SettingsStore {
        unimplemented!("not part of flow creation")
    }
    fn resources(&self) -> &dyn ResourceStore {
        unimplemented!("not part of flow creation")
    }
    fn state(&self) -> &dyn StateStore {
        unimplemented!("not part of flow creation")
    }
    fn checkpoints(&self) -> &dyn CheckpointStore {
        unimplemented!("not part of flow creation")
    }
    fn events(&self) -> &dyn EventBus {
        unimplemented!("not part of flow creation")
    }
    fn queue(&self) -> &dyn ExecutionQueue {
        unimplemented!("not part of flow creation")
    }
    fn executions(&self) -> &dyn ExecutionStore {
        unimplemented!("not part of flow creation")
    }
    fn triggers(&self) -> &dyn TriggerStore {
        unimplemented!("not part of flow creation")
    }
    async fn init(&self) -> FlowResult<()> {
        Ok(())
    }
    async fn close(&self) -> FlowResult<()> {
        Ok(())
    }
}

fn yaml(title: &str) -> String {
    format!(
        "document:\n  dsl: '1.0.3'\n  namespace: default\n  name: owned-flow\n  version: '1.0.0'\n  title: {title}\ndo:\n  - step1:\n      set:\n        a: 1\n"
    )
}

fn human(id: &str) -> flowr_policy::Principal {
    flowr_policy::Principal::human(id, vec!["Admin".to_string()])
}

async fn create(storage: &Arc<TestStorage>, who: &str, title: &str) {
    let res = handle_tool_call(
        storage.as_ref(),
        &human(who),
        "create_flow",
        Some(json!({ "yaml": yaml(title) })),
    )
    .await;
    assert_ne!(res.is_error, Some(true), "create_flow failed: {res:?}");
}

#[tokio::test]
async fn owner_is_stamped_from_the_caller() {
    let storage = Arc::new(TestStorage { workflows: MemWorkflows::default() });
    create(&storage, "usr-alice", "first").await;

    let rec = storage.workflows.get("default", "owned-flow").await.unwrap();
    assert_eq!(rec.metadata.owner.as_deref(), Some("usr-alice"));
}

#[tokio::test]
async fn owner_is_not_taken_from_the_payload() {
    // The YAML cannot name an owner: even if a caller smuggles one into the
    // document, the stamped owner is the authenticated principal.
    let storage = Arc::new(TestStorage { workflows: MemWorkflows::default() });
    let smuggled = "document:\n  dsl: '1.0.3'\n  namespace: default\n  name: owned-flow\n  version: '1.0.0'\ndo:\n  - step1:\n      set:\n        owner: usr-attacker\n";
    let res = handle_tool_call(
        storage.as_ref(),
        &human("usr-alice"),
        "create_flow",
        Some(json!({ "yaml": smuggled })),
    )
    .await;
    assert_ne!(res.is_error, Some(true), "create_flow failed: {res:?}");

    let rec = storage.workflows.get("default", "owned-flow").await.unwrap();
    assert_eq!(rec.metadata.owner.as_deref(), Some("usr-alice"));
}

#[tokio::test]
async fn editing_someone_elses_flow_does_not_transfer_ownership() {
    // The rule that matters. create_flow is also the save path; if a re-save
    // re-stamped the owner, anyone who could save could take the flow over.
    let storage = Arc::new(TestStorage { workflows: MemWorkflows::default() });
    create(&storage, "usr-alice", "first").await;
    create(&storage, "usr-bob", "bob edited this").await;

    let rec = storage.workflows.get("default", "owned-flow").await.unwrap();
    assert_eq!(
        rec.metadata.owner.as_deref(),
        Some("usr-alice"),
        "an edit by bob must not transfer ownership away from alice"
    );
    // …and the edit itself did land.
    assert!(rec.raw_yaml.contains("bob edited this"), "the edit should still be saved");
}
