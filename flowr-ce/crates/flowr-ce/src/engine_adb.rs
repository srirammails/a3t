//! FlowR's engine store is ADB (ADB `.ai/H_ENGINE_STORE.md`, stage H):
//!
//! - **Embedded** (CE, and any process without an ADB server): ADB's durable
//!   engine runs in this process, its data in `<data dir>/adb`. One engine per
//!   directory per process — opening the same directory again shares it.
//! - **Remote** (EE): an ADB server over Arrow Flight, `FLOWR_ENGINE_ADB_URL`
//!   (its Flight endpoint) with the engine token `FLOWR_ENGINE_ADB_TOKEN`.
//!
//! Either way the store is engine space `FLOWR_ENGINE_SPACE` (default
//! `default`; `auto` derives one from the data directory, for tests sharing one
//! ADB), spoken with `flowr_storage::engine_wire` — the same wire as FlowR's
//! own gateway.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use async_trait::async_trait;
use futures::stream::BoxStream;
use futures::StreamExt;
use serde_json::{json, Value};
use tracing::warn;

use flowr_storage::engine::{EngineStore, StoreError, StoreResult};
use flowr_storage::engine_wire::{unwrap, Client, Transport};

/// The engine space this process uses.
pub fn space(data_dir: &str) -> String {
    match std::env::var("FLOWR_ENGINE_SPACE").ok().filter(|s| !s.is_empty()).as_deref() {
        Some("auto") => {
            use std::hash::{Hash, Hasher};
            let mut h = std::collections::hash_map::DefaultHasher::new();
            data_dir.hash(&mut h);
            format!("t{:016x}", h.finish())
        }
        Some(s) => s.to_string(),
        None => "default".to_string(),
    }
}

/// ADB's engine in this process, for `data_dir` (shared by every storage the
/// process opens on that directory).
pub async fn embedded(data_dir: &str) -> anyhow::Result<Arc<dyn EngineStore>> {
    static ENGINES: OnceLock<tokio::sync::Mutex<HashMap<String, Arc<adb_executor::LocalEngine>>>> = OnceLock::new();
    let dir = std::path::Path::new(data_dir).join("adb");
    let key = dir.to_string_lossy().to_string();
    let mut engines = ENGINES.get_or_init(Default::default).lock().await;
    let engine = match engines.get(&key) {
        Some(e) => Arc::clone(e),
        None => {
            let (e, stats) = adb_executor::LocalEngine::durable(&dir).await.map_err(|e| anyhow::anyhow!("ADB engine at {key}: {e}"))?;
            tracing::info!(path = %key, records = stats.records, "engine store: ADB (embedded)");
            let e = Arc::new(e);
            engines.insert(key.clone(), Arc::clone(&e));
            e
        }
    };
    let store = engine.engine_store(&space(data_dir)).ok_or_else(|| anyhow::anyhow!("ADB engine at {key} has no engine store"))?;
    Ok(Arc::new(store))
}

/// The ADB server's engine store, if `FLOWR_ENGINE_ADB_URL` is set.
pub fn remote_from_env(data_dir: &str) -> Option<Client<FlightTransport>> {
    let url = std::env::var("FLOWR_ENGINE_ADB_URL").ok().filter(|u| !u.is_empty())?;
    let token = std::env::var("FLOWR_ENGINE_ADB_TOKEN").ok().filter(|t| !t.is_empty());
    match FlightTransport::new(&url, token, space(data_dir)) {
        Ok(t) => Some(Client::new(t)),
        Err(e) => {
            warn!(url, error = %e, "FLOWR_ENGINE_ADB_URL is not a usable Flight endpoint");
            None
        }
    }
}

/// The engine wire over ADB's Flight: `do_action("engine")` per op,
/// `do_action("engine_subscribe")` for notices.
pub struct FlightTransport {
    url: String,
    channel: tonic::transport::Channel,
    bearer: Option<tonic::metadata::MetadataValue<tonic::metadata::Ascii>>,
    space: String,
}

impl FlightTransport {
    pub fn new(url: &str, token: Option<String>, space: String) -> Result<Self, String> {
        let channel = tonic::transport::Channel::from_shared(url.to_string())
            .map_err(|e| format!("bad ADB Flight URL {url}: {e}"))?
            .connect_timeout(Duration::from_secs(5))
            .connect_lazy();
        let bearer = match token {
            Some(t) => Some(format!("Bearer {t}").parse().map_err(|_| "the engine token is not a valid header".to_string())?),
            None => None,
        };
        Ok(Self { url: url.to_string(), channel, bearer, space })
    }

    fn request<T>(&self, body: T) -> tonic::Request<T> {
        let mut r = tonic::Request::new(body);
        if let Some(b) = &self.bearer {
            r.metadata_mut().insert("authorization", b.clone());
        }
        r
    }

    fn client(&self) -> arrow_flight::flight_service_client::FlightServiceClient<tonic::transport::Channel> {
        arrow_flight::flight_service_client::FlightServiceClient::new(self.channel.clone())
            .max_decoding_message_size(64 << 20)
            .max_encoding_message_size(64 << 20)
    }
}

#[async_trait]
impl Transport for FlightTransport {
    async fn call(&self, op: &str, args: Value) -> StoreResult<Value> {
        let body = json!({ "space": self.space, "op": op, "args": args }).to_string();
        // Retried briefly, like the gateway client: ADB restarting must not fail a run's write.
        let mut wait = Duration::from_millis(200);
        for attempt in 0..5 {
            let action = arrow_flight::Action { r#type: "engine".into(), body: body.clone().into() };
            match self.client().do_action(self.request(action)).await {
                Ok(r) => {
                    let mut s = r.into_inner();
                    let answer = s.next().await.ok_or_else(|| StoreError::Backend(format!("ADB {op}: no answer")))?;
                    let answer = answer.map_err(|e| StoreError::Backend(format!("ADB {op}: {}", e.message())))?;
                    let v: Value = serde_json::from_slice(&answer.body).map_err(|e| StoreError::Backend(format!("ADB {op}: {e}")))?;
                    return unwrap(v);
                }
                Err(e) if matches!(e.code(), tonic::Code::Unauthenticated | tonic::Code::PermissionDenied) => {
                    return Err(StoreError::Backend(format!("ADB at {} refused the engine token: {}", self.url, e.message())));
                }
                Err(e) if matches!(e.code(), tonic::Code::Unavailable | tonic::Code::Unknown) && attempt < 4 => {
                    tokio::time::sleep(wait).await;
                    wait *= 2;
                }
                Err(e) => return Err(StoreError::Backend(format!("ADB at {}: {}", self.url, e.message()))),
            }
        }
        unreachable!()
    }

    /// Notices from ADB, reconnecting until the receiver is dropped (best effort).
    fn subscribe(&self, topic: &str) -> BoxStream<'static, Value> {
        let (tx, rx) = futures::channel::mpsc::unbounded();
        let body = json!({ "space": self.space, "topic": topic }).to_string();
        let (channel, bearer) = (self.channel.clone(), self.bearer.clone());
        tokio::spawn(async move {
            while !tx.is_closed() {
                let mut client = arrow_flight::flight_service_client::FlightServiceClient::new(channel.clone());
                let mut req = tonic::Request::new(arrow_flight::Action { r#type: "engine_subscribe".into(), body: body.clone().into() });
                if let Some(b) = &bearer {
                    req.metadata_mut().insert("authorization", b.clone());
                }
                if let Ok(r) = client.do_action(req).await {
                    let mut s = r.into_inner();
                    while let Some(Ok(n)) = s.next().await {
                        if tx.unbounded_send(serde_json::from_slice(&n.body).unwrap_or(Value::Null)).is_err() {
                            return;
                        }
                    }
                }
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
        });
        rx.boxed()
    }
}

/// The router sends runs only to pools with a live worker (stage 8). With an
/// ADB store, workers claim from ADB, not through this process: learn the live
/// pools from the store every few seconds.
pub(crate) fn spawn_live_pools(engine: Arc<dyn EngineStore>) {
    static STARTED: OnceLock<Mutex<bool>> = OnceLock::new();
    let mut started = STARTED.get_or_init(Default::default).lock().unwrap();
    if *started {
        return;
    }
    *started = true;
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(Duration::from_secs(5));
        loop {
            tick.tick().await;
            match engine.live_pools(Duration::from_secs(30)).await {
                Ok(pools) => pools.iter().for_each(|p| crate::engine_remote::mark_live(p)),
                Err(e) => tracing::debug!(error = %e, "live pools unavailable"),
            }
        }
    });
}
