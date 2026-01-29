//! # Messaging Service Providers
//!
//! Concrete implementations of the `MessagingService` trait for different backends.
//!
//! ## Providers
//!
//! - [`PgmqMessagingService`] - PostgreSQL Message Queue via tasker-pgmq
//! - [`RabbitMqMessagingService`] - RabbitMQ via lapin crate (TAS-133d)
//! - [`InMemoryMessagingService`] - Thread-safe in-memory queues for testing

mod in_memory;
mod pgmq;
mod rabbitmq;

pub use in_memory::InMemoryMessagingService;
pub use pgmq::PgmqMessagingService;
pub use rabbitmq::RabbitMqMessagingService;
