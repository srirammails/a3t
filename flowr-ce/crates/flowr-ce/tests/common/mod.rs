//! A fake model and a fake MCP server for agent tests: the model asks for the
//! `poke` tool once, then answers; every model call and every `poke` is
//! counted, so "not repeated" is measured.

#![allow(dead_code)]

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use axum::response::IntoResponse;
use serde_json::{json, Value};

#[derive(Default)]
pub struct Counts {
    pub model: AtomicUsize,
    pub pokes: AtomicUsize,
}

impl Counts {
    pub fn model(&self) -> usize {
        self.model.load(Ordering::SeqCst)
    }
    pub fn pokes(&self) -> usize {
        self.pokes.load(Ordering::SeqCst)
    }
}

fn sse(frames: Vec<Value>) -> axum::response::Response {
    let mut body = String::new();
    for f in frames {
        body.push_str(&format!("data: {f}\n\n"));
    }
    body.push_str("data: [DONE]\n\n");
    ([("content-type", "text/event-stream")], body).into_response()
}

/// Serves `/v1/chat/completions` (streamed) and `/mcp/message`; returns its base URL.
pub async fn fake_server() -> (String, Arc<Counts>) {
    let counts = Arc::new(Counts::default());
    let (c1, c2) = (counts.clone(), counts.clone());
    let app = axum::Router::new()
        .route(
            "/v1/chat/completions",
            axum::routing::post(move |axum::Json(body): axum::Json<Value>| {
                let c = c1.clone();
                async move {
                    c.model.fetch_add(1, Ordering::SeqCst);
                    let msgs = body["messages"].as_array().cloned().unwrap_or_default();
                    match msgs.iter().rev().find(|m| m["role"] == "tool") {
                        None => sse(vec![json!({ "choices": [{ "index": 0, "delta": { "tool_calls": [{
                            "index": 0, "id": "c1", "type": "function",
                            "function": { "name": "poke", "arguments": "{\"n\":1}" }
                        }] } }] })]),
                        Some(t) => sse(vec![json!({ "choices": [{ "index": 0, "delta": {
                            "content": format!("all done: {}", t["content"].as_str().unwrap_or_default())
                        } }] })]),
                    }
                }
            }),
        )
        .route(
            "/mcp/message",
            axum::routing::post(move |axum::Json(body): axum::Json<Value>| {
                let c = c2.clone();
                async move {
                    let id = body["id"].clone();
                    let result = match body["method"].as_str() {
                        Some("tools/list") => json!({ "tools": [{ "name": "poke", "description": "Poke once", "inputSchema": { "type": "object" } }] }),
                        Some("tools/call") => {
                            let n = c.pokes.fetch_add(1, Ordering::SeqCst) + 1;
                            json!({ "content": [{ "type": "text", "text": json!({ "poked": n }).to_string() }] })
                        }
                        _ => json!({}),
                    };
                    axum::Json(json!({ "jsonrpc": "2.0", "id": id, "result": result }))
                }
            }),
        );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    (format!("http://{addr}"), counts)
}

/// The fake model and MCP server as resources `default/fake` and `t/tools`.
pub async fn register_fakes(s: &dyn flowr_storage::StorageBackend, base: &str) {
    let llm = flowr_types::Resource::new("fake", "llm", json!({ "api_url": format!("{base}/v1"), "model": "m", "api_key": "x" }));
    s.resources().create(llm).await.unwrap();
    let mcp = flowr_types::Resource::new("tools", "mcp_server", json!({ "url": format!("{base}/mcp") })).with_namespace("t");
    s.resources().create(mcp).await.unwrap();
}

/// A flow whose one step is an agent that may call `poke`.
pub fn agent_flow(name: &str) -> String {
    format!(
        r#"
document: {{ dsl: '1.0.3', namespace: t, name: {name}, version: '1.0.0' }}
do:
  - chat:
      call: a3t:agent
      with:
        message: please poke
        session_id: s-{name}
        config:
          llm_resource: default/fake
          mcp_servers: [t/tools]
"#
    )
}
