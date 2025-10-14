//! # Monitoring and Observability
//!
//! TAS-51: Comprehensive monitoring infrastructure for tasker-core components.
//!
//! ## Features
//!
//! - **Channel Metrics**: MPSC channel health and performance monitoring
//! - **Health Checks**: System health status reporting
//! - **Performance Tracking**: Throughput and latency metrics
//!
//! ## Modules
//!
//! - `channel_metrics`: MPSC channel observability and metrics

pub mod channel_metrics;

pub use channel_metrics::{ChannelHealthStatus, ChannelMetrics, ChannelMonitor};
