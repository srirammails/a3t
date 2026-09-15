//! In-memory execution queue implementation
//!
//! Uses tokio mpsc channels for the execution queue within a single process.

use std::collections::VecDeque;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use tokio::sync::{Mutex, Notify};
use uuid::Uuid;

use flowr_storage::ExecutionQueue;
use flowr_types::{ExecutionRequest, FlowResult};

/// In-memory execution queue using a VecDeque with condition variable
pub struct InMemoryExecutionQueue {
    queue: Arc<Mutex<VecDeque<ExecutionRequest>>>,
    notify: Arc<Notify>,
}

impl InMemoryExecutionQueue {
    pub fn new() -> Self {
        Self {
            queue: Arc::new(Mutex::new(VecDeque::new())),
            notify: Arc::new(Notify::new()),
        }
    }
}

impl Default for InMemoryExecutionQueue {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ExecutionQueue for InMemoryExecutionQueue {
    async fn enqueue(&self, request: ExecutionRequest) -> FlowResult<()> {
        let mut queue = self.queue.lock().await;
        queue.push_back(request);
        self.notify.notify_one();
        Ok(())
    }

    async fn dequeue(&self, timeout: Duration) -> FlowResult<Option<ExecutionRequest>> {
        // Try to get from queue immediately
        {
            let mut queue = self.queue.lock().await;
            if let Some(request) = queue.pop_front() {
                return Ok(Some(request));
            }
        }

        // Wait for notification with timeout
        let result = tokio::time::timeout(timeout, self.notify.notified()).await;

        if result.is_err() {
            // Timeout
            return Ok(None);
        }

        // Try again after notification
        let mut queue = self.queue.lock().await;
        Ok(queue.pop_front())
    }

    async fn ack(&self, _instance_id: Uuid) -> FlowResult<()> {
        // In-memory queue doesn't need explicit ack
        Ok(())
    }

    async fn nack(&self, request: ExecutionRequest) -> FlowResult<()> {
        // Return to front of queue for retry
        let mut queue = self.queue.lock().await;
        queue.push_front(request);
        self.notify.notify_one();
        Ok(())
    }

    async fn len(&self) -> FlowResult<usize> {
        let queue = self.queue.lock().await;
        Ok(queue.len())
    }
}
