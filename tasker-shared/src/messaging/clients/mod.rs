//! # Message Client Module (TAS-40 Phase 5)
//!
//! Contains all message client implementations for different queue backends.
//! This module provides the core abstraction layer for message queue operations
//! in the tasker system.
//!
//! ## Structure
//!
//! - `unified_client.rs` - Main abstraction trait and enum for backend switching
//! - `pgmq_client.rs` - PostgreSQL Message Queue (pgmq) implementation
//! - `protected_pgmq_client.rs` - Circuit breaker protected pgmq client
//! - `in_memory_client.rs` - In-memory implementation for testing
//!
//! ## Usage
//!
//! ```rust
//! use crate::messaging::clients::{UnifiedMessageClient, MessageClient};
//!
//! // Create a client based on your needs
//! let client = UnifiedMessageClient::new_in_memory(); // For testing
//! let client = UnifiedMessageClient::new_pgmq(database_url).await?; // For production
//!
//! // Use the unified interface
//! client.send_step_message(namespace, message).await?;
//! let messages = client.receive_step_messages(namespace, 10, 30).await?;
//! ```

pub mod in_memory_client;
pub mod pgmq_client;
pub mod protected_pgmq_client;
pub mod traits;
pub mod types;
pub mod unified_client;

// Re-export the main types for convenience
pub use in_memory_client::{InMemoryClient, InMemoryMessage, InMemoryQueue};
pub use pgmq_client::PgmqClient;
pub use protected_pgmq_client::{ProtectedPgmqClient, ProtectedPgmqError};
pub use traits::PgmqClientTrait;
pub use types::{ClientStatus, PgmqStepMessage, PgmqStepMessageMetadata, QueueMetrics};
pub use unified_client::{MessageClient, UnifiedMessageClient};
