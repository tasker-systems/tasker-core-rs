//! # Channel Health Evaluator
//!
//! TAS-75 Phase 5: Evaluates MPSC channel saturation and health.
//!
//! This module provides functions to check channel health without blocking the
//! API hot path. The evaluator is called by the background `StatusEvaluator` task.

use tasker_shared::monitoring::channel_metrics::ChannelMonitor;
use tokio::sync::mpsc;
use tracing::debug;

use super::types::{ChannelHealthConfig, ChannelHealthStatus};

/// Evaluate channel health status
///
/// Checks the command channel saturation and determines health state.
///
/// # Arguments
/// * `channel_monitor` - Monitor for the command channel
/// * `command_sender` - Sender handle to get current capacity
/// * `config` - Health check configuration with thresholds
///
/// # Type Parameters
/// * `T` - The message type of the channel
///
/// # Returns
/// `ChannelHealthStatus` with current channel health information
pub fn evaluate_channel_status<T>(
    channel_monitor: &ChannelMonitor,
    command_sender: &mpsc::Sender<T>,
    config: &ChannelHealthConfig,
) -> ChannelHealthStatus {
    let available_capacity = command_sender.capacity();
    let buffer_size = channel_monitor.buffer_size();
    let metrics = channel_monitor.metrics();

    // Calculate saturation percentage
    let saturation_percent = if buffer_size > 0 {
        let used = buffer_size.saturating_sub(available_capacity);
        (used as f64 / buffer_size as f64) * 100.0
    } else {
        0.0
    };

    // Determine if saturated/critical based on config thresholds
    let is_saturated = saturation_percent >= config.critical_threshold;
    let is_critical = saturation_percent >= config.emergency_threshold;

    debug!(
        channel_name = %channel_monitor.channel_name(),
        saturation_percent = saturation_percent,
        available_capacity = available_capacity,
        buffer_size = buffer_size,
        messages_sent = metrics.messages_sent,
        overflow_events = metrics.overflow_events,
        is_saturated = is_saturated,
        is_critical = is_critical,
        "Channel health evaluated"
    );

    ChannelHealthStatus {
        evaluated: true, // We actually evaluated the channel status
        command_saturation_percent: saturation_percent,
        command_available_capacity: available_capacity,
        command_messages_sent: metrics.messages_sent,
        command_overflow_events: metrics.overflow_events,
        is_saturated,
        is_critical,
    }
}

/// Create a channel health status without a monitor (for testing or fallback)
///
/// Uses direct sender capacity when a monitor is not available.
///
/// # Arguments
/// * `command_sender` - Sender handle to get current capacity
/// * `buffer_size` - The configured buffer size of the channel
/// * `config` - Health check configuration with thresholds
///
/// # Type Parameters
/// * `T` - The message type of the channel
///
/// # Returns
/// `ChannelHealthStatus` with current channel health information
pub fn evaluate_channel_status_basic<T>(
    command_sender: &mpsc::Sender<T>,
    buffer_size: usize,
    config: &ChannelHealthConfig,
) -> ChannelHealthStatus {
    let available_capacity = command_sender.capacity();

    // Calculate saturation percentage
    let saturation_percent = if buffer_size > 0 {
        let used = buffer_size.saturating_sub(available_capacity);
        (used as f64 / buffer_size as f64) * 100.0
    } else {
        0.0
    };

    // Determine if saturated/critical based on config thresholds
    let is_saturated = saturation_percent >= config.critical_threshold;
    let is_critical = saturation_percent >= config.emergency_threshold;

    ChannelHealthStatus {
        evaluated: true, // We actually evaluated the channel status
        command_saturation_percent: saturation_percent,
        command_available_capacity: available_capacity,
        command_messages_sent: 0,   // Not tracked without monitor
        command_overflow_events: 0, // Not tracked without monitor
        is_saturated,
        is_critical,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = ChannelHealthConfig::default();
        assert_eq!(config.warning_threshold, 70.0);
        assert_eq!(config.critical_threshold, 80.0);
        assert_eq!(config.emergency_threshold, 95.0);
    }

    #[tokio::test]
    async fn test_evaluate_channel_status_basic_empty_channel() {
        let (tx, _rx) = mpsc::channel::<()>(100);
        let config = ChannelHealthConfig::default();

        let status = evaluate_channel_status_basic(&tx, 100, &config);

        assert_eq!(status.command_saturation_percent, 0.0);
        assert_eq!(status.command_available_capacity, 100);
        assert!(!status.is_saturated);
        assert!(!status.is_critical);
    }

    #[tokio::test]
    async fn test_evaluate_channel_status_basic_partial_fill() {
        let (tx, _rx) = mpsc::channel::<()>(100);
        let config = ChannelHealthConfig::default();

        // Fill 50 of 100 slots
        for _ in 0..50 {
            tx.send(()).await.unwrap();
        }

        let status = evaluate_channel_status_basic(&tx, 100, &config);

        // 50 used out of 100 = 50% saturation
        assert!((status.command_saturation_percent - 50.0).abs() < 1.0);
        assert_eq!(status.command_available_capacity, 50);
        assert!(!status.is_saturated); // Below 80%
        assert!(!status.is_critical); // Below 95%
    }

    #[tokio::test]
    async fn test_evaluate_channel_status_basic_saturated() {
        let (tx, _rx) = mpsc::channel::<()>(100);
        let config = ChannelHealthConfig::default();

        // Fill 85 of 100 slots (85% > 80% critical threshold)
        for _ in 0..85 {
            tx.send(()).await.unwrap();
        }

        let status = evaluate_channel_status_basic(&tx, 100, &config);

        // 85 used out of 100 = 85% saturation
        assert!((status.command_saturation_percent - 85.0).abs() < 1.0);
        assert!(status.is_saturated); // Above 80%
        assert!(!status.is_critical); // Below 95%
    }

    #[tokio::test]
    async fn test_evaluate_channel_status_basic_critical() {
        let (tx, _rx) = mpsc::channel::<()>(100);
        let config = ChannelHealthConfig::default();

        // Fill 96 of 100 slots (96% > 95% emergency threshold)
        for _ in 0..96 {
            tx.send(()).await.unwrap();
        }

        let status = evaluate_channel_status_basic(&tx, 100, &config);

        // 96 used out of 100 = 96% saturation
        assert!((status.command_saturation_percent - 96.0).abs() < 1.0);
        assert!(status.is_saturated); // Above 80%
        assert!(status.is_critical); // Above 95%
    }

    #[tokio::test]
    async fn test_evaluate_channel_status_with_monitor() {
        let (tx, _rx) = mpsc::channel::<()>(100);
        let monitor = ChannelMonitor::new("test_channel", 100);
        let config = ChannelHealthConfig::default();

        // Fill 30 of 100 slots
        for _ in 0..30 {
            tx.send(()).await.unwrap();
            monitor.record_send_success();
        }

        let status = evaluate_channel_status(&monitor, &tx, &config);

        // 30 used out of 100 = 30% saturation
        assert!((status.command_saturation_percent - 30.0).abs() < 1.0);
        assert_eq!(status.command_messages_sent, 30);
        assert!(!status.is_saturated);
        assert!(!status.is_critical);
    }

    #[test]
    fn test_evaluate_channel_status_zero_buffer() {
        // Edge case: zero buffer size should not panic
        let config = ChannelHealthConfig::default();

        // We can't create a channel with 0 buffer, but we can test the calculation
        // with evaluate_channel_status_basic using a channel and pretending buffer is 0
        let (tx, _rx) = mpsc::channel::<()>(1);

        let status = evaluate_channel_status_basic(&tx, 0, &config);

        // With zero buffer, saturation should be 0
        assert_eq!(status.command_saturation_percent, 0.0);
        assert!(!status.is_saturated);
        assert!(!status.is_critical);
    }
}
