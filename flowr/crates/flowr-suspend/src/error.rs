//! Suspend manager errors

use thiserror::Error;
use uuid::Uuid;

/// Result type for suspend operations
pub type SuspendResult<T> = Result<T, SuspendError>;

/// Suspend manager errors
#[derive(Error, Debug)]
pub enum SuspendError {
    /// Flow not found
    #[error("suspended flow not found: {0}")]
    NotFound(Uuid),

    /// Flow already suspended
    #[error("flow already suspended: {0}")]
    AlreadySuspended(Uuid),

    /// Invalid suspension state
    #[error("invalid suspension state: {0}")]
    InvalidState(String),

    /// Timer error
    #[error("timer error: {0}")]
    Timer(String),

    /// Event filter error
    #[error("event filter error: {0}")]
    EventFilter(String),

    /// Storage error
    #[error("storage error: {0}")]
    Storage(String),

    /// Actor error
    #[error("actor error: {0}")]
    Actor(String),

    /// Serialization error
    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    /// Timeout
    #[error("suspension timeout: {0}")]
    Timeout(Uuid),

    /// Internal error
    #[error("internal error: {0}")]
    Internal(String),
}

impl From<redis::RedisError> for SuspendError {
    fn from(err: redis::RedisError) -> Self {
        SuspendError::Storage(err.to_string())
    }
}
