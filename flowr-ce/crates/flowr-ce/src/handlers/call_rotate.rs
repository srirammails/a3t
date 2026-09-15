//! call:rotate handler — RotatE knowledge-graph embeddings over LanceDB
//!
//! RotatE (Sun et al., 2019) models a relation as an element-wise rotation in
//! complex space: given head/tail entity embeddings as complex vectors
//! `h, t ∈ C^d` and a relation phase vector `θ_r ∈ (-π, π]^d`, the relation is
//! `r = e^{iθ_r}` and a fact `(h, r, t)` is scored by
//!
//!     score(h, r, t) = γ − ‖ h ∘ r − t ‖₁
//!
//! where `∘` is the Hadamard (element-wise) complex product. Plausible triples
//! have small distance `d = ‖h ∘ r − t‖`. This handler ingests ConceptNet-style
//! triples, trains the embeddings with negative-sampling SGD, and answers link
//! prediction / nearest-neighbour / lookup queries — all persisted in LanceDB.
//!
//! Storage layout (one LanceDB database, three tables):
//!   - kg_entities(id Utf8, name Utf8, etype Utf8, emb FixedSizeList<Float32; 2d>)
//!       emb = [ re_0..re_{d-1}, im_0..im_{d-1} ]
//!   - kg_relations(id Utf8, name Utf8, phase FixedSizeList<Float32; d>)
//!   - kg_triples(head Utf8, relation Utf8, tail Utf8, weight Float32)
//!
//! Operations (the `operation` field of `with`):
//!   ingest, train, score, predict_tail, predict_head, neighbors, lookup,
//!   entities, relations, stats
//!
//! Compute (training, scoring, ranking) runs in-process in Rust; LanceDB is the
//! durable store for triples and trained vectors.

use async_trait::async_trait;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::f32::consts::PI;
use std::sync::Arc;
use tracing::{debug, info, warn};

use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};

use arrow_array::{
    Array, FixedSizeListArray, Float32Array, RecordBatch, RecordBatchIterator, StringArray,
};
use arrow_schema::{DataType, Field, Schema};

use swf_parser::{NamedTask, Task};
use swf_runtime::context::ExecutionContext;
use swf_runtime::error::{Result, RuntimeError};
use swf_runtime::traits::{TaskHandler, TaskResult};

const DEFAULT_DIM: usize = 256;
const DEFAULT_GAMMA: f32 = 12.0;
const EPS: f32 = 1e-6;
const ENTITIES_TABLE: &str = "kg_entities";
const RELATIONS_TABLE: &str = "kg_relations";
const TRIPLES_TABLE: &str = "kg_triples";

pub struct RotateHandler;

#[async_trait]
impl TaskHandler for RotateHandler {
    fn task_type(&self) -> &str {
        "call:rotate"
    }

    async fn execute(
        &self,
        task: &NamedTask,
        _input: Value,
        ctx: &mut ExecutionContext,
    ) -> Result<TaskResult> {
        let with = match &task.task {
            Task::Call(c) => c.with.clone().unwrap_or(json!({})),
            _ => return Err(RuntimeError::task_failed(&task.name, "expected call task")),
        };

        let operation = with.get("operation").and_then(|v| v.as_str()).unwrap_or("stats");

        let resource_name = with.get("_resource").and_then(|v| v.as_str());
        let db_path = match resource_name {
            Some(rn) => resolve_db_path(ctx, rn)
                .await
                .unwrap_or_else(|| "/data/a3t/a3t-kg.lance".to_string()),
            None => "/data/a3t/a3t-kg.lance".to_string(),
        };

        info!(task = %task.name, op = %operation, db = %db_path, "call:rotate");

        let db = lancedb::connect(&db_path)
            .execute()
            .await
            .map_err(|e| RuntimeError::Communication(format!("LanceDB connect error: {}", e)))?;

        let out = match operation {
            "ingest" => op_ingest(&db, &with).await,
            "train" => op_train(&db, &with).await,
            "score" => op_score(&db, &with).await,
            "predict_tail" => op_predict(&db, &with, true).await,
            "predict_head" => op_predict(&db, &with, false).await,
            "neighbors" => op_neighbors(&db, &with).await,
            "lookup" => op_lookup(&db, &with).await,
            "entities" => op_entities(&db, &with).await,
            "relations" => op_relations(&db).await,
            "stats" => op_stats(&db, &with).await,
            other => {
                warn!(op = %other, "unsupported rotate operation");
                Err(RuntimeError::task_failed(
                    &task.name,
                    format!("unsupported rotate operation: {}", other),
                ))
            }
        }?;

        Ok(TaskResult::new(out))
    }
}

// ─────────────────────────────────────────────────────────────────────────
// In-memory model
// ─────────────────────────────────────────────────────────────────────────

/// RotatE model held in memory while serving an operation.
struct Model {
    dim: usize,
    /// entity id → row index
    ent_index: HashMap<String, usize>,
    ent_ids: Vec<String>,
    ent_names: Vec<String>,
    ent_types: Vec<String>,
    /// flat entity embeddings, row-major: row r occupies [r*2d .. r*2d+2d),
    /// first d are real parts, next d are imaginary parts.
    ent_emb: Vec<f32>,
    /// relation id → row index
    rel_index: HashMap<String, usize>,
    rel_ids: Vec<String>,
    rel_names: Vec<String>,
    /// flat relation phases, row-major: row r occupies [r*d .. r*d+d).
    rel_phase: Vec<f32>,
    /// triples as (head_idx, rel_idx, tail_idx, weight)
    triples: Vec<(usize, usize, usize, f32)>,
}

impl Model {
    fn n_ent(&self) -> usize {
        self.ent_ids.len()
    }
    fn n_rel(&self) -> usize {
        self.rel_ids.len()
    }

    fn ent_re(&self, idx: usize) -> &[f32] {
        &self.ent_emb[idx * 2 * self.dim..idx * 2 * self.dim + self.dim]
    }
    fn ent_im(&self, idx: usize) -> &[f32] {
        &self.ent_emb[idx * 2 * self.dim + self.dim..idx * 2 * self.dim + 2 * self.dim]
    }
    fn rel_phase(&self, idx: usize) -> &[f32] {
        &self.rel_phase[idx * self.dim..idx * self.dim + self.dim]
    }

    /// Distance d = ‖ h ∘ r − t ‖₁ (sum of complex moduli).
    fn distance(&self, h: usize, r: usize, t: usize) -> f32 {
        let (hre, him, phase, tre, tim) = (
            self.ent_re(h),
            self.ent_im(h),
            self.rel_phase(r),
            self.ent_re(t),
            self.ent_im(t),
        );
        let mut d = 0.0f32;
        for k in 0..self.dim {
            let (c, s) = (phase[k].cos(), phase[k].sin());
            let a = hre[k] * c - him[k] * s; // rotated real
            let b = hre[k] * s + him[k] * c; // rotated imag
            let dr = a - tre[k];
            let di = b - tim[k];
            d += (dr * dr + di * di).sqrt();
        }
        d
    }

    fn score(&self, h: usize, r: usize, t: usize, gamma: f32) -> f32 {
        gamma - self.distance(h, r, t)
    }

    /// Apply an SGD step for one triple given a loss-gradient coefficient
    /// `coef = ∂loss/∂d`. Updates head, tail and relation parameters in place.
    fn step(&mut self, h: usize, r: usize, t: usize, coef: f32, lr: f32) {
        let d = self.dim;
        let lc = lr * coef;
        for k in 0..d {
            let phase = self.rel_phase[r * d + k];
            let (c, s) = (phase.cos(), phase.sin());
            let hre = self.ent_emb[h * 2 * d + k];
            let him = self.ent_emb[h * 2 * d + d + k];
            let tre = self.ent_emb[t * 2 * d + k];
            let tim = self.ent_emb[t * 2 * d + d + k];

            let a = hre * c - him * s;
            let b = hre * s + him * c;
            let dr = a - tre;
            let di = b - tim;
            let m = (dr * dr + di * di).sqrt() + EPS;
            let gma = dr / m; // ∂m/∂a
            let gmb = di / m; // ∂m/∂b

            // gradients of m w.r.t. parameters
            let g_hre = gma * c + gmb * s;
            let g_him = -gma * s + gmb * c;
            let g_tre = -gma;
            let g_tim = -gmb;
            let g_phase = gma * (-b) + gmb * a;

            self.ent_emb[h * 2 * d + k] -= lc * g_hre;
            self.ent_emb[h * 2 * d + d + k] -= lc * g_him;
            self.ent_emb[t * 2 * d + k] -= lc * g_tre;
            self.ent_emb[t * 2 * d + d + k] -= lc * g_tim;
            // keep phase within (-π, π]
            let mut np = self.rel_phase[r * d + k] - lc * g_phase;
            np = ((np + PI).rem_euclid(2.0 * PI)) - PI;
            self.rel_phase[r * d + k] = np;
        }
    }
}

fn sigmoid(x: f32) -> f32 {
    if x >= 0.0 {
        1.0 / (1.0 + (-x).exp())
    } else {
        let e = x.exp();
        e / (1.0 + e)
    }
}

// ─────────────────────────────────────────────────────────────────────────
// Operations
// ─────────────────────────────────────────────────────────────────────────

/// ingest: merge new ConceptNet triples + entities/relations into the store.
/// Embeddings for new entities/relations are randomly initialised; existing
/// ones are preserved. Re-ingesting the same triple is a no-op (deduplicated).
async fn op_ingest(db: &lancedb::Connection, with: &Value) -> Result<Value> {
    let raw = with
        .get("triples")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    if raw.is_empty() {
        return Err(RuntimeError::Communication(
            "ingest requires non-empty 'triples'".to_string(),
        ));
    }

    let mut model = load_model(db).await?;
    let dim = if model.dim != 0 {
        model.dim
    } else {
        with.get("dim").and_then(|v| v.as_u64()).unwrap_or(DEFAULT_DIM as u64) as usize
    };
    model.dim = dim;

    let mut rng = StdRng::seed_from_u64(seed_from(with, 42));

    let ensure_entity =
        |model: &mut Model, rng: &mut StdRng, name: &str, etype: &str| -> usize {
            if let Some(&i) = model.ent_index.get(name) {
                // upgrade unknown type if a better one arrives
                if model.ent_types[i] == "Concept" && etype != "Concept" && !etype.is_empty() {
                    model.ent_types[i] = etype.to_string();
                }
                return i;
            }
            let i = model.ent_ids.len();
            model.ent_index.insert(name.to_string(), i);
            model.ent_ids.push(name.to_string());
            model.ent_names.push(name.to_string());
            model
                .ent_types
                .push(if etype.is_empty() { "Concept".to_string() } else { etype.to_string() });
            // RotatE entities init ~ U(-1, 1) per real/imag component
            for _ in 0..2 * model.dim {
                model.ent_emb.push(rng.gen_range(-1.0f32..1.0f32));
            }
            i
        };
    let ensure_relation = |model: &mut Model, rng: &mut StdRng, name: &str| -> usize {
        if let Some(&i) = model.rel_index.get(name) {
            return i;
        }
        let i = model.rel_ids.len();
        model.rel_index.insert(name.to_string(), i);
        model.rel_ids.push(name.to_string());
        model.rel_names.push(name.to_string());
        for _ in 0..model.dim {
            model.rel_phase.push(rng.gen_range(-PI..PI));
        }
        i
    };

    // existing triple keys for dedup
    let mut seen: std::collections::HashSet<(usize, usize, usize)> =
        model.triples.iter().map(|&(h, r, t, _)| (h, r, t)).collect();

    let mut added = 0usize;
    for item in &raw {
        let head = item.get("head").and_then(|v| v.as_str()).unwrap_or("").trim();
        let rel = item.get("relation").and_then(|v| v.as_str()).unwrap_or("").trim();
        let tail = item.get("tail").and_then(|v| v.as_str()).unwrap_or("").trim();
        if head.is_empty() || rel.is_empty() || tail.is_empty() {
            continue;
        }
        let weight = item.get("weight").and_then(|v| v.as_f64()).unwrap_or(1.0) as f32;
        let htype = item.get("head_type").and_then(|v| v.as_str()).unwrap_or("");
        let ttype = item.get("tail_type").and_then(|v| v.as_str()).unwrap_or("");

        let h = ensure_entity(&mut model, &mut rng, head, htype);
        let t = ensure_entity(&mut model, &mut rng, tail, ttype);
        let r = ensure_relation(&mut model, &mut rng, rel);
        if seen.insert((h, r, t)) {
            model.triples.push((h, r, t, weight));
            added += 1;
        }
    }

    persist_model(db, &model).await?;

    Ok(json!({
        "ingested": added,
        "entities": model.n_ent(),
        "relations": model.n_rel(),
        "triples": model.triples.len(),
        "dim": model.dim,
    }))
}

/// train: negative-sampling SGD on the RotatE objective.
async fn op_train(db: &lancedb::Connection, with: &Value) -> Result<Value> {
    let mut model = load_model(db).await?;
    if model.triples.is_empty() {
        return Err(RuntimeError::Communication(
            "no triples to train on — ingest first".to_string(),
        ));
    }
    let epochs = with.get("epochs").and_then(|v| v.as_u64()).unwrap_or(200) as usize;
    let lr = with.get("lr").and_then(|v| v.as_f64()).unwrap_or(0.001) as f32;
    let gamma = with.get("gamma").and_then(|v| v.as_f64()).unwrap_or(DEFAULT_GAMMA as f64) as f32;
    let neg = with.get("neg_samples").and_then(|v| v.as_u64()).unwrap_or(8).max(1) as usize;

    let mut rng = StdRng::seed_from_u64(seed_from(with, 7));
    let n_ent = model.n_ent();
    let triples = model.triples.clone();

    let mut last_loss = 0.0f32;
    let mut order: Vec<usize> = (0..triples.len()).collect();

    for epoch in 0..epochs {
        shuffle(&mut order, &mut rng);
        let mut epoch_loss = 0.0f32;
        for &ti in &order {
            let (h, r, t, _w) = triples[ti];

            // positive
            let d_pos = model.distance(h, r, t);
            let pos_sig = sigmoid(gamma - d_pos);
            epoch_loss += -(pos_sig.max(EPS)).ln();
            // ∂loss/∂d for positive = 1 − σ(γ − d)
            model.step(h, r, t, 1.0 - pos_sig, lr);

            // negatives: corrupt head or tail uniformly
            let coef_scale = 1.0 / neg as f32;
            for _ in 0..neg {
                let corrupt_tail = rng.gen_bool(0.5);
                let (nh, nt) = if corrupt_tail {
                    (h, rng.gen_range(0..n_ent))
                } else {
                    (rng.gen_range(0..n_ent), t)
                };
                let d_neg = model.distance(nh, r, nt);
                let neg_sig = sigmoid(d_neg - gamma);
                epoch_loss += -coef_scale * (neg_sig.max(EPS)).ln();
                // ∂loss/∂d for negative = σ(d − γ) − 1  (≤ 0 ⇒ pushes distance up)
                model.step(nh, r, nt, (neg_sig - 1.0) * coef_scale, lr);
            }
        }
        last_loss = epoch_loss / triples.len() as f32;
        if epoch % 20 == 0 || epoch == epochs - 1 {
            debug!(epoch, loss = last_loss, "rotate train");
        }
    }

    let (mrr, hits10) = eval_metrics(&model, gamma, &mut rng);

    persist_model(db, &model).await?;

    Ok(json!({
        "trained": true,
        "epochs": epochs,
        "loss": last_loss,
        "mrr": mrr,
        "hits_at_10": hits10,
        "entities": model.n_ent(),
        "relations": model.n_rel(),
        "triples": model.triples.len(),
        "dim": model.dim,
        "gamma": gamma,
    }))
}

async fn op_score(db: &lancedb::Connection, with: &Value) -> Result<Value> {
    let model = load_model(db).await?;
    let gamma = with.get("gamma").and_then(|v| v.as_f64()).unwrap_or(DEFAULT_GAMMA as f64) as f32;
    let head = with.get("head").and_then(|v| v.as_str()).unwrap_or("");
    let rel = with.get("relation").and_then(|v| v.as_str()).unwrap_or("");
    let tail = with.get("tail").and_then(|v| v.as_str()).unwrap_or("");

    let (h, r, t) = match (
        model.ent_index.get(head),
        model.rel_index.get(rel),
        model.ent_index.get(tail),
    ) {
        (Some(&h), Some(&r), Some(&t)) => (h, r, t),
        _ => {
            return Ok(json!({
                "found": false,
                "reason": "head, relation or tail not in graph",
            }))
        }
    };
    let d = model.distance(h, r, t);
    Ok(json!({
        "found": true,
        "head": head, "relation": rel, "tail": tail,
        "distance": d,
        "score": gamma - d,
    }))
}

/// predict_tail (forward=true): rank candidate tails for (head, relation).
/// predict_head (forward=false): rank candidate heads for (relation, tail).
async fn op_predict(db: &lancedb::Connection, with: &Value, forward: bool) -> Result<Value> {
    let model = load_model(db).await?;
    let gamma = with.get("gamma").and_then(|v| v.as_f64()).unwrap_or(DEFAULT_GAMMA as f64) as f32;
    let limit = with.get("limit").and_then(|v| v.as_u64()).unwrap_or(10) as usize;

    let rel = with.get("relation").and_then(|v| v.as_str()).unwrap_or("");
    let r = match model.rel_index.get(rel) {
        Some(&r) => r,
        None => return Ok(json!({ "found": false, "reason": "relation not in graph", "results": [] })),
    };

    let anchor_name = if forward {
        with.get("head").and_then(|v| v.as_str()).unwrap_or("")
    } else {
        with.get("tail").and_then(|v| v.as_str()).unwrap_or("")
    };
    let anchor = match model.ent_index.get(anchor_name) {
        Some(&a) => a,
        None => return Ok(json!({ "found": false, "reason": "anchor entity not in graph", "results": [] })),
    };

    // known facts to flag (so the UI can hide/keep them)
    let known: std::collections::HashSet<usize> = model
        .triples
        .iter()
        .filter_map(|&(h, rr, t, _)| {
            if rr != r {
                return None;
            }
            if forward && h == anchor {
                Some(t)
            } else if !forward && t == anchor {
                Some(h)
            } else {
                None
            }
        })
        .collect();

    let mut scored: Vec<(usize, f32)> = (0..model.n_ent())
        .map(|cand| {
            let s = if forward {
                model.score(anchor, r, cand, gamma)
            } else {
                model.score(cand, r, anchor, gamma)
            };
            (cand, s)
        })
        .collect();
    scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

    let results: Vec<Value> = scored
        .into_iter()
        .take(limit)
        .map(|(i, s)| {
            json!({
                "id": model.ent_ids[i],
                "name": model.ent_names[i],
                "type": model.ent_types[i],
                "score": s,
                "known": known.contains(&i),
            })
        })
        .collect();

    Ok(json!({
        "found": true,
        "relation": rel,
        "anchor": anchor_name,
        "direction": if forward { "tail" } else { "head" },
        "results": results,
    }))
}

/// neighbors: nearest entities to a given entity in embedding space (L2).
async fn op_neighbors(db: &lancedb::Connection, with: &Value) -> Result<Value> {
    let model = load_model(db).await?;
    let limit = with.get("limit").and_then(|v| v.as_u64()).unwrap_or(10) as usize;
    let entity = with.get("entity").and_then(|v| v.as_str()).unwrap_or("");
    let anchor = match model.ent_index.get(entity) {
        Some(&a) => a,
        None => return Ok(json!({ "found": false, "reason": "entity not in graph", "results": [] })),
    };

    let w = 2 * model.dim;
    let av = &model.ent_emb[anchor * w..anchor * w + w];
    let mut scored: Vec<(usize, f32)> = (0..model.n_ent())
        .filter(|&i| i != anchor)
        .map(|i| {
            let bv = &model.ent_emb[i * w..i * w + w];
            let mut dist = 0.0f32;
            for k in 0..w {
                let dx = av[k] - bv[k];
                dist += dx * dx;
            }
            (i, dist.sqrt())
        })
        .collect();
    scored.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));

    let results: Vec<Value> = scored
        .into_iter()
        .take(limit)
        .map(|(i, dist)| {
            json!({
                "id": model.ent_ids[i],
                "name": model.ent_names[i],
                "type": model.ent_types[i],
                "distance": dist,
                "similarity": 1.0 / (1.0 + dist),
            })
        })
        .collect();

    Ok(json!({ "found": true, "entity": entity, "results": results }))
}

/// lookup: filter stored triples by any combination of head / relation / tail.
async fn op_lookup(db: &lancedb::Connection, with: &Value) -> Result<Value> {
    let model = load_model(db).await?;
    let limit = with.get("limit").and_then(|v| v.as_u64()).unwrap_or(100) as usize;
    let head = with.get("head").and_then(|v| v.as_str());
    let rel = with.get("relation").and_then(|v| v.as_str());
    let tail = with.get("tail").and_then(|v| v.as_str());

    let hi = head.and_then(|h| model.ent_index.get(h).copied());
    let ri = rel.and_then(|r| model.rel_index.get(r).copied());
    let ti = tail.and_then(|t| model.ent_index.get(t).copied());

    let mut rows = Vec::new();
    for &(h, r, t, w) in &model.triples {
        if head.is_some() && hi != Some(h) {
            continue;
        }
        if rel.is_some() && ri != Some(r) {
            continue;
        }
        if tail.is_some() && ti != Some(t) {
            continue;
        }
        rows.push(json!({
            "head": model.ent_ids[h],
            "head_type": model.ent_types[h],
            "relation": model.rel_ids[r],
            "tail": model.ent_ids[t],
            "tail_type": model.ent_types[t],
            "weight": w,
        }));
        if rows.len() >= limit {
            break;
        }
    }
    Ok(json!({ "count": rows.len(), "triples": rows }))
}

/// entities: paginated entity rows with vector norm + preview (Vectors tab).
async fn op_entities(db: &lancedb::Connection, with: &Value) -> Result<Value> {
    let model = load_model(db).await?;
    let limit = with.get("limit").and_then(|v| v.as_u64()).unwrap_or(100) as usize;
    let offset = with.get("offset").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
    let q = with.get("query").and_then(|v| v.as_str()).unwrap_or("").to_lowercase();
    let w = 2 * model.dim;

    let mut rows = Vec::new();
    let mut matched = 0usize;
    for i in 0..model.n_ent() {
        if !q.is_empty()
            && !model.ent_names[i].to_lowercase().contains(&q)
            && !model.ent_types[i].to_lowercase().contains(&q)
        {
            continue;
        }
        if matched < offset {
            matched += 1;
            continue;
        }
        matched += 1;
        if rows.len() >= limit {
            continue;
        }
        let emb = &model.ent_emb[i * w..i * w + w];
        let norm = emb.iter().map(|x| x * x).sum::<f32>().sqrt();
        let preview: Vec<f32> = emb.iter().take(8).copied().collect();
        rows.push(json!({
            "id": model.ent_ids[i],
            "name": model.ent_names[i],
            "type": model.ent_types[i],
            "dims": model.dim,
            "norm": norm,
            "preview": preview,
        }));
    }
    Ok(json!({ "count": matched, "dim": model.dim, "entities": rows }))
}

async fn op_relations(db: &lancedb::Connection) -> Result<Value> {
    let model = load_model(db).await?;
    let mut counts: HashMap<usize, usize> = HashMap::new();
    for &(_, r, _, _) in &model.triples {
        *counts.entry(r).or_insert(0) += 1;
    }
    let rows: Vec<Value> = (0..model.n_rel())
        .map(|i| {
            json!({
                "id": model.rel_ids[i],
                "name": model.rel_names[i],
                "triples": counts.get(&i).copied().unwrap_or(0),
            })
        })
        .collect();
    Ok(json!({ "count": rows.len(), "relations": rows }))
}

/// stats: graph summary + a 3D random-projection of entity embeddings and the
/// edges among the sampled nodes, for the 3D Graph tab.
async fn op_stats(db: &lancedb::Connection, with: &Value) -> Result<Value> {
    let model = load_model(db).await?;
    let sample = with.get("sample").and_then(|v| v.as_u64()).unwrap_or(2000) as usize;
    let n = model.n_ent().min(sample);
    let w = 2 * model.dim;

    // type histogram (over all entities)
    let mut type_counts: HashMap<String, usize> = HashMap::new();
    for ty in &model.ent_types {
        *type_counts.entry(ty.clone()).or_insert(0) += 1;
    }

    // 3 fixed random orthonormal-ish projection vectors (seeded → deterministic)
    let mut prng = StdRng::seed_from_u64(1234567);
    let proj: Vec<Vec<f32>> = (0..3)
        .map(|_| (0..w).map(|_| prng.gen_range(-1.0f32..1.0f32)).collect())
        .collect();

    let mut points = Vec::with_capacity(n);
    for i in 0..n {
        let emb = &model.ent_emb[i * w..i * w + w];
        let mut xyz = [0.0f32; 3];
        for (a, pv) in proj.iter().enumerate() {
            let mut acc = 0.0f32;
            for k in 0..w {
                acc += emb[k] * pv[k];
            }
            xyz[a] = acc * (60.0 / (w as f32).sqrt());
        }
        points.push(json!({
            "id": model.ent_ids[i],
            "name": model.ent_names[i],
            "type": model.ent_types[i],
            "x": xyz[0], "y": xyz[1], "z": xyz[2],
        }));
    }

    // edges among sampled nodes
    let max_edges = with.get("max_edges").and_then(|v| v.as_u64()).unwrap_or(8000) as usize;
    let mut edges = Vec::new();
    for &(h, r, t, _) in &model.triples {
        if h < n && t < n {
            edges.push(json!({
                "source": h,
                "target": t,
                "relation": model.rel_ids[r],
            }));
            if edges.len() >= max_edges {
                break;
            }
        }
    }

    Ok(json!({
        "entities": model.n_ent(),
        "relations": model.n_rel(),
        "triples": model.triples.len(),
        "dim": model.dim,
        "types": type_counts,
        "sampled": n,
        "points": points,
        "edges": edges,
    }))
}

// ─────────────────────────────────────────────────────────────────────────
// Metrics
// ─────────────────────────────────────────────────────────────────────────

/// Filtered-ish MRR / Hits@10 over a sample of triples via tail ranking.
fn eval_metrics(model: &Model, gamma: f32, rng: &mut StdRng) -> (f32, f32) {
    let n = model.triples.len();
    if n == 0 || model.n_ent() == 0 {
        return (0.0, 0.0);
    }
    let sample = n.min(200);
    let mut mrr = 0.0f32;
    let mut hits = 0.0f32;
    for _ in 0..sample {
        let (h, r, t, _) = model.triples[rng.gen_range(0..n)];
        let true_score = model.score(h, r, t, gamma);
        // rank = 1 + #candidates strictly better than the true tail
        let mut rank = 1usize;
        let cand_cap = model.n_ent().min(1000);
        for c in 0..cand_cap {
            if c == t {
                continue;
            }
            if model.score(h, r, c, gamma) > true_score {
                rank += 1;
            }
        }
        mrr += 1.0 / rank as f32;
        if rank <= 10 {
            hits += 1.0;
        }
    }
    (mrr / sample as f32, hits / sample as f32)
}

// ─────────────────────────────────────────────────────────────────────────
// Persistence (LanceDB <-> Model)
// ─────────────────────────────────────────────────────────────────────────

async fn load_model(db: &lancedb::Connection) -> Result<Model> {
    let tables = db
        .table_names()
        .execute()
        .await
        .map_err(|e| RuntimeError::Communication(format!("list tables: {}", e)))?;

    let mut model = Model {
        dim: 0,
        ent_index: HashMap::new(),
        ent_ids: Vec::new(),
        ent_names: Vec::new(),
        ent_types: Vec::new(),
        ent_emb: Vec::new(),
        rel_index: HashMap::new(),
        rel_ids: Vec::new(),
        rel_names: Vec::new(),
        rel_phase: Vec::new(),
        triples: Vec::new(),
    };

    if tables.contains(&ENTITIES_TABLE.to_string()) {
        let batches = scan_all(db, ENTITIES_TABLE).await?;
        for batch in &batches {
            let ids = str_col(batch, "id");
            let names = str_col(batch, "name");
            let types = str_col(batch, "etype");
            let (embs, width) = vec_col(batch, "emb");
            if width > 0 && model.dim == 0 {
                model.dim = width / 2;
            }
            for row in 0..batch.num_rows() {
                let id = ids[row].clone();
                model.ent_index.insert(id.clone(), model.ent_ids.len());
                model.ent_ids.push(id);
                model.ent_names.push(names[row].clone());
                model.ent_types.push(types[row].clone());
                model.ent_emb.extend_from_slice(&embs[row]);
            }
        }
    }

    if tables.contains(&RELATIONS_TABLE.to_string()) {
        let batches = scan_all(db, RELATIONS_TABLE).await?;
        for batch in &batches {
            let ids = str_col(batch, "id");
            let names = str_col(batch, "name");
            let (phases, width) = vec_col(batch, "phase");
            if width > 0 && model.dim == 0 {
                model.dim = width;
            }
            for row in 0..batch.num_rows() {
                let id = ids[row].clone();
                model.rel_index.insert(id.clone(), model.rel_ids.len());
                model.rel_ids.push(id);
                model.rel_names.push(names[row].clone());
                model.rel_phase.extend_from_slice(&phases[row]);
            }
        }
    }

    if tables.contains(&TRIPLES_TABLE.to_string()) {
        let batches = scan_all(db, TRIPLES_TABLE).await?;
        for batch in &batches {
            let heads = str_col(batch, "head");
            let rels = str_col(batch, "relation");
            let tails = str_col(batch, "tail");
            let weights = f32_col(batch, "weight");
            for row in 0..batch.num_rows() {
                let (h, r, t) = (
                    model.ent_index.get(&heads[row]).copied(),
                    model.rel_index.get(&rels[row]).copied(),
                    model.ent_index.get(&tails[row]).copied(),
                );
                if let (Some(h), Some(r), Some(t)) = (h, r, t) {
                    model.triples.push((h, r, t, weights.get(row).copied().unwrap_or(1.0)));
                }
            }
        }
    }

    Ok(model)
}

async fn persist_model(db: &lancedb::Connection, model: &Model) -> Result<()> {
    let d = model.dim;

    // entities
    {
        let ids = StringArray::from(model.ent_ids.clone());
        let names = StringArray::from(model.ent_names.clone());
        let types = StringArray::from(model.ent_types.clone());
        let emb = fixed_list_array(&model.ent_emb, 2 * d);
        let schema = Arc::new(Schema::new(vec![
            Field::new("id", DataType::Utf8, false),
            Field::new("name", DataType::Utf8, true),
            Field::new("etype", DataType::Utf8, true),
            Field::new("emb", emb.data_type().clone(), false),
        ]));
        let batch = RecordBatch::try_new(
            schema.clone(),
            vec![Arc::new(ids), Arc::new(names), Arc::new(types), Arc::new(emb)],
        )
        .map_err(|e| RuntimeError::Communication(format!("entities batch: {}", e)))?;
        recreate_table(db, ENTITIES_TABLE, batch, schema).await?;
    }

    // relations
    {
        let ids = StringArray::from(model.rel_ids.clone());
        let names = StringArray::from(model.rel_names.clone());
        let phase = fixed_list_array(&model.rel_phase, d);
        let schema = Arc::new(Schema::new(vec![
            Field::new("id", DataType::Utf8, false),
            Field::new("name", DataType::Utf8, true),
            Field::new("phase", phase.data_type().clone(), false),
        ]));
        let batch = RecordBatch::try_new(
            schema.clone(),
            vec![Arc::new(ids), Arc::new(names), Arc::new(phase)],
        )
        .map_err(|e| RuntimeError::Communication(format!("relations batch: {}", e)))?;
        recreate_table(db, RELATIONS_TABLE, batch, schema).await?;
    }

    // triples
    {
        let heads = StringArray::from(model.triples.iter().map(|t| model.ent_ids[t.0].clone()).collect::<Vec<_>>());
        let rels = StringArray::from(model.triples.iter().map(|t| model.rel_ids[t.1].clone()).collect::<Vec<_>>());
        let tails = StringArray::from(model.triples.iter().map(|t| model.ent_ids[t.2].clone()).collect::<Vec<_>>());
        let weights = Float32Array::from(model.triples.iter().map(|t| t.3).collect::<Vec<_>>());
        let schema = Arc::new(Schema::new(vec![
            Field::new("head", DataType::Utf8, false),
            Field::new("relation", DataType::Utf8, false),
            Field::new("tail", DataType::Utf8, false),
            Field::new("weight", DataType::Float32, true),
        ]));
        let batch = RecordBatch::try_new(
            schema.clone(),
            vec![Arc::new(heads), Arc::new(rels), Arc::new(tails), Arc::new(weights)],
        )
        .map_err(|e| RuntimeError::Communication(format!("triples batch: {}", e)))?;
        recreate_table(db, TRIPLES_TABLE, batch, schema).await?;
    }

    Ok(())
}

async fn recreate_table(
    db: &lancedb::Connection,
    name: &str,
    batch: RecordBatch,
    schema: Arc<Schema>,
) -> Result<()> {
    let _ = db.drop_table(name, &[]).await; // ignore "not found"
    let batches = RecordBatchIterator::new(vec![Ok(batch)], schema);
    db.create_table(name, Box::new(batches))
        .execute()
        .await
        .map_err(|e| RuntimeError::Communication(format!("create table {}: {}", name, e)))?;
    Ok(())
}

// ─────────────────────────────────────────────────────────────────────────
// Arrow helpers
// ─────────────────────────────────────────────────────────────────────────

fn fixed_list_array(flat: &[f32], width: usize) -> FixedSizeListArray {
    let field = Arc::new(Field::new("item", DataType::Float32, true));
    let values = Arc::new(Float32Array::from(flat.to_vec()));
    // try_new validates len == n*width; flat is always a multiple of width here
    FixedSizeListArray::try_new(field, width as i32, values, None)
        .expect("fixed size list construction")
}

async fn scan_all(db: &lancedb::Connection, name: &str) -> Result<Vec<RecordBatch>> {
    use futures::TryStreamExt;
    use lancedb::query::{ExecutableQuery, QueryBase};

    let table = db
        .open_table(name)
        .execute()
        .await
        .map_err(|e| RuntimeError::Communication(format!("open {}: {}", name, e)))?;
    // NB: lancedb maps the limit to an i64; usize::MAX overflows to a negative
    // value and is rejected. i32::MAX is an effectively-unbounded full scan.
    let batches: Vec<RecordBatch> = table
        .query()
        .limit(i32::MAX as usize)
        .execute()
        .await
        .map_err(|e| RuntimeError::Communication(format!("scan {}: {}", name, e)))?
        .try_collect()
        .await
        .map_err(|e| RuntimeError::Communication(format!("collect {}: {}", name, e)))?;
    Ok(batches)
}

fn col_idx(batch: &RecordBatch, name: &str) -> Option<usize> {
    batch.schema().fields().iter().position(|f| f.name() == name)
}

fn str_col(batch: &RecordBatch, name: &str) -> Vec<String> {
    match col_idx(batch, name) {
        Some(i) => {
            let col = batch.column(i);
            if let Some(a) = col.as_any().downcast_ref::<StringArray>() {
                (0..a.len())
                    .map(|r| if a.is_valid(r) { a.value(r).to_string() } else { String::new() })
                    .collect()
            } else {
                vec![String::new(); batch.num_rows()]
            }
        }
        None => vec![String::new(); batch.num_rows()],
    }
}

fn f32_col(batch: &RecordBatch, name: &str) -> Vec<f32> {
    match col_idx(batch, name) {
        Some(i) => {
            let col = batch.column(i);
            if let Some(a) = col.as_any().downcast_ref::<Float32Array>() {
                (0..a.len()).map(|r| if a.is_valid(r) { a.value(r) } else { 0.0 }).collect()
            } else {
                vec![1.0; batch.num_rows()]
            }
        }
        None => vec![1.0; batch.num_rows()],
    }
}

/// Read a FixedSizeList<Float32> column → (per-row Vec<f32>, element width).
fn vec_col(batch: &RecordBatch, name: &str) -> (Vec<Vec<f32>>, usize) {
    let idx = match col_idx(batch, name) {
        Some(i) => i,
        None => return (vec![Vec::new(); batch.num_rows()], 0),
    };
    let col = batch.column(idx);
    let fsl = match col.as_any().downcast_ref::<FixedSizeListArray>() {
        Some(a) => a,
        None => return (vec![Vec::new(); batch.num_rows()], 0),
    };
    let width = fsl.value_length() as usize;
    let mut out = Vec::with_capacity(batch.num_rows());
    for r in 0..fsl.len() {
        let elem = fsl.value(r);
        if let Some(f) = elem.as_any().downcast_ref::<Float32Array>() {
            out.push((0..f.len()).map(|k| f.value(k)).collect());
        } else {
            out.push(vec![0.0; width]);
        }
    }
    (out, width)
}

// ─────────────────────────────────────────────────────────────────────────
// Misc helpers
// ─────────────────────────────────────────────────────────────────────────

fn seed_from(with: &Value, default: u64) -> u64 {
    with.get("seed").and_then(|v| v.as_u64()).unwrap_or(default)
}

fn shuffle<T>(v: &mut [T], rng: &mut StdRng) {
    // Fisher–Yates
    for i in (1..v.len()).rev() {
        let j = rng.gen_range(0..=i);
        v.swap(i, j);
    }
}

async fn resolve_db_path(ctx: &ExecutionContext, resource_name: &str) -> Option<String> {
    let storage_any = ctx.storage.as_ref()?;
    let storage = storage_any
        .downcast_ref::<std::sync::Arc<dyn flowr_storage::StorageBackend>>()?;
    let (ns, name) = if resource_name.contains('/') {
        let parts: Vec<&str> = resource_name.splitn(2, '/').collect();
        (parts[0], parts[1])
    } else {
        ("a3t", resource_name)
    };
    let resource = storage.resources().get(ns, name).await.ok()?;
    resource.config.get("db_path").and_then(|v| v.as_str()).map(String::from)
}

// ─────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_db() -> String {
        // unique-ish path under the system temp dir (no Date/rand-time needed)
        static CTR: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let n = CTR.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let mut p = std::env::temp_dir();
        p.push(format!("a3t-rotate-test-{}-{}.lance", std::process::id(), n));
        let _ = std::fs::remove_dir_all(&p);
        p.to_string_lossy().to_string()
    }

    fn triple(h: &str, r: &str, t: &str) -> Value {
        json!({ "head": h, "relation": r, "tail": t, "weight": 1.0 })
    }

    #[tokio::test]
    async fn ingest_train_predict_roundtrip() {
        let path = tmp_db();
        let db = lancedb::connect(&path).execute().await.unwrap();

        // Small "IsA" star + a chain so RotatE has consistent structure to learn.
        let triples = vec![
            triple("cat", "IsA", "animal"),
            triple("dog", "IsA", "animal"),
            triple("cow", "IsA", "animal"),
            triple("sparrow", "IsA", "bird"),
            triple("eagle", "IsA", "bird"),
            triple("bird", "IsA", "animal"),
            triple("rust", "IsA", "language"),
            triple("python", "IsA", "language"),
        ];
        let with = json!({ "operation": "ingest", "triples": triples, "dim": 32 });
        let res = op_ingest(&db, &with).await.unwrap();
        assert_eq!(res["ingested"], 8, "all triples ingested");
        assert_eq!(res["dim"], 32);
        let n_ent = res["entities"].as_u64().unwrap();
        assert!(n_ent >= 9, "distinct entities created, got {}", n_ent);

        // Re-ingesting the same triples must be deduplicated.
        let res2 = op_ingest(&db, &json!({ "operation": "ingest", "triples":
            vec![triple("cat", "IsA", "animal")] })).await.unwrap();
        assert_eq!(res2["ingested"], 0, "duplicate ingest is a no-op");

        // Persistence round-trip: model reloads with identical counts + dim.
        let reloaded = load_model(&db).await.unwrap();
        assert_eq!(reloaded.dim, 32);
        assert_eq!(reloaded.triples.len(), 8);
        assert_eq!(reloaded.n_ent() as u64, n_ent);

        // Train.
        let tr = op_train(&db, &json!({
            "operation": "train", "epochs": 400, "lr": 0.05, "gamma": 6.0, "neg_samples": 8, "seed": 1
        })).await.unwrap();
        assert!(tr["trained"].as_bool().unwrap());
        let loss = tr["loss"].as_f64().unwrap();
        assert!(loss.is_finite() && loss >= 0.0, "loss finite & non-negative: {}", loss);
        let mrr = tr["mrr"].as_f64().unwrap();
        assert!((0.0..=1.0).contains(&mrr), "mrr in range: {}", mrr);

        // After training, true facts should out-score corrupted ones on average.
        let model = load_model(&db).await.unwrap();
        let gamma = 6.0f32;
        let pos: Vec<(usize, usize, usize)> = model
            .triples
            .iter()
            .map(|&(h, r, t, _)| (h, r, t))
            .collect();
        let mut pos_sum = 0.0f32;
        let mut neg_sum = 0.0f32;
        for &(h, r, t) in &pos {
            pos_sum += model.score(h, r, t, gamma);
            // corrupt the tail with a different, definitely-wrong entity
            let bad = (t + 1) % model.n_ent();
            let bad = if bad == t { (t + 2) % model.n_ent() } else { bad };
            neg_sum += model.score(h, r, bad, gamma);
        }
        let (pos_avg, neg_avg) = (pos_sum / pos.len() as f32, neg_sum / pos.len() as f32);
        assert!(
            pos_avg > neg_avg,
            "RotatE learned signal: pos_avg {} should exceed neg_avg {}",
            pos_avg,
            neg_avg
        );

        // predict_tail for a known fact should surface the true tail in top-k.
        let pred = op_predict(
            &db,
            &json!({ "operation": "predict_tail", "head": "cat", "relation": "IsA", "limit": 5, "gamma": 6.0 }),
            true,
        )
        .await
        .unwrap();
        assert_eq!(pred["found"], true);
        let names: Vec<String> = pred["results"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|r| r["name"].as_str().map(String::from))
            .collect();
        assert!(names.contains(&"animal".to_string()), "true tail in top-5: {:?}", names);

        // neighbors + lookup return sane shapes.
        let nb = op_neighbors(&db, &json!({ "operation": "neighbors", "entity": "cat", "limit": 3 }))
            .await
            .unwrap();
        assert_eq!(nb["found"], true);
        assert!(nb["results"].as_array().unwrap().len() <= 3);

        let lk = op_lookup(&db, &json!({ "operation": "lookup", "relation": "IsA", "limit": 100 }))
            .await
            .unwrap();
        assert_eq!(lk["count"], 8);

        let st = op_stats(&db, &json!({ "operation": "stats" })).await.unwrap();
        assert_eq!(st["triples"], 8);
        assert!(st["points"].as_array().unwrap().len() >= 9);

        let _ = std::fs::remove_dir_all(&path);
    }
}
