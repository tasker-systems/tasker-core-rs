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
//!
//! SupportsPushNotifications     <- Push delivery abstraction (TAS-133)
//!   ├── PGMQ: Signal-only (pg_notify), requires fallback polling
//!   └── RabbitMQ: Full message push (basic_consume), no polling needed
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
//! // Receive messages (polling)
//! let messages = provider.receive_messages::<MyMessage>(
//!     "my_queue",
//!     10,
//!     Duration::from_secs(30),
//! ).await?;
//!
//! // Push notifications (TAS-133)
//! let stream = provider.subscribe("my_queue")?;
//! while let Some(notification) = stream.next().await {
//!     match notification {
//!         MessageNotification::Available { queue_name } => { /* fetch message */ }
//!         MessageNotification::Message(msg) => { /* process directly */ }
//!     }
//! }
//! ```
//!
//! ## Design Decisions
//!
//! - **Enum dispatch** instead of `Arc<dyn MessagingService>` for hot-path performance
//! - **QueueMessage trait** for serialization abstraction (JSON now, MessagePack later)
//! - **MessageRouter** separates queue naming from messaging operations
//! - **SupportsPushNotifications trait** abstracts delivery model differences (TAS-133)
//! - **MessageNotification enum** captures signal-only vs full-message push (TAS-133)

mod provider;
pub mod providers;
mod router;
mod traits;
mod types;

pub use provider::MessagingProvider;
pub use providers::{InMemoryMessagingService, PgmqMessagingService};
pub use router::{DefaultMessageRouter, MessageRouter, MessageRouterKind};
pub use traits::{MessagingService, NotificationStream, QueueMessage, SupportsPushNotifications};
pub use types::{
    AtomicQueueStats, MessageEvent, MessageHandle, MessageId, MessageMetadata, MessageNotification,
    QueueHealthReport, QueueStats, QueuedMessage, ReceiptHandle,
};

// Re-export MessagingError from parent module - no duplication
pub use super::errors::MessagingError;
pub use super::MessagingResult;

// Re-export PgmqNotifyConfig for convenient access when constructing providers
pub use tasker_pgmq::PgmqNotifyConfig;
