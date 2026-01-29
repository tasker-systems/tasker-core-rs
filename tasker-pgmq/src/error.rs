//! Error types for tasker-pgmq

use crate::types::MessagingError;
use thiserror::Error;

/// Result type for tasker-pgmq operations
pub type Result<T> = std::result::Result<T, PgmqNotifyError>;

/// Errors that can occur in tasker-pgmq operations
#[derive(Error, Debug)]
pub enum PgmqNotifyError {
    /// Database connection or query errors
    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),

    /// JSON serialization/deserialization errors
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    /// Invalid configuration
    #[error("Configuration error: {message}")]
    Configuration { message: String },

    /// Invalid channel name
    #[error("Invalid channel name: {channel}")]
    InvalidChannel { channel: String },

    /// Invalid queue name pattern
    #[error("Invalid queue name pattern: {pattern}")]
    InvalidPattern { pattern: String },

    /// Listener not connected
    #[error("Listener is not connected to database")]
    NotConnected,

    /// Channel already being listened to
    #[error("Already listening to channel: {channel}")]
    AlreadyListening { channel: String },

    /// Regex compilation error
    #[error("Regex error: {0}")]
    Regex(#[from] regex::Error),

    /// Generic error for compatibility
    #[error("Generic error: {0}")]
    Generic(#[from] anyhow::Error),

    /// Messaging error for compatibility with tasker-shared
    #[error("Messaging error: {0}")]
    Messaging(#[from] MessagingError),

    /// PGMQ error from underlying library
    #[error("PGMQ error: {0}")]
    Pgmq(#[from] pgmq::errors::PgmqError),
}

impl PgmqNotifyError {
    /// Create a configuration error
    pub fn config<S: Into<String>>(message: S) -> Self {
        Self::Configuration {
            message: message.into(),
        }
    }

    /// Create an invalid channel error
    pub fn invalid_channel<S: Into<String>>(channel: S) -> Self {
        Self::InvalidChannel {
            channel: channel.into(),
        }
    }

    /// Create an invalid pattern error
    pub fn invalid_pattern<S: Into<String>>(pattern: S) -> Self {
        Self::InvalidPattern {
            pattern: pattern.into(),
        }
    }

    /// Create an already listening error
    pub fn already_listening<S: Into<String>>(channel: S) -> Self {
        Self::AlreadyListening {
            channel: channel.into(),
        }
    }
}
