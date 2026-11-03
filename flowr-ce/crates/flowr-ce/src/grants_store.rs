//! Session grants kept in the state store (L39): an answer given to an agent
//! on one process (the console, a runtime pod) holds on every other. The
//! ledger (`flowr_policy::grants`) stays the fast, synchronous lookup; every
//! change is written through here in order, and a turn loads the session's
//! answers before it starts.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use flowr_policy::grants::{self, Decision};
use flowr_storage::StorageBackend;
use serde_json::json;
use tracing::warn;

/// The state-store key of a session's answers.
pub fn key(session_id: &str) -> String {
    format!("agent:grants:{session_id}")
}

/// Writes not yet in the store, per session: a load then keeps what this
/// process holds (it is newer).
fn pending() -> &'static Mutex<HashMap<String, usize>> {
    static P: OnceLock<Mutex<HashMap<String, usize>>> = OnceLock::new();
    P.get_or_init(Default::default)
}

/// Write every ledger change to `storage`, in order (once per process).
pub fn install(storage: Arc<dyn StorageBackend>) {
    static ONCE: OnceLock<()> = OnceLock::new();
    if ONCE.set(()).is_err() {
        return;
    }
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<(String, Vec<Decision>)>();
    grants::set_persist(Box::new(move |session, decisions| {
        *pending().lock().unwrap().entry(session.to_string()).or_default() += 1;
        let _ = tx.send((session.to_string(), decisions));
    }));
    tokio::spawn(async move {
        while let Some((session, decisions)) = rx.recv().await {
            let k = key(&session);
            let done = if decisions.is_empty() {
                storage.state().delete(&k).await.map(|_| ())
            } else {
                storage.state().set(&k, json!({ "decisions": decisions }), None).await
            };
            if let Err(e) = done {
                warn!(session = %session, error = %e, "could not keep the session's approval answers; they hold on this process only");
            }
            let mut p = pending().lock().unwrap();
            if let Some(n) = p.get_mut(&session) {
                *n -= 1;
                if *n == 0 {
                    p.remove(&session);
                }
            }
        }
    });
}

/// Load a session's kept answers into the ledger (another process may have
/// answered since), unless this process has newer ones still being written.
pub async fn load(storage: &dyn StorageBackend, session_id: &str) {
    if pending().lock().unwrap().contains_key(session_id) {
        return;
    }
    match storage.state().get(&key(session_id)).await {
        Ok(Some(v)) => {
            let decisions: Vec<Decision> = v.get("decisions").cloned().and_then(|d| serde_json::from_value(d).ok()).unwrap_or_default();
            if !pending().lock().unwrap().contains_key(session_id) {
                grants::hydrate(session_id, decisions);
            }
        }
        Ok(None) => {}
        Err(e) => warn!(session = %session_id, error = %e, "could not load the session's approval answers; using this process's"),
    }
}
