//! Cache error types (TAS-156)

use thiserror::Error;

/// Errors that can occur during cache operations
#[derive(Debug, Error)]
pub enum CacheError {
    /// Failed to connect to cache backend
    #[error("Cache connection error: {0}")]
    ConnectionError(String),

    /// Failed to serialize or deserialize cache value
    #[error("Cache serialization error: {0}")]
    SerializationError(String),

    /// Cache operation timed out
    #[error("Cache operation timed out: {0}")]
    Timeout(String),

    /// Generic backend error
    #[error("Cache backend error: {0}")]
    BackendError(String),
}

/// Result type for cache operations
pub type CacheResult<T> = Result<T, CacheError>;
