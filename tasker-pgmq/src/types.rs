//! # Types for tasker-pgmq client
//!
//! Contains generic types and shared structures used by the tasker-pgmq client.
//! Tasker-specific types like `PgmqStepMessage` are defined in tasker-shared.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Queue metrics for monitoring
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct QueueMetrics {
    /// Name of the queue
    pub queue_name: String,
    /// Current message count in queue
    pub message_count: i64,
    /// Number of active consumers (if available)
    pub consumer_count: Option<i64>,
    /// Age of oldest message in seconds (if any)
    pub oldest_message_age_seconds: Option<i64>,
}

/// Client status information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientStatus {
    /// Type of client (e.g., "tasker-pgmq")
    pub client_type: String,
    /// Whether client is connected
    pub connected: bool,
    /// Connection information
    pub connection_info: HashMap<String, serde_json::Value>,
    /// Last activity timestamp
    pub last_activity: Option<DateTime<Utc>>,
}

/// Error type for messaging operations
#[derive(Debug, thiserror::Error)]
pub enum MessagingError {
    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("PGMQ error: {0}")]
    Pgmq(String),

    #[error("Configuration error: {0}")]
    Configuration(String),

    #[error("Generic error: {0}")]
    Generic(String),
}

impl From<String> for MessagingError {
    fn from(msg: String) -> Self {
        Self::Generic(msg)
    }
}

impl From<&str> for MessagingError {
    fn from(msg: &str) -> Self {
        Self::Generic(msg.to_string())
    }
}

impl From<anyhow::Error> for MessagingError {
    fn from(err: anyhow::Error) -> Self {
        Self::Generic(err.to_string())
    }
}

impl From<Box<dyn std::error::Error + Send + Sync>> for MessagingError {
    fn from(err: Box<dyn std::error::Error + Send + Sync>) -> Self {
        Self::Generic(err.to_string())
    }
}
