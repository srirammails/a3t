//! Flow actor for suspended workflows

use chrono::Utc;
use kameo::actor::ActorRef;
use kameo::mailbox::unbounded::UnboundedMailbox;
use kameo::message::{Context, Message};
use kameo::request::MessageSend;
use kameo::Actor;
use serde_json::Value;
use std::time::Duration;
use tokio::sync::oneshot;
use tracing::{debug, info, warn};
use uuid::Uuid;

use crate::event::CloudEvent;
use crate::suspension::{SuspendedFlow, SuspensionType};
use crate::SuspendError;

/// Messages that can be sent to a FlowActor
#[derive(Debug)]
pub enum FlowActorMessage {
    /// Timer expired, resume the flow
    TimerExpired,

    /// Event received that matches filters
    EventReceived {
        event: CloudEvent,
    },

    /// Manual signal to resume
    Signal {
        data: Value,
    },

    /// Cancel the suspension
    Cancel {
        reason: String,
    },

    /// Get current state
    GetState {
        reply: oneshot::Sender<SuspendedFlow>,
    },
}

/// Result of flow actor processing
#[derive(Debug, Clone)]
pub enum FlowActorResult {
    /// Flow should resume with this data
    Resume { context: Value, trigger: ResumeTrigger },

    /// Flow was cancelled
    Cancelled { reason: String },

    /// State returned
    State(SuspendedFlow),

    /// Timeout occurred
    Timeout,
}

/// What triggered the resume
#[derive(Debug, Clone)]
pub enum ResumeTrigger {
    /// Timer expired (Wait)
    Timer,
    /// Event matched (Listen)
    Event(CloudEvent),
    /// Manual signal
    Signal(Value),
}

/// Actor that holds a suspended flow state
pub struct FlowActor {
    /// Suspended flow state
    pub flow: SuspendedFlow,

    /// Channel to notify manager when resuming
    resume_tx: Option<tokio::sync::mpsc::Sender<(Uuid, FlowActorResult)>>,
}

impl FlowActor {
    /// Create a new flow actor
    pub fn new(
        flow: SuspendedFlow,
        resume_tx: tokio::sync::mpsc::Sender<(Uuid, FlowActorResult)>,
    ) -> Self {
        Self {
            flow,
            resume_tx: Some(resume_tx),
        }
    }

    /// Schedule a timer for Wait suspensions
    pub fn schedule_timer(&self, actor_ref: ActorRef<Self>) {
        if let SuspensionType::Wait { resume_at, .. } = &self.flow.suspension {
            let now = Utc::now();
            if *resume_at > now {
                let duration = (*resume_at - now).to_std().unwrap_or(Duration::ZERO);
                let instance_id = self.flow.instance_id;

                debug!(
                    %instance_id,
                    ?duration,
                    "scheduling timer for Wait suspension"
                );

                tokio::spawn(async move {
                    tokio::time::sleep(duration).await;
                    if let Err(e) = actor_ref.tell(FlowActorMessage::TimerExpired).send().await {
                        warn!(%instance_id, error = %e, "failed to send timer expired message");
                    }
                });
            } else {
                // Already past resume time, send immediately
                let instance_id = self.flow.instance_id;
                tokio::spawn(async move {
                    if let Err(e) = actor_ref.tell(FlowActorMessage::TimerExpired).send().await {
                        warn!(%instance_id, error = %e, "failed to send immediate timer message");
                    }
                });
            }
        }
    }

    /// Schedule a timeout for Listen suspensions
    pub fn schedule_timeout(&self, actor_ref: ActorRef<Self>) {
        if let SuspensionType::Listen { timeout_at: Some(timeout), .. } = &self.flow.suspension {
            let now = Utc::now();
            if *timeout > now {
                let duration = (*timeout - now).to_std().unwrap_or(Duration::ZERO);
                let instance_id = self.flow.instance_id;

                debug!(
                    %instance_id,
                    ?duration,
                    "scheduling timeout for Listen suspension"
                );

                tokio::spawn(async move {
                    tokio::time::sleep(duration).await;
                    // Send cancel with timeout reason
                    if let Err(e) = actor_ref
                        .tell(FlowActorMessage::Cancel {
                            reason: "timeout".to_string(),
                        })
                        .send()
                        .await
                    {
                        warn!(%instance_id, error = %e, "failed to send timeout message");
                    }
                });
            }
        }
    }

    /// Notify the manager that we're resuming
    async fn notify_resume(&mut self, result: FlowActorResult) {
        if let Some(tx) = &self.resume_tx {
            let _ = tx.send((self.flow.instance_id, result)).await;
        }
    }
}

impl Actor for FlowActor {
    type Mailbox = UnboundedMailbox<Self>;

    async fn on_start(&mut self, actor_ref: ActorRef<Self>) -> Result<(), kameo::error::BoxError> {
        info!(
            instance_id = %self.flow.instance_id,
            step = %self.flow.step_name,
            "flow actor started"
        );

        // Schedule timer or timeout based on suspension type
        match &self.flow.suspension {
            SuspensionType::Wait { .. } => {
                self.schedule_timer(actor_ref);
            }
            SuspensionType::Listen { timeout_at: Some(_), .. } => {
                self.schedule_timeout(actor_ref);
            }
            _ => {}
        }

        Ok(())
    }

    async fn on_stop(
        self,
        _actor_ref: kameo::actor::WeakActorRef<Self>,
        _reason: kameo::error::ActorStopReason,
    ) -> Result<(), kameo::error::BoxError> {
        info!(
            instance_id = %self.flow.instance_id,
            "flow actor stopped"
        );
        Ok(())
    }
}

impl Message<FlowActorMessage> for FlowActor {
    type Reply = Result<FlowActorResult, SuspendError>;

    async fn handle(
        &mut self,
        msg: FlowActorMessage,
        _ctx: Context<'_, Self, Self::Reply>,
    ) -> Self::Reply {
        match msg {
            FlowActorMessage::TimerExpired => {
                info!(
                    instance_id = %self.flow.instance_id,
                    "timer expired, resuming flow"
                );

                let result = FlowActorResult::Resume {
                    context: self.flow.context.clone(),
                    trigger: ResumeTrigger::Timer,
                };

                self.notify_resume(result.clone()).await;
                Ok(result)
            }

            FlowActorMessage::EventReceived { event } => {
                info!(
                    instance_id = %self.flow.instance_id,
                    event_type = %event.event_type,
                    "event received, resuming flow"
                );

                // Merge event data into context
                let mut context = self.flow.context.clone();
                if let Value::Object(ref mut ctx_obj) = context {
                    ctx_obj.insert("event".to_string(), serde_json::to_value(&event).unwrap_or(Value::Null));
                }

                let result = FlowActorResult::Resume {
                    context,
                    trigger: ResumeTrigger::Event(event),
                };

                self.notify_resume(result.clone()).await;
                Ok(result)
            }

            FlowActorMessage::Signal { data } => {
                info!(
                    instance_id = %self.flow.instance_id,
                    "signal received, resuming flow"
                );

                // Merge signal data into context
                let mut context = self.flow.context.clone();
                if let Value::Object(ref mut ctx_obj) = context {
                    ctx_obj.insert("signal".to_string(), data.clone());
                }

                let result = FlowActorResult::Resume {
                    context,
                    trigger: ResumeTrigger::Signal(data),
                };

                self.notify_resume(result.clone()).await;
                Ok(result)
            }

            FlowActorMessage::Cancel { reason } => {
                info!(
                    instance_id = %self.flow.instance_id,
                    %reason,
                    "flow cancelled"
                );

                let result = if reason == "timeout" {
                    FlowActorResult::Timeout
                } else {
                    FlowActorResult::Cancelled { reason }
                };

                self.notify_resume(result.clone()).await;
                Ok(result)
            }

            FlowActorMessage::GetState { reply } => {
                let _ = reply.send(self.flow.clone());
                Ok(FlowActorResult::State(self.flow.clone()))
            }
        }
    }
}
