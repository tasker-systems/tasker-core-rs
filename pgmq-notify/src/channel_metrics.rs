//! # Channel Metrics and Observability for PGMQ-Notify
//!
//! TAS-51: Comprehensive observability for MPSC channel health and performance.
//!
//! ## Features
//!
//! - **Saturation Tracking**: Real-time channel capacity monitoring
//! - **Overflow Detection**: Metrics for backpressure events
//! - **Throughput Monitoring**: Message send/receive counters
//! - **Health Checks**: Integration with system health endpoints

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tracing::{debug, info, warn};

/// Channel health status
#[derive(Debug, Clone, PartialEq)]
pub enum ChannelHealthStatus {
    /// Channel operating normally (<80% capacity)
    Healthy,
    /// Channel approaching capacity (80-95%)
    Degraded { saturation_percent: f64 },
    /// Channel critically full (>95%)
    Critical { saturation_percent: f64 },
}

impl ChannelHealthStatus {
    /// Create health status from saturation percentage
    pub fn from_saturation(saturation: f64) -> Self {
        if saturation >= 0.95 {
            Self::Critical {
                saturation_percent: saturation * 100.0,
            }
        } else if saturation >= 0.80 {
            Self::Degraded {
                saturation_percent: saturation * 100.0,
            }
        } else {
            Self::Healthy
        }
    }

    /// Check if status indicates a problem
    pub fn is_healthy(&self) -> bool {
        matches!(self, Self::Healthy)
    }

    /// Get saturation percentage if available
    pub fn saturation_percent(&self) -> Option<f64> {
        match self {
            Self::Healthy => None,
            Self::Degraded { saturation_percent } | Self::Critical { saturation_percent } => {
                Some(*saturation_percent)
            }
        }
    }
}

/// Channel metrics collector
#[derive(Debug, Clone)]
pub struct ChannelMetrics {
    /// Total messages sent successfully
    pub messages_sent: u64,
    /// Total messages received
    pub messages_received: u64,
    /// Total overflow events (channel full)
    pub overflow_events: u64,
    /// Total messages dropped due to overflow
    pub messages_dropped: u64,
    /// Current saturation percentage (0.0-1.0)
    pub saturation: f64,
    /// Channel health status
    pub health_status: ChannelHealthStatus,
}

impl Default for ChannelMetrics {
    fn default() -> Self {
        Self {
            messages_sent: 0,
            messages_received: 0,
            overflow_events: 0,
            messages_dropped: 0,
            saturation: 0.0,
            health_status: ChannelHealthStatus::Healthy,
        }
    }
}

/// Channel monitor for tracking metrics
pub struct ChannelMonitor {
    /// Channel identifier
    channel_name: String,
    /// Component name (orchestration, worker, shared)
    component_name: String,
    /// Configured buffer size
    buffer_size: usize,
    /// Messages sent counter
    messages_sent: Arc<AtomicU64>,
    /// Messages received counter
    messages_received: Arc<AtomicU64>,
    /// Overflow events counter
    overflow_events: Arc<AtomicU64>,
    /// Messages dropped counter
    messages_dropped: Arc<AtomicU64>,
}

impl ChannelMonitor {
    /// Create a new channel monitor
    ///
    /// # Arguments
    /// * `channel_name` - Unique identifier for the channel (e.g., "orchestration_command")
    /// * `buffer_size` - Configured buffer size for saturation calculation
    pub fn new(channel_name: impl Into<String>, buffer_size: usize) -> Self {
        let channel_name = channel_name.into();
        let component_name = Self::extract_component(&channel_name);

        info!(
            channel = %channel_name,
            component = %component_name,
            buffer_size = buffer_size,
            "Channel monitor initialized"
        );

        Self {
            channel_name,
            component_name,
            buffer_size,
            messages_sent: Arc::new(AtomicU64::new(0)),
            messages_received: Arc::new(AtomicU64::new(0)),
            overflow_events: Arc::new(AtomicU64::new(0)),
            messages_dropped: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Extract component name from channel name
    fn extract_component(channel_name: &str) -> String {
        if channel_name.starts_with("orchestration") {
            "orchestration".to_string()
        } else if channel_name.starts_with("worker") {
            "worker".to_string()
        } else if channel_name.starts_with("shared") || channel_name.starts_with("ffi") {
            "shared".to_string()
        } else if channel_name.starts_with("pgmq") {
            "pgmq".to_string()
        } else {
            "unknown".to_string()
        }
    }

    /// Record a successful message send
    ///
    /// # Returns
    /// `true` if saturation should be checked (periodic sampling for performance)
    ///
    /// # Performance
    /// This method is optimized for the hot path:
    /// - Single atomic increment (always executed)
    /// - Cheap modulo check to determine if saturation check is needed
    /// - Returns boolean to let caller decide whether to perform expensive saturation calculation
    ///
    /// Saturation is checked every ~5% of buffer size (min 100, max 1000 messages)
    /// to balance responsiveness with performance overhead.
    pub fn record_send_success(&self) -> bool {
        let count = self.messages_sent.fetch_add(1, Ordering::Relaxed);

        // Log milestone metrics (every 10k messages)
        if count > 0 && count.is_multiple_of(10000) {
            debug!(
                channel = %self.channel_name,
                total_sent = count,
                "Channel throughput milestone"
            );
        }

        // Periodic saturation checks: ~5% of buffer (min 100, max 1000)
        // This dramatically reduces overhead while maintaining good visibility
        let check_interval = (self.buffer_size / 20).clamp(100, 1000);
        count.is_multiple_of(check_interval as u64)
    }

    /// Record a message receive
    pub fn record_receive(&self) {
        self.messages_received.fetch_add(1, Ordering::Relaxed);
    }

    /// Record an overflow event (channel full, message blocked)
    pub fn record_overflow_blocked(&self) {
        let count = self.overflow_events.fetch_add(1, Ordering::Relaxed);

        warn!(
            channel = %self.channel_name,
            overflow_count = count + 1,
            "Channel overflow - message blocked (backpressure applied)"
        );
    }

    /// Record a dropped message (overflow with drop policy)
    pub fn record_message_dropped(&self) {
        let sent = self.messages_sent.load(Ordering::Relaxed);
        let dropped = self.messages_dropped.fetch_add(1, Ordering::Relaxed);

        warn!(
            channel = %self.channel_name,
            messages_sent = sent,
            messages_dropped = dropped + 1,
            drop_rate_percent = ((dropped + 1) as f64 / (sent + dropped + 1) as f64) * 100.0,
            "Channel overflow - message DROPPED (consider increasing buffer_size)"
        );
    }

    /// Calculate current saturation from available capacity
    ///
    /// # Arguments
    /// * `available_capacity` - Current available slots in the channel
    ///
    /// # Returns
    /// Saturation as a value between 0.0 (empty) and 1.0 (full)
    ///
    /// # Performance Note
    /// This method is relatively expensive (division + comparisons + potential logging).
    /// Use `record_send_success()` return value to decide when to call this.
    pub fn calculate_saturation(&self, available_capacity: usize) -> f64 {
        if self.buffer_size == 0 {
            return 0.0;
        }

        let used = self.buffer_size.saturating_sub(available_capacity);
        let saturation = used as f64 / self.buffer_size as f64;

        // Log warnings at threshold crossings
        if saturation >= 0.95 {
            warn!(
                channel = %self.channel_name,
                saturation_percent = saturation * 100.0,
                available = available_capacity,
                buffer_size = self.buffer_size,
                "CRITICAL: Channel critically full (>95%)"
            );
        } else if saturation >= 0.80 {
            warn!(
                channel = %self.channel_name,
                saturation_percent = saturation * 100.0,
                available = available_capacity,
                buffer_size = self.buffer_size,
                "Channel approaching capacity (>80%)"
            );
        }

        saturation
    }

    /// Check saturation and log warning if approaching capacity (convenience method)
    ///
    /// This combines `calculate_saturation()` with automatic warning logging.
    /// Use with `record_send_success()` for optimal performance:
    ///
    /// ```rust,ignore
    /// if monitor.record_send_success() {
    ///     monitor.check_and_warn_saturation(sender.capacity());
    /// }
    /// ```
    pub fn check_and_warn_saturation(&self, available_capacity: usize) {
        let _saturation = self.calculate_saturation(available_capacity);
        // Warnings are logged inside calculate_saturation()
    }

    /// Get current channel metrics
    pub fn metrics(&self) -> ChannelMetrics {
        ChannelMetrics {
            messages_sent: self.messages_sent.load(Ordering::Relaxed),
            messages_received: self.messages_received.load(Ordering::Relaxed),
            overflow_events: self.overflow_events.load(Ordering::Relaxed),
            messages_dropped: self.messages_dropped.load(Ordering::Relaxed),
            saturation: 0.0, // Must be calculated with current capacity
            health_status: ChannelHealthStatus::Healthy,
        }
    }

    /// Check channel health based on current capacity
    pub fn check_health(&self, available_capacity: usize) -> ChannelHealthStatus {
        let saturation = self.calculate_saturation(available_capacity);
        ChannelHealthStatus::from_saturation(saturation)
    }

    /// Get channel name
    pub fn channel_name(&self) -> &str {
        &self.channel_name
    }

    /// Get component name
    pub fn component_name(&self) -> &str {
        &self.component_name
    }

    /// Get buffer size
    pub fn buffer_size(&self) -> usize {
        self.buffer_size
    }
}

impl Clone for ChannelMonitor {
    fn clone(&self) -> Self {
        Self {
            channel_name: self.channel_name.clone(),
            component_name: self.component_name.clone(),
            buffer_size: self.buffer_size,
            messages_sent: Arc::clone(&self.messages_sent),
            messages_received: Arc::clone(&self.messages_received),
            overflow_events: Arc::clone(&self.overflow_events),
            messages_dropped: Arc::clone(&self.messages_dropped),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_health_status_from_saturation() {
        assert!(matches!(
            ChannelHealthStatus::from_saturation(0.5),
            ChannelHealthStatus::Healthy
        ));

        assert!(matches!(
            ChannelHealthStatus::from_saturation(0.85),
            ChannelHealthStatus::Degraded { .. }
        ));

        assert!(matches!(
            ChannelHealthStatus::from_saturation(0.96),
            ChannelHealthStatus::Critical { .. }
        ));
    }

    #[test]
    fn test_channel_monitor_creation() {
        let monitor = ChannelMonitor::new("pgmq_notify_listener", 1000);
        assert_eq!(monitor.channel_name(), "pgmq_notify_listener");
        assert_eq!(monitor.component_name(), "pgmq");
        assert_eq!(monitor.buffer_size(), 1000);
    }

    #[test]
    fn test_saturation_calculation() {
        let monitor = ChannelMonitor::new("test_channel", 1000);

        // Empty channel
        assert_eq!(monitor.calculate_saturation(1000), 0.0);

        // Half full
        assert_eq!(monitor.calculate_saturation(500), 0.5);

        // Full channel
        assert_eq!(monitor.calculate_saturation(0), 1.0);
    }

    #[test]
    fn test_metrics_recording() {
        let monitor = ChannelMonitor::new("test_channel", 1000);

        monitor.record_send_success();
        monitor.record_send_success();
        monitor.record_receive();
        monitor.record_overflow_blocked();
        monitor.record_message_dropped();

        let metrics = monitor.metrics();
        assert_eq!(metrics.messages_sent, 2);
        assert_eq!(metrics.messages_received, 1);
        assert_eq!(metrics.overflow_events, 1);
        assert_eq!(metrics.messages_dropped, 1);
    }

    #[test]
    fn test_health_check() {
        let monitor = ChannelMonitor::new("test_channel", 1000);

        // Healthy
        let health = monitor.check_health(900);
        assert!(health.is_healthy());

        // Degraded
        let health = monitor.check_health(150);
        assert!(!health.is_healthy());
        assert!(matches!(health, ChannelHealthStatus::Degraded { .. }));

        // Critical
        let health = monitor.check_health(10);
        assert!(!health.is_healthy());
        assert!(matches!(health, ChannelHealthStatus::Critical { .. }));
    }

    #[test]
    fn test_component_extraction() {
        assert_eq!(
            ChannelMonitor::extract_component("pgmq_notify_listener"),
            "pgmq"
        );
        assert_eq!(
            ChannelMonitor::extract_component("orchestration_command"),
            "orchestration"
        );
        assert_eq!(ChannelMonitor::extract_component("worker_event"), "worker");
        assert_eq!(
            ChannelMonitor::extract_component("shared_publisher"),
            "shared"
        );
    }
}
