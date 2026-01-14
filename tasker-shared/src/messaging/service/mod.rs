//! # Messaging Service Abstraction Layer (TAS-133)
//!
//! Provider-agnostic messaging abstraction enabling multi-backend support
//! (PGMQ, RabbitMQ, InMemory) with enum dispatch for hot-path performance.
//!
//! ## Architecture
//!
//! ```text
//! MessagingProvider (enum)      <- Zero-cost dispatch, no vtable
//!   ├── Pgmq(PgmqMessagingService)
//!   ├── RabbitMq(RabbitMqMessagingService)
//!   └── InMemory(InMemoryMessagingService)
//!
//! MessageRouterKind (enum)      <- Queue name resolution
//!   └── Default(DefaultMessageRouter)
//! ```
//!
//! ## Usage
//!
//! ```ignore
//! // Provider selection via configuration
//! let provider = MessagingProvider::Pgmq(PgmqMessagingService::new(pool).await?);
//! let router = MessageRouterKind::Default(DefaultMessageRouter::from_config(&config));
//!
//! // Send a message
//! let msg_id = provider.send_message("my_queue", &message).await?;
//!
//! // Receive messages
//! let messages = provider.receive_messages::<MyMessage>(
//!     "my_queue",
//!     10,
//!     Duration::from_secs(30),
//! ).await?;
//! ```
//!
//! ## Design Decisions
//!
//! - **Enum dispatch** instead of `Arc<dyn MessagingService>` for hot-path performance
//! - **QueueMessage trait** for serialization abstraction (JSON now, MessagePack later)
//! - **MessageRouter** separates queue naming from messaging operations

mod provider;
pub mod providers;
mod router;
mod traits;
mod types;

pub use provider::MessagingProvider;
pub use providers::{InMemoryMessagingService, PgmqMessagingService};
pub use router::{DefaultMessageRouter, MessageRouter, MessageRouterKind};
pub use traits::{MessagingService, QueueMessage};
pub use types::{
    AtomicQueueStats, MessageId, QueueHealthReport, QueueStats, QueuedMessage, ReceiptHandle,
};

// Re-export MessagingError from parent module - no duplication
pub use super::errors::MessagingError;
pub use super::MessagingResult;

// Re-export PgmqNotifyConfig for convenient access when constructing providers
pub use pgmq_notify::PgmqNotifyConfig;
