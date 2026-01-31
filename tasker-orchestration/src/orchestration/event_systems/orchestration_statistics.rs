//! # Orchestration Statistics
//!
//! Runtime statistics tracking for the orchestration event system.
//! Provides atomic counters for event processing metrics and component aggregation.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use tasker_shared::{DeploymentMode, EventSystemStatistics};

use crate::orchestration::orchestration_queues::{
    OrchestrationFallbackPoller, OrchestrationListenerStats, OrchestrationPollerStats,
    OrchestrationQueueListener,
};

/// Runtime statistics for orchestration event system
#[derive(Debug, Default)]
pub struct OrchestrationStatistics {
    /// Events processed counter (system-level)
    pub(crate) events_processed: AtomicU64,
    /// Events failed counter (system-level)
    pub(crate) events_failed: AtomicU64,
    /// Operations coordinated counter (system-level)
    pub(crate) operations_coordinated: AtomicU64,
    /// Last processing timestamp as epoch nanos (0 = never processed)
    pub(crate) last_processing_time_epoch_nanos: AtomicU64,
    /// Processing latencies for rate calculation (system-level)
    pub(crate) processing_latencies: std::sync::Mutex<VecDeque<Duration>>,
}

impl Clone for OrchestrationStatistics {
    fn clone(&self) -> Self {
        Self {
            events_processed: AtomicU64::new(self.events_processed.load(Ordering::Relaxed)),
            events_failed: AtomicU64::new(self.events_failed.load(Ordering::Relaxed)),
            operations_coordinated: AtomicU64::new(
                self.operations_coordinated.load(Ordering::Relaxed),
            ),
            last_processing_time_epoch_nanos: AtomicU64::new(
                self.last_processing_time_epoch_nanos
                    .load(Ordering::Relaxed),
            ),
            processing_latencies: std::sync::Mutex::new(
                self.processing_latencies
                    .lock()
                    .unwrap_or_else(|p| p.into_inner())
                    .clone(),
            ),
        }
    }
}

impl EventSystemStatistics for OrchestrationStatistics {
    fn events_processed(&self) -> u64 {
        self.events_processed.load(Ordering::Relaxed)
    }

    fn events_failed(&self) -> u64 {
        self.events_failed.load(Ordering::Relaxed)
    }

    fn processing_rate(&self) -> f64 {
        let latencies = self
            .processing_latencies
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        if latencies.is_empty() {
            return 0.0;
        }

        // Calculate events per second based on recent latencies
        let recent_latencies: Vec<_> = latencies.iter().rev().take(100).collect();
        if recent_latencies.is_empty() {
            return 0.0;
        }

        let total_time: Duration = recent_latencies.iter().copied().sum();
        if total_time.as_secs_f64() > 0.0 {
            recent_latencies.len() as f64 / total_time.as_secs_f64()
        } else {
            0.0
        }
    }

    fn average_latency_ms(&self) -> f64 {
        let latencies = self
            .processing_latencies
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        if latencies.is_empty() {
            return 0.0;
        }

        let sum: Duration = latencies.iter().sum();
        sum.as_millis() as f64 / latencies.len() as f64
    }

    fn deployment_mode_score(&self) -> f64 {
        // Score based on success rate and processing efficiency
        let total_events = self.events_processed() + self.events_failed();
        if total_events == 0 {
            return 1.0; // No events yet, assume perfect
        }

        let success_rate = self.events_processed() as f64 / total_events as f64;
        let latency = self.average_latency_ms();

        // High score for high success rate and low latency
        let latency_score = if latency > 0.0 { 100.0 / latency } else { 1.0 };
        (success_rate + latency_score.min(1.0)) / 2.0
    }
}

impl OrchestrationStatistics {
    /// Create statistics aggregated from component statistics
    pub async fn with_component_aggregation(
        &self,
        fallback_poller: Option<&OrchestrationFallbackPoller>,
        queue_listener: Option<&OrchestrationQueueListener>,
    ) -> OrchestrationStatistics {
        let aggregated = self.clone();

        // Aggregate fallback poller statistics
        if let Some(poller) = fallback_poller {
            let poller_stats = poller.stats().await;
            // Add poller-specific events to system total
            let poller_processed = poller_stats.messages_processed.load(Ordering::Relaxed);
            aggregated
                .events_processed
                .fetch_add(poller_processed, Ordering::Relaxed);

            // Add poller errors to system failed events
            let poller_errors = poller_stats.polling_errors.load(Ordering::Relaxed);
            aggregated
                .events_failed
                .fetch_add(poller_errors, Ordering::Relaxed);
        }

        // Aggregate queue listener statistics
        if let Some(listener) = queue_listener {
            let listener_stats = listener.stats().await;
            // Add listener-specific events to system total
            let listener_processed = listener_stats.events_received.load(Ordering::Relaxed);
            aggregated
                .events_processed
                .fetch_add(listener_processed, Ordering::Relaxed);

            // Add listener errors to system failed events
            let listener_errors = listener_stats.connection_errors.load(Ordering::Relaxed);
            aggregated
                .events_failed
                .fetch_add(listener_errors, Ordering::Relaxed);
        }

        aggregated
    }
}

/// Detailed component statistics for monitoring and debugging
#[derive(Debug)]
pub struct OrchestrationComponentStatistics {
    /// Fallback poller statistics (if active)
    pub fallback_poller_stats: Option<OrchestrationPollerStats>,
    /// Queue listener statistics (if active)
    pub queue_listener_stats: Option<OrchestrationListenerStats>,
    /// System uptime since start
    pub system_uptime: Option<Duration>,
    /// Current deployment mode
    pub deployment_mode: DeploymentMode,
}
