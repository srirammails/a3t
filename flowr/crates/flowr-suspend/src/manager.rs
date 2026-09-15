//! Suspend manager

use chrono::Utc;
use kameo::actor::ActorRef;
use kameo::request::MessageSend;
use redis::AsyncCommands;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use tracing::{debug, error, info, instrument, warn};
use uuid::Uuid;

use crate::actor::{FlowActor, FlowActorMessage, FlowActorResult, ResumeTrigger};
use crate::event::{CloudEvent, EventFilter, EventMatcher};
use crate::suspension::{SuspendedFlow, SuspensionType};
use crate::{SuspendError, SuspendResult};

/// Dragonfly key prefixes
const SUSPEND_PREFIX: &str = "flowr:suspend";
const TIMERS_KEY: &str = "flowr:suspend:timers";
const LISTENERS_PREFIX: &str = "flowr:suspend:listeners";

/// Callback when a flow resumes
pub type ResumeCallback = Box<dyn Fn(Uuid, Value, ResumeTrigger) + Send + Sync>;

/// Suspend manager for Wait and Listen tasks
pub struct SuspendManager {
    /// Redis/Dragonfly connection
    redis: redis::aio::MultiplexedConnection,

    /// Active flow actors by instance ID
    actors: Arc<RwLock<HashMap<Uuid, ActorRef<FlowActor>>>>,

    /// Event type to instance IDs mapping for fast lookup
    event_listeners: Arc<RwLock<HashMap<String, Vec<Uuid>>>>,

    /// Resume callback channel
    resume_rx: Arc<RwLock<Option<mpsc::Receiver<(Uuid, FlowActorResult)>>>>,

    /// Resume callback sender (for actors)
    resume_tx: mpsc::Sender<(Uuid, FlowActorResult)>,

    /// Optional callback for resumed flows
    on_resume: Option<Arc<ResumeCallback>>,
}

impl SuspendManager {
    /// Create a new suspend manager
    pub fn new(redis: redis::aio::MultiplexedConnection) -> Self {
        let (resume_tx, resume_rx) = mpsc::channel(1000);

        Self {
            redis,
            actors: Arc::new(RwLock::new(HashMap::new())),
            event_listeners: Arc::new(RwLock::new(HashMap::new())),
            resume_rx: Arc::new(RwLock::new(Some(resume_rx))),
            resume_tx,
            on_resume: None,
        }
    }

    /// Set callback for when flows resume
    pub fn on_resume<F>(mut self, callback: F) -> Self
    where
        F: Fn(Uuid, Value, ResumeTrigger) + Send + Sync + 'static,
    {
        self.on_resume = Some(Arc::new(Box::new(callback)));
        self
    }

    /// Start the resume handler loop
    pub async fn start(&self) -> SuspendResult<()> {
        let mut rx = {
            let mut lock = self.resume_rx.write().await;
            lock.take()
                .ok_or_else(|| SuspendError::Internal("resume handler already started".to_string()))?
        };

        let actors = self.actors.clone();
        let event_listeners = self.event_listeners.clone();
        let mut redis = self.redis.clone();
        let on_resume = self.on_resume.clone();

        tokio::spawn(async move {
            while let Some((instance_id, result)) = rx.recv().await {
                debug!(%instance_id, ?result, "received resume notification");

                // Remove from actors map
                {
                    let mut actors = actors.write().await;
                    if let Some(actor_ref) = actors.remove(&instance_id) {
                        // Stop the actor
                        let _ = actor_ref.stop_gracefully().await;
                    }
                }

                // Remove from event listeners if applicable
                {
                    let mut listeners = event_listeners.write().await;
                    for instances in listeners.values_mut() {
                        instances.retain(|id| *id != instance_id);
                    }
                }

                // Clean up Dragonfly state
                let key = format!("{}:{}", SUSPEND_PREFIX, instance_id);
                let _: Result<(), _> = redis.del(&key).await;

                // Remove from timers sorted set
                let _: Result<(), _> = redis
                    .zrem::<_, _, ()>(TIMERS_KEY, instance_id.to_string())
                    .await;

                // Call resume callback if set
                if let Some(ref callback) = on_resume {
                    match result {
                        FlowActorResult::Resume { context, trigger } => {
                            callback(instance_id, context, trigger);
                        }
                        FlowActorResult::Cancelled { reason } => {
                            warn!(%instance_id, %reason, "flow cancelled");
                        }
                        FlowActorResult::Timeout => {
                            warn!(%instance_id, "flow timed out");
                        }
                        FlowActorResult::State(_) => {}
                    }
                }
            }
        });

        Ok(())
    }

    /// Suspend a flow for waiting
    #[instrument(skip(self, flow))]
    pub async fn suspend_wait(&self, flow: SuspendedFlow) -> SuspendResult<()> {
        let instance_id = flow.instance_id;

        info!(
            %instance_id,
            namespace = %flow.namespace,
            name = %flow.name,
            step = %flow.step_name,
            "suspending flow for wait"
        );

        // Store in Dragonfly
        self.store_suspension(&flow).await?;

        // Add to timers sorted set
        if let SuspensionType::Wait { resume_at, .. } = &flow.suspension {
            let score = resume_at.timestamp_millis() as f64;
            let mut redis = self.redis.clone();
            redis
                .zadd::<_, _, _, ()>(TIMERS_KEY, instance_id.to_string(), score)
                .await?;
        }

        // Spawn actor
        self.spawn_actor(flow).await?;

        Ok(())
    }

    /// Suspend a flow for listening
    #[instrument(skip(self, flow))]
    pub async fn suspend_listen(&self, flow: SuspendedFlow) -> SuspendResult<()> {
        let instance_id = flow.instance_id;

        info!(
            %instance_id,
            namespace = %flow.namespace,
            name = %flow.name,
            step = %flow.step_name,
            "suspending flow for listen"
        );

        // Store in Dragonfly
        self.store_suspension(&flow).await?;

        // Register event listeners
        if let SuspensionType::Listen { ref filters, .. } = flow.suspension {
            self.register_listeners(instance_id, filters).await?;
        }

        // Add to timers if timeout specified
        if let SuspensionType::Listen {
            timeout_at: Some(timeout),
            ..
        } = &flow.suspension
        {
            let score = timeout.timestamp_millis() as f64;
            let mut redis = self.redis.clone();
            redis
                .zadd::<_, _, _, ()>(TIMERS_KEY, instance_id.to_string(), score)
                .await?;
        }

        // Spawn actor
        self.spawn_actor(flow).await?;

        Ok(())
    }

    /// Store suspension in Dragonfly
    async fn store_suspension(&self, flow: &SuspendedFlow) -> SuspendResult<()> {
        let key = format!("{}:{}", SUSPEND_PREFIX, flow.instance_id);
        let json = serde_json::to_string(flow)?;
        let mut redis = self.redis.clone();
        redis.set::<_, _, ()>(&key, &json).await?;
        Ok(())
    }

    /// Register event listeners in Dragonfly and local cache
    async fn register_listeners(
        &self,
        instance_id: Uuid,
        filters: &[EventFilter],
    ) -> SuspendResult<()> {
        let mut redis = self.redis.clone();
        let mut listeners = self.event_listeners.write().await;

        for filter in filters {
            if let Some(ref event_type) = filter.event_type {
                // Store in Dragonfly
                let key = format!("{}:{}", LISTENERS_PREFIX, event_type);
                redis.sadd::<_, _, ()>(&key, instance_id.to_string()).await?;

                // Store in local cache
                listeners
                    .entry(event_type.clone())
                    .or_default()
                    .push(instance_id);
            }
        }

        Ok(())
    }

    /// Spawn a flow actor
    async fn spawn_actor(&self, flow: SuspendedFlow) -> SuspendResult<()> {
        let instance_id = flow.instance_id;
        let actor = FlowActor::new(flow, self.resume_tx.clone());

        let actor_ref = kameo::spawn(actor);

        let mut actors = self.actors.write().await;
        actors.insert(instance_id, actor_ref);

        Ok(())
    }

    /// Deliver an event to matching listeners
    #[instrument(skip(self, event))]
    pub async fn deliver_event(&self, event: &CloudEvent) -> SuspendResult<Vec<Uuid>> {
        let mut resumed = Vec::new();

        // Find listeners for this event type
        let event_type = &event.event_type;

        // Get potential listeners from local cache
        let potential_listeners = {
            let listeners = self.event_listeners.read().await;

            // Exact match
            let mut candidates: Vec<Uuid> = listeners
                .get(event_type)
                .cloned()
                .unwrap_or_default();

            // Wildcard matches (simple prefix matching)
            for (pattern, instances) in listeners.iter() {
                if pattern.ends_with('*') {
                    let prefix = pattern.trim_end_matches('*');
                    if event_type.starts_with(prefix) {
                        candidates.extend(instances.iter().cloned());
                    }
                }
            }

            candidates
        };

        // Check each potential listener
        let actors = self.actors.read().await;

        for instance_id in potential_listeners {
            if let Some(actor_ref) = actors.get(&instance_id) {
                // Get the flow state to check filters
                let (tx, rx) = tokio::sync::oneshot::channel();
                if actor_ref
                    .tell(FlowActorMessage::GetState { reply: tx })
                    .send()
                    .await
                    .is_ok()
                {
                    if let Ok(flow) = rx.await {
                        // Check if event matches filters
                        if let Some(filters) = flow.event_filters() {
                            let matches = EventMatcher::find_matching_filters(event, filters.iter());
                            if !matches.is_empty() {
                                // Event matches, send to actor
                                if actor_ref
                                    .tell(FlowActorMessage::EventReceived {
                                        event: event.clone(),
                                    })
                                    .send()
                                    .await
                                    .is_ok()
                                {
                                    resumed.push(instance_id);
                                    info!(
                                        %instance_id,
                                        event_type = %event.event_type,
                                        "event delivered to flow"
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }

        Ok(resumed)
    }

    /// Send a signal to a suspended flow
    #[instrument(skip(self, data))]
    pub async fn signal(&self, instance_id: Uuid, data: Value) -> SuspendResult<()> {
        let actors = self.actors.read().await;

        let actor_ref = actors
            .get(&instance_id)
            .ok_or(SuspendError::NotFound(instance_id))?;

        actor_ref
            .tell(FlowActorMessage::Signal { data })
            .send()
            .await
            .map_err(|e| SuspendError::Actor(e.to_string()))?;

        info!(%instance_id, "signal sent to flow");
        Ok(())
    }

    /// Cancel a suspended flow
    #[instrument(skip(self))]
    pub async fn cancel(&self, instance_id: Uuid, reason: String) -> SuspendResult<()> {
        let actors = self.actors.read().await;

        let actor_ref = actors
            .get(&instance_id)
            .ok_or(SuspendError::NotFound(instance_id))?;

        actor_ref
            .tell(FlowActorMessage::Cancel { reason: reason.clone() })
            .send()
            .await
            .map_err(|e| SuspendError::Actor(e.to_string()))?;

        info!(%instance_id, %reason, "flow cancelled");
        Ok(())
    }

    /// Get a suspended flow by instance ID
    pub async fn get(&self, instance_id: Uuid) -> SuspendResult<SuspendedFlow> {
        let key = format!("{}:{}", SUSPEND_PREFIX, instance_id);
        let mut redis = self.redis.clone();

        let json: Option<String> = redis.get(&key).await?;

        let json = json.ok_or(SuspendError::NotFound(instance_id))?;
        let flow: SuspendedFlow = serde_json::from_str(&json)?;

        Ok(flow)
    }

    /// List all suspended flows
    pub async fn list(&self) -> SuspendResult<Vec<SuspendedFlow>> {
        let actors = self.actors.read().await;
        let mut flows = Vec::with_capacity(actors.len());

        for instance_id in actors.keys() {
            match self.get(*instance_id).await {
                Ok(flow) => flows.push(flow),
                Err(e) => {
                    warn!(%instance_id, error = %e, "failed to get suspended flow");
                }
            }
        }

        Ok(flows)
    }

    /// Recover suspended flows from Dragonfly on restart
    #[instrument(skip(self))]
    pub async fn recover(&self) -> SuspendResult<usize> {
        let mut redis = self.redis.clone();
        let now = Utc::now().timestamp_millis() as f64;

        // Get all flows from timers sorted set
        let instance_ids: Vec<String> = redis
            .zrangebyscore(TIMERS_KEY, "-inf", "+inf")
            .await?;

        let mut recovered = 0;

        for id_str in instance_ids {
            let instance_id = match Uuid::parse_str(&id_str) {
                Ok(id) => id,
                Err(_) => continue,
            };

            match self.get(instance_id).await {
                Ok(flow) => {
                    // Check if already expired
                    if let Some(resume_at) = flow.resume_at() {
                        if resume_at.timestamp_millis() as f64 <= now {
                            // Already past resume time, trigger immediately
                            info!(%instance_id, "recovering expired suspension");
                        }
                    }

                    // Re-spawn actor
                    if let Err(e) = self.spawn_actor(flow).await {
                        error!(%instance_id, error = %e, "failed to recover flow actor");
                    } else {
                        recovered += 1;
                    }
                }
                Err(e) => {
                    warn!(%instance_id, error = %e, "failed to recover flow");
                    // Clean up orphaned entry
                    let _: Result<(), _> = redis
                        .zrem::<_, _, ()>(TIMERS_KEY, id_str)
                        .await;
                }
            }
        }

        // Recover event listeners
        let pattern = format!("{}:*", LISTENERS_PREFIX);
        let keys: Vec<String> = redis::cmd("KEYS")
            .arg(&pattern)
            .query_async(&mut redis)
            .await?;

        for key in keys {
            let event_type = key.strip_prefix(&format!("{}:", LISTENERS_PREFIX))
                .unwrap_or(&key)
                .to_string();

            let instance_ids: Vec<String> = redis.smembers(&key).await?;

            let mut listeners = self.event_listeners.write().await;
            for id_str in instance_ids {
                if let Ok(id) = Uuid::parse_str(&id_str) {
                    listeners.entry(event_type.clone()).or_default().push(id);
                }
            }
        }

        info!(recovered, "recovered suspended flows");
        Ok(recovered)
    }

    /// Get count of suspended flows
    pub async fn count(&self) -> usize {
        self.actors.read().await.len()
    }
}
