//! Live token streaming for agent chat — a process-local fan-out hub.
//!
//! Deliberately independent of `StorageBackend`: CE and EE both run embedded
//! LanceDB and both spawn the execution worker in-process (`spawn_worker`), so a
//! `tokio::broadcast` per session reaches the HTTP handler without any storage
//! round-trip. Keying on **session_id** (not instance_id) lets the browser
//! subscribe *before* it has an instance id, closing the race where the first
//! tokens are produced before the client is listening.
//!
//! Each session keeps a small replay ring so a reconnecting client can resume
//! from `Last-Event-ID` instead of losing the tokens it missed — the same
//! durability requirement the session-rehydration path addresses for whole
//! messages.
//!
//! Single-process by design. With more than one replica a client could connect
//! to a pod that is not running its flow; the SSE endpoint degrades to "no
//! tokens" and the UI still falls back to polling + rehydration.

use std::collections::{HashMap, VecDeque};
use std::sync::{Mutex, OnceLock};

use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;

/// How many chunks to retain per session for `Last-Event-ID` replay.
const RING_CAP: usize = 512;
/// Cap on tracked sessions; the least recently touched is evicted past this.
const MAX_SESSIONS: usize = 256;
const CHANNEL_CAP: usize = 1024;

/// One streamed event. `kind` is an open vocabulary so the UI can render
/// progress without the server dictating layout: `token` (text delta),
/// `tool` (a tool call started), `done` (run finished), `error`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamChunk {
    pub seq: u64,
    pub kind: String,
    pub data: String,
}

struct Session {
    tx: broadcast::Sender<StreamChunk>,
    ring: VecDeque<StreamChunk>,
    next_seq: u64,
    touched: u64,
}

static HUB: OnceLock<Mutex<HashMap<String, Session>>> = OnceLock::new();
static CLOCK: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

fn hub() -> &'static Mutex<HashMap<String, Session>> {
    HUB.get_or_init(|| Mutex::new(HashMap::new()))
}

fn tick() -> u64 {
    CLOCK.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

/// Publish a chunk for `session_id`. Never fails and never blocks: with no
/// subscriber the chunk still lands in the replay ring, so a client that
/// connects a moment later receives it.
/// Where a runtime pod sends its chunks: the console that holds the chat
/// (set when this process runs against a remote engine store).
static FORWARD: std::sync::OnceLock<futures::channel::mpsc::UnboundedSender<(String, String, String)>> = std::sync::OnceLock::new();

/// Send every chunk published here on to `tx` as well.
pub fn forward_to(tx: futures::channel::mpsc::UnboundedSender<(String, String, String)>) {
    let _ = FORWARD.set(tx);
}

pub fn publish(session_id: &str, kind: &str, data: impl Into<String>) {
    let data: String = data.into();
    if let Some(tx) = FORWARD.get() {
        let _ = tx.unbounded_send((session_id.to_string(), kind.to_string(), data.clone()));
    }
    publish_local(session_id, kind, data)
}

/// Publish into this process's ring only (chunks forwarded from a runtime).
pub fn publish_local(session_id: &str, kind: &str, data: impl Into<String>) {
    let Ok(mut map) = hub().lock() else { return };
    if !map.contains_key(session_id) && map.len() >= MAX_SESSIONS {
        if let Some(oldest) = map
            .iter()
            .min_by_key(|(_, s)| s.touched)
            .map(|(k, _)| k.clone())
        {
            map.remove(&oldest);
        }
    }
    let entry = map.entry(session_id.to_string()).or_insert_with(|| Session {
        tx: broadcast::channel(CHANNEL_CAP).0,
        ring: VecDeque::with_capacity(RING_CAP),
        next_seq: 1,
        touched: 0,
    });
    let chunk = StreamChunk { seq: entry.next_seq, kind: kind.to_string(), data: data.into() };
    entry.next_seq += 1;
    entry.touched = tick();
    if entry.ring.len() == RING_CAP {
        entry.ring.pop_front();
    }
    entry.ring.push_back(chunk.clone());
    let _ = entry.tx.send(chunk); // Err only means "no receivers"
}

/// Subscribe to `session_id`, returning any retained chunks newer than
/// `after_seq` followed by a live receiver. Replay and subscription are taken
/// under one lock so nothing can slip between them.
pub fn subscribe(
    session_id: &str,
    after_seq: u64,
) -> (Vec<StreamChunk>, broadcast::Receiver<StreamChunk>) {
    let mut map = match hub().lock() {
        Ok(m) => m,
        Err(_) => return (Vec::new(), broadcast::channel(1).0.subscribe()),
    };
    let entry = map.entry(session_id.to_string()).or_insert_with(|| Session {
        tx: broadcast::channel(CHANNEL_CAP).0,
        ring: VecDeque::with_capacity(RING_CAP),
        next_seq: 1,
        touched: 0,
    });
    entry.touched = tick();
    let replay = entry.ring.iter().filter(|c| c.seq > after_seq).cloned().collect();
    (replay, entry.tx.subscribe())
}

/// Subscribe to *future* chunks only, ignoring the replay ring.
///
/// For a newly started run: the caller subscribes before enqueueing, so the
/// worker cannot have emitted anything yet and there is nothing legitimate to
/// replay. Replaying here would resend the previous turn's tokens — the ring is
/// retained across turns so `Last-Event-ID` resume keeps working — and the
/// client would render the last answer again the moment a new message is sent.
pub fn subscribe_live(session_id: &str) -> broadcast::Receiver<StreamChunk> {
    let Ok(mut map) = hub().lock() else {
        return broadcast::channel(1).0.subscribe();
    };
    let entry = map.entry(session_id.to_string()).or_insert_with(|| Session {
        tx: broadcast::channel(CHANNEL_CAP).0,
        ring: VecDeque::with_capacity(RING_CAP),
        next_seq: 1,
        touched: 0,
    });
    entry.touched = tick();
    entry.tx.subscribe()
}

/// Drop a finished session's buffer. Called after `done`/`error` so a long-lived
/// process does not accumulate transcripts.
pub fn close(session_id: &str) {
    if let Ok(mut map) = hub().lock() {
        map.remove(session_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chunks_are_sequenced_and_replayed_from_after_seq() {
        let s = "sess-seq";
        publish(s, "token", "a");
        publish(s, "token", "b");
        publish(s, "token", "c");
        let (replay, _rx) = subscribe(s, 1); // client already saw seq 1
        assert_eq!(replay.iter().map(|c| c.data.as_str()).collect::<Vec<_>>(), vec!["b", "c"]);
        assert_eq!(replay[0].seq, 2);
        close(s);
    }

    #[test]
    fn chunks_published_before_any_subscriber_are_not_lost() {
        let s = "sess-early";
        publish(s, "token", "early"); // nobody listening yet
        let (replay, _rx) = subscribe(s, 0);
        assert_eq!(replay.len(), 1, "replay ring must cover the pre-subscribe race");
        assert_eq!(replay[0].data, "early");
        close(s);
    }

    #[tokio::test]
    async fn live_chunks_reach_an_active_subscriber() {
        let s = "sess-live";
        let (_replay, mut rx) = subscribe(s, 0);
        publish(s, "token", "hello");
        publish(s, "done", "");
        let a = rx.recv().await.unwrap();
        let b = rx.recv().await.unwrap();
        assert_eq!((a.kind.as_str(), a.data.as_str()), ("token", "hello"));
        assert_eq!(b.kind, "done");
        close(s);
    }

    #[test]
    fn close_discards_the_buffer() {
        let s = "sess-close";
        publish(s, "token", "x");
        close(s);
        let (replay, _rx) = subscribe(s, 0);
        assert!(replay.is_empty());
        close(s);
    }

    // ── AI SDK UI Message Stream v1 contract ──
    //
    // These assert the exact frame vocabulary @ai-sdk/svelte expects. The server
    // is Rust and the client is a JS library, so nothing but a test like this
    // catches drift if the protocol version moves.

    /// Drive the translator the same way the endpoint does, collecting frame JSON.
    async fn frames_for(chunks: &[(&str, &str)]) -> Vec<serde_json::Value> {
        let sid = format!("contract-{}", chunks.len());
        for (k, d) in chunks {
            publish(&sid, k, *d);
        }
        let (replay, _rx) = subscribe(&sid, 0);
        // Mirror the endpoint's translation over the replayed chunks.
        let mut out = vec![serde_json::json!({"type":"start"})];
        let (mut text_open, mut step_open) = (false, false);
        let id = "msg_test";
        for c in replay {
            match c.kind.as_str() {
                "step_start" => {
                    if step_open { out.push(serde_json::json!({"type":"finish-step"})); }
                    out.push(serde_json::json!({"type":"start-step"}));
                    step_open = true;
                }
                "token" => {
                    if !text_open { out.push(serde_json::json!({"type":"text-start","id":id})); text_open = true; }
                    out.push(serde_json::json!({"type":"text-delta","id":id,"delta":c.data}));
                }
                "tool_input" => {
                    let v: serde_json::Value = serde_json::from_str(&c.data).unwrap();
                    out.push(serde_json::json!({"type":"tool-input-available",
                        "toolCallId":v["toolCallId"],"toolName":v["toolName"],"input":v["input"]}));
                }
                "tool_output" => {
                    let v: serde_json::Value = serde_json::from_str(&c.data).unwrap();
                    out.push(serde_json::json!({"type":"tool-output-available",
                        "toolCallId":v["toolCallId"],"output":v["output"]}));
                }
                "done" => {
                    if text_open { out.push(serde_json::json!({"type":"text-end","id":id})); text_open = false; }
                    if step_open { out.push(serde_json::json!({"type":"finish-step"})); step_open = false; }
                    out.push(serde_json::json!({"type":"finish"}));
                }
                _ => {}
            }
        }
        close(&sid);
        out
    }

    #[tokio::test]
    async fn text_run_emits_the_v1_frame_sequence() {
        let f = frames_for(&[("step_start",""),("token","Hi"),("token"," there"),("done","Hi there")]).await;
        let kinds: Vec<&str> = f.iter().map(|v| v["type"].as_str().unwrap()).collect();
        assert_eq!(kinds, vec!["start","start-step","text-start","text-delta","text-delta","text-end","finish-step","finish"]);
        // deltas carry the same id as their text-start, which the client uses to join them
        let id = f[2]["id"].as_str().unwrap();
        assert!(f[3]["id"] == id && f[4]["id"] == id);
        assert_eq!(f[3]["delta"], "Hi");
    }

    #[tokio::test]
    async fn tool_call_emits_input_then_output_parts() {
        let f = frames_for(&[
            ("step_start",""),
            ("tool_input", r#"{"toolCallId":"call_1","toolName":"list_flows","input":{"namespace":"a3t"}}"#),
            ("tool_output", r#"{"toolCallId":"call_1","output":"[]"}"#),
            ("step_start",""),
            ("token","Done"),
            ("done","Done"),
        ]).await;
        let kinds: Vec<&str> = f.iter().map(|v| v["type"].as_str().unwrap()).collect();
        assert_eq!(kinds, vec!["start","start-step","tool-input-available","tool-output-available",
                               "finish-step","start-step","text-start","text-delta","text-end","finish-step","finish"]);
        let ti = &f[2];
        assert_eq!(ti["toolCallId"], "call_1");
        assert_eq!(ti["toolName"], "list_flows");
        assert_eq!(ti["input"]["namespace"], "a3t");
        assert_eq!(f[3]["toolCallId"], "call_1");
        // a second iteration opens its own step — multi-turn runs are multi-step
        assert_eq!(kinds.iter().filter(|k| **k == "start-step").count(), 2);
        assert_eq!(kinds.iter().filter(|k| **k == "finish-step").count(), 2);
    }

    #[tokio::test]
    async fn every_opened_block_is_closed_before_finish() {
        let f = frames_for(&[("step_start",""),("token","x"),("done","x")]).await;
        let kinds: Vec<&str> = f.iter().map(|v| v["type"].as_str().unwrap()).collect();
        let pos = |k: &str| kinds.iter().position(|x| *x == k).unwrap();
        assert!(pos("text-start") < pos("text-end"));
        assert!(pos("start-step") < pos("finish-step"));
        assert!(pos("text-end") < pos("finish-step"));
        assert_eq!(*kinds.last().unwrap(), "finish");
    }

    #[tokio::test]
    async fn subscribe_live_ignores_the_previous_turn() {
        // Turn 1 leaves its tokens in the ring (retained for resume).
        let s = "sess-two-turns";
        publish(s, "token", "turn-one-answer");
        publish(s, "done", "turn-one-answer");

        // Turn 2 must not replay them — that re-rendered the prior answer the
        // instant a second message was sent.
        let mut rx = subscribe_live(s);
        publish(s, "token", "turn-two");
        let got = rx.recv().await.unwrap();
        assert_eq!(got.data, "turn-two", "a new run must stream only its own output");

        // The ring still holds everything, so resume-by-seq is unaffected.
        let (replay, _) = subscribe(s, 0);
        assert!(replay.len() >= 3);
        close(s);
    }
}
