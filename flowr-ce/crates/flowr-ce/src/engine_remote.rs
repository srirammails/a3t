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
use chrono::{DateTime, Utc};
use futures::stream::BoxStream;
use futures::StreamExt;
use serde::de::DeserializeOwned;
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

// ── Errors on the wire ──

fn err_json(e: &StoreError) -> Value {
    match e {
        StoreError::Fenced(r) => json!({ "kind": "fenced", "id": r, "message": e.to_string() }),
        StoreError::LeaseLost(t) => json!({ "kind": "lease_lost", "id": t, "message": e.to_string() }),
        StoreError::NotFound(m) => json!({ "kind": "not_found", "message": m }),
        StoreError::Conflict(m) => json!({ "kind": "conflict", "message": m }),
        StoreError::Backend(m) => json!({ "kind": "backend", "message": m }),
    }
}

fn err_from(v: &Value) -> StoreError {
    let id = || v.get("id").and_then(|i| i.as_str()).and_then(|i| i.parse().ok()).unwrap_or_default();
    let msg = v.get("message").and_then(|m| m.as_str()).unwrap_or("").to_string();
    match v.get("kind").and_then(|k| k.as_str()).unwrap_or("") {
        "fenced" => StoreError::Fenced(id()),
        "lease_lost" => StoreError::LeaseLost(id()),
        "not_found" => StoreError::NotFound(msg),
        "conflict" => StoreError::Conflict(msg),
        _ => StoreError::Backend(msg),
    }
}

fn arg<T: DeserializeOwned>(args: &Value, k: &str) -> StoreResult<T> {
    serde_json::from_value(args.get(k).cloned().unwrap_or(Value::Null)).map_err(|e| StoreError::Backend(format!("bad argument `{k}`: {e}")))
}

fn ms(args: &Value, k: &str) -> StoreResult<Duration> {
    Ok(Duration::from_millis(arg::<u64>(args, k)?))
}

fn out<T: serde::Serialize>(v: StoreResult<T>) -> StoreResult<Value> {
    v.map(|x| serde_json::to_value(x).unwrap_or(Value::Null))
}

/// Run one op against the console's store.
async fn dispatch(store: &dyn EngineStore, op: &str, a: &Value) -> StoreResult<Value> {
    match op {
        "enqueue" => out(store.enqueue(arg(a, "task")?).await),
        "heartbeat" => out(store.heartbeat(&arg::<Vec<Lease>>(a, "leases")?, ms(a, "extend_ms")?).await),
        "ack" => out(store.ack(&arg(a, "lease")?).await),
        "nack" => out(store.nack(&arg(a, "lease")?, arg(a, "retry_at")?).await),
        "pending" => out(store.pending(&arg::<String>(a, "pool")?).await),
        "live_pools" => out(store.live_pools(ms(a, "within_ms")?).await),
        "start" => out(store.start(arg(a, "run")?, arg(a, "task")?).await),
        "claim_runs" => {
            let pool: String = arg(a, "pool")?;
            mark_live(&pool);
            out(store.claim_runs(&pool, &arg::<String>(a, "worker")?, arg(a, "n")?, ms(a, "lease_ms")?).await)
        }
        "renew" => out(store.renew(arg(a, "run")?, arg(a, "epoch")?, ms(a, "lease_ms")?).await),
        "release" => out(store.release(arg(a, "run")?, arg(a, "epoch")?, arg(a, "phase")?, arg(a, "outcome")?, &arg(a, "lease")?).await),
        "request" => out(store.request(arg(a, "run")?, arg(a, "req")?).await),
        "get" => out(store.get(arg(a, "run")?).await),
        "list" => out(store.list(arg(a, "q")?).await),
        "orphaned" => out(store.orphaned().await),
        "purge_finished" => out(store.purge_finished(arg(a, "before")?).await),
        "open_wait" => out(store.open_wait(arg(a, "run")?, arg(a, "epoch")?, &arg::<String>(a, "wait_id")?, arg(a, "correlation")?).await),
        "close_wait" => out(store.close_wait(arg(a, "run")?, arg(a, "epoch")?, &arg::<String>(a, "wait_id")?).await),
        "deliver_event" => out(store.deliver_event(&arg::<String>(a, "namespace")?, arg(a, "event")?).await),
        "open_approval" => out(store.open_approval(arg(a, "run")?, arg(a, "epoch")?, &arg::<String>(a, "wait_id")?, arg(a, "request")?).await),
        "approvals" => out(store.approvals().await),
        "decide_approval" => out(store.decide_approval(&arg::<String>(a, "wait_id")?, arg(a, "decision")?, &arg::<String>(a, "by")?).await),
        "append" => out(store.append(arg(a, "run")?, arg(a, "epoch")?, &arg::<Vec<Record>>(a, "records")?).await),
        "read_from" => out(store.read_from(arg(a, "run")?, arg(a, "after_seq")?).await),
        "commit_step" => out(store.commit_step(arg(a, "run")?, arg(a, "epoch")?, arg(a, "commit")?).await),
        "latest_checkpoint" => out(store.latest_checkpoint(arg(a, "run")?).await),
        "checkpoints" => out(store.checkpoints(arg(a, "run")?).await),
        "outbox" => out(store.outbox(arg(a, "limit")?).await),
        "outbox_done" => out(store.outbox_done(arg(a, "up_to_id")?).await),
        "publish" => out(store.publish(&arg::<String>(a, "topic")?, arg(a, "msg")?).await),
        other => Err(StoreError::Backend(format!("unknown op `{other}`"))),
    }
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
pub struct RemoteEngineStore {
    base: String,
    token: String,
    http: reqwest::Client,
}

impl RemoteEngineStore {
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

    async fn op<T: DeserializeOwned>(&self, op: &str, args: Value) -> StoreResult<T> {
        // Retried briefly: the console restarting must not fail a run's write.
        let mut wait = Duration::from_millis(200);
        for attempt in 0..5 {
            let res = self.http.post(format!("{}/v1/engine/call", self.base)).bearer_auth(&self.token).json(&json!({ "op": op, "args": args })).send().await;
            match res {
                Ok(r) if r.status().is_success() => {
                    let body: Value = r.json().await.map_err(|e| StoreError::Backend(e.to_string()))?;
                    if let Some(e) = body.get("err") {
                        return Err(err_from(e));
                    }
                    return serde_json::from_value(body.get("ok").cloned().unwrap_or(Value::Null)).map_err(|e| StoreError::Backend(e.to_string()));
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
impl TaskQueue for RemoteEngineStore {
    async fn enqueue(&self, t: NewTask) -> StoreResult<EnqueueOutcome> {
        self.op("enqueue", json!({ "task": t })).await
    }
    async fn heartbeat(&self, leases: &[Lease], extend: Duration) -> StoreResult<Vec<LeaseStatus>> {
        self.op("heartbeat", json!({ "leases": leases, "extend_ms": extend.as_millis() as u64 })).await
    }
    async fn ack(&self, lease: &Lease) -> StoreResult<()> {
        self.op("ack", json!({ "lease": lease })).await
    }
    async fn nack(&self, lease: &Lease, retry_at: Option<DateTime<Utc>>) -> StoreResult<()> {
        self.op("nack", json!({ "lease": lease, "retry_at": retry_at })).await
    }
    async fn pending(&self, pool: &str) -> StoreResult<usize> {
        self.op("pending", json!({ "pool": pool })).await
    }
    async fn live_pools(&self, within: Duration) -> StoreResult<Vec<String>> {
        self.op("live_pools", json!({ "within_ms": within.as_millis() as u64 })).await
    }
}

#[async_trait]
impl RunStore for RemoteEngineStore {
    async fn start(&self, run: NewRun, task: NewTask) -> StoreResult<(RunId, EnqueueOutcome)> {
        self.op("start", json!({ "run": run, "task": task })).await
    }
    async fn claim_runs(&self, pool: &str, worker: &str, n: usize, lease: Duration) -> StoreResult<Vec<(Lease, Task, Epoch)>> {
        self.op("claim_runs", json!({ "pool": pool, "worker": worker, "n": n, "lease_ms": lease.as_millis() as u64 })).await
    }
    async fn renew(&self, run: RunId, epoch: Epoch, lease: Duration) -> StoreResult<bool> {
        self.op("renew", json!({ "run": run, "epoch": epoch, "lease_ms": lease.as_millis() as u64 })).await
    }
    async fn release(&self, run: RunId, epoch: Epoch, phase: RunPhase, outcome: RunOutcome, lease: &Lease) -> StoreResult<()> {
        self.op("release", json!({ "run": run, "epoch": epoch, "phase": phase, "outcome": outcome, "lease": lease })).await
    }
    async fn request(&self, run: RunId, req: RunRequest) -> StoreResult<()> {
        self.op("request", json!({ "run": run, "req": req })).await
    }
    async fn get(&self, run: RunId) -> StoreResult<Option<RunRow>> {
        self.op("get", json!({ "run": run })).await
    }
    async fn list(&self, q: RunQuery) -> StoreResult<Page<RunRow>> {
        self.op("list", json!({ "q": q })).await
    }
    async fn orphaned(&self) -> StoreResult<Vec<RunRow>> {
        self.op("orphaned", json!({})).await
    }
    async fn purge_finished(&self, before: DateTime<Utc>) -> StoreResult<usize> {
        self.op("purge_finished", json!({ "before": before })).await
    }
    async fn open_wait(&self, run: RunId, epoch: Epoch, wait_id: &str, correlation: Value) -> StoreResult<()> {
        self.op("open_wait", json!({ "run": run, "epoch": epoch, "wait_id": wait_id, "correlation": correlation })).await
    }
    async fn close_wait(&self, run: RunId, epoch: Epoch, wait_id: &str) -> StoreResult<bool> {
        self.op("close_wait", json!({ "run": run, "epoch": epoch, "wait_id": wait_id })).await
    }
    async fn deliver_event(&self, namespace: &str, event: Value) -> StoreResult<Vec<RunId>> {
        self.op("deliver_event", json!({ "namespace": namespace, "event": event })).await
    }
    async fn open_approval(&self, run: RunId, epoch: Epoch, wait_id: &str, request: Value) -> StoreResult<()> {
        self.op("open_approval", json!({ "run": run, "epoch": epoch, "wait_id": wait_id, "request": request })).await
    }
    async fn approvals(&self) -> StoreResult<Vec<(RunId, String, Value)>> {
        self.op("approvals", json!({})).await
    }
    async fn decide_approval(&self, wait_id: &str, decision: Value, by: &str) -> StoreResult<bool> {
        self.op("decide_approval", json!({ "wait_id": wait_id, "decision": decision, "by": by })).await
    }
}

#[async_trait]
impl Journal for RemoteEngineStore {
    async fn append(&self, run: RunId, epoch: Epoch, records: &[Record]) -> StoreResult<AppendOutcome> {
        self.op("append", json!({ "run": run, "epoch": epoch, "records": records })).await
    }
    async fn read_from(&self, run: RunId, after_seq: u64) -> StoreResult<Vec<JournalEntry>> {
        self.op("read_from", json!({ "run": run, "after_seq": after_seq })).await
    }
    async fn commit_step(&self, run: RunId, epoch: Epoch, c: StepCommit) -> StoreResult<u64> {
        self.op("commit_step", json!({ "run": run, "epoch": epoch, "commit": c })).await
    }
    async fn latest_checkpoint(&self, run: RunId) -> StoreResult<Option<(u64, Vec<u8>)>> {
        self.op("latest_checkpoint", json!({ "run": run })).await
    }
    async fn checkpoints(&self, run: RunId) -> StoreResult<Vec<CheckpointMeta>> {
        self.op("checkpoints", json!({ "run": run })).await
    }
    async fn outbox(&self, limit: usize) -> StoreResult<Vec<OutboxRow>> {
        self.op("outbox", json!({ "limit": limit })).await
    }
    async fn outbox_done(&self, up_to_id: u64) -> StoreResult<()> {
        self.op("outbox_done", json!({ "up_to_id": up_to_id })).await
    }
}

#[async_trait]
impl Notifier for RemoteEngineStore {
    async fn publish(&self, topic: &str, msg: Value) -> StoreResult<()> {
        self.op("publish", json!({ "topic": topic, "msg": msg })).await
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
