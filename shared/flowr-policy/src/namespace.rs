//! Namespace access — who may see and change what in which project.
//!
//! A namespace is a project (A3T's Projects app). Cedar decides *what kind* of
//! step a principal may take; this decides *where*: a member of org A must not
//! list, read, run or change org B's flows, resources, triggers, runs or
//! runtimes, whatever their org-wide role.
//!
//! The rule itself lives with the project data (FlowR CE `access.rs`, over the
//! tables the A3T project flows keep); this module is the seam every boundary
//! asks — MCP tools, the registry, ops and canvas routes. The default guard
//! allows everything, so a FlowR without projects (CE) behaves as before.

use std::collections::BTreeMap;
use std::sync::{Arc, OnceLock, RwLock};

use async_trait::async_trait;

use crate::Principal;

/// What a principal may do in a namespace, weakest first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum NsAccess {
    /// Not even see that it exists.
    None,
    /// List and read.
    Read,
    /// Read, and run its flows.
    Run,
    /// Read, run, and create / change / delete.
    Write,
}

/// Kinds of items, for [`NamespaceGuard::effective_namespace`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ItemKind {
    Flow,
    Resource,
    Trigger,
    Agent,
}

#[async_trait]
pub trait NamespaceGuard: Send + Sync {
    /// What `principal` may do in `namespace`.
    async fn access(&self, principal: &Principal, namespace: &str) -> NsAccess;

    /// A namespace the platform itself owns (`a3t`, `system`): readable by
    /// everyone signed in, its own flows run by anyone, its runs private to who
    /// ran them.
    fn is_platform(&self, _namespace: &str) -> bool {
        false
    }

    /// The namespace an item really belongs to. Before projects, users' work was
    /// saved in the platform namespace; a guard maps that work to the project
    /// that owns it, so it is protected like the rest of that project.
    fn effective_namespace(&self, _kind: ItemKind, namespace: &str, _name: &str, _type_ref: &str, _created_by: &str) -> String {
        namespace.to_string()
    }

    /// Whether `principal` may control (restart, pause, terminate) a runtime with
    /// these labels. A shared runtime is the platform's: superadmin only. A
    /// runtime labelled `a3t.io/org` is that org's: its admins too.
    async fn can_control_runtime(&self, _principal: &Principal, _labels: &BTreeMap<String, String>) -> bool {
        true
    }

    /// Whether `principal` may see a runtime with these labels at all (the
    /// shared ones always; an org's only by its members).
    async fn can_see_runtime(&self, _principal: &Principal, _labels: &BTreeMap<String, String>) -> bool {
        true
    }

    /// Whether a flow of `namespace` may run on a runtime with these labels —
    /// the runtime policy: an org (or a project) set to *isolated* runs only on
    /// its own runtimes; nothing runs on another org's runtime.
    async fn runtime_allowed(&self, _namespace: &str, _labels: &BTreeMap<String, String>) -> bool {
        true
    }

    /// False for the default guard, which allows everything.
    fn enforcing(&self) -> bool {
        false
    }
}

/// Allows everything (CE, and before a guard is installed).
pub struct AllowAllNamespaces;

#[async_trait]
impl NamespaceGuard for AllowAllNamespaces {
    async fn access(&self, _principal: &Principal, _namespace: &str) -> NsAccess {
        NsAccess::Write
    }
}

fn slot() -> &'static RwLock<Arc<dyn NamespaceGuard>> {
    static SLOT: OnceLock<RwLock<Arc<dyn NamespaceGuard>>> = OnceLock::new();
    SLOT.get_or_init(|| RwLock::new(Arc::new(AllowAllNamespaces)))
}

/// The installed guard.
pub fn namespace_guard() -> Arc<dyn NamespaceGuard> {
    slot().read().unwrap().clone()
}

/// Install a guard (FlowR EE, when policy is enforcing).
pub fn set_namespace_guard(guard: Arc<dyn NamespaceGuard>) {
    *slot().write().unwrap() = guard;
}

/// The human a principal acts for: a user, or the user an agent acts on behalf
/// of. `None` for a service (internal machinery, trusted) or an unattended agent.
pub fn acting_user(principal: &Principal) -> Option<&str> {
    match principal {
        Principal::Human { user_id, .. } => Some(user_id),
        Principal::Agent { .. } => principal.on_behalf_of(),
        Principal::Service { .. } => None,
    }
}

/// Whether a principal holds the SuperAdmin role.
pub fn is_superadmin(principal: &Principal) -> bool {
    principal.roles().iter().any(|r| r == crate::roles::SUPER_ADMIN)
}
