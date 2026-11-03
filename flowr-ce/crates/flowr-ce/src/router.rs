//! The router (`.ai/A3T_CLUSTER_ARCHITECTURE.md` §10, G1): which pool a run goes to.
//!
//! Rules, first match wins:
//! 1. `gateway` — the gateway flow (`system-gateway/*`) runs in pool `gateway`, the
//!    only pool holding downstream credentials.
//! 2. `platform` — A3T's own flows (`a3t`, `system`, `a3t-*`, `system-*`) run on the
//!    console pool (`default`): they read and write the platform store.
//! 3. `canvas` / `deployment` / `default_pool` — a canvas sandbox, else a runtime the
//!    flow is deployed on, else `FLOWR_DEFAULT_POOL`: the first with a live worker.
//! 4. `fallback` — `default`.
//!
//! A process that keeps its own store and serves no gateway (a canvas sandbox or a
//! CE install) is the only worker there: its runs go to a pool it claims (`alone`).

use flowr_storage::engine::DEFAULT_POOL;

/// The gateway's pool and the namespace of its flows.
pub const GATEWAY_POOL: &str = "gateway";
pub const GATEWAY_NAMESPACE: &str = "system-gateway";

/// A routing decision: the pool, and the rule that chose it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Route {
    pub pool: String,
    pub rule: &'static str,
}

impl Route {
    fn new(pool: impl Into<String>, rule: &'static str) -> Self {
        Self { pool: pool.into(), rule }
    }
}

/// The engine's pool router (installed with `flowr_storage::engine::set_pool_router`).
pub(crate) fn route(namespace: &str, flow: &str) -> String {
    decide(namespace, flow).pool
}

/// Where a run of `namespace/flow` goes, and why.
pub fn decide(namespace: &str, flow: &str) -> Route {
    let shared = shared_route(namespace, flow, &|p| crate::engine_remote::pool_is_live(p), &deployed_on);
    let alone = !crate::engine_remote_enabled() && std::env::var("FLOWR_ENGINE_TOKEN").map_or(true, |t| t.is_empty());
    if alone {
        let mine = crate::engine_worker::worker_pools();
        if !mine.is_empty() && !mine.contains(&shared.pool) {
            return Route::new(mine[0].clone(), "alone");
        }
    }
    shared
}

fn deployed_on(flow_ref: &str) -> Vec<String> {
    crate::ops::deployments_of_flow(flow_ref)
        .into_iter()
        .map(|d| d.runtime)
        .filter(|r| !r.is_empty() && !r.starts_with('('))
        .collect()
}

/// The rules shared by every process (no local-store special case). `live` and
/// `deployed` are passed in so the rules can be tested without a store.
pub(crate) fn shared_route(namespace: &str, flow: &str, live: &dyn Fn(&str) -> bool, deployed: &dyn Fn(&str) -> Vec<String>) -> Route {
    if namespace == GATEWAY_NAMESPACE {
        return Route::new(GATEWAY_POOL, "gateway");
    }
    if is_platform(namespace) {
        return Route::new(DEFAULT_POOL, "platform");
    }
    let flow_ref = format!("{namespace}/{flow}");
    let mut candidates: Vec<(String, &'static str)> = vec![(format!("canvas/{flow_ref}"), "canvas")];
    candidates.extend(deployed(&flow_ref).into_iter().map(|r| (r, "deployment")));
    if let Some(p) = std::env::var("FLOWR_DEFAULT_POOL").ok().filter(|p| !p.is_empty()) {
        candidates.push((p, "default_pool"));
    }
    candidates
        .into_iter()
        .find(|(p, _)| live(p))
        .map(|(p, rule)| Route::new(p, rule))
        .unwrap_or_else(|| Route::new(DEFAULT_POOL, "fallback"))
}

pub(crate) fn is_platform(namespace: &str) -> bool {
    matches!(namespace, "a3t" | "system") || namespace.starts_with("a3t-") || namespace.starts_with("system-")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn none(_: &str) -> Vec<String> {
        vec![]
    }

    #[test]
    fn gateway_flows_go_to_the_gateway_pool_before_the_platform_rule() {
        let r = shared_route(GATEWAY_NAMESPACE, "call", &|_| true, &none);
        assert_eq!(r, Route::new("gateway", "gateway"));
        assert_eq!(shared_route("system-x", "f", &|_| true, &none), Route::new("default", "platform"));
        assert_eq!(shared_route("a3t", "a3t-projects", &|_| true, &none), Route::new("default", "platform"));
    }

    #[test]
    fn user_flows_go_to_the_first_live_candidate() {
        let deployed = |f: &str| if f == "acme/pay" { vec!["worker".to_string()] } else { vec![] };
        assert_eq!(shared_route("acme", "pay", &|p| p == "worker", &deployed), Route::new("worker", "deployment"));
        assert_eq!(shared_route("acme", "pay", &|p| p == "canvas/acme/pay", &deployed), Route::new("canvas/acme/pay", "canvas"));
        assert_eq!(shared_route("acme", "pay", &|_| false, &deployed), Route::new("default", "fallback"));
    }
}
