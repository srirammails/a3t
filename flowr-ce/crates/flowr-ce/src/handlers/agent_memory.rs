//! `call:a3t:agent` memory on ADB (ADB stage F, `F_FLOWR_WIRING.md` F1–F4).
//!
//! With `memory_resource` set, each turn:
//! - **recalls** WORKING and EPISODIC of this session (newest 20 each; they are put on
//!   the context for extensions — the session history already carries them), the
//!   SEMANTIC facts in the agent's namespace most relevant to the message (ADB's
//!   `RECALL … LIKE $q`, MVP W4; `memory_config.semantic.{limit, min_confidence}`,
//!   default 5 and none; the newest facts on an ADB without it), and — when
//!   PROCEDURAL reads are on — the procedure whose pattern best matches the message
//!   (ADB's `PATTERN $p`, its overlap score in `metadata.confidence`);
//! - pulls in **related** knowledge (ADB P1.5, `P1_GRAPH.md` D3): other facts about the
//!   entities of the recalled facts, and the records linked to the recalled ones, by
//!   link weight (`memory_config.semantic.{related, related_min_weight}`, default 5
//!   and 0.5; `related: 0` turns it off); a fact's confidence also weighs its rank;
//! - adds the facts, the related ones (and a weaker procedure match) to the system
//!   prompt, bounded;
//! - **stores** the user turn in WORKING (TTL = the session's) and the reply in
//!   EPISODIC;
//! - with `memory_config.semantic.reflect`, **reflects** (F-L3): after the turn, in
//!   the background, the agent's LLM picks out the lasting facts the exchange stated
//!   and they go to SEMANTIC as facts (`source = "reflection"`), so long-term memory
//!   grows without the `remember` tool; a newer fact for the same subject and
//!   predicate replaces the older (ADB keeps it as history). Facts about the user
//!   have the subject `user:<owner>` and carry `owner`: an agent's namespace is
//!   shared by its users, and recall leaves out other owners' facts.
//!
//! Memory is best effort: an ADB error is logged and the turn goes on without it.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tracing::warn;

use super::call_adb::{adb_query, resolve_adb, AdbEndpoint};

/// Per-tier settings (`memory_config.<tier>.{read, write, limit, min_confidence}`);
/// unset = the tier default.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TierMode {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub read: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub write: Option<bool>,
    /// How many records a read brings back (AQL `LIMIT`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<u64>,
    /// SEMANTIC: the least similarity a fact needs (AQL `MIN_CONFIDENCE`, 0–1).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_confidence: Option<f64>,
    /// SEMANTIC: how many related facts / linked records to add (default 5, 0 = none).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub related: Option<u64>,
    /// SEMANTIC: the least link weight (fact confidence) a related one needs (default 0.5).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub related_min_weight: Option<f64>,
    /// SEMANTIC: turn each exchange's lasting facts into SEMANTIC facts (F-L3, default off).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reflect: Option<bool>,
}

/// `memory_config` (F3 defaults: WORKING and EPISODIC read/write, SEMANTIC read only —
/// facts are written deliberately with `remember` — PROCEDURAL off).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MemoryTierConfig {
    #[serde(default)]
    pub working: TierMode,
    #[serde(default)]
    pub episodic: TierMode,
    #[serde(default)]
    pub semantic: TierMode,
    #[serde(default)]
    pub procedural: TierMode,
}

impl MemoryTierConfig {
    fn working_read(&self) -> bool { self.working.read.unwrap_or(true) }
    fn working_write(&self) -> bool { self.working.write.unwrap_or(true) }
    fn episodic_read(&self) -> bool { self.episodic.read.unwrap_or(true) }
    fn episodic_write(&self) -> bool { self.episodic.write.unwrap_or(true) }
    fn semantic_read(&self) -> bool { self.semantic.read.unwrap_or(true) }
    pub fn procedural_read(&self) -> bool { self.procedural.read.unwrap_or(false) }
    pub fn reflect(&self) -> bool { self.semantic.reflect.unwrap_or(false) }
}

/// A procedure at or above this pattern overlap answers without the LLM (F4).
pub const PROCEDURE_ANSWERS: f64 = 0.5;
/// A weaker match is offered to the LLM as a suggested approach.
pub const PROCEDURE_SUGGESTS: f64 = 0.2;
/// The "What you remember" section's budget (F2).
const PROMPT_BUDGET: usize = 3_000;
const FACTS: usize = 5;
const RELATED: u64 = 5;
const RELATED_MIN_WEIGHT: f64 = 0.5;
/// The most facts one reflection stores.
const REFLECT_MAX: usize = 10;
/// How many earlier messages a reflection sees besides the exchange (for context).
pub const REFLECT_CONTEXT: usize = 4;

/// What one turn recalled.
#[derive(Debug, Default)]
pub struct Recalled {
    pub working: Vec<Value>,
    pub episodic: Vec<Value>,
    pub facts: Vec<Value>,
    /// Knowledge linked to the facts, strongest first, with its weight.
    pub related: Vec<(f64, Value)>,
    /// The best-matching procedure and its pattern overlap.
    pub procedure: Option<(f64, Value)>,
}

/// An agent's memory for one session.
#[derive(Clone)]
pub struct AgentMemory {
    ep: AdbEndpoint,
    namespace: String,
    session_id: String,
    /// Who the session belongs to (the principal): reflected facts carry it.
    owner: String,
}

/// A double-quoted AQL string literal.
fn aql_str(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
}

/// The records of a query result (`{type: records, records: [...]}`).
fn records(result: &Value) -> Vec<Value> {
    let data = result.get("data").unwrap_or(result);
    data.get("records").and_then(|r| r.as_array()).cloned().unwrap_or_default()
}

fn created_at(r: &Value) -> String {
    r.pointer("/metadata/created_at").and_then(|v| v.as_str()).unwrap_or_default().to_string()
}

/// Lowercase word set.
fn words(s: &str) -> HashSet<String> {
    s.split(|c: char| !c.is_alphanumeric()).filter(|w| !w.is_empty()).map(|w| w.to_lowercase()).collect()
}

/// Token overlap (Jaccard), as ADB's procedural `PATTERN` match scores it.
pub fn overlap(a: &str, b: &str) -> f64 {
    let (a, b) = (words(a), words(b));
    let union = a.union(&b).count();
    if union == 0 { 0.0 } else { a.intersection(&b).count() as f64 / union as f64 }
}

/// A record's text for the prompt: its `content`, else its fields as JSON.
fn text(r: &Value) -> String {
    let data = r.get("data").cloned().unwrap_or(Value::Null);
    match data.get("content").and_then(|c| c.as_str()) {
        Some(c) => c.to_string(),
        None => {
            let mut d = data;
            if let Some(m) = d.as_object_mut() {
                m.remove("key");
            }
            d.to_string()
        }
    }
}

/// A fact's (subject, object) when `r` is one (ADB P1.4: `STORE … FACT`).
fn fact_ends(r: &Value) -> Option<(String, Option<String>)> {
    let d = r.get("data")?;
    d.get("predicate")?;
    let subject = d.get("subject")?.as_str()?.to_string();
    Some((subject, d.get("object").and_then(|o| o.as_str()).map(String::from)))
}

/// A link's weight: a fact's confidence, a followed record's path weight, else 1.
fn weight(r: &Value) -> f64 {
    let d = r.get("data");
    d.and_then(|d| d.get("_path_weight"))
        .or_else(|| fact_ends(r).and(d.and_then(|d| d.get("confidence"))))
        .and_then(|v| v.as_f64())
        .unwrap_or(1.0)
}

/// Rank recalled facts by relevance weighed by confidence: `similarity × (½ + ½ w)`.
fn rerank(facts: &mut [Value]) {
    let score = |r: &Value| r.pointer("/metadata/confidence").and_then(|v| v.as_f64()).unwrap_or(0.5) * (0.5 + 0.5 * weight(r));
    facts.sort_by(|a, b| score(b).total_cmp(&score(a)));
}

fn key(r: &Value) -> Option<String> {
    r.pointer("/data/key").or_else(|| r.get("id")).and_then(|v| v.as_str()).map(String::from)
}

fn aql_list(items: &[String]) -> String {
    format!("[{}]", items.iter().map(|s| aql_str(s)).collect::<Vec<_>>().join(", "))
}

/// A procedure's steps as one line.
pub fn steps(procedure: &Value) -> String {
    match procedure.pointer("/data/steps") {
        Some(Value::Array(s)) => s.iter().filter_map(|v| v.as_str()).collect::<Vec<_>>().join("; "),
        Some(Value::String(s)) => s.clone(),
        _ => String::new(),
    }
}

impl AgentMemory {
    /// The memory of `session_id` on ADB resource `resource`.
    pub async fn connect(
        storage: &dyn flowr_storage::StorageBackend,
        resource: &str,
        namespace: &str,
        session_id: &str,
        owner: &str,
    ) -> Result<Self, String> {
        // A bare name (A3T's Memory tab saves one) is looked up in `default`, then in
        // `a3t`, as agent MCP servers are.
        let ep = match resource.contains('/') {
            true => resolve_adb(storage, resource).await?,
            false => match resolve_adb(storage, resource).await {
                Ok(e) => e,
                Err(_) => resolve_adb(storage, &format!("a3t/{resource}")).await?,
            },
        };
        Ok(Self { ep, namespace: namespace.to_string(), session_id: session_id.to_string(), owner: owner.to_string() })
    }

    async fn query(&self, aql: &str) -> Vec<Value> {
        match adb_query(&self.ep, aql, None).await {
            Ok(r) => records(&r),
            Err(e) => {
                warn!(error = %e, aql = %aql, "agent memory: recall failed; continuing without it");
                Vec::new()
            }
        }
    }

    /// The facts most relevant to `message` (`RECALL … LIKE $q`), or `None` when
    /// this ADB can't rank (older, or no durable engine): the caller falls back.
    async fn relevant_facts(&self, message: &str, tier: &TierMode) -> Option<Vec<Value>> {
        let mut aql = format!("RECALL FROM SEMANTIC LIKE $q NAMESPACE {}", aql_str(&self.namespace));
        if let Some(c) = tier.min_confidence {
            aql.push_str(&format!(" MIN_CONFIDENCE {c:?}"));
        }
        aql.push_str(&format!(" LIMIT {}", tier.limit.unwrap_or(FACTS as u64)));
        match adb_query(&self.ep, &aql, Some(&serde_json::json!({ "q": message }))).await {
            Ok(r) => Some(records(&r)),
            Err(e) => {
                warn!(error = %e, "agent memory: relevance recall unavailable; using the newest facts");
                None
            }
        }
    }

    /// Knowledge linked to `facts` (D3): other facts about the entities they mention,
    /// and the records their (non-fact) records link to or from — at least
    /// `min_weight`, strongest first, at most `limit`, none already in `facts`.
    async fn related(&self, facts: &[Value], limit: usize, min_weight: f64) -> Vec<(f64, Value)> {
        let ns = aql_str(&self.namespace);
        let mut entities: Vec<String> = Vec::new();
        let mut records: Vec<String> = Vec::new();
        for f in facts {
            match fact_ends(f) {
                Some((s, o)) => entities.extend(std::iter::once(s).chain(o)),
                None => records.extend(key(f)),
            }
        }
        entities.sort();
        entities.dedup();
        let mut found: Vec<Value> = Vec::new();
        if !entities.is_empty() {
            let list = aql_list(&entities);
            for end in ["subject", "object"] {
                found.extend(self.query(&format!("RECALL FROM SEMANTIC WHERE {end} IN {list} NAMESPACE {ns} LIMIT 50")).await);
            }
        }
        if !records.is_empty() {
            found.extend(
                self.query(&format!(
                    "RECALL FROM SEMANTIC WHERE key IN {} NAMESPACE {ns} FOLLOW LINKS ALL BOTH MIN_WEIGHT {min_weight:?}",
                    aql_list(&records)
                ))
                .await,
            );
        }
        let mut seen: HashSet<String> = facts.iter().filter_map(key).collect();
        let mut out: Vec<(f64, Value)> = found
            .into_iter()
            // Entity stubs (key and name only) say nothing on their own.
            .filter(|r| fact_ends(r).is_some() || r.pointer("/data/content").is_some())
            .filter(|r| self.visible(r))
            .filter(|r| key(r).is_some_and(|k| seen.insert(k)))
            .map(|r| (weight(&r), r))
            .filter(|(w, _)| *w >= min_weight)
            .collect();
        out.sort_by(|a, b| b.0.total_cmp(&a.0));
        out.truncate(limit);
        out
    }

    pub async fn recall(&self, tiers: &MemoryTierConfig, message: &str) -> Recalled {
        let (sid, ns) = (aql_str(&self.session_id), aql_str(&self.namespace));
        let mut out = Recalled::default();
        if tiers.working_read() {
            let n = tiers.working.limit.unwrap_or(20);
            out.working = self.query(&format!("RECALL FROM WORKING WHERE session_id = {sid} NAMESPACE {ns} LIMIT {n}")).await;
        }
        if tiers.episodic_read() {
            let n = tiers.episodic.limit.unwrap_or(20);
            out.episodic = self.query(&format!("RECALL FROM EPISODIC WHERE session_id = {sid} NAMESPACE {ns} LIMIT {n}")).await;
        }
        if tiers.semantic_read() {
            let limit = tiers.semantic.limit.unwrap_or(FACTS as u64) as usize;
            let relevant = match message.trim().is_empty() {
                true => None,
                false => self.relevant_facts(message, &tiers.semantic).await,
            };
            out.facts = match relevant {
                Some(facts) => facts,
                // No message, or an ADB that can't rank: the newest facts.
                None => {
                    let mut facts = self.query(&format!("RECALL FROM SEMANTIC ALL NAMESPACE {ns} LIMIT 200")).await;
                    facts.sort_by_key(|r| std::cmp::Reverse(created_at(r)));
                    facts.truncate(limit);
                    facts
                }
            };
            // Reflected facts are their owner's (F-L3).
            out.facts.retain(|r| self.visible(r));
            rerank(&mut out.facts);
            let related = tiers.semantic.related.unwrap_or(RELATED) as usize;
            if related > 0 && !out.facts.is_empty() {
                let min = tiers.semantic.related_min_weight.unwrap_or(RELATED_MIN_WEIGHT);
                out.related = self.related(&out.facts, related, min).await;
            }
        }
        if tiers.procedural_read() && !message.trim().is_empty() {
            // ADB matches (F-L2): the best procedure at or above the suggestion
            // threshold, scored in `metadata.confidence`. Procedures carry no
            // namespace yet.
            let aql = format!("RECALL FROM PROCEDURAL PATTERN $m THRESHOLD {PROCEDURE_SUGGESTS:?} LIMIT 1");
            out.procedure = match adb_query(&self.ep, &aql, Some(&serde_json::json!({ "m": message }))).await {
                Ok(r) => records(&r).into_iter().next().map(|p| {
                    let score = p.pointer("/metadata/confidence").and_then(|v| v.as_f64());
                    // An ADB from before the score: the same overlap, computed here.
                    let pattern = p.pointer("/data/pattern").and_then(|v| v.as_str()).unwrap_or_default();
                    (score.unwrap_or_else(|| overlap(message, pattern)), p)
                }),
                Err(e) => {
                    warn!(error = %e, "agent memory: procedure match failed; continuing without it");
                    None
                }
            };
        }
        out
    }

    /// Whether `r` may reach this session's prompt: not another owner's (F-L3).
    fn visible(&self, r: &Value) -> bool {
        r.pointer("/data/owner").and_then(|o| o.as_str()).is_none_or(|o| o == self.owner)
    }

    /// The entity key of the session's user in reflected facts.
    fn user_key(&self) -> String {
        let id: String = self.owner.chars().map(|c| if c.is_alphanumeric() || "-_.@".contains(c) { c } else { '-' }).collect();
        format!("user:{}", if id.is_empty() { "anonymous" } else { &id })
    }

    /// Store the facts a reflection found (F-L3): `user` as a subject or object is
    /// the session's user. Returns how many were stored.
    pub async fn store_reflection(&self, facts: &[Reflected]) -> usize {
        let ns = aql_str(&self.namespace);
        let user = self.user_key();
        let end = |k: &str| if k == "user" { user.clone() } else { k.to_string() };
        let mut stored = 0;
        for f in facts.iter().take(REFLECT_MAX) {
            let mut fields = vec![
                format!("subject = {}", aql_str(&end(&f.subject))),
                format!("predicate = {}", aql_str(&f.predicate)),
            ];
            match (&f.object, &f.value) {
                (Some(o), _) => fields.push(format!("object = {}", aql_str(&end(o)))),
                (None, Some(v)) => fields.push(format!("value = {}", aql_str(v))),
                (None, None) => continue,
            }
            if let Some(c) = &f.content {
                fields.push(format!("content = {}", aql_str(c)));
            }
            fields.push(format!("confidence = {:?}", f.confidence.clamp(0.0, 1.0)));
            fields.push(format!("source = \"reflection\", owner = {}, session_id = {}", aql_str(&self.owner), aql_str(&self.session_id)));
            let aql = format!("STORE INTO SEMANTIC FACT ({}) NAMESPACE {ns}", fields.join(", "));
            match adb_query(&self.ep, &aql, None).await {
                Ok(_) => stored += 1,
                Err(e) => warn!(error = %e, "agent memory: a reflected fact was not stored"),
            }
        }
        stored
    }

    /// Store the turn: the user message in WORKING, the reply in EPISODIC.
    pub async fn store_turn(&self, tiers: &MemoryTierConfig, ttl_secs: u64, user: &str, reply: &str, tool_calls: u32, iterations: u32) {
        let (sid, ns) = (aql_str(&self.session_id), aql_str(&self.namespace));
        let mut writes = Vec::new();
        if tiers.working_write() && !user.is_empty() {
            writes.push(format!(
                "STORE INTO WORKING (session_id = {sid}, role = \"user\", content = {}) TTL {ttl_secs}s NAMESPACE {ns}",
                aql_str(user)
            ));
        }
        if tiers.episodic_write() && !reply.is_empty() {
            writes.push(format!(
                "STORE INTO EPISODIC (session_id = {sid}, role = \"assistant\", content = {}, tool_calls_made = {tool_calls}, iterations = {iterations}) NAMESPACE {ns}",
                aql_str(reply)
            ));
        }
        for aql in writes {
            if let Err(e) = adb_query(&self.ep, &aql, None).await {
                warn!(error = %e, "agent memory: store failed; the turn is not remembered");
            }
        }
    }
}

/// A fact a reflection found (F-L3).
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Reflected {
    pub subject: String,
    pub predicate: String,
    #[serde(default)]
    pub object: Option<String>,
    #[serde(default, deserialize_with = "text_value")]
    pub value: Option<String>,
    /// The fact in a sentence (what the prompt shows when it is recalled).
    #[serde(default)]
    pub content: Option<String>,
    #[serde(default = "default_confidence")]
    pub confidence: f64,
}

fn default_confidence() -> f64 {
    0.8
}

/// A value as text: models answer `"value": 36` as often as `"value": "36"`.
fn text_value<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<String>, D::Error> {
    Ok(match Option::<Value>::deserialize(d)? {
        None | Some(Value::Null) => None,
        Some(Value::String(s)) => Some(s),
        Some(v) => Some(v.to_string()),
    })
}

/// The reflection's system prompt (F-L3).
pub const REFLECT_PROMPT: &str = "You maintain an assistant's long-term memory. From the conversation, \
pick out lasting facts the USER stated or confirmed in the last exchange: about themselves, their work, \
preferences, plans, decisions and the things they deal with. Leave out small talk, questions, guesses, \
the assistant's own suggestions, and anything only true for this moment. \
Answer with JSON only: {\"facts\": [{\"subject\": \"user\", \"predicate\": \"lives_in\", \"object\": \"city:lisbon\", \
\"content\": \"The user lives in Lisbon\", \"confidence\": 0.9}]}. Use \"user\" for the user; other subjects \
and objects are keys like \"org:acme\" or \"person:dana\" (lowercase, reuse a known key when it names the same thing); \
predicates are snake_case verbs; use \"value\" instead of \"object\" for a number, date or free text. \
No lasting facts: {\"facts\": []}.";

/// The reflection's user message: the exchange, a little context before it, and
/// the entity keys memory already knows.
pub fn reflection_input(context: &[(String, String)], user: &str, reply: &str, known: &[String]) -> String {
    let mut s = String::new();
    if !context.is_empty() {
        s.push_str("Earlier:\n");
        for (role, text) in context {
            s.push_str(&format!("{role}: {}\n", text.chars().take(1_000).collect::<String>()));
        }
    }
    s.push_str(&format!("\nLast exchange:\nuser: {user}\nassistant: {}\n", reply.chars().take(2_000).collect::<String>()));
    if !known.is_empty() {
        s.push_str(&format!("\nKnown keys: {}\n", known.join(", ")));
    }
    s
}

/// The facts in a reflection's answer: the first JSON object in it (models wrap it
/// in prose or code fences), entries without subject or predicate dropped.
pub fn parse_reflection(answer: &str) -> Vec<Reflected> {
    #[derive(Deserialize)]
    struct Answer {
        #[serde(default)]
        facts: Vec<Value>,
    }
    let (Some(start), Some(end)) = (answer.find('{'), answer.rfind('}')) else { return Vec::new() };
    let Some(json) = answer.get(start..=end) else { return Vec::new() };
    let Ok(a) = serde_json::from_str::<Answer>(json) else { return Vec::new() };
    a.facts
        .into_iter()
        .filter_map(|f| serde_json::from_value::<Reflected>(f).ok())
        .filter(|f| !f.subject.trim().is_empty() && !f.predicate.trim().is_empty() && (f.object.is_some() || f.value.is_some()))
        .collect()
}

impl Recalled {
    /// The entity keys of the recalled facts, for a reflection to reuse.
    pub fn known_keys(&self) -> Vec<String> {
        let mut keys: Vec<String> = self
            .facts
            .iter()
            .chain(self.related.iter().map(|(_, r)| r))
            .filter_map(fact_ends)
            .flat_map(|(s, o)| std::iter::once(s).chain(o))
            .collect();
        keys.sort();
        keys.dedup();
        keys.truncate(30);
        keys
    }

    /// The answer of a procedure that matches the message well enough (F4).
    pub fn procedure_answer(&self) -> Option<String> {
        let (score, p) = self.procedure.as_ref()?;
        (*score >= PROCEDURE_ANSWERS).then(|| steps(p)).filter(|s| !s.is_empty())
    }

    /// The "What you remember" section for the system prompt (F2), or `None`.
    pub fn prompt_section(&self) -> Option<String> {
        let mut lines = Vec::new();
        for f in &self.facts {
            lines.push(format!("- {}", text(f)));
        }
        for (w, r) in &self.related {
            lines.push(format!("- (related, link weight {w:.2}) {}", text(r)));
        }
        if let Some((score, p)) = &self.procedure {
            if *score >= PROCEDURE_SUGGESTS && *score < PROCEDURE_ANSWERS {
                let pattern = p.pointer("/data/pattern").and_then(|v| v.as_str()).unwrap_or_default();
                lines.push(format!("- A procedure that may apply (for \"{pattern}\"): {}", steps(p)));
            }
        }
        if lines.is_empty() {
            return None;
        }
        let mut section = String::from("\n\n## What you remember\nFrom your long-term memory (most relevant first); use it if relevant:\n");
        for l in lines {
            if section.len() + l.len() + 1 > PROMPT_BUDGET {
                break;
            }
            section.push_str(&l);
            section.push('\n');
        }
        Some(section)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn quoting_escapes_quotes_and_backslashes() {
        assert_eq!(aql_str(r#"say "hi" C:\x"#), r#""say \"hi\" C:\\x""#);
    }

    #[test]
    fn overlap_is_jaccard_on_words() {
        assert!((overlap("please reset my password now", "reset my password") - 0.6).abs() < 1e-9);
        assert_eq!(overlap("", ""), 0.0);
    }

    #[test]
    fn reflections_parse_from_wrapped_json() {
        let a = "Sure:\n```json\n{\"facts\": [{\"subject\": \"user\", \"predicate\": \"lives_in\", \"object\": \"city:lisbon\"}, \
                 {\"subject\": \"user\", \"predicate\": \"age\", \"value\": 36, \"confidence\": 0.6}, {\"subject\": \"user\"}]}\n```";
        let f = parse_reflection(a);
        assert_eq!(f.len(), 2, "{f:?}");
        assert_eq!((f[0].object.as_deref(), f[0].confidence), (Some("city:lisbon"), 0.8));
        assert_eq!((f[1].value.as_deref(), f[1].confidence), (Some("36"), 0.6));
        assert!(parse_reflection("nothing to remember").is_empty());
        assert!(parse_reflection("{\"facts\": []}").is_empty());
    }

    #[test]
    fn defaults_follow_f3() {
        let t: MemoryTierConfig = serde_json::from_value(json!({ "semantic": { "write": true } })).unwrap();
        assert!(t.working_read() && t.working_write() && t.episodic_read() && t.episodic_write());
        assert!(t.semantic_read() && !t.procedural_read());
    }

    #[test]
    fn facts_rank_by_relevance_and_confidence() {
        let f = |c: f64, conf: f64, k: &str| json!({ "id": k, "data": { "key": k, "subject": "a", "predicate": "p", "confidence": conf }, "metadata": { "confidence": c } });
        let plain = json!({ "id": "n", "data": { "key": "n", "content": "note" }, "metadata": { "confidence": 0.6 } });
        let mut facts = vec![f(0.7, 0.2, "weak"), plain, f(0.65, 1.0, "strong")];
        rerank(&mut facts);
        let order: Vec<&str> = facts.iter().map(|r| r["id"].as_str().unwrap()).collect();
        assert_eq!(order, ["strong", "n", "weak"]);
        assert_eq!(fact_ends(&facts[0]), Some(("a".into(), None)));
        assert!(fact_ends(&facts[1]).is_none());
    }

    #[test]
    fn related_lines_follow_the_facts() {
        let r = Recalled {
            facts: vec![json!({ "data": { "content": "Ada works at Acme" } })],
            related: vec![(0.8, json!({ "data": { "content": "Acme is based in London" } }))],
            ..Default::default()
        };
        let s = r.prompt_section().unwrap();
        let (a, b) = (s.find("Ada works at Acme").unwrap(), s.find("(related, link weight 0.80) Acme is based in London").unwrap());
        assert!(a < b, "{s}");
    }

    #[test]
    fn prompt_section_lists_facts_and_a_weak_procedure_only() {
        let fact = json!({ "data": { "content": "Sriram prefers dark mode" } });
        let p = json!({ "data": { "pattern": "reset password", "steps": ["Open settings", "Reset"] } });
        let weak = Recalled { facts: vec![fact.clone()], procedure: Some((0.3, p.clone())), ..Default::default() };
        let s = weak.prompt_section().unwrap();
        assert!(s.contains("Sriram prefers dark mode") && s.contains("Open settings; Reset"), "{s}");
        assert!(weak.procedure_answer().is_none());
        let strong = Recalled { procedure: Some((0.6, p)), ..Default::default() };
        assert_eq!(strong.procedure_answer().as_deref(), Some("Open settings; Reset"));
        assert!(strong.prompt_section().is_none(), "a strong match answers instead");
        assert!(Recalled::default().prompt_section().is_none());
    }
}
