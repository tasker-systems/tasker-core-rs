//! # tasker-pgmq
//!
//! Generic `PostgreSQL` LISTEN/NOTIFY integration for PGMQ queues.
//!
//! This crate provides event-driven capabilities for PGMQ queue operations,
//! enabling real-time notifications for queue creation, message enqueueing,
//! and other queue lifecycle events.
//!
//! ## Features
//!
//! - **Generic Design**: Works with any PGMQ setup, not tied to specific applications
//! - **Namespace Awareness**: Supports namespace extraction from queue names
//! - **Reliable Notifications**: Built on `sqlx::PgListener` with auto-reconnection
//! - **Minimal Payloads**: Keeps notifications under `pg_notify` 8KB limit
//! - **Configurable Channels**: Customizable channel naming and prefixes
//!
//! ## Architecture
//!
//! The crate provides three main components:
//!
//! 1. **Event Types**: Structured representations of PGMQ events
//! 2. **Database Triggers**: Automatically publish notifications via SQL triggers (recommended)  
//! 3. **Listeners**: Subscribe to and receive typed events via `pg_notify`
//! 4. **Enhanced Client**: PGMQ wrapper that integrates with trigger-based notifications
//!
//! ## Usage - Trigger-Based (Recommended)
//!
//! ```rust,ignore
//! use tasker_pgmq::{PgmqNotifyClient, PgmqNotifyListener, PgmqNotifyConfig, PgmqNotifyEvent};
//!
//! // 1. Install database triggers via migration (one-time setup)
//! // Run: tasker-pgmq-cli generate-migration --name pgmq_notifications
//! // Then apply the generated migration to your database
//!
//! // 2. Create enhanced PGMQ client with trigger-based notifications
//! let config = PgmqNotifyConfig::new()
//!     .with_queue_naming_pattern(r"(?P<namespace>\w+)_queue")
//!     .with_default_namespace("orders");
//!
//! let mut client = PgmqNotifyClient::new(database_url, config).await?;
//!
//! // 3. Create listener for real-time event processing (TAS-51: bounded channel)
//! let buffer_size = 1000; // From config.mpsc_channels.*.event_listeners.pgmq_event_buffer_size
//! let mut listener = PgmqNotifyListener::new(pool, config, buffer_size).await?;
//! listener.connect().await?;
//! listener.listen_message_ready_for_namespace("orders").await?;
//!
//! // 4. Queue operations automatically emit notifications via triggers
//! client.create_queue("orders_queue").await?; // Triggers queue_created notification
//! client.send("orders_queue", &my_message).await?; // Triggers message_ready notification
//!
//! // 5. Process real-time events
//! while let Some(event) = listener.next_event().await? {
//!     match event {
//!         PgmqNotifyEvent::MessageReady { msg_id, queue_name, .. } => {
//!             println!("Message {} ready in queue {}", msg_id, queue_name);
//!             // Process message with <10ms latency
//!         }
//!     }
//! }
//! ```

pub mod channel_metrics;
pub mod client;
pub mod config;
pub mod emitter;
pub mod error;
pub mod events;
pub mod listener;
pub mod types;

// Re-export main types for trigger-based usage (recommended)
pub use client::{PgmqClient, PgmqNotifyClient, PgmqNotifyClientFactory};
pub use config::PgmqNotifyConfig;
pub use error::{PgmqNotifyError, Result};
pub use events::{MessageReadyEvent, MessageWithPayloadEvent, PgmqNotifyEvent, QueueCreatedEvent};
pub use listener::PgmqNotifyListener;
pub use types::{ClientStatus, MessagingError, QueueMetrics};

// Legacy application-level emitters (for advanced use cases only)
// Most users should use database triggers instead
pub use emitter::{DbEmitter, NoopEmitter, PgmqNotifyEmitter};
