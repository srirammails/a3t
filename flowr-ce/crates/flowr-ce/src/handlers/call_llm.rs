//! call:llm handler — calls LLM providers (OpenAI, Anthropic, Ollama)
//!
//! In CNCF syntax: `call: llm` with `with:` containing prompt, model, etc.
//! The LLM resource (API key, endpoint, model) is resolved from a named resource.
//!
//! Provider routing:
//!   - "anthropic" → Anthropic Messages API (/v1/messages)
//!   - everything else → OpenAI-compatible (/v1/chat/completions)

use async_trait::async_trait;
use serde_json::{json, Value};
use tracing::info;

use swf_parser::{NamedTask, Task};
use swf_runtime::context::ExecutionContext;
use swf_runtime::error::{RuntimeError, Result};
use swf_runtime::traits::{TaskHandler, TaskResult};

pub struct LlmHandler {
    client: reqwest::Client,
}

impl LlmHandler {
    pub fn new() -> Self {
        Self { client: reqwest::Client::new() }
    }
}

#[async_trait]
impl TaskHandler for LlmHandler {
    fn task_type(&self) -> &str {
        "call:llm"
    }

    /// Gate on the *LLM resource* being called (`Llm{resource}`), with the model as
    /// an attribute — so policy can restrict which models/providers a flow may use.
    fn policy_resource(&self, task: &NamedTask) -> flowr_policy::Resource {
        let with = match &task.task {
            Task::Call(c) => c.with.clone().unwrap_or(json!({})),
            _ => json!({}),
        };
        let resource = with.get("_resource").and_then(|v| v.as_str()).unwrap_or("default-llm");
        let model = with.get("model").and_then(|v| v.as_str()).unwrap_or("");
        flowr_policy::Resource::new("Llm", resource)
            .with_attr("task_type", "call:llm")
            .with_attr("model", model)
    }

    fn policy_action(&self) -> flowr_policy::Action {
        flowr_policy::actions::llm_call()
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

        let prompt = with.get("prompt").and_then(|v| v.as_str()).unwrap_or("hello");
        let system_prompt = with.get("system_prompt").and_then(|v| v.as_str());
        let model = with.get("model").and_then(|v| v.as_str());
        let max_tokens = with.get("max_tokens").and_then(|v| v.as_u64()).unwrap_or(1024);
        let temperature = with.get("temperature").and_then(|v| v.as_f64()).unwrap_or(0.7);
        let messages = with.get("messages").and_then(|v| v.as_array());
        let tools = with.get("tools").and_then(|v| v.as_array());

        // Resolve LLM resource from storage
        let resource_name = with.get("_resource").and_then(|v| v.as_str()).unwrap_or("default-llm");
        let resource = if let Some(ref storage_any) = ctx.storage {
            if let Some(storage) = storage_any.downcast_ref::<std::sync::Arc<dyn flowr_storage::StorageBackend>>() {
                let (ns, rn) = if resource_name.contains('/') {
                    let parts: Vec<&str> = resource_name.splitn(2, '/').collect();
                    (parts[0], parts[1])
                } else {
                    ("default", resource_name)
                };
                storage.resources().get(ns, rn).await
                    .map(|r| r.config)
                    .unwrap_or(json!({}))
            } else { json!({}) }
        } else { json!({}) };

        let provider = resource.get("provider").and_then(|v| v.as_str()).unwrap_or("openai");
        let api_url = resource.get("api_url").and_then(|v| v.as_str());

        // Resolve API key: prefer api_key_ref (secret resource reference) over embedded api_key
        let resolved_key: String;
        if let Some(secret_ref) = resource.get("api_key_ref").and_then(|v| v.as_str()) {
            resolved_key = Self::resolve_secret(ctx, secret_ref).await.unwrap_or_default();
        } else {
            resolved_key = resource.get("api_key").and_then(|v| v.as_str()).unwrap_or("").to_string();
        }
        let api_key = resolved_key.as_str();
        let model = model.unwrap_or(
            resource.get("model").and_then(|v| v.as_str()).unwrap_or("gpt-4o-mini")
        );

        info!(task = %task.name, provider = %provider, model = %model, "calling LLM");

        if provider == "anthropic" {
            self.call_anthropic(api_key, api_url, model, max_tokens, temperature, system_prompt, prompt, messages, tools).await
        } else {
            self.call_openai(api_key, api_url, model, max_tokens, temperature, system_prompt, prompt, messages, tools).await
        }
    }
}

impl LlmHandler {
    /// Resolve a secret resource reference (namespace/name) to its value
    async fn resolve_secret(ctx: &ExecutionContext, secret_ref: &str) -> Option<String> {
        let storage_any = ctx.storage.as_ref()?;
        let storage = storage_any.downcast_ref::<std::sync::Arc<dyn flowr_storage::StorageBackend>>()?;
        let (ns, name) = if secret_ref.contains('/') {
            let parts: Vec<&str> = secret_ref.splitn(2, '/').collect();
            (parts[0], parts[1])
        } else {
            ("default", secret_ref)
        };
        let secret = storage.resources().get(ns, name).await.ok()?;
        // Secret resources store the value in config.value; credential resources in config.token
        secret.config.get("value")
            .or_else(|| secret.config.get("token"))
            .or_else(|| secret.config.get("api_key"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
    }

    /// Anthropic Messages API — /v1/messages
    async fn call_anthropic(
        &self,
        api_key: &str,
        api_url: Option<&str>,
        model: &str,
        max_tokens: u64,
        temperature: f64,
        system_prompt: Option<&str>,
        prompt: &str,
        messages: Option<&Vec<Value>>,
        tools: Option<&Vec<Value>>,
    ) -> Result<TaskResult> {
        let base = api_url.unwrap_or("https://api.anthropic.com");
        let url = format!("{}/v1/messages", base.trim_end_matches('/'));

        // Convert messages from OpenAI/agent-loop format to Anthropic format
        let mut anthropic_msgs: Vec<Value> = Vec::new();

        if let Some(msgs) = messages {
            for msg in msgs {
                let role = msg.get("role").and_then(|v| v.as_str()).unwrap_or("user");
                match role {
                    "system" => {} // handled separately as top-level param
                    "tool" => {
                        // OpenAI tool result → Anthropic tool_result content block
                        let tool_call_id = msg.get("tool_call_id").and_then(|v| v.as_str()).unwrap_or("tool");
                        let content = msg.get("content").cloned().unwrap_or(json!(""));
                        let content_str = if content.is_string() {
                            content.as_str().unwrap_or("").to_string()
                        } else {
                            content.to_string()
                        };
                        let block = json!({
                            "type": "tool_result",
                            "tool_use_id": tool_call_id,
                            "content": content_str
                        });
                        // Anthropic requires tool_result in a user message;
                        // merge with previous user message if exists
                        if let Some(last) = anthropic_msgs.last_mut() {
                            if last.get("role").and_then(|v| v.as_str()) == Some("user") {
                                if let Some(arr) = last.get_mut("content").and_then(|v| v.as_array_mut()) {
                                    arr.push(block);
                                    continue;
                                }
                            }
                        }
                        anthropic_msgs.push(json!({"role": "user", "content": [block]}));
                    }
                    "assistant" => {
                        // Reconstruct content blocks: text + tool_use
                        let mut blocks: Vec<Value> = Vec::new();
                        let text = msg.get("content").and_then(|v| v.as_str()).unwrap_or("");
                        if !text.is_empty() {
                            blocks.push(json!({"type": "text", "text": text}));
                        }
                        if let Some(tcs) = msg.get("tool_calls").and_then(|v| v.as_array()) {
                            for tc in tcs {
                                blocks.push(json!({
                                    "type": "tool_use",
                                    "id": tc.get("id").and_then(|v| v.as_str()).unwrap_or("tool"),
                                    "name": tc.get("name").and_then(|v| v.as_str()).unwrap_or(""),
                                    "input": tc.get("input").cloned().unwrap_or(json!({}))
                                }));
                            }
                        }
                        if blocks.is_empty() {
                            blocks.push(json!({"type": "text", "text": ""}));
                        }
                        anthropic_msgs.push(json!({"role": "assistant", "content": blocks}));
                    }
                    _ => {
                        // user messages
                        let content = msg.get("content").and_then(|v| v.as_str()).unwrap_or("");
                        anthropic_msgs.push(json!({"role": "user", "content": content}));
                    }
                }
            }
        }

        // Add current prompt as user message only if last message isn't already user
        let last_is_user = anthropic_msgs.last()
            .and_then(|m| m.get("role"))
            .and_then(|r| r.as_str()) == Some("user");
        if !last_is_user {
            anthropic_msgs.push(json!({"role": "user", "content": prompt}));
        }

        // Ensure non-empty
        if anthropic_msgs.is_empty() {
            anthropic_msgs.push(json!({"role": "user", "content": prompt}));
        }

        let mut body = json!({
            "model": model,
            "max_tokens": max_tokens,
            "messages": anthropic_msgs,
        });

        if temperature != 1.0 {
            body["temperature"] = json!(temperature);
        }

        if let Some(sp) = system_prompt {
            body["system"] = json!(sp);
        }

        // Tools in Anthropic format (name, description, input_schema — no wrapper)
        if let Some(tool_defs) = tools {
            if !tool_defs.is_empty() {
                let anthropic_tools: Vec<Value> = tool_defs.iter().map(|t| {
                    json!({
                        "name": t.get("name").cloned().unwrap_or(json!("")),
                        "description": t.get("description").cloned().unwrap_or(json!("")),
                        "input_schema": t.get("input_schema").cloned().unwrap_or(json!({"type": "object"}))
                    })
                }).collect();
                body["tools"] = json!(anthropic_tools);
            }
        }

        // OAuth tokens (sk-ant-oat*) use Bearer; API keys use x-api-key
        let is_oauth = api_key.starts_with("sk-ant-oat");
        let mut req = self.client.post(&url)
            .header("Content-Type", "application/json")
            .header("anthropic-version", "2023-06-01");
        if is_oauth {
            req = req.header("Authorization", format!("Bearer {}", api_key));
        } else {
            req = req.header("x-api-key", api_key);
        }

        let resp = req.json(&body).send().await
            .map_err(|e| RuntimeError::Communication(format!("Anthropic API error: {}", e)))?;

        let status = resp.status().as_u16();
        let resp_body: Value = resp.json().await
            .map_err(|e| RuntimeError::Communication(format!("Anthropic response parse error: {}", e)))?;

        if status >= 400 {
            return Err(RuntimeError::Communication(format!(
                "Anthropic API error ({}): {}", status, resp_body
            )));
        }

        // Parse Anthropic response content blocks
        let content_blocks = resp_body.get("content").and_then(|v| v.as_array());
        let mut text_content = String::new();
        let mut tool_calls: Vec<Value> = Vec::new();

        if let Some(blocks) = content_blocks {
            for block in blocks {
                match block.get("type").and_then(|v| v.as_str()) {
                    Some("text") => {
                        if let Some(t) = block.get("text").and_then(|v| v.as_str()) {
                            text_content.push_str(t);
                        }
                    }
                    Some("tool_use") => {
                        tool_calls.push(json!({
                            "id": block.get("id"),
                            "name": block.get("name"),
                            "input": block.get("input").cloned().unwrap_or(json!({}))
                        }));
                    }
                    _ => {}
                }
            }
        }

        let mut result = json!({
            "content": text_content,
            "model": resp_body.get("model"),
            "usage": resp_body.get("usage"),
        });

        if !tool_calls.is_empty() {
            result["tool_calls"] = json!(tool_calls);
            result["stop_reason"] = json!("tool_use");
        }

        Ok(TaskResult::new(result))
    }

    /// OpenAI-compatible API — /v1/chat/completions (OpenAI, Groq, Ollama, Cerebras, etc.)
    async fn call_openai(
        &self,
        api_key: &str,
        api_url: Option<&str>,
        model: &str,
        max_tokens: u64,
        temperature: f64,
        system_prompt: Option<&str>,
        prompt: &str,
        messages: Option<&Vec<Value>>,
        tools: Option<&Vec<Value>>,
    ) -> Result<TaskResult> {
        let mut msg_array = Vec::new();
        if let Some(sp) = system_prompt {
            msg_array.push(json!({"role": "system", "content": sp}));
        }
        if let Some(msgs) = messages {
            msg_array.extend(msgs.iter().cloned());
        }
        msg_array.push(json!({"role": "user", "content": prompt}));

        let mut body = json!({
            "model": model,
            "messages": msg_array,
            "max_tokens": max_tokens,
            "temperature": temperature,
        });

        if let Some(tool_defs) = tools {
            if !tool_defs.is_empty() {
                let openai_tools: Vec<Value> = tool_defs.iter().map(|t| {
                    json!({
                        "type": "function",
                        "function": {
                            "name": t.get("name").cloned().unwrap_or(json!("")),
                            "description": t.get("description").cloned().unwrap_or(json!("")),
                            "parameters": t.get("input_schema").cloned().unwrap_or(json!({"type": "object"}))
                        }
                    })
                }).collect();
                body["tools"] = json!(openai_tools);
            }
        }

        let base = api_url.unwrap_or("https://api.openai.com/v1");
        let url = if base.ends_with("/chat/completions") {
            base.to_string()
        } else {
            format!("{}/chat/completions", base.trim_end_matches('/'))
        };

        let resp = self.client
            .post(url)
            .header("Authorization", format!("Bearer {}", api_key))
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| RuntimeError::Communication(format!("LLM API error: {}", e)))?;

        let status = resp.status().as_u16();
        let resp_body: Value = resp.json().await
            .map_err(|e| RuntimeError::Communication(format!("LLM response parse error: {}", e)))?;

        if status >= 400 {
            return Err(RuntimeError::Communication(format!(
                "LLM API error ({}): {}", status, resp_body
            )));
        }

        let content = resp_body.pointer("/choices/0/message/content")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let tool_calls: Vec<Value> = resp_body.pointer("/choices/0/message/tool_calls")
            .and_then(|v| v.as_array())
            .map(|calls| {
                calls.iter().map(|tc| {
                    json!({
                        "id": tc.get("id"),
                        "name": tc.pointer("/function/name"),
                        "input": tc.pointer("/function/arguments")
                            .and_then(|v| v.as_str())
                            .and_then(|s| serde_json::from_str::<Value>(s).ok())
                            .unwrap_or(json!({}))
                    })
                }).collect()
            })
            .unwrap_or_default();

        let mut result = json!({
            "content": content,
            "model": resp_body.get("model"),
            "usage": resp_body.get("usage"),
        });

        if !tool_calls.is_empty() {
            result["tool_calls"] = json!(tool_calls);
            result["stop_reason"] = json!("tool_use");
        }

        Ok(TaskResult::new(result))
    }
}
