//! `call:a3t:agent` memory on ADB (ADB stage F, `F_FLOWR_WIRING.md` F1–F4).
//!
//! With `memory_resource` set, each turn:
//! - **recalls** WORKING and EPISODIC of this session (newest 20 each; they are put on
//!   the context for extensions — the session history already carries them), the
//!   newest SEMANTIC facts in the agent's namespace, and — when PROCEDURAL reads are
//!   on — the procedure whose pattern best matches the message;
//! - adds the facts (and a weaker procedure match) to the system prompt, bounded;
//! - **stores** the user turn in WORKING (TTL = the session's) and the reply in
//!   EPISODIC.
//!
//! Memory is best effort: an ADB error is logged and the turn goes on without it.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tracing::warn;

use super::call_adb::{adb_query, resolve_adb_url};

/// Per-tier switches (`memory_config.<tier>.{read,write}`); unset = the tier default.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TierMode {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub read: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub write: Option<bool>,
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
}

/// A procedure at or above this pattern overlap answers without the LLM (F4).
pub const PROCEDURE_ANSWERS: f64 = 0.5;
/// A weaker match is offered to the LLM as a suggested approach.
pub const PROCEDURE_SUGGESTS: f64 = 0.2;
/// The "What you remember" section's budget (F2).
const PROMPT_BUDGET: usize = 3_000;
const FACTS: usize = 5;

/// What one turn recalled.
#[derive(Debug, Default)]
pub struct Recalled {
    pub working: Vec<Value>,
    pub episodic: Vec<Value>,
    pub facts: Vec<Value>,
    /// The best-matching procedure and its pattern overlap.
    pub procedure: Option<(f64, Value)>,
}

/// An agent's memory for one session.
pub struct AgentMemory {
    client: reqwest::Client,
    url: String,
    namespace: String,
    session_id: String,
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
        client: reqwest::Client,
        resource: &str,
        namespace: &str,
        session_id: &str,
    ) -> Result<Self, String> {
        let url = resolve_adb_url(storage, resource).await?;
        Ok(Self { client, url, namespace: namespace.to_string(), session_id: session_id.to_string() })
    }

    async fn query(&self, aql: &str) -> Vec<Value> {
        match adb_query(&self.client, &self.url, aql, None).await {
            Ok(r) => records(&r),
            Err(e) => {
                warn!(error = %e, aql = %aql, "agent memory: recall failed; continuing without it");
                Vec::new()
            }
        }
    }

    pub async fn recall(&self, tiers: &MemoryTierConfig, message: &str) -> Recalled {
        let (sid, ns) = (aql_str(&self.session_id), aql_str(&self.namespace));
        let mut out = Recalled::default();
        if tiers.working_read() {
            out.working = self.query(&format!("RECALL FROM WORKING WHERE session_id = {sid} NAMESPACE {ns} LIMIT 20")).await;
        }
        if tiers.episodic_read() {
            out.episodic = self.query(&format!("RECALL FROM EPISODIC WHERE session_id = {sid} NAMESPACE {ns} LIMIT 20")).await;
        }
        if tiers.semantic_read() {
            // Newest facts first (relevance ranking arrives with ADB's hybrid recall, M2).
            let mut facts = self.query(&format!("RECALL FROM SEMANTIC ALL NAMESPACE {ns} LIMIT 200")).await;
            facts.sort_by_key(|r| std::cmp::Reverse(created_at(r)));
            facts.truncate(FACTS);
            out.facts = facts;
        }
        if tiers.procedural_read() {
            // Matched here: ADB's `PATTERN` takes a bound variable only (limitations S7),
            // and procedures carry no namespace yet.
            let procs = self.query("RECALL FROM PROCEDURAL ALL LIMIT 500").await;
            out.procedure = procs
                .into_iter()
                .map(|p| {
                    let pattern = p.pointer("/data/pattern").and_then(|v| v.as_str()).unwrap_or_default().to_string();
                    (overlap(message, &pattern), p)
                })
                .filter(|(s, _)| *s > 0.0)
                .max_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
        }
        out
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
            if let Err(e) = adb_query(&self.client, &self.url, &aql, None).await {
                warn!(error = %e, "agent memory: store failed; the turn is not remembered");
            }
        }
    }
}

impl Recalled {
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
        if let Some((score, p)) = &self.procedure {
            if *score >= PROCEDURE_SUGGESTS && *score < PROCEDURE_ANSWERS {
                let pattern = p.pointer("/data/pattern").and_then(|v| v.as_str()).unwrap_or_default();
                lines.push(format!("- A procedure that may apply (for \"{pattern}\"): {}", steps(p)));
            }
        }
        if lines.is_empty() {
            return None;
        }
        let mut section = String::from("\n\n## What you remember\nFrom your long-term memory (newest first); use it if relevant:\n");
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
    fn defaults_follow_f3() {
        let t: MemoryTierConfig = serde_json::from_value(json!({ "semantic": { "write": true } })).unwrap();
        assert!(t.working_read() && t.working_write() && t.episodic_read() && t.episodic_write());
        assert!(t.semantic_read() && !t.procedural_read());
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
