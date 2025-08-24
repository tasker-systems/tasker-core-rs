//! Worker metrics collection

use prometheus::{Counter, Gauge, Histogram, Registry};
use std::sync::Arc;

/// Worker metrics collector
pub struct WorkerMetrics {
    registry: Arc<Registry>,
    step_processing_counter: Counter,
    step_processing_duration: Histogram,
    active_executors: Gauge,
    queue_depth: Gauge,
}

impl WorkerMetrics {
    /// Create new metrics collector
    pub fn new() -> crate::error::Result<Self> {
        let registry = Arc::new(Registry::new());

        let step_processing_counter = Counter::new(
            "worker_steps_processed_total",
            "Total number of steps processed by workers",
        )
        .map_err(|e| crate::error::WorkerError::Other(e.into()))?;

        let step_processing_duration = Histogram::new(
            "worker_step_processing_duration_seconds",
            "Time spent processing steps",
        )
        .map_err(|e| crate::error::WorkerError::Other(e.into()))?;

        let active_executors = Gauge::new(
            "worker_active_executors",
            "Number of active worker executors",
        )
        .map_err(|e| crate::error::WorkerError::Other(e.into()))?;

        let queue_depth = Gauge::new(
            "worker_queue_depth",
            "Current queue depth for worker namespaces",
        )
        .map_err(|e| crate::error::WorkerError::Other(e.into()))?;

        // Register metrics
        registry
            .register(Box::new(step_processing_counter.clone()))
            .map_err(|e| crate::error::WorkerError::Other(e.into()))?;
        registry
            .register(Box::new(step_processing_duration.clone()))
            .map_err(|e| crate::error::WorkerError::Other(e.into()))?;
        registry
            .register(Box::new(active_executors.clone()))
            .map_err(|e| crate::error::WorkerError::Other(e.into()))?;
        registry
            .register(Box::new(queue_depth.clone()))
            .map_err(|e| crate::error::WorkerError::Other(e.into()))?;

        Ok(Self {
            registry,
            step_processing_counter,
            step_processing_duration,
            active_executors,
            queue_depth,
        })
    }

    /// Record step processing
    pub fn record_step_processed(&self, duration: f64) {
        self.step_processing_counter.inc();
        self.step_processing_duration.observe(duration);
    }

    /// Update active executors count
    pub fn set_active_executors(&self, count: usize) {
        self.active_executors.set(count as f64);
    }

    /// Update queue depth
    pub fn set_queue_depth(&self, depth: usize) {
        self.queue_depth.set(depth as f64);
    }

    /// Get metrics as Prometheus format
    pub fn render_metrics(&self) -> Result<String, Box<dyn std::error::Error>> {
        use prometheus::Encoder;
        let encoder = prometheus::TextEncoder::new();
        let metric_families = self.registry.gather();
        encoder.encode_to_string(&metric_families)
    }
}

impl Default for WorkerMetrics {
    fn default() -> Self {
        Self::new().expect("Failed to create default metrics")
    }
}