//! The engine store across processes (`.ai/FLOWR_QUEUE_CHECKPOINT_SPEC.md` §9,
//! plan stage 7): the console serves its store to runtime pods through a
//! gateway; a runtime runs the same engine worker against [`RemoteEngineStore`],
//! so runs execute on runtimes while the console stays the one place that
//! holds the queue, runs and journals — and enforces fencing.
//!
//! Protocol (HTTP/JSON on its own port, bearer `FLOWR_ENGINE_TOKEN`):
//! - `POST /v1/engine/call` `{op, args}` → `{ok: result}` or `{err: {kind, id?, message}}`
//!   — one op per store method; a store error keeps its kind, so a fenced
//!   writer is told it is fenced.
//! - `GET /v1/engine/subscribe?topic=` → server-sent events, one per notice.
//! - `POST /v1/engine/stream` `[{session_id, kind, data}]` → chat and agent
//!   progress from a runtime, published into the console's session rings.
//!
//! The gateway also records which pools have a worker claiming (the router
//! sends runs only to pools that are live).

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use futures::stream::BoxStream;
use futures::StreamExt;
use serde_json::{json, Value};
use tracing::{info, warn};

use flowr_storage::engine::*;

// ── Live pools ──

fn live() -> &'static Mutex<HashMap<String, Instant>> {
    static L: OnceLock<Mutex<HashMap<String, Instant>>> = OnceLock::new();
    L.get_or_init(Default::default)
}

/// Whether a worker has claimed from `pool` within the last 30 s.
pub fn pool_is_live(pool: &str) -> bool {
    live().lock().unwrap().get(pool).is_some_and(|t| t.elapsed() < Duration::from_secs(30))
}

/// Pools with a live worker, for the console's runtime view.
pub fn live_pools() -> Vec<String> {
    let mut v: Vec<String> = live().lock().unwrap().iter().filter(|(_, t)| t.elapsed() < Duration::from_secs(30)).map(|(p, _)| p.clone()).collect();
    v.sort();
    v
}

pub(crate) fn mark_live(pool: &str) {
    live().lock().unwrap().insert(pool.to_string(), Instant::now());
}

use flowr_storage::engine_wire::{dispatch as dispatch_op, err_json, unwrap, Client, Transport};

/// The console's engine store, used from a runtime pod.
pub type RemoteEngineStore = Client<HttpTransport>;

/// The console's engine store at `base` (its gateway), with `token`.
pub fn remote(base: &str, token: &str) -> RemoteEngineStore {
    Client::new(HttpTransport::new(base, token))
}

/// From `FLOWR_ENGINE_URL` / `FLOWR_ENGINE_TOKEN`, if set.
pub fn remote_from_env() -> Option<RemoteEngineStore> {
    HttpTransport::from_env().map(Client::new)
}

/// Run one op against the console's store, noting which pools claim.
async fn dispatch(store: &dyn EngineStore, op: &str, a: &Value) -> StoreResult<Value> {
    if op == "claim_runs" {
        if let Some(pool) = a.get("pool").and_then(|p| p.as_str()) {
            mark_live(pool);
        }
    }
    dispatch_op(store, op, a).await
}

// ── Gateway (console) ──

#[derive(Clone)]
struct Gateway {
    store: Arc<dyn EngineStore>,
    token: Arc<String>,
}

fn authorised(g: &Gateway, headers: &HeaderMap) -> bool {
    headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .is_some_and(|t| constant_time_eq(t.as_bytes(), g.token.as_bytes()))
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

async fn call(State(g): State<Gateway>, headers: HeaderMap, Json(body): Json<Value>) -> impl IntoResponse {
    if !authorised(&g, &headers) {
        return (StatusCode::UNAUTHORIZED, Json(json!({ "error": "unauthorised" })));
    }
    let op = body.get("op").and_then(|o| o.as_str()).unwrap_or_default().to_string();
    let args = body.get("args").cloned().unwrap_or(Value::Null);
    match dispatch(g.store.as_ref(), &op, &args).await {
        Ok(v) => (StatusCode::OK, Json(json!({ "ok": v }))),
        Err(e) => (StatusCode::OK, Json(json!({ "err": err_json(&e) }))),
    }
}

#[derive(serde::Deserialize)]
struct Topic {
    topic: String,
}

async fn subscribe(State(g): State<Gateway>, headers: HeaderMap, Query(t): Query<Topic>) -> axum::response::Response {
    if !authorised(&g, &headers) {
        return (StatusCode::UNAUTHORIZED, "unauthorised").into_response();
    }
    let events = g.store.subscribe(&t.topic).map(|v| Ok::<_, std::convert::Infallible>(Event::default().data(v.to_string())));
    Sse::new(events).keep_alive(KeepAlive::new().interval(Duration::from_secs(15))).into_response()
}

async fn stream_in(State(g): State<Gateway>, headers: HeaderMap, Json(chunks): Json<Vec<Value>>) -> impl IntoResponse {
    if !authorised(&g, &headers) {
        return StatusCode::UNAUTHORIZED;
    }
    for c in chunks {
        let (Some(s), Some(k)) = (c.get("session_id").and_then(|v| v.as_str()), c.get("kind").and_then(|v| v.as_str())) else { continue };
        crate::token_stream::publish_local(s, k, c.get("data").and_then(|v| v.as_str()).unwrap_or_default());
    }
    StatusCode::NO_CONTENT
}

pub fn gateway_router(store: Arc<dyn EngineStore>, token: String) -> Router {
    Router::new()
        .route("/v1/engine/call", post(call))
        .route("/v1/engine/subscribe", get(subscribe))
        .route("/v1/engine/stream", post(stream_in))
        .with_state(Gateway { store, token: Arc::new(token) })
}

/// Serve the gateway when `FLOWR_ENGINE_TOKEN` is set (port `FLOWR_ENGINE_PORT`,
/// default 3003). Only a process that holds the store serves it.
pub(crate) fn spawn_gateway(store: Arc<dyn EngineStore>) {
    let Some(token) = std::env::var("FLOWR_ENGINE_TOKEN").ok().filter(|t| !t.is_empty()) else { return };
    let port: u16 = std::env::var("FLOWR_ENGINE_PORT").ok().and_then(|p| p.parse().ok()).unwrap_or(3003);
    tokio::spawn(async move {
        let addr = format!("0.0.0.0:{port}");
        match tokio::net::TcpListener::bind(&addr).await {
            Ok(l) => {
                info!(%addr, "engine gateway listening");
                if let Err(e) = axum::serve(l, gateway_router(store, token)).await {
                    warn!(error = %e, "engine gateway stopped");
                }
            }
            Err(e) => warn!(%addr, error = %e, "engine gateway could not bind"),
        }
    });
}

// ── Remote store (runtime) ──

/// The console's engine store, used from a runtime pod.
/// The gateway over HTTP: `RemoteEngineStore` is the wire client on it.
pub struct HttpTransport {
    base: String,
    token: String,
    http: reqwest::Client,
}

impl HttpTransport {
    pub fn new(base: &str, token: &str) -> Self {
        Self {
            base: base.trim_end_matches('/').to_string(),
            token: token.to_string(),
            http: reqwest::Client::builder().timeout(Duration::from_secs(60)).build().expect("http client"),
        }
    }

    /// From `FLOWR_ENGINE_URL` / `FLOWR_ENGINE_TOKEN`, if set.
    pub fn from_env() -> Option<Self> {
        let url = std::env::var("FLOWR_ENGINE_URL").ok().filter(|u| !u.is_empty())?;
        Some(Self::new(&url, &std::env::var("FLOWR_ENGINE_TOKEN").unwrap_or_default()))
    }

    async fn op(&self, op: &str, args: Value) -> StoreResult<Value> {
        // Retried briefly: the console restarting must not fail a run's write.
        let mut wait = Duration::from_millis(200);
        for attempt in 0..5 {
            let res = self.http.post(format!("{}/v1/engine/call", self.base)).bearer_auth(&self.token).json(&json!({ "op": op, "args": args })).send().await;
            match res {
                Ok(r) if r.status().is_success() => {
                    let body: Value = r.json().await.map_err(|e| StoreError::Backend(e.to_string()))?;
                    return unwrap(body);
                }
                Ok(r) if r.status() == reqwest::StatusCode::UNAUTHORIZED => {
                    return Err(StoreError::Backend("the engine gateway refused this runtime's token".into()))
                }
                Ok(r) if attempt == 4 => return Err(StoreError::Backend(format!("engine gateway answered {}", r.status()))),
                Err(e) if attempt == 4 => return Err(StoreError::Backend(format!("engine gateway unreachable: {e}"))),
                _ => {}
            }
            tokio::time::sleep(wait).await;
            wait *= 2;
        }
        unreachable!()
    }

    /// Forward this process's chat/agent chunks to the console.
    pub fn forward_streams(&self) {
        let (tx, mut rx) = futures::channel::mpsc::unbounded::<(String, String, String)>();
        crate::token_stream::forward_to(tx);
        let (base, token, http) = (self.base.clone(), self.token.clone(), self.http.clone());
        tokio::spawn(async move {
            while let Some(first) = rx.next().await {
                let mut batch = vec![first];
                while let Ok(Some(more)) = rx.try_next() {
                    batch.push(more);
                    if batch.len() >= 256 {
                        break;
                    }
                }
                let body: Vec<Value> = batch.into_iter().map(|(s, k, d)| json!({ "session_id": s, "kind": k, "data": d })).collect();
                if let Err(e) = http.post(format!("{base}/v1/engine/stream")).bearer_auth(&token).json(&body).send().await {
                    warn!(error = %e, "could not forward stream chunks to the console");
                }
            }
        });
    }
}

#[async_trait]
impl Transport for HttpTransport {
    async fn call(&self, op: &str, args: Value) -> StoreResult<Value> {
        self.op(op, args).await
    }

    /// Server-sent events from the console, reconnecting until the receiver
    /// is dropped. Notices are best effort: a gap only delays a wake-up.
    fn subscribe(&self, topic: &str) -> BoxStream<'static, Value> {
        let (tx, rx) = futures::channel::mpsc::unbounded();
        let url = format!("{}/v1/engine/subscribe?topic={}", self.base, urlencode(topic));
        let token = self.token.clone();
        let http = reqwest::Client::new();
        tokio::spawn(async move {
            while !tx.is_closed() {
                match http.get(&url).bearer_auth(&token).send().await {
                    Ok(r) if r.status().is_success() => {
                        let mut body = r.bytes_stream();
                        let mut buf = String::new();
                        while let Some(Ok(bytes)) = body.next().await {
                            buf.push_str(&String::from_utf8_lossy(&bytes));
                            while let Some(i) = buf.find("\n\n") {
                                let event: String = buf.drain(..i + 2).collect();
                                for line in event.lines() {
                                    if let Some(d) = line.strip_prefix("data:") {
                                        let v = serde_json::from_str(d.trim()).unwrap_or(Value::Null);
                                        if tx.unbounded_send(v).is_err() {
                                            return;
                                        }
                                    }
                                }
                            }
                        }
                    }
                    _ => {}
                }
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
        });
        rx.boxed()
    }
}

fn urlencode(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => (b as char).to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}
