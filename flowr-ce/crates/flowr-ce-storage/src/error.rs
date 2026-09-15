//! Storage error types

use thiserror::Error;

#[derive(Debug, Error)]
pub enum StorageError {
    #[error("LanceDB error: {0}")]
    Lance(String),
    
    #[error("Serialization error: {0}")]
    Serialization(String),
    
    #[error("Not found: {0}")]
    NotFound(String),
    
    #[error("Already exists: {0}")]
    AlreadyExists(String),
}

pub type StorageResult<T> = Result<T, StorageError>;
