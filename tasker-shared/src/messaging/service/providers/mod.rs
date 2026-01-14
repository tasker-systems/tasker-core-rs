//! # Messaging Service Providers
//!
//! Concrete implementations of the `MessagingService` trait for different backends.
//!
//! ## Providers
//!
//! - [`PgmqMessagingService`] - PostgreSQL Message Queue via pgmq-notify
//! - [`InMemoryMessagingService`] - Thread-safe in-memory queues for testing

mod in_memory;
mod pgmq;

pub use in_memory::InMemoryMessagingService;
pub use pgmq::PgmqMessagingService;
