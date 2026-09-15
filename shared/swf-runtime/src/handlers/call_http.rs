//! call:http handler — makes HTTP requests

use async_trait::async_trait;
use serde_json::{json, Value};
use tracing::{debug, info};

use swf_parser::{NamedTask, Task, CallTarget};
use crate::context::ExecutionContext;
use crate::error::{RuntimeError, Result};
use crate::traits::{TaskHandler, TaskResult};

pub struct HttpHandler {
    client: reqwest::Client,
}

impl HttpHandler {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::new(),
        }
    }

    pub fn with_client(client: reqwest::Client) -> Self {
        Self { client }
    }
}

#[async_trait]
impl TaskHandler for HttpHandler {
    fn task_type(&self) -> &str {
        "call:http"
    }

    async fn execute(
        &self,
        task: &NamedTask,
        input: Value,
        _ctx: &mut ExecutionContext,
    ) -> Result<TaskResult> {
        let call_task = match &task.task {
            Task::Call(c) => c,
            _ => return Err(RuntimeError::task_failed(&task.name, "expected call task")),
        };

        let (method, uri, headers, body) = match &call_task.target {
            CallTarget::Http(http) => {
                let method = http.method.as_deref().unwrap_or("GET");
                let uri = http.endpoint.as_ref()
                    .map(|e| e.uri.clone())
                    .or_else(|| call_task.with.as_ref()
                        .and_then(|w| w.get("endpoint"))
                        .and_then(|e| e.get("uri").or_else(|| Some(e)))
                        .and_then(|v| v.as_str())
                        .map(String::from))
                    .ok_or_else(|| RuntimeError::task_failed(&task.name, "HTTP call requires endpoint.uri"))?;

                let headers = call_task.with.as_ref()
                    .and_then(|w| w.get("headers"))
                    .cloned();
                let body = call_task.with.as_ref()
                    .and_then(|w| w.get("body"))
                    .cloned();

                (method.to_string(), uri, headers, body)
            }
            _ => return Err(RuntimeError::task_failed(&task.name, "expected HTTP call target")),
        };

        info!(task = %task.name, method = %method, uri = %uri, "executing HTTP call");

        let mut req = match method.to_uppercase().as_str() {
            "GET" => self.client.get(&uri),
            "POST" => self.client.post(&uri),
            "PUT" => self.client.put(&uri),
            "DELETE" => self.client.delete(&uri),
            "PATCH" => self.client.patch(&uri),
            other => return Err(RuntimeError::task_failed(&task.name, format!("unsupported HTTP method: {}", other))),
        };

        // Add headers
        if let Some(Value::Object(hdrs)) = headers {
            for (key, val) in hdrs {
                if let Some(v) = val.as_str() {
                    req = req.header(&key, v);
                }
            }
        }

        // Add body
        if let Some(body_val) = body {
            req = req.header("Content-Type", "application/json");
            req = req.json(&body_val);
        }

        let resp = req.send().await
            .map_err(|e| RuntimeError::Communication(format!("HTTP request failed: {}", e)))?;

        let status = resp.status().as_u16();
        let resp_body: Value = resp.json().await.unwrap_or(Value::Null);

        debug!(task = %task.name, status = status, "HTTP response received");

        if status >= 400 {
            return Err(RuntimeError::Communication(format!(
                "HTTP {} ({}): {}", status, uri, resp_body
            )));
        }

        Ok(TaskResult::new(json!({
            "status": status,
            "body": resp_body,
        })))
    }
}
