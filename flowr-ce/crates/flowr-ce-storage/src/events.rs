//! In-memory event bus implementation
//!
//! Uses tokio broadcast channels for pub/sub within a single process.

use async_trait::async_trait;
use futures::StreamExt;
use tokio::sync::broadcast;
use tokio_stream::wrappers::BroadcastStream;
use uuid::Uuid;

use flowr_storage::{EventBus, EventStream, LifecycleEvent};
use flowr_types::FlowResult;

/// In-memory event bus using tokio broadcast channels
pub struct InMemoryEventBus {
    sender: broadcast::Sender<LifecycleEvent>,
}

impl InMemoryEventBus {
    pub fn new(sender: broadcast::Sender<LifecycleEvent>) -> Self {
        Self { sender }
    }
}

#[async_trait]
impl EventBus for InMemoryEventBus {
    async fn emit(&self, event: LifecycleEvent) -> FlowResult<()> {
        // Ignore send errors (no receivers)
        let _ = self.sender.send(event);
        Ok(())
    }

    async fn subscribe(&self) -> FlowResult<EventStream> {
        let receiver = self.sender.subscribe();
        let stream = BroadcastStream::new(receiver);

        // Filter out lagged errors
        let filtered = futures::stream::unfold(
            stream,
            |mut stream: BroadcastStream<LifecycleEvent>| async move {
                loop {
                    match stream.next().await {
                        Some(Ok(event)) => return Some((event, stream)),
                        Some(Err(_)) => continue, // Skip lagged messages
                        None => return None,
                    }
                }
            },
        );

        Ok(Box::pin(filtered))
    }

    async fn subscribe_instance(&self, instance_id: Uuid) -> FlowResult<EventStream> {
        let receiver = self.sender.subscribe();
        let stream = BroadcastStream::new(receiver);

        // Filter for specific instance
        let filtered = futures::stream::unfold(
            (stream, instance_id),
            |(mut stream, id): (BroadcastStream<LifecycleEvent>, Uuid)| async move {
                loop {
                    match stream.next().await {
                        Some(Ok(event)) if event.instance_id == id => {
                            return Some((event, (stream, id)));
                        }
                        Some(Ok(_)) => continue, // Wrong instance
                        Some(Err(_)) => continue, // Skip lagged messages
                        None => return None,
                    }
                }
            },
        );

        Ok(Box::pin(filtered))
    }
}
