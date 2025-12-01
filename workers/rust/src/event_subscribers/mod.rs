//! # Event Subscribers
//!
//! TAS-65: Example event subscribers for fast/in-process domain events.
//!
//! ## Purpose
//!
//! This module provides example implementations of domain event subscribers
//! that can be used with the `InProcessEventBus`. These subscribers demonstrate
//! common patterns for handling fast/in-process events.
//!
//! ## Available Subscribers
//!
//! - [`logging_subscriber`] - Logs events using the tracing framework
//! - [`metrics_subscriber`] - Collects event metrics and statistics
//!
//! ## Usage
//!
//! ```rust,ignore
//! use tasker_worker_rust::event_subscribers::{
//!     logging_subscriber::create_logging_subscriber,
//!     metrics_subscriber::EventMetricsCollector,
//! };
//! use tasker_worker::worker::in_process_event_bus::InProcessEventBus;
//!
//! // Create bus and register subscribers
//! let mut bus = InProcessEventBus::new(config);
//!
//! // Add logging for all events
//! bus.subscribe("*", create_logging_subscriber("[ALL]")).unwrap();
//!
//! // Add metrics collection
//! let metrics = EventMetricsCollector::new();
//! bus.subscribe("*", metrics.create_handler()).unwrap();
//!
//! // Query metrics later
//! println!("Total events: {}", metrics.events_received());
//! ```
//!
//! ## Important Notes
//!
//! - These subscribers are for **fast/in-process** events only
//! - Fast events are fire-and-forget with no persistence
//! - Durable events go to PGMQ and require separate consumer processes
//! - Fast events are only observable in integration tests (same memory space)

pub mod logging_subscriber;
pub mod metrics_subscriber;

// Re-export common types
pub use logging_subscriber::{
    create_debug_logging_subscriber, create_logging_subscriber, create_verbose_logging_subscriber,
};
pub use metrics_subscriber::{EventMetricsCollector, MetricsSummary};
