//! # Channel Metrics (TAS-51 Phase 4 - Observability)
//!
//! OpenTelemetry metrics for MPSC channel health and performance including:
//! - Message throughput counters (sent/received)
//! - Channel saturation gauges
//! - Overflow and backpressure metrics
//! - Health status indicators
//!
//! ## Usage
//!
//! ```rust
//! use tasker_shared::metrics::channels::*;
//! use opentelemetry::KeyValue;
//!
//! // Record message sent
//! let channel_name = "orchestration_command";
//! let component = "orchestration";
//! channel_messages_sent().add(
//!     1,
//!     &[
//!         KeyValue::new("channel_name", channel_name.to_string()),
//!         KeyValue::new("component", component.to_string()),
//!     ],
//! );
//!
//! // Record saturation
//! let saturation = 0.75; // 75% full
//! channel_saturation().record(
//!     saturation,
//!     &[
//!         KeyValue::new("channel_name", channel_name.to_string()),
//!         KeyValue::new("component", component.to_string()),
//!     ],
//! );
//! ```
//!
//! ## Integration with ChannelMonitor
//!
//! The ChannelMetricsRecorder integrates directly with ChannelMonitor to
//! automatically export metrics to OpenTelemetry:
//!
//! ```rust,ignore
//! use tasker_shared::monitoring::ChannelMonitor;
//! use tasker_shared::metrics::channels::ChannelMetricsRecorder;
//!
//! let monitor = ChannelMonitor::new("worker_command", 1000);
//! let recorder = ChannelMetricsRecorder::new(monitor.clone());
//!
//! // Start background metrics export
//! recorder.start_periodic_export(Duration::from_secs(60));
//! ```

use opentelemetry::metrics::{Counter, Gauge, Meter};
use std::sync::{Arc, OnceLock};
use std::time::Duration;
use tracing::{debug, warn};

use crate::monitoring::channel_metrics::ChannelMonitor;

/// Lazy-initialized meter for channel metrics
static CHANNEL_METER: OnceLock<Meter> = OnceLock::new();

/// Get or initialize the channel meter
fn meter() -> &'static Meter {
    CHANNEL_METER.get_or_init(|| opentelemetry::global::meter_provider().meter("tasker-channels"))
}

// ============================================================================
// Counters - Message Throughput
// ============================================================================

/// Total number of messages sent through channel
///
/// Labels:
/// - channel_name: Channel identifier (e.g., "orchestration_command")
/// - component: Component name (orchestration, worker, shared)
pub fn channel_messages_sent() -> Counter<u64> {
    meter()
        .u64_counter("tasker.channel.messages.sent")
        .with_description("Total number of messages sent through channel")
        .build()
}

/// Total number of messages received from channel
///
/// Labels:
/// - channel_name: Channel identifier
/// - component: Component name
pub fn channel_messages_received() -> Counter<u64> {
    meter()
        .u64_counter("tasker.channel.messages.received")
        .with_description("Total number of messages received from channel")
        .build()
}

/// Total number of channel overflow events (backpressure applied)
///
/// Labels:
/// - channel_name: Channel identifier
/// - component: Component name
pub fn channel_overflow_events() -> Counter<u64> {
    meter()
        .u64_counter("tasker.channel.overflow.events")
        .with_description("Total number of channel overflow events (backpressure applied)")
        .build()
}

/// Total number of messages dropped due to overflow
///
/// Labels:
/// - channel_name: Channel identifier
/// - component: Component name
pub fn channel_messages_dropped() -> Counter<u64> {
    meter()
        .u64_counter("tasker.channel.messages.dropped")
        .with_description("Total number of messages dropped due to channel overflow")
        .build()
}

// ============================================================================
// Gauges - Channel State
// ============================================================================

/// Current channel saturation ratio (0.0 to 1.0)
///
/// Values approaching 1.0 indicate channel is near capacity.
/// Thresholds: 0.8 = degraded, 0.95 = critical
///
/// Labels:
/// - channel_name: Channel identifier
/// - component: Component name
pub fn channel_saturation() -> Gauge<f64> {
    meter()
        .f64_gauge("tasker.channel.saturation")
        .with_description("Current channel saturation ratio (0.0 to 1.0)")
        .build()
}

/// Configured channel buffer size
///
/// Labels:
/// - channel_name: Channel identifier
/// - component: Component name
pub fn channel_buffer_size() -> Gauge<u64> {
    meter()
        .u64_gauge("tasker.channel.buffer_size")
        .with_description("Configured channel buffer size")
        .build()
}

/// Channel health status indicator (0 = healthy, 1 = degraded, 2 = critical)
///
/// Labels:
/// - channel_name: Channel identifier
/// - component: Component name
/// - status: healthy, degraded, critical
pub fn channel_health_status() -> Gauge<u64> {
    meter()
        .u64_gauge("tasker.channel.health_status")
        .with_description("Channel health status (0=healthy, 1=degraded, 2=critical)")
        .build()
}

/// Current message backlog (messages in channel)
///
/// Calculated as: messages_sent - messages_received
///
/// Labels:
/// - channel_name: Channel identifier
/// - component: Component name
pub fn channel_message_backlog() -> Gauge<u64> {
    meter()
        .u64_gauge("tasker.channel.backlog")
        .with_description("Current number of messages in channel")
        .build()
}

// ============================================================================
// Static instances for convenience
// ============================================================================

/// Static counter: channel_messages_sent
pub static CHANNEL_MESSAGES_SENT: OnceLock<Counter<u64>> = OnceLock::new();

/// Static counter: channel_messages_received
pub static CHANNEL_MESSAGES_RECEIVED: OnceLock<Counter<u64>> = OnceLock::new();

/// Static counter: channel_overflow_events
pub static CHANNEL_OVERFLOW_EVENTS: OnceLock<Counter<u64>> = OnceLock::new();

/// Static counter: channel_messages_dropped
pub static CHANNEL_MESSAGES_DROPPED: OnceLock<Counter<u64>> = OnceLock::new();

/// Static gauge: channel_saturation
pub static CHANNEL_SATURATION: OnceLock<Gauge<f64>> = OnceLock::new();

/// Static gauge: channel_buffer_size
pub static CHANNEL_BUFFER_SIZE: OnceLock<Gauge<u64>> = OnceLock::new();

/// Static gauge: channel_health_status
pub static CHANNEL_HEALTH_STATUS: OnceLock<Gauge<u64>> = OnceLock::new();

/// Static gauge: channel_message_backlog
pub static CHANNEL_MESSAGE_BACKLOG: OnceLock<Gauge<u64>> = OnceLock::new();

/// Initialize all channel metrics
///
/// This should be called during application startup after init_metrics().
pub fn init() {
    CHANNEL_MESSAGES_SENT.get_or_init(channel_messages_sent);
    CHANNEL_MESSAGES_RECEIVED.get_or_init(channel_messages_received);
    CHANNEL_OVERFLOW_EVENTS.get_or_init(channel_overflow_events);
    CHANNEL_MESSAGES_DROPPED.get_or_init(channel_messages_dropped);
    CHANNEL_SATURATION.get_or_init(channel_saturation);
    CHANNEL_BUFFER_SIZE.get_or_init(channel_buffer_size);
    CHANNEL_HEALTH_STATUS.get_or_init(channel_health_status);
    CHANNEL_MESSAGE_BACKLOG.get_or_init(channel_message_backlog);

    debug!("Channel metrics initialized");
}

// ============================================================================
// Channel Metrics Recorder - Automatic Integration
// ============================================================================

/// Periodic metrics recorder for ChannelMonitor
///
/// Automatically exports channel metrics to OpenTelemetry at regular intervals.
/// This integrates with the existing ChannelMonitor infrastructure to provide
/// observability without changing the core monitoring code.
#[derive(Clone, Debug)]
pub struct ChannelMetricsRecorder {
    monitor: ChannelMonitor,
}

impl ChannelMetricsRecorder {
    /// Create a new metrics recorder for a channel monitor
    pub fn new(monitor: ChannelMonitor) -> Self {
        Self { monitor }
    }

    /// Export current metrics snapshot to OpenTelemetry
    ///
    /// This method reads the current state from ChannelMonitor and exports
    /// all metrics to OpenTelemetry with appropriate labels.
    pub fn export_snapshot(&self, available_capacity: usize) {
        let metrics = self.monitor.metrics();
        let channel_name = self.monitor.channel_name();
        let component = self.monitor.component_name();

        // Create labels for all metrics
        let labels = &[
            opentelemetry::KeyValue::new("channel_name", channel_name.to_string()),
            opentelemetry::KeyValue::new("component", component.to_string()),
        ];

        // Export counters
        CHANNEL_MESSAGES_SENT
            .get()
            .unwrap()
            .add(metrics.messages_sent, labels);

        CHANNEL_MESSAGES_RECEIVED
            .get()
            .unwrap()
            .add(metrics.messages_received, labels);

        CHANNEL_OVERFLOW_EVENTS
            .get()
            .unwrap()
            .add(metrics.overflow_events, labels);

        CHANNEL_MESSAGES_DROPPED
            .get()
            .unwrap()
            .add(metrics.messages_dropped, labels);

        // Calculate and export saturation
        let saturation = self.monitor.calculate_saturation(available_capacity);
        CHANNEL_SATURATION.get().unwrap().record(saturation, labels);

        // Export buffer size
        CHANNEL_BUFFER_SIZE
            .get()
            .unwrap()
            .record(self.monitor.buffer_size() as u64, labels);

        // Export health status
        let health = self.monitor.check_health(available_capacity);
        let health_value = match health {
            crate::monitoring::channel_metrics::ChannelHealthStatus::Healthy => 0,
            crate::monitoring::channel_metrics::ChannelHealthStatus::Degraded { .. } => 1,
            crate::monitoring::channel_metrics::ChannelHealthStatus::Critical { .. } => 2,
        };
        CHANNEL_HEALTH_STATUS
            .get()
            .unwrap()
            .record(health_value, labels);

        // Calculate and export backlog
        let backlog = metrics
            .messages_sent
            .saturating_sub(metrics.messages_received);
        CHANNEL_MESSAGE_BACKLOG
            .get()
            .unwrap()
            .record(backlog, labels);

        debug!(
            channel = channel_name,
            component = component,
            sent = metrics.messages_sent,
            received = metrics.messages_received,
            saturation = saturation,
            health = ?health,
            "Exported channel metrics snapshot"
        );
    }

    /// Start periodic metrics export in background
    ///
    /// Spawns a background task that exports metrics at the specified interval.
    /// Returns a handle that can be used to stop the export loop.
    ///
    /// # Arguments
    /// * `interval` - How often to export metrics (recommended: 60 seconds to match OTLP export)
    /// * `get_capacity` - Function to get current channel capacity (e.g., `|| sender.capacity()`)
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let recorder = ChannelMetricsRecorder::new(monitor);
    /// let sender_clone = sender.clone();
    /// let handle = recorder.start_periodic_export(
    ///     Duration::from_secs(60),
    ///     move || sender_clone.capacity()
    /// );
    ///
    /// // Later, to stop:
    /// handle.abort();
    /// ```
    pub fn start_periodic_export<F>(
        self,
        interval: Duration,
        get_capacity: F,
    ) -> tokio::task::JoinHandle<()>
    where
        F: Fn() -> usize + Send + 'static,
    {
        tokio::spawn(async move {
            let mut interval_timer = tokio::time::interval(interval);
            interval_timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

            loop {
                interval_timer.tick().await;

                let capacity = get_capacity();
                self.export_snapshot(capacity);
            }
        })
    }

    /// Export metrics once and return
    ///
    /// Useful for on-demand metric collection or manual integration with
    /// existing export loops.
    pub async fn export_once(&self, available_capacity: usize) {
        self.export_snapshot(available_capacity);
    }
}

// ============================================================================
// Channel Metrics Registry - System-Wide Tracking
// ============================================================================

/// Global registry for tracking all instrumented channels
///
/// This allows system health endpoints to report on all channels without
/// needing explicit references to each ChannelMonitor.
#[derive(Debug)]
pub struct ChannelMetricsRegistry {
    recorders: tokio::sync::RwLock<Vec<ChannelMetricsRecorder>>,
}

impl ChannelMetricsRegistry {
    /// Create a new empty registry
    pub fn new() -> Self {
        Self {
            recorders: tokio::sync::RwLock::new(Vec::new()),
        }
    }

    /// Register a channel monitor for metrics export
    pub async fn register(&self, monitor: ChannelMonitor) {
        let channel_name = monitor.channel_name().to_string();
        let component_name = monitor.component_name().to_string();

        let recorder = ChannelMetricsRecorder::new(monitor);
        self.recorders.write().await.push(recorder);

        debug!(
            channel = channel_name,
            component = component_name,
            "Registered channel in metrics registry"
        );
    }

    /// Export metrics for all registered channels
    ///
    /// # Arguments
    /// * `get_capacity` - Function that returns available capacity for a given channel name
    pub async fn export_all<F>(&self, get_capacity: F)
    where
        F: Fn(&str) -> Option<usize>,
    {
        let recorders = self.recorders.read().await;

        for recorder in recorders.iter() {
            let channel_name = recorder.monitor.channel_name();
            if let Some(capacity) = get_capacity(channel_name) {
                recorder.export_snapshot(capacity);
            } else {
                warn!(
                    channel = channel_name,
                    "No capacity function provided for channel, skipping metrics export"
                );
            }
        }
    }

    /// Get health status for all channels
    ///
    /// Returns a vector of (channel_name, component, health_status) tuples.
    pub async fn get_all_health<F>(&self, get_capacity: F) -> Vec<(String, String, String)>
    where
        F: Fn(&str) -> Option<usize>,
    {
        let recorders = self.recorders.read().await;
        let mut health_reports = Vec::new();

        for recorder in recorders.iter() {
            let channel_name = recorder.monitor.channel_name();
            if let Some(capacity) = get_capacity(channel_name) {
                let health = recorder.monitor.check_health(capacity);
                let health_str = match health {
                    crate::monitoring::channel_metrics::ChannelHealthStatus::Healthy => {
                        "healthy".to_string()
                    }
                    crate::monitoring::channel_metrics::ChannelHealthStatus::Degraded {
                        saturation_percent,
                    } => {
                        format!("degraded ({}%)", saturation_percent)
                    }
                    crate::monitoring::channel_metrics::ChannelHealthStatus::Critical {
                        saturation_percent,
                    } => {
                        format!("critical ({}%)", saturation_percent)
                    }
                };

                health_reports.push((
                    channel_name.to_string(),
                    recorder.monitor.component_name().to_string(),
                    health_str,
                ));
            }
        }

        health_reports
    }

    /// Check if any channel is unhealthy
    pub async fn has_unhealthy_channels<F>(&self, get_capacity: F) -> bool
    where
        F: Fn(&str) -> Option<usize>,
    {
        let recorders = self.recorders.read().await;

        for recorder in recorders.iter() {
            let channel_name = recorder.monitor.channel_name();
            if let Some(capacity) = get_capacity(channel_name) {
                let health = recorder.monitor.check_health(capacity);
                if !health.is_healthy() {
                    return true;
                }
            }
        }

        false
    }
}

impl Default for ChannelMetricsRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Global channel metrics registry instance
static CHANNEL_REGISTRY: OnceLock<Arc<ChannelMetricsRegistry>> = OnceLock::new();

/// Get or initialize the global channel metrics registry
pub fn global_registry() -> &'static Arc<ChannelMetricsRegistry> {
    CHANNEL_REGISTRY.get_or_init(|| Arc::new(ChannelMetricsRegistry::new()))
}

/// Register a channel monitor with the global registry
///
/// Note: This spawns a background task to perform the async registration.
/// For most use cases, this is fine since registration happens during startup.
pub fn register_channel(monitor: ChannelMonitor) {
    let registry = global_registry().clone();
    tokio::spawn(async move {
        registry.register(monitor).await;
    });
}
